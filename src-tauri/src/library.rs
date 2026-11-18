//! Library queries: tracks, albums, artists, playlists, favorites, history,
//! play counts, metadata edits, duplicates, the recycle bin, and local search.
//! Only BitNot's own database is touched; music files are never modified here.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::textfold;

const MAX_PLAYLIST_NAME: usize = 100;
const MAX_DESCRIPTION: usize = 300;
const MAX_FIELD: usize = 200;
const HISTORY_LIMIT: i64 = 200;
const RECENT_LIMIT: usize = 200;
const SEARCH_LIMIT: usize = 100;
const POPULAR_LIMIT: usize = 5;
const ID_SEPARATOR: char = '\u{1f}';
/// Two files are treated as the same recording only when their folded title,
/// artist, and album are identical and their lengths differ by at most this.
const DUPLICATE_TOLERANCE_MS: i64 = 2000;
pub const TRASH_RETENTION_DAYS: i64 = 90;
const SECONDS_PER_DAY: i64 = 86_400;

/// The fixed genre list (CLAUDE.md §13). Users pick from it; they cannot add to it.
pub const GENRES: &[&str] = &[
    "Acoustic",
    "Afrobeat",
    "Alternative",
    "Alternative Rock",
    "Ambient",
    "Americana",
    "Anime",
    "Bluegrass",
    "Blues",
    "Bossa Nova",
    "Chillout",
    "Classical",
    "Country",
    "Dance",
    "Dancehall",
    "Deep House",
    "Disco",
    "Downtempo",
    "Drum and Bass",
    "Dubstep",
    "EDM",
    "Electronic",
    "Electropop",
    "Emo",
    "Experimental",
    "Folk",
    "Funk",
    "Garage",
    "Gospel",
    "Grunge",
    "Hard Rock",
    "Hardcore",
    "Heavy Metal",
    "Hip-Hop",
    "House",
    "Indie",
    "Indie Folk",
    "Indie Pop",
    "Indie Rock",
    "Industrial",
    "Instrumental",
    "J-Pop",
    "J-Rock",
    "Jazz",
    "K-Pop",
    "Latin",
    "Lo-Fi",
    "Metal",
    "Neo Soul",
    "New Age",
    "New Wave",
    "Opera",
    "Orchestral",
    "Pop",
    "Pop Rock",
    "Post-Punk",
    "Post-Rock",
    "Progressive Rock",
    "Psychedelic",
    "Punk",
    "R&B",
    "Rap",
    "Reggae",
    "Reggaeton",
    "Rock",
    "Shoegaze",
    "Singer-Songwriter",
    "Ska",
    "Soul",
    "Soundtrack",
    "Spoken Word",
    "Synthpop",
    "Synthwave",
    "Techno",
    "Trance",
    "Trap",
    "Trip-Hop",
    "Vocal",
    "World",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
    pub duration_ms: i64,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub channels: Option<i64>,
    /// File name inside the artwork cache.
    pub artwork: Option<String>,
    pub added_at: i64,
    pub favorite: bool,
    /// False when the file is missing, removed, or its folder was removed.
    pub available: bool,
    pub play_count: i64,
    pub path: String,
    /// True when the user edited this track's metadata in BitNot.
    pub edited: bool,
    /// How many files in the library are this same recording (1 = no duplicates).
    pub sources: i64,
    #[serde(skip)]
    pub search_key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// `playlist:<id>`, `album:<album artist>\u{1f}<album>`, or `artist:<name>`.
    pub id: String,
    pub kind: &'static str,
    pub title: String,
    pub subtitle: String,
    pub artwork: Option<String>,
    pub track_count: i64,
    pub year: Option<i64>,
    /// Playlists only.
    pub description: String,
    /// Playlists only: true when the user chose the cover.
    pub custom_cover: bool,
    #[serde(skip)]
    owner: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDetail {
    #[serde(flatten)]
    pub summary: Collection,
    pub owner: String,
    pub tracks: Vec<Track>,
    /// Artist pages: most played tracks by local play count; empty when the
    /// artist has no listening history.
    pub popular: Vec<Track>,
    /// Artist pages: albums, newest first.
    pub albums: Vec<Collection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub track_count: i64,
    pub playlists: Vec<Collection>,
    pub albums: Vec<Collection>,
    pub artists: Vec<Collection>,
    pub favorite_ids: Vec<i64>,
    pub issue_count: i64,
    pub trash_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub played_at: i64,
    pub track: Track,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub query: String,
    pub songs: Vec<Track>,
    pub albums: Vec<Collection>,
    pub artists: Vec<Collection>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub path: String,
    pub reason: String,
}

/// A metadata edit. `None` leaves a field as the file reports it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackEdit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub id: i64,
    /// `playlist`, `favorite`, `track`, or `metadata`.
    pub kind: String,
    pub label: String,
    pub deleted_at: i64,
    pub days_remaining: i64,
}

// ── Tracks ───────────────────────────────────────────────────────────────

const TRACK_SELECT: &str = "
    SELECT t.id, t.title, t.artist, t.album, t.album_artist, t.genre, t.year, t.track_no,
           t.duration_ms, t.codec, t.bitrate_kbps, t.sample_rate, t.bit_depth, t.channels,
           t.artwork, t.added_at, f.track_id IS NOT NULL, t.status = 'ok',
           COALESCE(p.count, 0), t.path, t.edited, t.search_key
    FROM library_tracks t
    LEFT JOIN favorites f ON f.track_id = t.id
    LEFT JOIN play_counts p ON p.track_id = t.id ";

fn track_from_row(row: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        title: row.get(1)?,
        artist: row.get(2)?,
        album: row.get(3)?,
        album_artist: row.get(4)?,
        genre: row.get::<_, Option<String>>(5)?.filter(|g| !g.is_empty()),
        year: row.get(6)?,
        track_no: row.get(7)?,
        duration_ms: row.get(8)?,
        codec: row.get(9)?,
        bitrate_kbps: row.get(10)?,
        sample_rate: row.get(11)?,
        bit_depth: row.get(12)?,
        channels: row.get(13)?,
        artwork: row.get(14)?,
        added_at: row.get(15)?,
        favorite: row.get(16)?,
        available: row.get(17)?,
        play_count: row.get(18)?,
        path: row.get(19)?,
        edited: row.get(20)?,
        sources: 1,
        search_key: row.get(21)?,
    })
}

fn query_tracks<P: rusqlite::Params>(
    conn: &Connection,
    tail: &str,
    params: P,
) -> Result<Vec<Track>> {
    let mut stmt = conn.prepare(&format!("{TRACK_SELECT}{tail}"))?;
    let rows = stmt.query_map(params, track_from_row)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

const ALBUM_ORDER: &str = "COALESCE(t.disc_no, 1), COALESCE(t.track_no, 0), t.title COLLATE NOCASE";

/// Every available track, ordered for the Songs view, duplicates collapsed.
pub fn all_tracks(conn: &Connection) -> Result<Vec<Track>> {
    let tracks = query_tracks(
        conn,
        &format!(
            "WHERE t.status = 'ok'
             ORDER BY t.artist COLLATE NOCASE, t.album COLLATE NOCASE, {ALBUM_ORDER}"
        ),
        [],
    )?;
    collapse_duplicates(conn, tracks)
}

/// The most recently added tracks, newest first.
pub fn recent_tracks(conn: &Connection) -> Result<Vec<Track>> {
    let tracks = query_tracks(
        conn,
        "WHERE t.status = 'ok' ORDER BY t.added_at DESC, t.id DESC",
        [],
    )?;
    let mut tracks = collapse_duplicates(conn, tracks)?;
    tracks.truncate(RECENT_LIMIT);
    Ok(tracks)
}

pub fn track(conn: &Connection, id: i64) -> Result<Option<Track>> {
    Ok(query_tracks(conn, "WHERE t.id = ?1", [id])?
        .into_iter()
        .next())
}

/// Tracks for the given ids, in the same order. Unknown ids are skipped.
pub fn tracks_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<Track>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut unique: Vec<i64> = ids.to_vec();
    unique.sort_unstable();
    unique.dedup();
    let mut found: HashMap<i64, Track> = HashMap::new();
    for chunk in unique.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let tail = format!("WHERE t.id IN ({marks})");
        for track in query_tracks(conn, &tail, params_from_iter(chunk.iter()))? {
            found.insert(track.id, track);
        }
    }
    Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
}

