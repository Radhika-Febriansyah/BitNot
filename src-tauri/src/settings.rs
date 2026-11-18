//! Typed per-profile settings, stored one row per field so a single damaged
//! value never takes the rest of the settings with it.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{AppError, Result};

/// Languages with a complete translation in this build.
pub const SUPPORTED_LANGUAGES: &[&str] = &["en"];
pub const DEFAULT_ACCENT: &str = "#36C5F0";
pub const MIN_SCALE: u32 = 80;
pub const MAX_SCALE: u32 = 150;
const MAX_MUSIC_FOLDERS: usize = 64;
const MAX_PATH_CHARS: usize = 1024;
const MAX_RECENT_SEARCHES: usize = 5;
const MAX_SEARCH_CHARS: usize = 200;
const MAX_LAST_PAGE_BYTES: usize = 600;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MotionPreference {
    /// Follow the Windows animation setting.
    System,
    Reduce,
    Full,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LibraryView {
    Grid,
    List,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub language: String,
    pub theme: Theme,
    /// `#RRGGBB`
    pub accent_color: String,
    /// Interface scale in percent.
    pub display_scale: u32,
    pub reduce_motion: MotionPreference,
    /// Folders the user chose for their music. Stored only; never modified.
    pub music_folders: Vec<String>,
    /// Also scan folders inside the music folders. Off by default (CLAUDE.md §7).
    pub scan_subfolders: bool,
    /// Playback volume, 0–100.
    pub volume: u32,
    /// The latest searches, newest first (at most five).
    pub recent_searches: Vec<String>,
    /// How albums, artists, and playlists are listed.
    pub library_view: LibraryView,
    /// The page that was open last, restored on the next start. Opaque to the core.
    pub last_page: String,
    pub queue_panel_open: bool,
    pub sidebar_collapsed: bool,
    pub guide_dismissed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "en".into(),
            theme: Theme::Dark,
            accent_color: DEFAULT_ACCENT.into(),
            display_scale: 100,
            reduce_motion: MotionPreference::System,
            music_folders: Vec::new(),
            scan_subfolders: false,
            volume: 80,
            recent_searches: Vec::new(),
            library_view: LibraryView::Grid,
            last_page: String::new(),
            queue_panel_open: true,
            sidebar_collapsed: false,
            guide_dismissed: false,
        }
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn normalise(settings: &mut Settings) {
    settings.accent_color = settings.accent_color.to_ascii_uppercase();
    let mut searches: Vec<String> = Vec::new();
    for query in &settings.recent_searches {
        let query: String = query.trim().chars().take(MAX_SEARCH_CHARS).collect();
        if !query.is_empty() && !searches.iter().any(|s| s.eq_ignore_ascii_case(&query)) {
            searches.push(query);
        }
    }
    searches.truncate(MAX_RECENT_SEARCHES);
    settings.recent_searches = searches;
    let mut seen: Vec<String> = Vec::new();
    settings.music_folders.retain(|folder| {
        let key = folder
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
            .replace('/', "\\");
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
}

fn validate(settings: &Settings) -> Result<()> {
    if !SUPPORTED_LANGUAGES.contains(&settings.language.as_str()) {
        return Err(AppError::invalid("That language is not available yet."));
    }
    if !is_hex_color(&settings.accent_color) {
        return Err(AppError::invalid(
            "The accent colour must be a hex colour such as #36C5F0.",
        ));
    }
    if !(MIN_SCALE..=MAX_SCALE).contains(&settings.display_scale) {
        return Err(AppError::invalid(format!(
            "Display scale must be between {MIN_SCALE}% and {MAX_SCALE}%."
        )));
    }
    if settings.last_page.len() > MAX_LAST_PAGE_BYTES {
        return Err(AppError::invalid("The saved page is too long."));
    }
    if settings.volume > 100 {
        return Err(AppError::invalid("Volume must be between 0 and 100."));
    }
    if settings.music_folders.len() > MAX_MUSIC_FOLDERS {
        return Err(AppError::invalid(format!(
            "At most {MAX_MUSIC_FOLDERS} music folders can be added."
        )));
    }
    for folder in &settings.music_folders {
        if folder.chars().count() > MAX_PATH_CHARS || !Path::new(folder).is_absolute() {
            return Err(AppError::invalid(
                "Music folders must be full folder paths.",
            ));
        }
    }
    Ok(())
}

fn to_map(settings: &Settings) -> Result<Map<String, Value>> {
    match serde_json::to_value(settings)? {
        Value::Object(map) => Ok(map),
        _ => Err(AppError::invalid("Settings could not be serialised.")),
    }
}

fn from_map(map: Map<String, Value>) -> Result<Settings> {
    let mut settings: Settings = serde_json::from_value(Value::Object(map))
        .map_err(|e| AppError::invalid(format!("Invalid setting value: {e}")))?;
    normalise(&mut settings);
    validate(&settings)?;
    Ok(settings)
}

/// Loads settings, falling back to the default for any field whose stored
/// value is missing, unreadable, or no longer valid.
pub fn load(conn: &Connection) -> Result<Settings> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut current = Settings::default();
    for (key, raw) in rows {
        let mut candidate = to_map(&current)?;
        if !candidate.contains_key(&key) {
            continue; // written by another version; leave it alone
        }
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            log::warn!("setting '{key}' is unreadable; using default");
            continue;
        };
        candidate.insert(key.clone(), value);
        match from_map(candidate) {
            Ok(next) => current = next,
            Err(_) => log::warn!("setting '{key}' is invalid; using default"),
        }
    }
    Ok(current)
}

/// Applies a partial update. Nothing is written unless the whole result is valid.
pub fn apply_patch(conn: &Connection, patch: &Map<String, Value>, now: i64) -> Result<Settings> {
    let mut merged = to_map(&load(conn)?)?;
    for (key, value) in patch {
        if !merged.contains_key(key) {
            return Err(AppError::invalid(format!("Unknown setting '{key}'.")));
        }
        merged.insert(key.clone(), value.clone());
    }
    let next = from_map(merged)?;
    let stored = to_map(&next)?;

    let tx = conn.unchecked_transaction()?;
    for key in patch.keys() {
        let value = serde_json::to_string(&stored[key])?;
        tx.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now],
        )?;
    }
    tx.commit()?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use serde_json::json;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::migrate(&conn, db::PROFILE_MIGRATIONS).unwrap();
        conn
    }

    fn patch(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn abs(name: &str) -> String {
        if cfg!(windows) {
            format!("C:\\Music\\{name}")
        } else {
            format!("/music/{name}")
        }
    }

    #[test]
    fn empty_database_yields_brand_defaults() {
        let s = load(&conn()).unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.accent_color, "#36C5F0");
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn patch_persists_and_leaves_other_fields() {
        let conn = conn();
        let s = apply_patch(
            &conn,
            &patch(json!({"theme": "light", "accentColor": "#ff8800", "displayScale": 125})),
            1,
        )
        .unwrap();
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.accent_color, "#FF8800");
        assert_eq!(s.display_scale, 125);
        assert!(s.queue_panel_open);
        assert_eq!(load(&conn).unwrap(), s);
    }

    #[test]
    fn invalid_patches_are_rejected_without_writing() {
        let conn = conn();
        for bad in [
            json!({"accentColor": "blue"}),
            json!({"accentColor": "#12345"}),
            json!({"displayScale": 300}),
            json!({"displayScale": "big"}),
            json!({"theme": "sepia"}),
            json!({"language": "xx"}),
            json!({"musicFolders": ["relative\\path"]}),
            json!({"notASetting": true}),
            // One bad field rejects the whole patch.
            json!({"theme": "light", "displayScale": 10}),
        ] {
            assert!(apply_patch(&conn, &patch(bad.clone()), 1).is_err(), "{bad}");
        }
        assert_eq!(load(&conn).unwrap(), Settings::default());
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn music_folders_are_deduplicated_ignoring_case_and_trailing_slash() {
        let conn = conn();
        let a = abs("Rock");
        let s = apply_patch(
            &conn,
            &patch(json!({"musicFolders": [a, a.to_uppercase(), format!("{a}\\"), abs("Jazz")]})),
            1,
        )
        .unwrap();
        assert_eq!(s.music_folders, vec![abs("Rock"), abs("Jazz")]);
    }

    #[test]
    fn recent_searches_keep_five_unique_and_volume_is_bounded() {
        let conn = conn();
        let s = apply_patch(
            &conn,
            &patch(json!({"recentSearches": [" jazz ", "JAZZ", "a", "b", "c", "d", "e", ""], "volume": 35})),
            1,
        )
        .unwrap();
        assert_eq!(s.recent_searches, vec!["jazz", "a", "b", "c", "d"]);
        assert_eq!(s.volume, 35);
        assert!(!s.scan_subfolders);
        assert!(apply_patch(&conn, &patch(json!({"volume": 101})), 1).is_err());
        let s = apply_patch(
            &conn,
            &patch(json!({"libraryView": "list", "lastPage": "{}"})),
            1,
        )
        .unwrap();
        assert_eq!(s.library_view, LibraryView::List);
        assert!(apply_patch(&conn, &patch(json!({"libraryView": "table"})), 1).is_err());
        assert!(apply_patch(&conn, &patch(json!({"lastPage": "x".repeat(601)})), 1).is_err());
    }

    #[test]
    fn damaged_rows_fall_back_per_field() {
        let conn = conn();
        apply_patch(
            &conn,
            &patch(json!({"theme": "light", "displayScale": 110})),
            1,
        )
        .unwrap();
        conn.execute(
            "UPDATE settings SET value = 'not json' WHERE key = 'theme'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('accentColor', '\"red\"', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('fromTheFuture', '1', 1)",
            [],
        )
        .unwrap();
        let s = load(&conn).unwrap();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.accent_color, DEFAULT_ACCENT);
        assert_eq!(s.display_scale, 110);
    }
}
