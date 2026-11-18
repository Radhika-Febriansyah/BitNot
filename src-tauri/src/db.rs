//! SQLite access: opening, integrity check, and forward-only migrations.
//!
//! Each migration is one SQL script. `PRAGMA user_version` records how many
//! have been applied. A database with a higher version than this build knows
//! is refused rather than modified.

use std::path::Path;

use rusqlite::Connection;

use crate::error::{AppError, Result};

/// Application-level database: profiles and app-wide state.
pub const APP_MIGRATIONS: &[&str] = &["
    CREATE TABLE profiles (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        is_main    INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        deleted_at INTEGER
    );
    CREATE UNIQUE INDEX profiles_single_main ON profiles (is_main) WHERE is_main = 1;
    CREATE TABLE app_meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
"];

/// Per-profile database.
pub const PROFILE_MIGRATIONS: &[&str] = &[
    "
    CREATE TABLE settings (
        key        TEXT PRIMARY KEY,
        value      TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );
",
    // 2: local library, favorites, playlists, history, play counts, queue.
    "
    CREATE TABLE tracks (
        id            INTEGER PRIMARY KEY,
        path          TEXT NOT NULL UNIQUE,
        folder        TEXT NOT NULL,
        title         TEXT NOT NULL,
        artist        TEXT NOT NULL,
        album         TEXT NOT NULL,
        album_artist  TEXT NOT NULL,
        genre         TEXT,
        year          INTEGER,
        track_no      INTEGER,
        disc_no       INTEGER,
        duration_ms   INTEGER NOT NULL,
        codec         TEXT,
        bitrate_kbps  INTEGER,
        sample_rate   INTEGER,
        bit_depth     INTEGER,
        channels      INTEGER,
        artwork       TEXT,
        file_size     INTEGER NOT NULL,
        file_mtime    INTEGER NOT NULL,
        -- ok | missing (file gone) | detached (its folder was removed from BitNot)
        status        TEXT NOT NULL DEFAULT 'ok',
        added_at      INTEGER NOT NULL
    );
    CREATE INDEX tracks_by_album ON tracks (album_artist, album);
    CREATE INDEX tracks_by_artist ON tracks (artist);
    CREATE TABLE favorites (
        track_id INTEGER PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE,
        added_at INTEGER NOT NULL
    );
    CREATE TABLE playlists (
        id          INTEGER PRIMARY KEY,
        name        TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        created_at  INTEGER NOT NULL
    );
    CREATE TABLE playlist_tracks (
        playlist_id INTEGER NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
        position    INTEGER NOT NULL,
        track_id    INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
        added_at    INTEGER NOT NULL,
        PRIMARY KEY (playlist_id, position)
    );
    CREATE TABLE history (
        id        INTEGER PRIMARY KEY,
        track_id  INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
        played_at INTEGER NOT NULL
    );
    CREATE INDEX history_by_time ON history (played_at DESC);
    CREATE TABLE play_counts (
        track_id INTEGER PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE,
        count    INTEGER NOT NULL
    );
    -- Files the scanner could not add, with the reason shown to the user.
    CREATE TABLE scan_issues (
        path   TEXT PRIMARY KEY,
        reason TEXT NOT NULL
    );
    CREATE TABLE queue_state (
        id   INTEGER PRIMARY KEY CHECK (id = 1),
        data TEXT NOT NULL
    );
",
    // 3: playlist details, edits kept apart from scanned tags, duplicates,
    //    recycle bin, artwork overrides, fast search, cheap position saves.
    "
    ALTER TABLE playlists ADD COLUMN cover TEXT;
    ALTER TABLE playlists ADD COLUMN deleted_at INTEGER;
    ALTER TABLE tracks ADD COLUMN custom_artwork TEXT;
    -- Folded title, artist, album (see textfold.rs). Filled by the application.
    ALTER TABLE tracks ADD COLUMN search_key TEXT NOT NULL DEFAULT '';
    -- The user's metadata edits. The scanned values in `tracks` and the audio
    -- files themselves are never changed by editing.
    CREATE TABLE track_edits (
        track_id INTEGER PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE,
        title    TEXT,
        artist   TEXT,
        album    TEXT,
        genre    TEXT,
        year     INTEGER,
        track_no INTEGER
    );
    -- The version of a duplicated track the user chose to prefer.
    CREATE TABLE preferred_sources (
        track_id INTEGER PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE
    );
    -- Recycle bin: playlist | favorite | track | metadata.
    CREATE TABLE trash (
        id         INTEGER PRIMARY KEY,
        kind       TEXT NOT NULL,
        label      TEXT NOT NULL,
        payload    TEXT NOT NULL,
        deleted_at INTEGER NOT NULL
    );
    CREATE TABLE playback_position (
        id          INTEGER PRIMARY KEY CHECK (id = 1),
        position_ms INTEGER NOT NULL
    );
    -- Tracks as the library shows them: edits applied over scanned values.
    -- tracks.status also takes 'removed' (taken out of the library by the user).
    CREATE VIEW library_tracks AS
    SELECT t.id, t.path, t.folder,
           COALESCE(e.title, t.title) AS title,
           COALESCE(e.artist, t.artist) AS artist,
           COALESCE(e.album, t.album) AS album,
           CASE WHEN e.artist IS NOT NULL AND t.album_artist = t.artist
                THEN e.artist ELSE t.album_artist END AS album_artist,
           COALESCE(e.genre, t.genre) AS genre,
           COALESCE(e.year, t.year) AS year,
           COALESCE(e.track_no, t.track_no) AS track_no,
           t.disc_no, t.duration_ms, t.codec, t.bitrate_kbps, t.sample_rate,
           t.bit_depth, t.channels,
           COALESCE(t.custom_artwork, t.artwork) AS artwork,
           t.status, t.added_at, t.search_key,
           e.track_id IS NOT NULL AS edited
    FROM tracks t LEFT JOIN track_edits e ON e.track_id = t.id;
",
];

/// What a repair did, for the notice shown to the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Repair {
    /// File name the damaged original was kept under, next to the database.
    pub kept_as: String,
    /// Tables whose contents could not be read and start empty.
    pub lost_tables: Vec<String>,
}

