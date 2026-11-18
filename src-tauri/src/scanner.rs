//! Folder scanner. Reads audio files' tags and technical properties into the
//! library database and caches artwork. It only ever *reads* music files.
//!
//! Rules from the specification:
//! - Incremental: unchanged files (same size and modified time) are not re-read.
//! - A file that disappears is marked missing, never deleted from the database,
//!   so playlists keep their references.
//! - A file that was moved or renamed keeps its identity (playlists, favorites,
//!   history, play count, edits) when the move can be recognised with confidence.
//! - A folder that cannot be reached is skipped; its tracks are left alone.
//! - A track the user removed from the library is not added back.
//! - An unreadable or unsupported file is recorded with a reason and the scan
//!   continues.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::tag::{Accessor, ItemKey, Tag};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::artwork;
use crate::error::Result;
use crate::library;

/// Extensions the scanner looks at.
const PLAYABLE_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "m4a", "aac", "ogg", "oga", "aif", "aiff",
];
/// Known audio formats this build cannot decode. Reported, not added.
const UNSUPPORTED_EXTENSIONS: &[&str] = &["opus", "wma", "ape", "dsf", "dff", "wv", "mka"];
const FOLDER_ART_NAMES: &[&str] = &["cover", "folder", "front", "album"];
const BATCH: usize = 100;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// Files recognised as moved or renamed; they kept their library identity.
    pub moved: usize,
    /// Tracks whose file is no longer where it was.
    pub missing: usize,
    /// Files that could not be added (see the issue list).
    pub issues: usize,
    /// Folders that could not be opened. Their tracks were left as they were.
    pub unavailable_folders: Vec<String>,
}

struct Parsed {
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    genre: Option<String>,
    year: Option<i64>,
    track_no: Option<i64>,
    disc_no: Option<i64>,
    duration_ms: i64,
    codec: String,
    bitrate_kbps: Option<i64>,
    sample_rate: Option<i64>,
    bit_depth: Option<i64>,
    channels: Option<i64>,
    picture: Option<Vec<u8>>,
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// True for files the scanner would look at (used to filter folder-watch events).
pub fn is_audio_path(path: &Path) -> bool {
    let ext = extension(path);
    PLAYABLE_EXTENSIONS.contains(&ext.as_str()) || UNSUPPORTED_EXTENSIONS.contains(&ext.as_str())
}

fn collect_files(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_file() {
            if is_audio_path(&path) {
                out.push(path);
            }
        } else if kind.is_dir() && recursive {
            // Symbolic links are not followed, which also prevents loops.
            let _ = collect_files(&path, true, out);
        }
    }
    Ok(())
}

fn non_empty(value: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn leading_number(text: &str) -> Option<i64> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn codec_name(file_type: FileType) -> Option<&'static str> {
    Some(match file_type {
        FileType::Mpeg => "MP3",
        FileType::Flac => "FLAC",
        FileType::Wav => "WAV",
        FileType::Mp4 => "AAC/ALAC (MP4)",
        FileType::Aac => "AAC",
        FileType::Vorbis => "Ogg Vorbis",
        FileType::Aiff => "AIFF",
        _ => return None,
    })
}