// ── Duplicates ───────────────────────────────────────────────────────────

fn is_lossless(track: &Track) -> bool {
    matches!(track.codec.as_deref(), Some("FLAC" | "WAV" | "AIFF"))
}

/// Higher is better: lossless first, then resolution, then bitrate.
fn quality(track: &Track) -> (bool, i64, i64, i64) {
    (
        is_lossless(track),
        track.bit_depth.unwrap_or(0),
        track.sample_rate.unwrap_or(0),
        track.bitrate_kbps.unwrap_or(0),
    )
}

fn same_recording(a: &Track, b: &Track) -> bool {
    a.search_key == b.search_key && (a.duration_ms - b.duration_ms).abs() <= DUPLICATE_TOLERANCE_MS
}

fn preferred_ids(conn: &Connection) -> Result<HashSet<i64>> {
    let mut stmt = conn.prepare("SELECT track_id FROM preferred_sources")?;
    let ids = stmt
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(ids)
}

/// Shows confidently identical recordings as one track. The version shown is
/// the one the user preferred, otherwise the best quality. Uncertain matches
/// are left as separate tracks.
fn collapse_duplicates(conn: &Connection, tracks: Vec<Track>) -> Result<Vec<Track>> {
    let mut by_key: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, track) in tracks.iter().enumerate() {
        by_key
            .entry(track.search_key.as_str())
            .or_default()
            .push(index);
    }
    if by_key.values().all(|group| group.len() == 1) {
        return Ok(tracks);
    }
    let preferred = preferred_ids(conn)?;
    // For each track: the index of its group's first member, and the group.
    let mut drop: HashSet<usize> = HashSet::new();
    let mut replace: HashMap<usize, (usize, i64)> = HashMap::new();
    for indexes in by_key.values().filter(|group| group.len() > 1) {
        let mut remaining: Vec<usize> = indexes.clone();
        while let Some(first) = remaining.first().copied() {
            let (group, rest): (Vec<usize>, Vec<usize>) = remaining
                .iter()
                .copied()
                .partition(|i| same_recording(&tracks[first], &tracks[*i]));
            remaining = rest;
            if group.len() < 2 {
                continue;
            }
            let best = group
                .iter()
                .copied()
                .max_by(|a, b| {
                    let (ta, tb) = (&tracks[*a], &tracks[*b]);
                    (
                        preferred.contains(&ta.id),
                        quality(ta),
                        std::cmp::Reverse(ta.id),
                    )
                        .cmp(&(
                            preferred.contains(&tb.id),
                            quality(tb),
                            std::cmp::Reverse(tb.id),
                        ))
                })
                .unwrap_or(first);
            replace.insert(first, (best, group.len() as i64));
            drop.extend(group.iter().skip(1));
        }
    }
    let mut out = Vec::with_capacity(tracks.len() - drop.len());
    for (index, track) in tracks.iter().enumerate() {
        if drop.contains(&index) {
            continue;
        }
        match replace.get(&index) {
            Some((best, count)) => {
                let mut shown = tracks[*best].clone();
                shown.sources = *count;
                out.push(shown);
            }
            None => out.push(track.clone()),
        }
    }
    Ok(out)
}

/// Every available file that is the same recording as `id`, best quality first.
pub fn track_sources(conn: &Connection, id: i64) -> Result<Vec<Track>> {
    let Some(target) = track(conn, id)? else {
        return Ok(Vec::new());
    };
    let mut sources: Vec<Track> = query_tracks(
        conn,
        "WHERE t.status = 'ok' AND t.search_key = ?1",
        [&target.search_key],
    )?
    .into_iter()
    .filter(|other| other.id == id || same_recording(&target, other))
    .collect();
    sources.sort_by(|a, b| quality(b).cmp(&quality(a)).then(a.id.cmp(&b.id)));
    Ok(sources)
}

/// Makes `id` the version shown and played for its group of duplicates.
pub fn prefer_source(conn: &Connection, id: i64) -> Result<()> {
    let sources = track_sources(conn, id)?;
    if !sources.iter().any(|t| t.id == id) {
        return Err(AppError::NotFound(
            "That track is no longer in your library.".into(),
        ));
    }
    let tx = conn.unchecked_transaction()?;
    for source in &sources {
        tx.execute(
            "DELETE FROM preferred_sources WHERE track_id = ?1",
            [source.id],
        )?;
    }
    tx.execute("INSERT INTO preferred_sources (track_id) VALUES (?1)", [id])?;
    tx.commit()?;
    Ok(())
}

// ── Albums, artists, playlists ───────────────────────────────────────────

fn album_id(album_artist: &str, album: &str) -> String {
    format!("album:{album_artist}{ID_SEPARATOR}{album}")
}

fn album_from_row(row: &Row) -> rusqlite::Result<Collection> {
    let artist: String = row.get(0)?;
    let album: String = row.get(1)?;
    Ok(Collection {
        id: album_id(&artist, &album),
        kind: "album",
        title: album,
        subtitle: format!("Album • {artist}"),
        artwork: row.get(3)?,
        track_count: row.get(2)?,
        year: row.get(4)?,
        description: String::new(),
        custom_cover: false,
        owner: artist,
    })
}

pub fn albums(conn: &Connection) -> Result<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT album_artist, album, COUNT(*), MAX(artwork), MAX(year) FROM library_tracks
         WHERE status = 'ok' GROUP BY album_artist, album
         ORDER BY album COLLATE NOCASE, album_artist COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], album_from_row)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Albums containing tracks by `artist`, newest first; undated albums last.
fn albums_by_artist(conn: &Connection, artist: &str) -> Result<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT album_artist, album, COUNT(*), MAX(artwork), MAX(year) FROM library_tracks
         WHERE status = 'ok' AND artist = ?1 GROUP BY album_artist, album
         ORDER BY MAX(year) IS NULL, MAX(year) DESC, album COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([artist], album_from_row)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn artists(conn: &Connection) -> Result<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT artist, COUNT(*), MAX(artwork) FROM library_tracks
         WHERE status = 'ok' GROUP BY artist ORDER BY artist COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        Ok(Collection {
            id: format!("artist:{name}"),
            kind: "artist",
            title: name.clone(),
            subtitle: "Artist".to_string(),
            artwork: row.get(2)?,
            track_count: row.get(1)?,
            year: None,
            description: String::new(),
            custom_cover: false,
            owner: name,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

const PLAYLIST_SELECT: &str = "
    SELECT p.id, p.name, p.description, p.cover,
           (SELECT COUNT(*) FROM playlist_tracks x WHERE x.playlist_id = p.id),
           (SELECT t.artwork FROM playlist_tracks x JOIN library_tracks t ON t.id = x.track_id
            WHERE x.playlist_id = p.id AND t.artwork IS NOT NULL
            ORDER BY x.position LIMIT 1)
    FROM playlists p ";

fn playlist_from_row(row: &Row) -> rusqlite::Result<Collection> {
    let id: i64 = row.get(0)?;
    let cover: Option<String> = row.get(3)?;
    let first_art: Option<String> = row.get(5)?;
    Ok(Collection {
        id: format!("playlist:{id}"),
        kind: "playlist",
        title: row.get(1)?,
        subtitle: "Playlist".to_string(),
        custom_cover: cover.is_some(),
        artwork: cover.or(first_art),
        track_count: row.get(4)?,
        year: None,
        description: row.get(2)?,
        owner: "Playlist".to_string(),
    })
}

pub fn playlists(conn: &Connection) -> Result<Vec<Collection>> {
    let mut stmt = conn.prepare(&format!(
        "{PLAYLIST_SELECT} WHERE p.deleted_at IS NULL ORDER BY p.created_at DESC, p.id DESC"
    ))?;
    let rows = stmt.query_map([], playlist_from_row)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn overview(conn: &Connection) -> Result<Overview> {
    let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |row| row.get(0))?) };
    let favorite_ids = conn
        .prepare(
            "SELECT f.track_id FROM favorites f JOIN tracks t ON t.id = f.track_id
             WHERE t.status = 'ok' ORDER BY f.added_at DESC",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(Overview {
        track_count: count("SELECT COUNT(*) FROM tracks WHERE status = 'ok'")?,
        playlists: playlists(conn)?,
        albums: albums(conn)?,
        artists: artists(conn)?,
        favorite_ids,
        issue_count: count("SELECT COUNT(*) FROM scan_issues")?,
        trash_count: count("SELECT COUNT(*) FROM trash")?,
    })
}