fn sidecars(path: &Path) -> [std::path::PathBuf; 2] {
    let with = |suffix: &str| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        std::path::PathBuf::from(name)
    };
    [with("-wal"), with("-shm")]
}

/// Copies every readable table from a damaged database into a fresh one.
/// The damaged file is only read here.
fn rebuild(
    path: &Path,
    migrations: &[&str],
    required: &[&str],
) -> Result<(std::path::PathBuf, Vec<String>)> {
    let corrupt = || {
        AppError::Corrupt(
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    };
    let version = {
        let source = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| corrupt())?;
        let version: i64 = source
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|_| corrupt())?;
        version
    };
    if version < 1 || version as usize > migrations.len() {
        return Err(corrupt());
    }
    let fresh_path = path.with_extension("repair");
    let _ = std::fs::remove_file(&fresh_path);
    let fresh = Connection::open(&fresh_path)?;
    // Same schema version as the damaged file, so rows copy column for column.
    migrate(&fresh, &migrations[..version as usize])?;
    fresh
        .execute(
            "ATTACH DATABASE ?1 AS damaged",
            [path.to_string_lossy().as_ref()],
        )
        .map_err(|_| corrupt())?;
    let tables: Vec<String> = fresh
        .prepare(
            "SELECT name FROM main.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut lost = Vec::new();
    for table in &tables {
        let copied = fresh.execute_batch(&format!(
            "INSERT OR IGNORE INTO main.\"{table}\" SELECT * FROM damaged.\"{table}\";"
        ));
        if copied.is_err() {
            let _ = fresh.execute_batch(&format!("DELETE FROM main.\"{table}\";"));
            lost.push(table.clone());
        }
    }
    let _ = fresh.execute_batch("DETACH DATABASE damaged");
    let healthy: String = fresh.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    drop(fresh);
    if healthy != "ok" || required.iter().any(|name| lost.iter().any(|l| l == name)) {
        let _ = std::fs::remove_file(&fresh_path);
        return Err(corrupt());
    }
    Ok((fresh_path, lost))
}