fn parse(path: &Path) -> std::result::Result<Parsed, String> {
    let tagged = lofty::read_from_path(path)
        .map_err(|_| "The file could not be read as audio. It may be damaged.".to_string())?;
    let Some(codec) = codec_name(tagged.file_type()) else {
        return Err(format!(
            "{:?} audio is not supported for playback in this version.",
            tagged.file_type()
        ));
    };
    let tag: Option<&Tag> = tagged.primary_tag().or_else(|| tagged.first_tag());
    let properties = tagged.properties();

    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());
    let title = tag.and_then(|t| non_empty(t.title())).unwrap_or(stem);
    let artist = tag
        .and_then(|t| non_empty(t.artist()))
        .unwrap_or_else(|| "Unknown Artist".to_string());
    let album = tag
        .and_then(|t| non_empty(t.album()))
        .unwrap_or_else(|| "Unknown Album".to_string());
    let album_artist = tag
        .and_then(|t| t.get_string(ItemKey::AlbumArtist))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| artist.clone());
    let year = tag.and_then(|t| {
        t.get_string(ItemKey::RecordingDate)
            .or_else(|| t.get_string(ItemKey::Year))
            .and_then(leading_number)
            .filter(|y| (1000..=9999).contains(y))
    });
    let picture = tag.and_then(|t| {
        t.get_picture_type(PictureType::CoverFront)
            .or_else(|| t.pictures().first())
            .map(|p| p.data().to_vec())
    });

    Ok(Parsed {
        title,
        artist,
        album,
        album_artist,
        genre: tag.and_then(|t| non_empty(t.genre())),
        year,
        track_no: tag.and_then(|t| t.track()).map(i64::from),
        disc_no: tag.and_then(|t| t.disk()).map(i64::from),
        duration_ms: properties.duration().as_millis() as i64,
        codec: codec.to_string(),
        bitrate_kbps: properties.audio_bitrate().map(i64::from),
        sample_rate: properties.sample_rate().map(i64::from),
        bit_depth: properties.bit_depth().map(i64::from),
        channels: properties.channels().map(i64::from),
        picture,
    })
}

/// Finds `cover.jpg`, `folder.png`, and similar in a folder and caches it.
fn folder_artwork(dir: &Path, cache_dir: &Path) -> Option<String> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let stem = path.file_stem()?.to_string_lossy().to_lowercase();
        let ext = extension(&path);
        if FOLDER_ART_NAMES.contains(&stem.as_str())
            && matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
            && entry
                .metadata()
                .map(|m| m.len() <= artwork::MAX_SOURCE_BYTES)
                .unwrap_or(false)
        {
            if let Some(name) = fs::read(&path)
                .ok()
                .and_then(|bytes| artwork::store(cache_dir, &bytes))
            {
                return Some(name);
            }
        }
    }
    None
}

fn record_issue(conn: &Connection, path: &str, reason: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scan_issues (path, reason) VALUES (?1, ?2)",
        params![path, reason],
    )?;
    Ok(())
}