fn parse_playlist_id(id: &str) -> Result<i64> {
    id.strip_prefix("playlist:")
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| AppError::NotFound("That playlist no longer exists.".into()))
}

fn playlist_track_ids(conn: &Connection, playlist_id: i64) -> Result<Vec<i64>> {
    let ids = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")?
        .query_map([playlist_id], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(ids)
}

pub fn collection(conn: &Connection, id: &str) -> Result<CollectionDetail> {
    let missing = || AppError::NotFound("That item is no longer in your library.".into());
    if let Some(rest) = id.strip_prefix("album:") {
        let (artist, album) = rest.split_once(ID_SEPARATOR).ok_or_else(missing)?;
        let tracks = query_tracks(
            conn,
            &format!(
                "WHERE t.status = 'ok' AND t.album_artist = ?1 AND t.album = ?2 ORDER BY {ALBUM_ORDER}"
            ),
            params![artist, album],
        )?;
        let tracks = collapse_duplicates(conn, tracks)?;
        let first = tracks.first().ok_or_else(missing)?;
        let year = tracks.iter().filter_map(|t| t.year).max();
        let owner = match year {
            Some(year) => format!("{artist} • {year}"),
            None => artist.to_string(),
        };
        return Ok(CollectionDetail {
            summary: Collection {
                id: id.to_string(),
                kind: "album",
                title: album.to_string(),
                subtitle: format!("Album • {artist}"),
                artwork: tracks
                    .iter()
                    .find_map(|t| t.artwork.clone())
                    .or(first.artwork.clone()),
                track_count: tracks.len() as i64,
                year,
                description: String::new(),
                custom_cover: false,
                owner: artist.to_string(),
            },
            owner,
            tracks,
            popular: Vec::new(),
            albums: Vec::new(),
        });
    }
    if let Some(name) = id.strip_prefix("artist:") {
        let tracks = query_tracks(
            conn,
            &format!(
                "WHERE t.status = 'ok' AND t.artist = ?1
                 ORDER BY t.album COLLATE NOCASE, {ALBUM_ORDER}"
            ),
            [name],
        )?;
        let tracks = collapse_duplicates(conn, tracks)?;
        if tracks.is_empty() {
            return Err(missing());
        }
        // Popular Tracks use only local play counts; none played means no section.
        let mut popular: Vec<Track> = tracks
            .iter()
            .filter(|t| t.play_count > 0)
            .cloned()
            .collect();
        popular.sort_by(|a, b| {
            b.play_count
                .cmp(&a.play_count)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        popular.truncate(POPULAR_LIMIT);
        return Ok(CollectionDetail {
            summary: Collection {
                id: id.to_string(),
                kind: "artist",
                title: name.to_string(),
                subtitle: "Artist".to_string(),
                artwork: tracks.iter().find_map(|t| t.artwork.clone()),
                track_count: tracks.len() as i64,
                year: None,
                description: String::new(),
                custom_cover: false,
                owner: name.to_string(),
            },
            owner: "Artist".to_string(),
            tracks,
            popular,
            albums: albums_by_artist(conn, name)?,
        });
    }
    let playlist_id = parse_playlist_id(id)?;
    let mut summary = conn
        .query_row(
            &format!("{PLAYLIST_SELECT} WHERE p.id = ?1 AND p.deleted_at IS NULL"),
            [playlist_id],
            playlist_from_row,
        )
        .optional()?
        .ok_or_else(missing)?;
    // Unavailable tracks stay in the playlist so their place is not lost, and
    // duplicates stay separate: a playlist refers to exact entries.
    let tracks = tracks_by_ids(conn, &playlist_track_ids(conn, playlist_id)?)?;
    summary.track_count = tracks.len() as i64;
    Ok(CollectionDetail {
        summary,
        owner: "Playlist".to_string(),
        tracks,
        popular: Vec::new(),
        albums: Vec::new(),
    })
}

// ── Recycle bin ──────────────────────────────────────────────────────────

fn trash_add(
    conn: &Connection,
    kind: &str,
    label: &str,
    payload: serde_json::Value,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO trash (kind, label, payload, deleted_at) VALUES (?1, ?2, ?3, ?4)",
        params![kind, label, payload.to_string(), now],
    )?;
    Ok(())
}

pub fn trash(conn: &Connection, now: i64) -> Result<Vec<TrashItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, label, deleted_at FROM trash ORDER BY deleted_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let deleted_at: i64 = row.get(3)?;
        let elapsed = (now - deleted_at).max(0) / SECONDS_PER_DAY;
        Ok(TrashItem {
            id: row.get(0)?,
            kind: row.get(1)?,
            label: row.get(2)?,
            deleted_at,
            days_remaining: (TRASH_RETENTION_DAYS - elapsed).max(0),
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn trash_entry(conn: &Connection, id: i64) -> Result<(String, serde_json::Value)> {
    let (kind, payload): (String, String) = conn
        .query_row(
            "SELECT kind, payload FROM trash WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("That item is no longer in the Recycle Bin.".into()))?;
    Ok((kind, serde_json::from_str(&payload)?))
}

/// Puts an item back where it was.
pub fn trash_restore(conn: &Connection, id: i64) -> Result<()> {
    let (kind, payload) = trash_entry(conn, id)?;
    let number = |key: &str| payload.get(key).and_then(|v| v.as_i64()).unwrap_or(-1);
    let tx = conn.unchecked_transaction()?;
    match kind.as_str() {
        "playlist" => {
            tx.execute(
                "UPDATE playlists SET deleted_at = NULL WHERE id = ?1",
                [number("playlistId")],
            )?;
        }
        "favorite" => {
            tx.execute(
                "INSERT OR IGNORE INTO favorites (track_id, added_at)
                 SELECT id, ?2 FROM tracks WHERE id = ?1",
                params![number("trackId"), number("addedAt")],
            )?;
        }
        "track" => {
            tx.execute(
                "UPDATE tracks SET status = 'ok' WHERE id = ?1 AND status = 'removed'",
                [number("trackId")],
            )?;
        }
        "metadata" => {
            let track_id = number("trackId");
            let previous: Option<TrackEdit> = payload
                .get("previous")
                .filter(|v| !v.is_null())
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            write_edit(&tx, track_id, previous.as_ref())?;
            refresh_search_key(&tx, track_id)?;
        }
        _ => {}
    }
    tx.execute("DELETE FROM trash WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

fn purge_entry(conn: &Connection, id: i64) -> Result<()> {
    let (kind, payload) = trash_entry(conn, id)?;
    let number = |key: &str| payload.get(key).and_then(|v| v.as_i64()).unwrap_or(-1);
    match kind.as_str() {
        "playlist" => {
            conn.execute(
                "DELETE FROM playlists WHERE id = ?1 AND deleted_at IS NOT NULL",
                [number("playlistId")],
            )?;
        }
        "track" => {
            // The row stays as a marker so a later scan does not add the file
            // again; everything that referred to it is dropped.
            let track_id = number("trackId");
            for table in [
                "playlist_tracks",
                "favorites",
                "history",
                "play_counts",
                "track_edits",
            ] {
                conn.execute(
                    &format!(
                        "DELETE FROM {table} WHERE track_id IN
                         (SELECT id FROM tracks WHERE id = ?1 AND status = 'removed')"
                    ),
                    [track_id],
                )?;
            }
        }
        _ => {}
    }
    conn.execute("DELETE FROM trash WHERE id = ?1", [id])?;
    Ok(())
}

/// Permanently deletes one item now. Music files are never involved.
pub fn trash_purge(conn: &Connection, id: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    purge_entry(&tx, id)?;
    tx.commit()?;
    Ok(())
}

/// Permanently deletes everything in the bin, or only what is past 90 days.
pub fn trash_purge_all(conn: &Connection, older_than: Option<i64>) -> Result<usize> {
    let ids: Vec<i64> = match older_than {
        Some(cutoff) => conn
            .prepare("SELECT id FROM trash WHERE deleted_at <= ?1")?
            .query_map([cutoff], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?,
        None => conn
            .prepare("SELECT id FROM trash")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?,
    };
    let tx = conn.unchecked_transaction()?;
    for id in &ids {
        purge_entry(&tx, *id)?;
    }
    tx.commit()?;
    Ok(ids.len())
}

pub fn trash_purge_expired(conn: &Connection, now: i64) -> Result<usize> {
    trash_purge_all(conn, Some(now - TRASH_RETENTION_DAYS * SECONDS_PER_DAY))
}

// ── Favorites ────────────────────────────────────────────────────────────

pub fn set_favorite(conn: &Connection, track_id: i64, on: bool, now: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    if on {
        tx.execute(
            "INSERT OR IGNORE INTO favorites (track_id, added_at) VALUES (?1, ?2)",
            params![track_id, now],
        )
        .map_err(|_| AppError::NotFound("That track is no longer in your library.".into()))?;
        // Favoriting again makes the earlier removal moot.
        tx.execute(
            "DELETE FROM trash WHERE kind = 'favorite'
             AND CAST(json_extract(payload, '$.trackId') AS INTEGER) = ?1",
            [track_id],
        )?;
    } else {
        let added_at: Option<i64> = tx
            .query_row(
                "SELECT added_at FROM favorites WHERE track_id = ?1",
                [track_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(added_at) = added_at {
            tx.execute("DELETE FROM favorites WHERE track_id = ?1", [track_id])?;
            let title = track(&tx, track_id)?.map(|t| t.title).unwrap_or_default();
            trash_add(
                &tx,
                "favorite",
                &title,
                serde_json::json!({ "trackId": track_id, "addedAt": added_at }),
                now,
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn favorites(conn: &Connection) -> Result<Vec<Track>> {
    query_tracks(
        conn,
        "WHERE f.track_id IS NOT NULL AND t.status = 'ok' ORDER BY f.added_at DESC, t.id DESC",
        [],
    )
}

// ── History and play counts ──────────────────────────────────────────────

/// Called the moment playback of a track starts.
pub fn record_play(conn: &Connection, track_id: i64, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO history (track_id, played_at) VALUES (?1, ?2)",
        params![track_id, now],
    )?;
    conn.execute(
        "INSERT INTO play_counts (track_id, count) VALUES (?1, 1)
         ON CONFLICT (track_id) DO UPDATE SET count = count + 1",
        [track_id],
    )?;
    Ok(())
}

pub fn history(conn: &Connection) -> Result<Vec<HistoryEntry>> {
    let rows: Vec<(i64, i64, i64)> = conn
        .prepare(
            "SELECT h.id, h.played_at, h.track_id FROM history h
             JOIN tracks t ON t.id = h.track_id AND t.status <> 'removed'
             ORDER BY h.played_at DESC, h.id DESC LIMIT ?1",
        )?
        .query_map([HISTORY_LIMIT], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let ids: Vec<i64> = rows.iter().map(|r| r.2).collect();
    let by_id: HashMap<i64, Track> = tracks_by_ids(conn, &ids)?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();
    Ok(rows
        .into_iter()
        .filter_map(|(id, played_at, track_id)| {
            by_id.get(&track_id).map(|track| HistoryEntry {
                id,
                played_at,
                track: track.clone(),
            })
        })
        .collect())
}

pub fn remove_history_entry(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM history WHERE id = ?1", [id])?;
    Ok(())
}

pub fn clear_history(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM history", [])?;
    Ok(())
}

// ── Playlists ────────────────────────────────────────────────────────────

fn clean_playlist_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("Enter a playlist name."));
    }
    if name.chars().count() > MAX_PLAYLIST_NAME {
        return Err(AppError::invalid(format!(
            "Playlist names can be at most {MAX_PLAYLIST_NAME} characters."
        )));
    }
    Ok(name.to_string())
}

fn live_playlist(conn: &Connection, id: &str) -> Result<i64> {
    let playlist_id = parse_playlist_id(id)?;
    conn.query_row(
        "SELECT id FROM playlists WHERE id = ?1 AND deleted_at IS NULL",
        [playlist_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound("That playlist no longer exists.".into()))
}

pub fn create_playlist(conn: &Connection, name: &str, now: i64) -> Result<String> {
    let name = clean_playlist_name(name)?;
    conn.execute(
        "INSERT INTO playlists (name, created_at) VALUES (?1, ?2)",
        params![name, now],
    )?;
    Ok(format!("playlist:{}", conn.last_insert_rowid()))
}

pub fn update_playlist(conn: &Connection, id: &str, name: &str, description: &str) -> Result<()> {
    let name = clean_playlist_name(name)?;
    let description = description.trim();
    if description.chars().count() > MAX_DESCRIPTION {
        return Err(AppError::invalid(format!(
            "Descriptions can be at most {MAX_DESCRIPTION} characters."
        )));
    }
    conn.execute(
        "UPDATE playlists SET name = ?1, description = ?2 WHERE id = ?3",
        params![name, description, live_playlist(conn, id)?],
    )?;
    Ok(())
}

/// Sets or clears the cover the user chose (a name in the artwork cache).
pub fn set_playlist_cover(conn: &Connection, id: &str, cover: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE playlists SET cover = ?1 WHERE id = ?2",
        params![cover, live_playlist(conn, id)?],
    )?;
    Ok(())
}

/// Moves the playlist to the Recycle Bin. Tracks and files are untouched.
pub fn delete_playlist(conn: &Connection, id: &str, now: i64) -> Result<()> {
    let playlist_id = live_playlist(conn, id)?;
    let tx = conn.unchecked_transaction()?;
    let name: String = tx.query_row(
        "SELECT name FROM playlists WHERE id = ?1",
        [playlist_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE playlists SET deleted_at = ?1 WHERE id = ?2",
        params![now, playlist_id],
    )?;
    trash_add(
        &tx,
        "playlist",
        &name,
        serde_json::json!({ "playlistId": playlist_id }),
        now,
    )?;
    tx.commit()?;
    Ok(())
}

pub fn add_to_playlist(conn: &Connection, id: &str, track_ids: &[i64], now: i64) -> Result<()> {
    let playlist_id = live_playlist(conn, id)?;
    let tx = conn.unchecked_transaction()?;
    let first: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
        [playlist_id],
        |row| row.get(0),
    )?;
    for (offset, track_id) in track_ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at)
             SELECT ?1, ?2, id, ?3 FROM tracks WHERE id = ?4",
            params![playlist_id, first + offset as i64, now, track_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn playlist_entries(conn: &Connection, playlist_id: i64) -> Result<Vec<(i64, i64)>> {
    let entries = conn
        .prepare(
            "SELECT track_id, added_at FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
        )?
        .query_map([playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(entries)
}

fn rewrite_playlist(conn: &Connection, playlist_id: i64, entries: &[(i64, i64)]) -> Result<()> {
    conn.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
        [playlist_id],
    )?;
    for (position, (track_id, added_at)) in entries.iter().enumerate() {
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![playlist_id, position as i64, track_id, added_at],
        )?;
    }
    Ok(())
}

/// Removes the entry at `index` (0-based, in playlist order).
pub fn remove_from_playlist(conn: &Connection, id: &str, index: usize) -> Result<()> {
    let playlist_id = live_playlist(conn, id)?;
    let tx = conn.unchecked_transaction()?;
    let mut entries = playlist_entries(&tx, playlist_id)?;
    if index >= entries.len() {
        return Err(AppError::NotFound(
            "That track is no longer in the playlist.".into(),
        ));
    }
    entries.remove(index);
    rewrite_playlist(&tx, playlist_id, &entries)?;
    tx.commit()?;
    Ok(())
}

/// Moves the entry at `from` to `to` (drag-and-drop reorder).
pub fn move_in_playlist(conn: &Connection, id: &str, from: usize, to: usize) -> Result<()> {
    let playlist_id = live_playlist(conn, id)?;
    let tx = conn.unchecked_transaction()?;
    let mut entries = playlist_entries(&tx, playlist_id)?;
    if from >= entries.len() || to >= entries.len() {
        return Err(AppError::NotFound(
            "That track is no longer in the playlist.".into(),
        ));
    }
    let entry = entries.remove(from);
    entries.insert(to, entry);
    rewrite_playlist(&tx, playlist_id, &entries)?;
    tx.commit()?;
    Ok(())
}

// ── Remove from Library ──────────────────────────────────────────────────

/// Takes tracks out of the library and puts them in the Recycle Bin.
/// Only BitNot's record changes: the audio files are not touched, and a scan
/// will not add them back while they are removed.
pub fn remove_from_library(conn: &Connection, track_ids: &[i64], now: i64) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut removed = 0;
    for id in track_ids {
        let Some(track) = track(&tx, *id)? else {
            continue;
        };
        let changed = tx.execute(
            "UPDATE tracks SET status = 'removed' WHERE id = ?1 AND status <> 'removed'",
            [id],
        )?;
        if changed > 0 {
            trash_add(
                &tx,
                "track",
                &track.title,
                serde_json::json!({ "trackId": id }),
                now,
            )?;
            removed += 1;
        }
    }
    tx.commit()?;
    Ok(removed)
}

// ── Metadata editing ─────────────────────────────────────────────────────

fn clean_text(value: Option<String>, what: &str, allow_empty: bool) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim().to_string();
    if value.is_empty() && !allow_empty {
        return Err(AppError::invalid(format!("The {what} cannot be empty.")));
    }
    if value.chars().count() > MAX_FIELD {
        return Err(AppError::invalid(format!(
            "The {what} can be at most {MAX_FIELD} characters."
        )));
    }
    Ok(Some(value))
}

fn current_edit(conn: &Connection, track_id: i64) -> Result<Option<TrackEdit>> {
    Ok(conn
        .query_row(
            "SELECT title, artist, album, genre, year, track_no FROM track_edits WHERE track_id = ?1",
            [track_id],
            |row| {
                Ok(TrackEdit {
                    title: row.get(0)?,
                    artist: row.get(1)?,
                    album: row.get(2)?,
                    genre: row.get(3)?,
                    year: row.get(4)?,
                    track_no: row.get(5)?,
                })
            },
        )
        .optional()?)
}

fn write_edit(conn: &Connection, track_id: i64, edit: Option<&TrackEdit>) -> Result<()> {
    conn.execute("DELETE FROM track_edits WHERE track_id = ?1", [track_id])?;
    if let Some(edit) = edit.filter(|e| **e != TrackEdit::default()) {
        conn.execute(
            "INSERT INTO track_edits (track_id, title, artist, album, genre, year, track_no)
             SELECT id, ?2, ?3, ?4, ?5, ?6, ?7 FROM tracks WHERE id = ?1",
            params![
                track_id,
                edit.title,
                edit.artist,
                edit.album,
                edit.genre,
                edit.year,
                edit.track_no
            ],
        )?;
    }
    Ok(())
}

/// Recomputes the folded search text from the values the library shows.
pub fn refresh_search_key(conn: &Connection, track_id: i64) -> Result<()> {
    let fields: Option<(String, String, String)> = conn
        .query_row(
            "SELECT title, artist, album FROM library_tracks WHERE id = ?1",
            [track_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((title, artist, album)) = fields {
        conn.execute(
            "UPDATE tracks SET search_key = ?1 WHERE id = ?2",
            params![textfold::search_key(&title, &artist, &album), track_id],
        )?;
    }
    Ok(())
}

/// Fills search keys for tracks stored before keys existed.
pub fn backfill_search_keys(conn: &Connection) -> Result<usize> {
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM tracks WHERE search_key = ''")?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    if ids.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    for id in &ids {
        refresh_search_key(&tx, *id)?;
    }
    tx.commit()?;
    Ok(ids.len())
}

/// Saves the user's metadata for a track. A field left as `None`, or set to
/// what the file already says, is not stored. The audio file is not written.
/// The previous state goes to the Recycle Bin so the edit can be undone.
pub fn edit_track(conn: &Connection, track_id: i64, edit: TrackEdit, now: i64) -> Result<Track> {
    let not_found = || AppError::NotFound("That track is no longer in your library.".into());
    let scanned: (
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT title, artist, album, genre, year, track_no FROM tracks WHERE id = ?1",
            [track_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(not_found)?;

    let mut next = TrackEdit {
        title: clean_text(edit.title, "title", false)?,
        artist: clean_text(edit.artist, "artist", false)?,
        album: clean_text(edit.album, "album", false)?,
        genre: clean_text(edit.genre, "genre", true)?,
        year: edit.year,
        track_no: edit.track_no,
    };
    if let Some(genre) = next.genre.as_deref().filter(|g| !g.is_empty()) {
        if !GENRES.contains(&genre) {
            return Err(AppError::invalid("Choose a genre from the list."));
        }
    }
    if next.year.is_some_and(|y| !(1000..=9999).contains(&y)) {
        return Err(AppError::invalid("Enter a four-digit year."));
    }
    if next.track_no.is_some_and(|n| !(1..=9999).contains(&n)) {
        return Err(AppError::invalid("Track numbers start at 1."));
    }
    // Store only what differs from the file's own tags.
    if next.title.as_deref() == Some(&scanned.0) {
        next.title = None;
    }
    if next.artist.as_deref() == Some(&scanned.1) {
        next.artist = None;
    }
    if next.album.as_deref() == Some(&scanned.2) {
        next.album = None;
    }
    if next.genre.as_deref() == scanned.3.as_deref()
        || (next.genre.as_deref() == Some("") && scanned.3.is_none())
    {
        next.genre = None;
    }
    if next.year == scanned.4 {
        next.year = None;
    }
    if next.track_no == scanned.5 {
        next.track_no = None;
    }

    let tx = conn.unchecked_transaction()?;
    let previous = current_edit(&tx, track_id)?;
    if previous.clone().unwrap_or_default() != next {
        let title = track(&tx, track_id)?.map(|t| t.title).unwrap_or_default();
        trash_add(
            &tx,
            "metadata",
            &title,
            serde_json::json!({ "trackId": track_id, "previous": previous }),
            now,
        )?;
        write_edit(&tx, track_id, Some(&next))?;
        refresh_search_key(&tx, track_id)?;
    }
    tx.commit()?;
    track(conn, track_id)?.ok_or_else(not_found)
}

/// Sets or clears a cover the user chose for these tracks.
pub fn set_custom_artwork(
    conn: &Connection,
    track_ids: &[i64],
    artwork: Option<&str>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for id in track_ids {
        tx.execute(
            "UPDATE tracks SET custom_artwork = ?1 WHERE id = ?2",
            params![artwork, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Every artwork cache name this profile still refers to.
pub fn referenced_artwork(conn: &Connection) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    for sql in [
        "SELECT artwork FROM tracks WHERE artwork IS NOT NULL",
        "SELECT custom_artwork FROM tracks WHERE custom_artwork IS NOT NULL",
        "SELECT cover FROM playlists WHERE cover IS NOT NULL",
    ] {
        let mut stmt = conn.prepare(sql)?;
        for name in stmt.query_map([], |row| row.get::<_, String>(0))? {
            names.insert(name?);
        }
    }
    Ok(names)
}

// ── Search ───────────────────────────────────────────────────────────────

fn like_pattern(needle: &str) -> String {
    let mut pattern = String::with_capacity(needle.len() + 2);
    pattern.push('%');
    for ch in needle.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern.push('%');
    pattern
}

/// Exact substring search over title, artist, and album, insensitive to case
/// and accents. Title matches rank first, then artist, then album.
/// There is no typo correction.
pub fn search(conn: &Connection, query: &str) -> Result<SearchResults> {
    let needle = textfold::fold(query.trim());
    let mut results = SearchResults {
        query: query.trim().to_string(),
        songs: Vec::new(),
        albums: Vec::new(),
        artists: Vec::new(),
    };
    if needle.is_empty() {
        return Ok(results);
    }
    // The database narrows by the folded key; ranking happens on the matches.
    let matches = query_tracks(
        conn,
        "WHERE t.status = 'ok' AND t.search_key LIKE ?1 ESCAPE '\\'",
        [like_pattern(&needle)],
    )?;
    let mut ranked: Vec<(u8, Track)> = collapse_duplicates(conn, matches)?
        .into_iter()
        .filter_map(|track| {
            let (title, artist, album) = textfold::key_parts(&track.search_key);
            let rank = if title.contains(&needle) {
                0
            } else if artist.contains(&needle) {
                1
            } else if album.contains(&needle) {
                2
            } else {
                return None; // matched only across the field separator
            };
            Some((rank, track))
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase()))
    });
    results.songs = ranked
        .into_iter()
        .take(SEARCH_LIMIT)
        .map(|(_, t)| t)
        .collect();
    results.albums = albums(conn)?
        .into_iter()
        .filter(|album| {
            textfold::fold(&album.title).contains(&needle)
                || textfold::fold(&album.owner).contains(&needle)
        })
        .take(SEARCH_LIMIT)
        .collect();
    results.artists = artists(conn)?
        .into_iter()
        .filter(|artist| textfold::fold(&artist.title).contains(&needle))
        .take(SEARCH_LIMIT)
        .collect();
    Ok(results)
}

pub fn scan_issues(conn: &Connection) -> Result<Vec<ScanIssue>> {
    let mut stmt = conn.prepare("SELECT path, reason FROM scan_issues ORDER BY path")?;
    let rows = stmt.query_map([], |row| {
        Ok(ScanIssue {
            path: row.get(0)?,
            reason: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db;

    pub fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        db::migrate(&conn, db::PROFILE_MIGRATIONS).unwrap();
        conn
    }

    /// Inserts a track row directly; returns its id.
    pub fn add_track(conn: &Connection, title: &str, artist: &str, album: &str, no: i64) -> i64 {
        add_file(conn, title, artist, album, no, "mp3", 180_000)
    }

    pub fn add_file(
        conn: &Connection,
        title: &str,
        artist: &str,
        album: &str,
        no: i64,
        ext: &str,
        duration_ms: i64,
    ) -> i64 {
        let codec = if ext == "flac" { "FLAC" } else { "MP3" };
        conn.execute(
            "INSERT INTO tracks (path, folder, title, artist, album, album_artist, track_no,
                                 duration_ms, codec, file_size, file_mtime, added_at, search_key)
             VALUES (?1, 'C:\\Music', ?2, ?3, ?4, ?3, ?5, ?6, ?7, 1, 1, 100, ?8)",
            params![
                format!("C:\\Music\\{artist}-{album}-{title}-{duration_ms}.{ext}"),
                title,
                artist,
                album,
                no,
                duration_ms,
                codec,
                textfold::search_key(title, artist, album)
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed(conn: &Connection) -> Vec<i64> {
        vec![
            add_track(conn, "Harbor Lights", "Mira Solen", "Low Tide", 2),
            add_track(conn, "Night Bus", "Mira Solen", "Low Tide", 1),
            add_track(conn, "Paper Kites", "The Oran Trio", "Harbor Sessions", 1),
            add_track(conn, "Ünder Glass", "Élan", "Night Bus Stories", 1),
        ]
    }

    fn id_list(tracks: &[Track]) -> Vec<i64> {
        tracks.iter().map(|t| t.id).collect()
    }

    #[test]
    fn albums_artists_and_album_order() {
        let conn = conn();
        seed(&conn);
        let overview = overview(&conn).unwrap();
        assert_eq!(overview.track_count, 4);
        assert_eq!(overview.albums.len(), 3);
        assert_eq!(overview.artists.len(), 3);

        let low_tide = overview
            .albums
            .iter()
            .find(|a| a.title == "Low Tide")
            .unwrap();
        assert_eq!(low_tide.track_count, 2);
        assert_eq!(low_tide.subtitle, "Album • Mira Solen");
        let detail = collection(&conn, &low_tide.id).unwrap();
        let titles: Vec<&str> = detail.tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["Night Bus", "Harbor Lights"],
            "album order by track number"
        );

        assert!(collection(&conn, "artist:Nobody").is_err());
        assert!(collection(&conn, "album:broken").is_err());
    }

    #[test]
    fn artist_page_has_popular_tracks_only_with_history_and_newest_albums_first() {
        let conn = conn();
        let a = add_track(&conn, "Old Song", "Mira", "First", 1);
        let b = add_track(&conn, "New Song", "Mira", "Second", 1);
        let c = add_track(&conn, "No Year", "Mira", "Undated", 1);
        conn.execute("UPDATE tracks SET year = 2019 WHERE id = ?1", [a])
            .unwrap();
        conn.execute("UPDATE tracks SET year = 2024 WHERE id = ?1", [b])
            .unwrap();

        let page = collection(&conn, "artist:Mira").unwrap();
        assert!(
            page.popular.is_empty(),
            "no listening history, no Popular Tracks"
        );
        let albums: Vec<&str> = page.albums.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(albums, vec!["Second", "First", "Undated"]);
        assert_eq!(page.tracks.len(), 3);

        record_play(&conn, a, 1).unwrap();
        record_play(&conn, a, 2).unwrap();
        record_play(&conn, c, 3).unwrap();
        let page = collection(&conn, "artist:Mira").unwrap();
        assert_eq!(
            id_list(&page.popular),
            vec![a, c],
            "ordered by local play count"
        );
    }

    #[test]
    fn unavailable_tracks_leave_library_lists_but_stay_in_playlists() {
        let conn = conn();
        let ids = seed(&conn);
        let playlist = create_playlist(&conn, "Mix", 1).unwrap();
        add_to_playlist(&conn, &playlist, &[ids[0], ids[2]], 2).unwrap();
        conn.execute(
            "UPDATE tracks SET status = 'missing' WHERE id = ?1",
            [ids[0]],
        )
        .unwrap();

        assert_eq!(all_tracks(&conn).unwrap().len(), 3);
        let detail = collection(&conn, &playlist).unwrap();
        assert_eq!(detail.tracks.len(), 2);
        assert!(!detail.tracks[0].available);
        assert!(detail.tracks[1].available);
    }

    #[test]
    fn favorites_toggle_and_recycle_bin() {
        let conn = conn();
        let ids = seed(&conn);
        set_favorite(&conn, ids[0], true, 10).unwrap();
        set_favorite(&conn, ids[2], true, 20).unwrap();
        set_favorite(&conn, ids[2], true, 30).unwrap(); // idempotent
        assert_eq!(id_list(&favorites(&conn).unwrap()), vec![ids[2], ids[0]]);
        assert_eq!(overview(&conn).unwrap().favorite_ids, vec![ids[2], ids[0]]);

        set_favorite(&conn, ids[2], false, 40).unwrap();
        set_favorite(&conn, ids[2], false, 41).unwrap(); // nothing more to remove
        let bin = trash(&conn, 40).unwrap();
        assert_eq!(bin.len(), 1);
        assert_eq!(
            (bin[0].kind.as_str(), bin[0].label.as_str()),
            ("favorite", "Paper Kites")
        );

        trash_restore(&conn, bin[0].id).unwrap();
        let restored = favorites(&conn).unwrap();
        assert_eq!(
            id_list(&restored),
            vec![ids[2], ids[0]],
            "original position by date kept"
        );
        assert!(trash(&conn, 50).unwrap().is_empty());

        // Favoriting again by hand clears the pending bin entry.
        set_favorite(&conn, ids[0], false, 60).unwrap();
        set_favorite(&conn, ids[0], true, 61).unwrap();
        assert!(trash(&conn, 62).unwrap().is_empty());
        assert!(set_favorite(&conn, 9999, true, 1).is_err());
    }

    #[test]
    fn plays_are_counted_and_listed_newest_first() {
        let conn = conn();
        let ids = seed(&conn);
        record_play(&conn, ids[0], 100).unwrap();
        record_play(&conn, ids[1], 200).unwrap();
        record_play(&conn, ids[0], 300).unwrap();
        assert_eq!(track(&conn, ids[0]).unwrap().unwrap().play_count, 2);
        assert_eq!(track(&conn, ids[3]).unwrap().unwrap().play_count, 0);

        let entries = history(&conn).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.track.id).collect::<Vec<_>>(),
            vec![ids[0], ids[1], ids[0]]
        );
        remove_history_entry(&conn, entries[0].id).unwrap();
        assert_eq!(history(&conn).unwrap().len(), 2);
        clear_history(&conn).unwrap();
        assert!(history(&conn).unwrap().is_empty());
        assert_eq!(
            track(&conn, ids[0]).unwrap().unwrap().play_count,
            2,
            "counts are kept"
        );
    }

    #[test]
    fn playlist_details_order_duplicates_and_reordering() {
        let conn = conn();
        let ids = seed(&conn);
        assert!(create_playlist(&conn, "  ", 1).is_err());
        let id = create_playlist(&conn, " Road trip ", 1).unwrap();
        add_to_playlist(&conn, &id, &[ids[2], ids[0], ids[2]], 2).unwrap();
        add_to_playlist(&conn, &id, &[ids[1], 424242], 3).unwrap(); // unknown id skipped

        let order = |conn: &Connection| id_list(&collection(conn, &id).unwrap().tracks);
        assert_eq!(order(&conn), vec![ids[2], ids[0], ids[2], ids[1]]);

        // Drag the last entry to the front, then the first to the end.
        move_in_playlist(&conn, &id, 3, 0).unwrap();
        assert_eq!(order(&conn), vec![ids[1], ids[2], ids[0], ids[2]]);
        move_in_playlist(&conn, &id, 0, 3).unwrap();
        assert_eq!(order(&conn), vec![ids[2], ids[0], ids[2], ids[1]]);
        assert!(move_in_playlist(&conn, &id, 0, 9).is_err());

        // Removing the second copy of a duplicate removes that copy, not the first.
        remove_from_playlist(&conn, &id, 2).unwrap();
        assert_eq!(order(&conn), vec![ids[2], ids[0], ids[1]]);
        assert!(remove_from_playlist(&conn, &id, 9).is_err());

        update_playlist(&conn, &id, "Road", "  Songs for the drive  ").unwrap();
        let summary = &playlists(&conn).unwrap()[0];
        assert_eq!(
            (summary.title.as_str(), summary.description.as_str()),
            ("Road", "Songs for the drive")
        );
        assert!(update_playlist(&conn, &id, "Road", &"x".repeat(301)).is_err());

        assert!(!summary.custom_cover);
        set_playlist_cover(&conn, &id, Some("abc.jpg")).unwrap();
        let summary = &playlists(&conn).unwrap()[0];
        assert!(summary.custom_cover);
        assert_eq!(summary.artwork.as_deref(), Some("abc.jpg"));
        set_playlist_cover(&conn, &id, None).unwrap();
        assert!(!playlists(&conn).unwrap()[0].custom_cover);
    }

    #[test]
    fn deleted_playlists_go_to_the_bin_and_can_be_restored_or_purged() {
        let conn = conn();
        let ids = seed(&conn);
        let id = create_playlist(&conn, "Keep", 1).unwrap();
        add_to_playlist(&conn, &id, &[ids[0], ids[1]], 2).unwrap();

        delete_playlist(&conn, &id, 100).unwrap();
        assert!(playlists(&conn).unwrap().is_empty());
        assert!(collection(&conn, &id).is_err());
        assert!(add_to_playlist(&conn, &id, &[ids[0]], 4).is_err());
        assert!(delete_playlist(&conn, &id, 101).is_err());
        let bin = trash(&conn, 100 + 10 * SECONDS_PER_DAY).unwrap();
        assert_eq!(bin[0].days_remaining, 80);

        trash_restore(&conn, bin[0].id).unwrap();
        assert_eq!(
            collection(&conn, &id).unwrap().tracks.len(),
            2,
            "tracks and order come back"
        );

        delete_playlist(&conn, &id, 200).unwrap();
        let entry = trash(&conn, 200).unwrap()[0].id;
        trash_purge(&conn, entry).unwrap();
        assert!(trash(&conn, 200).unwrap().is_empty());
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
        assert_eq!(
            all_tracks(&conn).unwrap().len(),
            4,
            "tracks survive playlist deletion"
        );
        assert!(trash_restore(&conn, entry).is_err());
    }

    #[test]
    fn bin_entries_expire_after_ninety_days() {
        let conn = conn();
        seed(&conn);
        let old = create_playlist(&conn, "Old", 1).unwrap();
        let recent = create_playlist(&conn, "Recent", 1).unwrap();
        let now = 400 * SECONDS_PER_DAY;
        delete_playlist(&conn, &old, now - 90 * SECONDS_PER_DAY).unwrap();
        delete_playlist(&conn, &recent, now - 89 * SECONDS_PER_DAY).unwrap();
        assert_eq!(trash_purge_expired(&conn, now).unwrap(), 1);
        let bin = trash(&conn, now).unwrap();
        assert_eq!(bin.len(), 1);
        assert_eq!(bin[0].label, "Recent");
        assert_eq!(trash_purge_all(&conn, None).unwrap(), 1);
    }

    #[test]
    fn removing_from_library_hides_the_track_and_is_reversible() {
        let conn = conn();
        let ids = seed(&conn);
        let playlist = create_playlist(&conn, "Mix", 1).unwrap();
        add_to_playlist(&conn, &playlist, &[ids[0], ids[1]], 1).unwrap();
        set_favorite(&conn, ids[0], true, 1).unwrap();
        record_play(&conn, ids[0], 1).unwrap();

        assert_eq!(
            remove_from_library(&conn, &[ids[0], ids[0], 777], 10).unwrap(),
            1
        );
        assert_eq!(all_tracks(&conn).unwrap().len(), 3);
        assert!(favorites(&conn).unwrap().is_empty());
        assert!(history(&conn).unwrap().is_empty());
        assert!(!collection(&conn, &playlist).unwrap().tracks[0].available);
        assert_eq!(search(&conn, "harbor lights").unwrap().songs.len(), 0);

        let bin = trash(&conn, 10).unwrap();
        assert_eq!(
            (bin[0].kind.as_str(), bin[0].label.as_str()),
            ("track", "Harbor Lights")
        );
        trash_restore(&conn, bin[0].id).unwrap();
        assert_eq!(all_tracks(&conn).unwrap().len(), 4);
        assert_eq!(
            favorites(&conn).unwrap().len(),
            1,
            "favorite and history return with it"
        );

        // Permanent deletion drops what referred to it; the marker row remains.
        remove_from_library(&conn, &[ids[0]], 20).unwrap();
        trash_purge(&conn, trash(&conn, 20).unwrap()[0].id).unwrap();
        assert_eq!(collection(&conn, &playlist).unwrap().tracks.len(), 1);
        let status: String = conn
            .query_row("SELECT status FROM tracks WHERE id = ?1", [ids[0]], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(status, "removed");
    }

    #[test]
    fn metadata_edits_overlay_scanned_values_and_can_be_undone() {
        let conn = conn();
        let ids = seed(&conn);
        let edit = TrackEdit {
            title: Some("  Harbour Lights ".into()),
            artist: Some("Mira Solén".into()),
            genre: Some("Indie Pop".into()),
            year: Some(2021),
            ..TrackEdit::default()
        };
        let edited = edit_track(&conn, ids[0], edit, 10).unwrap();
        assert_eq!(edited.title, "Harbour Lights");
        assert_eq!(edited.artist, "Mira Solén");
        assert_eq!(
            edited.album_artist, "Mira Solén",
            "album artist follows the artist"
        );
        assert_eq!(
            edited.album, "Low Tide",
            "untouched fields keep the file's value"
        );
        assert_eq!(
            (edited.genre.as_deref(), edited.year),
            (Some("Indie Pop"), Some(2021))
        );
        assert!(edited.edited);

        // The scanned row is unchanged; search follows the edited values.
        let scanned: String = conn
            .query_row("SELECT title FROM tracks WHERE id = ?1", [ids[0]], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(scanned, "Harbor Lights");
        assert_eq!(search(&conn, "harbour").unwrap().songs.len(), 1);
        assert!(overview(&conn)
            .unwrap()
            .artists
            .iter()
            .any(|a| a.title == "Mira Solén"));

        // Undo through the Recycle Bin.
        let bin = trash(&conn, 10).unwrap();
        assert_eq!(bin[0].kind, "metadata");
        trash_restore(&conn, bin[0].id).unwrap();
        let restored = track(&conn, ids[0]).unwrap().unwrap();
        assert_eq!(restored.title, "Harbor Lights");
        assert!(!restored.edited);
        assert_eq!(search(&conn, "harbour").unwrap().songs.len(), 0);
    }

    #[test]
    fn metadata_edits_are_validated_and_setting_file_values_stores_nothing() {
        let conn = conn();
        let ids = seed(&conn);
        let bad = |edit: TrackEdit| edit_track(&conn, ids[0], edit, 1).is_err();
        assert!(bad(TrackEdit {
            title: Some("  ".into()),
            ..Default::default()
        }));
        assert!(bad(TrackEdit {
            genre: Some("My Own Genre".into()),
            ..Default::default()
        }));
        assert!(bad(TrackEdit {
            year: Some(99),
            ..Default::default()
        }));
        assert!(bad(TrackEdit {
            track_no: Some(0),
            ..Default::default()
        }));
        assert!(edit_track(&conn, 9999, TrackEdit::default(), 1).is_err());

        let same = TrackEdit {
            title: Some("Harbor Lights".into()),
            track_no: Some(2),
            ..Default::default()
        };
        let track = edit_track(&conn, ids[0], same, 2).unwrap();
        assert!(!track.edited);
        assert!(
            trash(&conn, 3).unwrap().is_empty(),
            "nothing changed, nothing to undo"
        );
        assert!(GENRES.contains(&"Synthwave") && GENRES.contains(&"Deep House"));
    }

    #[test]
    fn identical_recordings_show_once_with_the_best_version() {
        let conn = conn();
        let mp3 = add_file(&conn, "Night Bus", "Mira", "Low Tide", 1, "mp3", 180_000);
        let flac = add_file(&conn, "night bus", "MIRA", "Low Tide", 1, "flac", 181_500);
        let live = add_file(&conn, "Night Bus", "Mira", "Low Tide", 1, "mp3", 240_000);
        let other = add_track(&conn, "Salt Air", "Mira", "Low Tide", 2);

        let songs = all_tracks(&conn).unwrap();
        assert_eq!(
            songs.len(),
            3,
            "the two matching files are one track; the longer take is not"
        );
        let shown = songs.iter().find(|t| t.sources == 2).unwrap();
        assert_eq!(shown.id, flac, "lossless is preferred");
        assert!(songs.iter().any(|t| t.id == live && t.sources == 1));
        assert!(songs.iter().any(|t| t.id == other));

        assert_eq!(
            id_list(&track_sources(&conn, mp3).unwrap()),
            vec![flac, mp3]
        );
        prefer_source(&conn, mp3).unwrap();
        let songs = all_tracks(&conn).unwrap();
        assert_eq!(
            songs.iter().find(|t| t.sources == 2).unwrap().id,
            mp3,
            "the user's choice wins"
        );
        assert_eq!(search(&conn, "night bus").unwrap().songs.len(), 2);

        // Playlists keep exact entries, duplicates included.
        let playlist = create_playlist(&conn, "Both", 1).unwrap();
        add_to_playlist(&conn, &playlist, &[mp3, flac], 1).unwrap();
        assert_eq!(
            id_list(&collection(&conn, &playlist).unwrap().tracks),
            vec![mp3, flac]
        );
        assert!(prefer_source(&conn, 9999).is_err());
    }

    #[test]
    fn search_ranks_title_then_artist_then_album_and_ignores_accents_only() {
        let conn = conn();
        seed(&conn);
        let r = search(&conn, "  NIGHT bus ").unwrap();
        let titles: Vec<&str> = r.songs.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["Night Bus", "Ünder Glass"],
            "title match before album match"
        );
        assert_eq!(r.albums.len(), 1);

        let r = search(&conn, "mira").unwrap();
        assert_eq!(r.songs.len(), 2);
        assert_eq!(r.artists.len(), 1);

        // Accents and case are ignored in both directions; typos are not guessed.
        assert_eq!(search(&conn, "under glass").unwrap().songs.len(), 1);
        assert_eq!(search(&conn, "ÜNDER").unwrap().songs.len(), 1);
        assert_eq!(search(&conn, "elan").unwrap().artists.len(), 1);
        assert_eq!(search(&conn, "harbour").unwrap().songs.len(), 0);
        assert!(search(&conn, "   ").unwrap().songs.is_empty());
        // Wildcard characters are literal, and a query cannot span two fields.
        assert_eq!(search(&conn, "%").unwrap().songs.len(), 0);
        assert_eq!(search(&conn, "lights mira").unwrap().songs.len(), 0);
    }

    #[test]
    fn recently_added_is_newest_first_and_keys_are_backfilled() {
        let conn = conn();
        let ids = seed(&conn);
        conn.execute("UPDATE tracks SET added_at = 500 WHERE id = ?1", [ids[2]])
            .unwrap();
        assert_eq!(recent_tracks(&conn).unwrap()[0].id, ids[2]);

        conn.execute("UPDATE tracks SET search_key = ''", [])
            .unwrap();
        assert_eq!(search(&conn, "night").unwrap().songs.len(), 0);
        assert_eq!(backfill_search_keys(&conn).unwrap(), 4);
        assert_eq!(backfill_search_keys(&conn).unwrap(), 0);
        assert_eq!(search(&conn, "night").unwrap().songs.len(), 2);
    }

    #[test]
    fn tracks_by_ids_keeps_order_and_repeats() {
        let conn = conn();
        let ids = seed(&conn);
        let got = tracks_by_ids(&conn, &[ids[2], ids[0], ids[2], 777]).unwrap();
        assert_eq!(id_list(&got), vec![ids[2], ids[0], ids[2]]);
    }

    #[test]
    fn referenced_artwork_covers_tracks_overrides_and_playlists() {
        let conn = conn();
        let ids = seed(&conn);
        conn.execute(
            "UPDATE tracks SET artwork = 'a.jpg' WHERE id = ?1",
            [ids[0]],
        )
        .unwrap();
        set_custom_artwork(&conn, &[ids[1]], Some("b.png")).unwrap();
        let playlist = create_playlist(&conn, "P", 1).unwrap();
        set_playlist_cover(&conn, &playlist, Some("c.jpg")).unwrap();
        let names = referenced_artwork(&conn).unwrap();
        assert_eq!(names, ["a.jpg", "b.png", "c.jpg"].map(String::from).into());
        assert_eq!(
            track(&conn, ids[1]).unwrap().unwrap().artwork.as_deref(),
            Some("b.png")
        );
        set_custom_artwork(&conn, &[ids[1]], None).unwrap();
        assert_eq!(track(&conn, ids[1]).unwrap().unwrap().artwork, None);
    }
}