/// Opens a database, repairing it when it is damaged and repair is safe.
///
/// The damaged original is never modified or deleted: it is renamed aside
/// (`<name>.damaged-<time>`) and a rebuilt copy takes its place. If the tables
/// in `required` cannot be recovered, nothing is changed and the error is
/// returned so the caller can explain it.
pub fn open_or_repair(
    path: &Path,
    migrations: &[&str],
    required: &[&str],
    now: i64,
) -> Result<(Connection, Option<Repair>)> {
    match open(path, migrations) {
        Ok(conn) => Ok((conn, None)),
        Err(AppError::Corrupt(_)) => {
            let (fresh_path, lost_tables) = rebuild(path, migrations, required)?;
            let kept_as = format!(
                "{}.damaged-{now}",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
            let kept_path = path.with_file_name(&kept_as);
            std::fs::rename(path, &kept_path)?;
            for (index, sidecar) in sidecars(path).iter().enumerate() {
                if sidecar.exists() {
                    let suffix = ["-wal", "-shm"][index];
                    let _ =
                        std::fs::rename(sidecar, path.with_file_name(format!("{kept_as}{suffix}")));
                }
            }
            std::fs::rename(&fresh_path, path)?;
            let conn = open(path, migrations)?;
            Ok((
                conn,
                Some(Repair {
                    kept_as,
                    lost_tables,
                }),
            ))
        }
        Err(other) => Err(other),
    }
}

pub fn open(path: &Path, migrations: &[&str]) -> Result<Connection> {
    let existed = path.exists();
    let conn = Connection::open(path)?;
    // Check before any pragma that could write, so a damaged file stays as it is.
    if existed {
        check_integrity(&conn, path)?;
    }
    configure(&conn)?;
    migrate(&conn, migrations)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

fn check_integrity(conn: &Connection, path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let verdict: std::result::Result<String, rusqlite::Error> =
        conn.query_row("PRAGMA quick_check", [], |row| row.get(0));
    match verdict {
        Ok(v) if v.eq_ignore_ascii_case("ok") => Ok(()),
        _ => Err(AppError::Corrupt(name)),
    }
}

pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

pub fn migrate(conn: &Connection, migrations: &[&str]) -> Result<()> {
    let supported = migrations.len() as i64;
    let current = schema_version(conn)?;
    if current > supported {
        return Err(AppError::SchemaTooNew {
            found: current,
            supported,
        });
    }
    for (index, sql) in migrations.iter().enumerate().skip(current as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", index + 1))?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_reaches_latest_version() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open(&tmp.path().join("a.db"), APP_MIGRATIONS).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), APP_MIGRATIONS.len() as i64);
        conn.execute(
            "INSERT INTO profiles (id, name, is_main, created_at) VALUES ('a', 'A', 1, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn reopening_is_idempotent_and_keeps_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.db");
        {
            let conn = open(&path, PROFILE_MIGRATIONS).unwrap();
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('k', '1', 0)",
                [],
            )
            .unwrap();
        }
        let conn = open(&path, PROFILE_MIGRATIONS).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn later_migrations_apply_on_top_of_existing_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("m.db");
        let v1: &[&str] = &["CREATE TABLE t (a INTEGER);"];
        let v2: &[&str] = &[
            "CREATE TABLE t (a INTEGER);",
            "ALTER TABLE t ADD COLUMN b TEXT;",
        ];
        {
            let conn = open(&path, v1).unwrap();
            conn.execute("INSERT INTO t (a) VALUES (7)", []).unwrap();
        }
        let conn = open(&path, v2).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 2);
        let a: i64 = conn
            .query_row("SELECT a FROM t WHERE b IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(a, 7);
    }

    #[test]
    fn failed_migration_rolls_back_and_keeps_version() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("f.db");
        let bad: &[&str] = &[
            "CREATE TABLE t (a INTEGER);",
            "CREATE TABLE u (x); NOT SQL;",
        ];
        assert!(open(&path, bad).is_err());
        let conn = Connection::open(&path).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 1);
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'u'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 0);
    }

    #[test]
    fn newer_schema_is_refused_and_left_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("n.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA user_version = 99").unwrap();
        }
        let err = open(&path, APP_MIGRATIONS).unwrap_err();
        assert!(matches!(err, AppError::SchemaTooNew { found: 99, .. }));
        let conn = Connection::open(&path).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 99);
    }

    /// Builds a profile database with data, then damages pages in its middle.
    fn damaged_profile_db(path: &Path) -> Vec<u8> {
        {
            let conn = open(path, PROFILE_MIGRATIONS).unwrap();
            conn.execute_batch("PRAGMA journal_mode = DELETE;").unwrap();
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('theme', '\"light\"', 1)",
                [],
            )
            .unwrap();
            for i in 0..600 {
                conn.execute(
                    "INSERT INTO tracks (path, folder, title, artist, album, album_artist,
                         duration_ms, file_size, file_mtime, added_at)
                     VALUES (?1, 'f', ?2, 'a', 'b', 'a', 1, 1, 1, 1)",
                    rusqlite::params![
                        format!("C:/music/{i:04}-{}", "x".repeat(200)),
                        format!("Track {i}")
                    ],
                )
                .unwrap();
            }
        }
        let mut bytes = std::fs::read(path).unwrap();
        let middle = bytes.len() / 2 / 4096 * 4096;
        for byte in &mut bytes[middle..middle + 4096 * 3] {
            *byte = 0xA5;
        }
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    #[test]
    fn a_damaged_database_is_rebuilt_and_the_original_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("profile.db");
        let damaged_bytes = damaged_profile_db(&path);
        assert!(matches!(
            open(&path, PROFILE_MIGRATIONS),
            Err(AppError::Corrupt(_))
        ));

        let (conn, repair) = open_or_repair(&path, PROFILE_MIGRATIONS, &[], 777).unwrap();
        let repair = repair.expect("a repair happened");
        assert_eq!(repair.kept_as, "profile.db.damaged-777");
        assert_eq!(
            std::fs::read(tmp.path().join(&repair.kept_as)).unwrap(),
            damaged_bytes,
            "the damaged original is preserved byte for byte"
        );
        // Undamaged data survives and the rebuilt database is usable.
        let theme: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'theme'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(theme, "\"light\"");
        assert_eq!(
            schema_version(&conn).unwrap(),
            PROFILE_MIGRATIONS.len() as i64
        );
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(check, "ok");
        drop(conn);
        // A second open is clean: no further repair.
        assert!(open_or_repair(&path, PROFILE_MIGRATIONS, &[], 778)
            .unwrap()
            .1
            .is_none());
    }

    #[test]
    fn repair_is_refused_when_a_required_table_is_lost_and_nothing_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("profile.db");
        let damaged_bytes = damaged_profile_db(&path);
        let result = open_or_repair(&path, PROFILE_MIGRATIONS, &["tracks"], 1);
        match result {
            // Either the table could not be read (refused) ...
            Err(AppError::Corrupt(_)) => {
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    damaged_bytes,
                    "original untouched"
                );
                assert!(!tmp.path().join("profile.repair").exists());
            }
            // ... or SQLite could still read it, in which case it was not lost.
            Ok((_, Some(repair))) => assert!(!repair.lost_tables.contains(&"tracks".to_string())),
            other => panic!("unexpected result: {:?}", other.map(|(_, r)| r)),
        }
    }

    #[test]
    fn a_file_that_is_not_a_database_cannot_be_repaired_and_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("c.db");
        let garbage = vec![0x42u8; 8192];
        std::fs::write(&path, &garbage).unwrap();
        assert!(open_or_repair(&path, APP_MIGRATIONS, &["profiles"], 1).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), garbage);
        assert_eq!(
            std::fs::read_dir(tmp.path()).unwrap().count(),
            1,
            "no stray files"
        );
    }

    #[test]
    fn profile_schema_v3_upgrades_v2_data_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("p.db");
        {
            let conn = open(&path, &PROFILE_MIGRATIONS[..2]).unwrap();
            conn.execute(
                "INSERT INTO tracks (path, folder, title, artist, album, album_artist,
                     duration_ms, file_size, file_mtime, added_at)
                 VALUES ('C:/a.mp3', 'C:/', 'Title', 'Artist', 'Album', 'Artist', 1, 1, 1, 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO playlists (name, created_at) VALUES ('Mix', 1)",
                [],
            )
            .unwrap();
            conn.execute("INSERT INTO queue_state (id, data) VALUES (1, '{}')", [])
                .unwrap();
        }
        let conn = open(&path, PROFILE_MIGRATIONS).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 3);
        let (title, key): (String, String) = conn
            .query_row("SELECT title, search_key FROM library_tracks", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(
            (title.as_str(), key.as_str()),
            ("Title", ""),
            "rows kept; key filled by the app"
        );
        let playlists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlists WHERE deleted_at IS NULL AND cover IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(playlists, 1);
    }

    #[test]
    fn corrupt_file_is_reported_and_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("c.db");
        let garbage = vec![0x42u8; 8192];
        std::fs::write(&path, &garbage).unwrap();
        assert!(open(&path, APP_MIGRATIONS).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), garbage);
    }
}