/// A track whose file vanished from its old path and has the same size and
/// modified time as a file that just appeared: the same file, moved.
fn find_moved(conn: &Connection, size: i64, mtime: i64) -> Result<Option<i64>> {
    let candidates: Vec<(i64, String)> = conn
        .prepare(
            "SELECT id, path FROM tracks
             WHERE file_size = ?1 AND file_mtime = ?2 AND status IN ('ok', 'missing', 'detached')",
        )?
        .query_map(params![size, mtime], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let gone: Vec<i64> = candidates
        .into_iter()
        .filter(|(_, path)| !Path::new(path).exists())
        .map(|(id, _)| id)
        .collect();
    // Only when exactly one candidate fits; otherwise it is not certain.
    Ok(if gone.len() == 1 {
        gone.first().copied()
    } else {
        None
    })
}

/// Scans `folders` into the library. `progress(done, total)` is called as
/// files are processed.
pub fn scan(
    conn: &Connection,
    folders: &[String],
    recursive: bool,
    artwork_dir: &Path,
    now: i64,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<ScanSummary> {
    fs::create_dir_all(artwork_dir)?;
    let mut summary = ScanSummary::default();
    conn.execute("DELETE FROM scan_issues", [])?;

    // Tracks under folders the user removed leave the library but keep their
    // rows, so playlists and history still know them if the folder returns.
    {
        let roots: Vec<String> = conn
            .prepare("SELECT DISTINCT folder FROM tracks WHERE status IN ('ok', 'missing')")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        for root in roots.iter().filter(|r| !folders.contains(r)) {
            conn.execute(
                "UPDATE tracks SET status = 'detached' WHERE folder = ?1 AND status IN ('ok', 'missing')",
                [root],
            )?;
        }
    }

    let mut work: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for folder in folders {
        let mut files = Vec::new();
        if collect_files(Path::new(folder), recursive, &mut files).is_err() {
            summary.unavailable_folders.push(folder.clone());
            continue;
        }
        files.sort();
        work.push((folder.clone(), files));
    }
    let total: usize = work.iter().map(|(_, files)| files.len()).sum();
    let mut done = 0usize;
    progress(0, total);

    // Each folder's cover image is looked up and hashed once per scan.
    let mut folder_art: HashMap<PathBuf, Option<String>> = HashMap::new();

    for (folder, files) in &work {
        let mut seen: HashSet<String> = HashSet::with_capacity(files.len());
        for batch in files.chunks(BATCH) {
            let tx = conn.unchecked_transaction()?;
            for path in batch {
                done += 1;
                let path_text = path.to_string_lossy().into_owned();
                if UNSUPPORTED_EXTENSIONS.contains(&extension(path).as_str()) {
                    record_issue(
                        &tx,
                        &path_text,
                        &format!(
                            ".{} files are not supported for playback in this version.",
                            extension(path)
                        ),
                    )?;
                    summary.issues += 1;
                    continue;
                }
                let Ok(meta) = fs::metadata(path) else {
                    record_issue(&tx, &path_text, "The file could not be opened.")?;
                    summary.issues += 1;
                    continue;
                };
                let size = meta.len() as i64;
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                let existing: Option<(i64, i64, String)> = tx
                    .query_row(
                        "SELECT file_size, file_mtime, status FROM tracks WHERE path = ?1",
                        [&path_text],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                if let Some((old_size, old_mtime, status)) = &existing {
                    if status == "removed" {
                        // The user took this track out of the library.
                        seen.insert(path_text);
                        continue;
                    }
                    if *old_size == size && *old_mtime == mtime {
                        if status != "ok" {
                            tx.execute(
                                "UPDATE tracks SET status = 'ok', folder = ?1 WHERE path = ?2",
                                params![folder, path_text],
                            )?;
                        }
                        seen.insert(path_text);
                        summary.unchanged += 1;
                        continue;
                    }
                } else if let Some(id) = find_moved(&tx, size, mtime)? {
                    tx.execute(
                        "UPDATE tracks SET path = ?1, folder = ?2, status = 'ok' WHERE id = ?3",
                        params![path_text, folder, id],
                    )?;
                    seen.insert(path_text);
                    summary.moved += 1;
                    continue;
                }

                let parsed = match parse(path) {
                    Ok(parsed) => parsed,
                    Err(reason) => {
                        record_issue(&tx, &path_text, &reason)?;
                        summary.issues += 1;
                        continue;
                    }
                };
                // Artwork priority: embedded, then a cover image in the folder.
                let cover = parsed
                    .picture
                    .as_deref()
                    .and_then(|bytes| artwork::store(artwork_dir, bytes))
                    .or_else(|| {
                        let dir = path.parent()?;
                        folder_art
                            .entry(dir.to_path_buf())
                            .or_insert_with(|| folder_artwork(dir, artwork_dir))
                            .clone()
                    });
                tx.execute(
                    "INSERT INTO tracks (path, folder, title, artist, album, album_artist, genre,
                         year, track_no, disc_no, duration_ms, codec, bitrate_kbps, sample_rate,
                         bit_depth, channels, artwork, file_size, file_mtime, status, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                             ?16, ?17, ?18, ?19, 'ok', ?20)
                     ON CONFLICT (path) DO UPDATE SET
                         folder = excluded.folder, title = excluded.title,
                         artist = excluded.artist, album = excluded.album,
                         album_artist = excluded.album_artist, genre = excluded.genre,
                         year = excluded.year, track_no = excluded.track_no,
                         disc_no = excluded.disc_no, duration_ms = excluded.duration_ms,
                         codec = excluded.codec, bitrate_kbps = excluded.bitrate_kbps,
                         sample_rate = excluded.sample_rate, bit_depth = excluded.bit_depth,
                         channels = excluded.channels, artwork = excluded.artwork,
                         file_size = excluded.file_size, file_mtime = excluded.file_mtime,
                         status = 'ok'",
                    params![
                        path_text,
                        folder,
                        parsed.title,
                        parsed.artist,
                        parsed.album,
                        parsed.album_artist,
                        parsed.genre,
                        parsed.year,
                        parsed.track_no,
                        parsed.disc_no,
                        parsed.duration_ms,
                        parsed.codec,
                        parsed.bitrate_kbps,
                        parsed.sample_rate,
                        parsed.bit_depth,
                        parsed.channels,
                        cover,
                        size,
                        mtime,
                        now
                    ],
                )?;
                let id: i64 = tx.query_row(
                    "SELECT id FROM tracks WHERE path = ?1",
                    [&path_text],
                    |row| row.get(0),
                )?;
                library::refresh_search_key(&tx, id)?;
                if existing.is_some() {
                    summary.updated += 1;
                } else {
                    summary.added += 1;
                }
                seen.insert(path_text);
            }
            tx.commit()?;
            progress(done, total);
        }

        // Files that were in this folder before and are gone now.
        let known: Vec<(i64, String)> = conn
            .prepare("SELECT id, path FROM tracks WHERE folder = ?1 AND status = 'ok'")?
            .query_map([folder], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (id, path) in known {
            if !seen.contains(&path) {
                conn.execute("UPDATE tracks SET status = 'missing' WHERE id = ?1", [id])?;
                summary.missing += 1;
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lofty::config::WriteOptions;
    use lofty::tag::{TagExt, TagType};

    /// Writes a short silent 16-bit mono WAV.
    pub fn write_wav(path: &Path, seconds: u32) {
        let rate = 8000u32;
        let data_len = rate * seconds * 2;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.resize(44 + data_len as usize, 0);
        fs::write(path, bytes).unwrap();
    }

    pub fn tag_wav(path: &Path, title: &str, artist: &str, album: &str, track: u32) {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title(title.to_string());
        tag.set_artist(artist.to_string());
        tag.set_album(album.to_string());
        tag.set_track(track);
        tag.save_to_path(path, WriteOptions::default()).unwrap();
    }

    fn run(conn: &Connection, folders: &[&Path], recursive: bool, art: &Path) -> ScanSummary {
        let folders: Vec<String> = folders
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        scan(conn, &folders, recursive, art, 1000, &mut |_, _| {}).unwrap()
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let art = tmp.path().join("art");
        fs::create_dir_all(&music).unwrap();
        (tmp, music, art, library::tests::conn())
    }

    #[test]
    fn scans_tags_properties_and_falls_back_to_file_name() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("tagged.wav"), 2);
        tag_wav(
            &music.join("tagged.wav"),
            "Night Bus",
            "Mira Solen",
            "Low Tide",
            3,
        );
        write_wav(&music.join("My Untagged Song.wav"), 1);
        fs::write(music.join("notes.txt"), "not audio").unwrap();

        let summary = run(&conn, &[&music], false, &art);
        assert_eq!(summary.added, 2);
        assert_eq!(summary.issues, 0);

        let tracks = library::all_tracks(&conn).unwrap();
        let tagged = tracks.iter().find(|t| t.title == "Night Bus").unwrap();
        assert_eq!(tagged.artist, "Mira Solen");
        assert_eq!(tagged.album, "Low Tide");
        assert_eq!(tagged.track_no, Some(3));
        assert_eq!(tagged.codec.as_deref(), Some("WAV"));
        assert_eq!(tagged.sample_rate, Some(8000));
        assert!((1900..=2100).contains(&tagged.duration_ms));
        assert_eq!(
            library::search(&conn, "night bus").unwrap().songs.len(),
            1,
            "search key is set"
        );

        let untagged = tracks
            .iter()
            .find(|t| t.title == "My Untagged Song")
            .unwrap();
        assert_eq!(untagged.artist, "Unknown Artist");
        assert_eq!(untagged.album, "Unknown Album");
    }

    #[test]
    fn rescan_is_incremental_and_detects_changes() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("a.wav"), 1);
        write_wav(&music.join("b.wav"), 2);
        assert_eq!(run(&conn, &[&music], false, &art).added, 2);

        let again = run(&conn, &[&music], false, &art);
        assert_eq!((again.added, again.updated, again.unchanged), (0, 0, 2));

        write_wav(&music.join("a.wav"), 3); // size changes
        write_wav(&music.join("c.wav"), 4);
        let third = run(&conn, &[&music], false, &art);
        assert_eq!((third.added, third.updated, third.unchanged), (1, 1, 1));
    }

    #[test]
    fn deleted_files_are_marked_missing_and_come_back() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("a.wav"), 1);
        write_wav(&music.join("b.wav"), 2);
        run(&conn, &[&music], false, &art);
        let id: i64 = conn
            .query_row("SELECT id FROM tracks WHERE path LIKE '%b.wav'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let playlist = library::create_playlist(&conn, "Keep", 1).unwrap();
        library::add_to_playlist(&conn, &playlist, &[id], 1).unwrap();

        fs::remove_file(music.join("b.wav")).unwrap();
        let summary = run(&conn, &[&music], false, &art);
        assert_eq!(summary.missing, 1);
        assert_eq!(library::all_tracks(&conn).unwrap().len(), 1);
        let detail = library::collection(&conn, &playlist).unwrap();
        assert_eq!(detail.tracks.len(), 1, "playlist keeps the reference");
        assert!(!detail.tracks[0].available);

        write_wav(&music.join("b.wav"), 2);
        run(&conn, &[&music], false, &art);
        let detail = library::collection(&conn, &playlist).unwrap();
        assert_eq!(detail.tracks[0].id, id, "same track row is reused");
        assert!(detail.tracks[0].available);
    }

    #[test]
    fn a_moved_or_renamed_file_keeps_its_identity() {
        let (_tmp, music, art, conn) = setup();
        fs::create_dir_all(music.join("Sub")).unwrap();
        write_wav(&music.join("song.wav"), 2);
        tag_wav(&music.join("song.wav"), "Song", "Artist", "Album", 1);
        run(&conn, &[&music], true, &art);
        let id = library::all_tracks(&conn).unwrap()[0].id;
        library::set_favorite(&conn, id, true, 1).unwrap();
        library::record_play(&conn, id, 2).unwrap();

        fs::rename(
            music.join("song.wav"),
            music.join("Sub").join("renamed.wav"),
        )
        .unwrap();
        let summary = run(&conn, &[&music], true, &art);
        assert_eq!((summary.moved, summary.added, summary.missing), (1, 0, 0));
        let tracks = library::all_tracks(&conn).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, id);
        assert!(tracks[0].path.ends_with("renamed.wav"));
        assert!(tracks[0].favorite);
        assert_eq!(tracks[0].play_count, 1);
    }

    #[test]
    fn a_copy_is_a_new_track_not_a_move() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("song.wav"), 2);
        run(&conn, &[&music], false, &art);
        fs::copy(music.join("song.wav"), music.join("copy.wav")).unwrap();
        let summary = run(&conn, &[&music], false, &art);
        assert_eq!(
            (summary.moved, summary.added),
            (0, 1),
            "the original still exists"
        );
    }

    #[test]
    fn removed_tracks_are_not_added_back() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("a.wav"), 1);
        write_wav(&music.join("b.wav"), 2);
        run(&conn, &[&music], false, &art);
        let id = library::all_tracks(&conn).unwrap()[0].id;
        library::remove_from_library(&conn, &[id], 5).unwrap();

        let summary = run(&conn, &[&music], false, &art);
        assert_eq!((summary.added, summary.missing), (0, 0));
        assert_eq!(library::all_tracks(&conn).unwrap().len(), 1);
        assert!(music.join("a.wav").exists(), "the audio file is untouched");

        // Restoring from the bin brings it back on the next scan.
        library::trash_restore(&conn, library::trash(&conn, 6).unwrap()[0].id).unwrap();
        run(&conn, &[&music], false, &art);
        assert_eq!(library::all_tracks(&conn).unwrap().len(), 2);
    }

    #[test]
    fn metadata_edits_survive_a_rescan_of_a_changed_file() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("a.wav"), 1);
        tag_wav(&music.join("a.wav"), "From File", "Artist", "Album", 1);
        run(&conn, &[&music], false, &art);
        let id = library::all_tracks(&conn).unwrap()[0].id;
        let edit = library::TrackEdit {
            title: Some("My Title".into()),
            ..Default::default()
        };
        library::edit_track(&conn, id, edit, 5).unwrap();

        write_wav(&music.join("a.wav"), 3);
        tag_wav(
            &music.join("a.wav"),
            "Changed In File",
            "Artist",
            "Album",
            1,
        );
        assert_eq!(run(&conn, &[&music], false, &art).updated, 1);
        let track = library::track(&conn, id).unwrap().unwrap();
        assert_eq!(track.title, "My Title", "the user's edit still applies");
        assert_eq!(library::search(&conn, "my title").unwrap().songs.len(), 1);
    }

    #[test]
    fn unreachable_folder_leaves_its_tracks_alone() {
        let (tmp, music, art, conn) = setup();
        write_wav(&music.join("a.wav"), 1);
        run(&conn, &[&music], false, &art);

        let moved = tmp.path().join("offline");
        fs::rename(&music, &moved).unwrap(); // simulates an unplugged drive
        let summary = run(&conn, &[&music], false, &art);
        assert_eq!(summary.unavailable_folders.len(), 1);
        assert_eq!(summary.missing, 0);
        assert_eq!(
            library::all_tracks(&conn).unwrap().len(),
            1,
            "nothing was marked missing"
        );
    }

    #[test]
    fn removing_a_folder_detaches_tracks_and_readding_restores_them() {
        let tmp = tempfile::tempdir().unwrap();
        let (one, two) = (tmp.path().join("one"), tmp.path().join("two"));
        let art = tmp.path().join("art");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        write_wav(&one.join("a.wav"), 1);
        write_wav(&two.join("b.wav"), 2);
        let conn = library::tests::conn();
        run(&conn, &[&one, &two], false, &art);
        assert_eq!(library::all_tracks(&conn).unwrap().len(), 2);

        run(&conn, &[&one], false, &art);
        assert_eq!(library::all_tracks(&conn).unwrap().len(), 1);
        run(&conn, &[&one, &two], false, &art);
        assert_eq!(library::all_tracks(&conn).unwrap().len(), 2);
    }

    #[test]
    fn subfolders_only_when_requested() {
        let (_tmp, music, art, conn) = setup();
        fs::create_dir_all(music.join("Album")).unwrap();
        write_wav(&music.join("top.wav"), 1);
        write_wav(&music.join("Album").join("nested.wav"), 2);
        assert_eq!(run(&conn, &[&music], false, &art).added, 1);
        assert_eq!(run(&conn, &[&music], true, &art).added, 1);
        assert_eq!(library::all_tracks(&conn).unwrap().len(), 2);
    }

    #[test]
    fn damaged_and_unsupported_files_are_reported_and_scan_continues() {
        let (_tmp, music, art, conn) = setup();
        fs::write(music.join("broken.flac"), b"this is not flac").unwrap();
        fs::write(music.join("voice.opus"), b"anything").unwrap();
        write_wav(&music.join("good.wav"), 1);

        let summary = run(&conn, &[&music], false, &art);
        assert_eq!(summary.added, 1);
        assert_eq!(summary.issues, 2);
        let issues = library::scan_issues(&conn).unwrap();
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .any(|i| i.path.ends_with("voice.opus") && i.reason.contains(".opus")));
        assert_eq!(
            fs::read(music.join("broken.flac")).unwrap(),
            b"this is not flac",
            "the file was not modified"
        );
    }

    #[test]
    fn folder_cover_is_cached_once_with_a_thumbnail() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("a.wav"), 1);
        write_wav(&music.join("b.wav"), 2);
        fs::write(music.join("Cover.png"), artwork::tests::png(64, 64, 9)).unwrap();

        run(&conn, &[&music], false, &art);
        let tracks = library::all_tracks(&conn).unwrap();
        let name = tracks[0].artwork.clone().expect("artwork found");
        assert_eq!(tracks[1].artwork.as_ref(), Some(&name));
        assert!(name.ends_with(".png"));
        assert_eq!(
            fs::read_dir(&art).unwrap().count(),
            2,
            "one image and its thumbnail"
        );
        assert!(art.join(artwork::thumb_name(&name)).exists());
    }

    #[test]
    fn scanning_never_modifies_audio_files() {
        let (_tmp, music, art, conn) = setup();
        write_wav(&music.join("a.wav"), 1);
        tag_wav(&music.join("a.wav"), "T", "A", "B", 1);
        let before = fs::read(music.join("a.wav")).unwrap();
        run(&conn, &[&music], true, &art);
        run(&conn, &[&music], true, &art);
        assert_eq!(fs::read(music.join("a.wav")).unwrap(), before);
    }
}
