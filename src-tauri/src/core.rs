//! Application core: owns the databases and implements every operation the
//! interface can request. It has no dependency on the UI framework, so it is
//! tested directly against a temporary data folder.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::artwork;
use crate::db;
use crate::diagnostics::{self, FileLogger, LogFile};
use crate::error::{AppError, Result};
use crate::library::{
    self, CollectionDetail, HistoryEntry, Overview, ScanIssue, SearchResults, Track, TrackEdit,
    TrashItem,
};
use crate::paths::AppPaths;
use crate::playback::{Controller, Output, PlaybackState, Progress};
use crate::profiles::{self, DeletedProfile, Profile};
use crate::queue::Repeat;
use crate::scanner::{self, ScanSummary};
use crate::settings::{self, Settings};
use crate::watcher::FolderWatcher;

/// Called when the watched music folders change on disk.
pub type ChangeListener = Arc<dyn Fn() + Send + Sync>;
/// Unreferenced artwork younger than this is kept, so a scan in progress is safe.
const ARTWORK_MIN_AGE: Duration = Duration::from_secs(3600);
/// While playing, the position is saved every this many ticks (about 5 seconds).
const POSITION_SAVE_TICKS: u32 = 20;

const SETUP_KEY: &str = "setup_complete";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub app_version: String,
    pub setup_complete: bool,
    pub previous_session_crashed: bool,
    pub active_profile_id: String,
    pub profiles: Vec<Profile>,
    pub deleted_profiles: Vec<DeletedProfile>,
    pub settings: Settings,
    pub supported_languages: Vec<String>,
    pub data_dir: String,
    /// Folder holding cached cover images.
    pub artwork_dir: String,
    /// Messages about a database that had to be repaired at start-up.
    pub recovery_notices: Vec<String>,
}

struct Inner {
    app_db: Connection,
    profile_db: Connection,
    active_profile_id: String,
}

pub struct AppCore {
    paths: AppPaths,
    logger: Option<&'static FileLogger>,
    previous_session_crashed: bool,
    /// Lock order: `playback` before `inner`, always.
    playback: Mutex<Controller>,
    inner: Mutex<Inner>,
    /// Only one scan runs at a time.
    scanning: Mutex<()>,
    notices: Mutex<Vec<String>>,
    watcher: Mutex<Option<FolderWatcher>>,
    change_listener: Mutex<Option<ChangeListener>>,
    ticks: AtomicU32,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn repair_notice(what: &str, repair: &db::Repair) -> String {
    let lost = if repair.lost_tables.is_empty() {
        "Everything that could be read was kept.".to_string()
    } else {
        format!(
            "These parts could not be read and start empty: {}.",
            repair.lost_tables.join(", ")
        )
    };
    format!(
        "{what} was damaged and has been rebuilt. {lost} The damaged file was kept as {}. Your music files are not affected.",
        repair.kept_as
    )
}

/// Opens a profile's database for use by the app: repairs it if it is damaged
/// and that is safe, fills search keys, and empties expired Recycle Bin items.
fn open_profile_db(paths: &AppPaths, profile_id: &str) -> Result<(Connection, Option<String>)> {
    fs::create_dir_all(paths.profile_dir(profile_id)?)?;
    let (conn, repair) = db::open_or_repair(
        &paths.profile_db(profile_id)?,
        db::PROFILE_MIGRATIONS,
        &[],
        now_unix(),
    )?;
    library::backfill_search_keys(&conn)?;
    library::trash_purge_expired(&conn, now_unix())?;
    let notice = repair.map(|r| {
        log::warn!(
            "a profile database was rebuilt; original kept as {}",
            r.kept_as
        );
        repair_notice("This profile's library database", &r)
    });
    Ok((conn, notice))
}

impl AppCore {
    /// Starts the core without an audio output (used by tests).
    #[cfg(test)]
    pub fn init(root: PathBuf, logger: Option<&'static FileLogger>) -> Result<Self> {
        Self::init_with_output(root, logger, None)
    }

    pub fn init_with_output(
        root: PathBuf,
        logger: Option<&'static FileLogger>,
        output: Option<Box<dyn Output>>,
    ) -> Result<Self> {
        let paths = AppPaths::new(root)?;
        // Under `cargo test` the release rule applies so it stays tested.
        let dev_build = cfg!(debug_assertions) && !cfg!(test);
        let previous_session_crashed =
            diagnostics::begin_session(&paths.session_marker(), dev_build)?;
        if previous_session_crashed {
            log::warn!("previous session did not shut down cleanly");
        }
        diagnostics::prune_old(&paths.logs_dir(), time::OffsetDateTime::now_utc());

        let now = now_unix();
        let mut notices = Vec::new();
        // Without the profile list there is nothing safe to rebuild from.
        let (app_db, app_repair) =
            db::open_or_repair(&paths.app_db(), db::APP_MIGRATIONS, &["profiles"], now)?;
        if let Some(repair) = &app_repair {
            log::warn!(
                "the application database was rebuilt; original kept as {}",
                repair.kept_as
            );
            notices.push(repair_notice("BitNot's application database", repair));
        }
        for id in profiles::purge_expired(&app_db, now)? {
            // Only BitNot's internal data for that profile; never music files.
            if let Ok(dir) = paths.profile_dir(&id) {
                if dir.exists() && fs::remove_dir_all(&dir).is_err() {
                    log::warn!("could not remove data of an expired profile");
                }
            }
            log::info!("purged a profile deleted more than 90 days ago");
        }
        let active_profile_id = profiles::ensure_default(&app_db, now)?;
        let (profile_db, profile_notice) = open_profile_db(&paths, &active_profile_id)?;
        notices.extend(profile_notice);
        if output.is_none() {
            log::warn!("no audio output device is available; playback is disabled");
        }
        let mut controller = Controller::new(output, settings::load(&profile_db)?.volume);
        if let Err(error) = controller.restore(&profile_db) {
            log::warn!("the saved queue could not be restored: {error}");
        }
        log::info!("core started (version {})", env!("CARGO_PKG_VERSION"));

        Ok(AppCore {
            paths,
            logger,
            previous_session_crashed,
            playback: Mutex::new(controller),
            scanning: Mutex::new(()),
            notices: Mutex::new(notices),
            watcher: Mutex::new(None),
            change_listener: Mutex::new(None),
            ticks: AtomicU32::new(0),
            inner: Mutex::new(Inner {
                app_db,
                profile_db,
                active_profile_id,
            }),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        self.inner.lock().map_err(|_| {
            AppError::invalid("BitNot's internal state is unavailable. Restart the app.")
        })
    }

    fn snapshot(&self, inner: &Inner) -> Result<Bootstrap> {
        let setup_complete: Option<String> = inner
            .app_db
            .query_row(
                "SELECT value FROM app_meta WHERE key = ?1",
                [SETUP_KEY],
                |row| row.get(0),
            )
            .optional()?;
        Ok(Bootstrap {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            setup_complete: setup_complete.as_deref() == Some("1"),
            previous_session_crashed: self.previous_session_crashed,
            active_profile_id: inner.active_profile_id.clone(),
            profiles: profiles::list(&inner.app_db)?,
            deleted_profiles: profiles::list_deleted(&inner.app_db, now_unix())?,
            settings: settings::load(&inner.profile_db)?,
            supported_languages: settings::SUPPORTED_LANGUAGES
                .iter()
                .map(|l| l.to_string())
                .collect(),
            data_dir: self.paths.root().to_string_lossy().into_owned(),
            artwork_dir: self.paths.artwork_dir().to_string_lossy().into_owned(),
            recovery_notices: self.notices.lock().map(|n| n.clone()).unwrap_or_default(),
        })
    }

    pub fn bootstrap(&self) -> Result<Bootstrap> {
        let inner = self.lock()?;
        self.snapshot(&inner)
    }

    pub fn update_settings(&self, patch: &Map<String, Value>) -> Result<Settings> {
        let mut playback = self.lock_playback()?;
        let inner = self.lock()?;
        let next = settings::apply_patch(&inner.profile_db, patch, now_unix())?;
        if patch.contains_key("volume") {
            playback.set_volume(next.volume);
        }
        if patch.contains_key("musicFolders") || patch.contains_key("scanSubfolders") {
            self.refresh_watcher(&next);
        }
        Ok(next)
    }

    pub fn complete_setup(&self) -> Result<Bootstrap> {
        let inner = self.lock()?;
        inner.app_db.execute(
            "INSERT INTO app_meta (key, value) VALUES (?1, '1')
             ON CONFLICT (key) DO UPDATE SET value = '1'",
            params![SETUP_KEY],
        )?;
        log::info!("quick setup completed");
        self.snapshot(&inner)
    }

    fn activate(
        &self,
        playback: &mut Controller,
        inner: &mut Inner,
        profile_id: &str,
    ) -> Result<()> {
        if inner.active_profile_id == profile_id {
            return Ok(());
        }
        // Open the target first so a failure leaves the current profile in use.
        let (next, notice) = open_profile_db(&self.paths, profile_id)?;
        if let (Some(notice), Ok(mut notices)) = (notice, self.notices.lock()) {
            notices.push(notice);
        }
        profiles::set_active(&inner.app_db, profile_id)?;
        // Each profile has its own queue: keep the old one, load the new one.
        let _ = playback.save(&inner.profile_db);
        playback.reset();
        inner.profile_db = next;
        inner.active_profile_id = profile_id.to_string();
        let settings = settings::load(&inner.profile_db)?;
        playback.set_volume(settings.volume);
        if let Err(error) = playback.restore(&inner.profile_db) {
            log::warn!("the saved queue could not be restored: {error}");
        }
        self.refresh_watcher(&settings);
        Ok(())
    }

    /// Registers what to do when the music folders change on disk, and starts
    /// watching the active profile's folders.
    pub fn set_change_listener(&self, listener: ChangeListener) -> Result<()> {
        if let Ok(mut slot) = self.change_listener.lock() {
            *slot = Some(listener);
        }
        let settings = settings::load(&self.lock()?.profile_db)?;
        self.refresh_watcher(&settings);
        Ok(())
    }

    /// (Re)starts folder watching for the given settings. Watching is a
    /// convenience: if it cannot start, scans on start-up and on request remain.
    fn refresh_watcher(&self, settings: &Settings) {
        let listener = self.change_listener.lock().ok().and_then(|l| l.clone());
        let Ok(mut slot) = self.watcher.lock() else {
            return;
        };
        *slot = None; // stop the previous watcher first
        let Some(listener) = listener else { return };
        if settings.music_folders.is_empty() {
            return;
        }
        *slot = FolderWatcher::start(
            &settings.music_folders,
            settings.scan_subfolders,
            move || listener(),
        );
        match slot.as_ref() {
            Some(watcher) => {
                log::info!("watching {} music folder(s) for changes", watcher.watching)
            }
            None => log::warn!("music folders cannot be watched; scans still run on request"),
        }
    }

    pub fn create_profile(&self, name: &str) -> Result<Bootstrap> {
        let inner = self.lock()?;
        profiles::create(&inner.app_db, name, now_unix())?;
        log::info!("profile created");
        self.snapshot(&inner)
    }

    pub fn rename_profile(&self, id: &str, name: &str) -> Result<Bootstrap> {
        let inner = self.lock()?;
        profiles::rename(&inner.app_db, id, name)?;
        self.snapshot(&inner)
    }

    pub fn switch_profile(&self, id: &str) -> Result<Bootstrap> {
        let mut playback = self.lock_playback()?;
        let mut inner = self.lock()?;
        self.activate(&mut playback, &mut inner, id)?;
        log::info!("switched profile");
        self.snapshot(&inner)
    }

    pub fn set_main_profile(&self, id: &str) -> Result<Bootstrap> {
        let inner = self.lock()?;
        profiles::set_main(&inner.app_db, id)?;
        log::info!("main profile changed");
        self.snapshot(&inner)
    }

    pub fn delete_profile(&self, id: &str) -> Result<Bootstrap> {
        let mut playback = self.lock_playback()?;
        let mut inner = self.lock()?;
        if inner.active_profile_id == id {
            // Move off the profile before it goes to the recycle bin.
            let main = profiles::list(&inner.app_db)?
                .into_iter()
                .find(|p| p.is_main && p.id != id)
                .ok_or_else(|| {
                    AppError::invalid(
                        "The main profile cannot be deleted. Make another profile the main profile first.",
                    )
                })?;
            self.activate(&mut playback, &mut inner, &main.id)?;
        }
        profiles::delete(&inner.app_db, id, now_unix())?;
        log::info!("profile moved to recycle bin");
        self.snapshot(&inner)
    }

    pub fn restore_profile(&self, id: &str) -> Result<Bootstrap> {
        let inner = self.lock()?;
        profiles::restore(&inner.app_db, id)?;
        log::info!("profile restored");
        self.snapshot(&inner)
    }

    pub fn list_logs(&self) -> Result<Vec<LogFile>> {
        diagnostics::list(&self.paths.logs_dir())
    }

    pub fn read_log(&self, name: &str) -> Result<String> {
        diagnostics::read(&self.paths.logs_dir(), name)
    }

    pub fn clear_logs(&self) -> Result<usize> {
        diagnostics::clear(&self.paths.logs_dir(), self.logger)
    }

    // ── Library ──────────────────────────────────────────────────────────

    fn lock_playback(&self) -> Result<MutexGuard<'_, Controller>> {
        self.playback
            .lock()
            .map_err(|_| AppError::invalid("Playback is unavailable. Restart the app."))
    }

    pub fn library_overview(&self) -> Result<Overview> {
        library::overview(&self.lock()?.profile_db)
    }

    pub fn library_tracks(&self) -> Result<Vec<Track>> {
        library::all_tracks(&self.lock()?.profile_db)
    }

    pub fn library_collection(&self, id: &str) -> Result<CollectionDetail> {
        library::collection(&self.lock()?.profile_db, id)
    }

    pub fn library_favorites(&self) -> Result<Vec<Track>> {
        library::favorites(&self.lock()?.profile_db)
    }

    pub fn library_history(&self) -> Result<Vec<HistoryEntry>> {
        library::history(&self.lock()?.profile_db)
    }

    pub fn remove_history_entry(&self, id: i64) -> Result<()> {
        library::remove_history_entry(&self.lock()?.profile_db, id)
    }

    pub fn clear_history(&self) -> Result<()> {
        library::clear_history(&self.lock()?.profile_db)
    }

    pub fn library_search(&self, query: &str) -> Result<SearchResults> {
        library::search(&self.lock()?.profile_db, query)
    }

    pub fn scan_issues(&self) -> Result<Vec<ScanIssue>> {
        library::scan_issues(&self.lock()?.profile_db)
    }

    pub fn set_favorite(&self, track_id: i64, on: bool) -> Result<()> {
        library::set_favorite(&self.lock()?.profile_db, track_id, on, now_unix())
    }

    pub fn create_playlist(&self, name: &str) -> Result<String> {
        library::create_playlist(&self.lock()?.profile_db, name, now_unix())
    }

    pub fn update_playlist(&self, id: &str, name: &str, description: &str) -> Result<()> {
        library::update_playlist(&self.lock()?.profile_db, id, name, description)
    }

    /// Moves the playlist to the Recycle Bin.
    pub fn delete_playlist(&self, id: &str) -> Result<()> {
        library::delete_playlist(&self.lock()?.profile_db, id, now_unix())
    }

    fn import_image(&self, image: Option<&str>) -> Result<Option<String>> {
        match image {
            Some(path) => artwork::import_file(&self.paths.artwork_dir(), Path::new(path))
                .map(Some)
                .map_err(AppError::Invalid),
            None => Ok(None),
        }
    }

    /// Sets the playlist cover from an image file, or clears it with `None`.
    pub fn set_playlist_cover(&self, id: &str, image: Option<&str>) -> Result<()> {
        let name = self.import_image(image)?;
        library::set_playlist_cover(&self.lock()?.profile_db, id, name.as_deref())
    }

    pub fn move_in_playlist(&self, id: &str, from: usize, to: usize) -> Result<()> {
        library::move_in_playlist(&self.lock()?.profile_db, id, from, to)
    }

    /// Creates a playlist from the playing track and everything queued after it.
    pub fn save_queue_as_playlist(&self, name: &str) -> Result<String> {
        let playback = self.lock_playback()?;
        let inner = self.lock()?;
        let ids = playback.remaining();
        if ids.is_empty() {
            return Err(AppError::invalid("The queue is empty."));
        }
        let now = now_unix();
        let id = library::create_playlist(&inner.profile_db, name, now)?;
        library::add_to_playlist(&inner.profile_db, &id, &ids, now)?;
        Ok(id)
    }

    pub fn library_recent(&self) -> Result<Vec<Track>> {
        library::recent_tracks(&self.lock()?.profile_db)
    }

    pub fn track_sources(&self, id: i64) -> Result<Vec<Track>> {
        library::track_sources(&self.lock()?.profile_db, id)
    }

    pub fn prefer_source(&self, id: i64) -> Result<()> {
        library::prefer_source(&self.lock()?.profile_db, id)
    }

    pub fn genres(&self) -> Vec<String> {
        library::GENRES.iter().map(|g| g.to_string()).collect()
    }

    pub fn edit_track(&self, id: i64, edit: TrackEdit) -> Result<Track> {
        library::edit_track(&self.lock()?.profile_db, id, edit, now_unix())
    }

    /// Sets a cover chosen by the user for these tracks, or clears it.
    pub fn set_track_artwork(&self, track_ids: &[i64], image: Option<&str>) -> Result<()> {
        let name = self.import_image(image)?;
        library::set_custom_artwork(&self.lock()?.profile_db, track_ids, name.as_deref())
    }

    /// Takes tracks out of the library (to the Recycle Bin) and out of the
    /// queue. The audio files are not touched.
    pub fn remove_from_library(&self, track_ids: &[i64]) -> Result<usize> {
        let mut playback = self.lock_playback()?;
        let inner = self.lock()?;
        let removed = library::remove_from_library(&inner.profile_db, track_ids, now_unix())?;
        if playback.drop_unavailable(&inner.profile_db)? {
            let _ = playback.save(&inner.profile_db);
        }
        Ok(removed)
    }

    pub fn trash(&self) -> Result<Vec<TrashItem>> {
        library::trash(&self.lock()?.profile_db, now_unix())
    }

    pub fn trash_restore(&self, id: i64) -> Result<()> {
        library::trash_restore(&self.lock()?.profile_db, id)
    }

    pub fn trash_purge(&self, id: i64) -> Result<()> {
        library::trash_purge(&self.lock()?.profile_db, id)
    }

    pub fn trash_empty(&self) -> Result<usize> {
        library::trash_purge_all(&self.lock()?.profile_db, None)
    }

    /// Permanently deletes a profile that is already in the recycle bin,
    /// together with BitNot's internal data for it. Music files are untouched.
    pub fn purge_profile(&self, id: &str) -> Result<Bootstrap> {
        let inner = self.lock()?;
        profiles::purge(&inner.app_db, id)?;
        if let Ok(dir) = self.paths.profile_dir(id) {
            if dir.exists() && fs::remove_dir_all(&dir).is_err() {
                log::warn!("could not remove data of a purged profile");
            }
        }
        log::info!("a deleted profile was permanently removed on request");
        self.snapshot(&inner)
    }

    /// Deletes cached cover images no profile refers to any more.
    /// If any profile's database cannot be read, nothing is deleted.
    fn clean_artwork_cache(&self) -> usize {
        let mut referenced: HashSet<String> = HashSet::new();
        let Ok(entries) = fs::read_dir(self.paths.profiles_dir()) else {
            return 0;
        };
        for entry in entries.flatten() {
            let db_path = entry.path().join("profile.db");
            if !db_path.exists() {
                continue;
            }
            let names =
                Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(AppError::from)
                    .and_then(|conn| library::referenced_artwork(&conn));
            match names {
                Ok(names) => referenced.extend(names),
                Err(_) => return 0,
            }
        }
        artwork::cleanup(&self.paths.artwork_dir(), &referenced, ARTWORK_MIN_AGE)
    }

    pub fn add_to_playlist(&self, id: &str, track_ids: &[i64]) -> Result<()> {
        library::add_to_playlist(&self.lock()?.profile_db, id, track_ids, now_unix())
    }

    pub fn remove_from_playlist(&self, id: &str, index: usize) -> Result<()> {
        library::remove_from_playlist(&self.lock()?.profile_db, id, index)
    }

    /// Scans the active profile's music folders. Uses its own database
    /// connection so the interface and playback keep working meanwhile.
    pub fn scan_library(&self, progress: &mut dyn FnMut(usize, usize)) -> Result<ScanSummary> {
        let Ok(_running) = self.scanning.try_lock() else {
            return Err(AppError::invalid("A scan is already running."));
        };
        let (profile_id, folders, recursive) = {
            let inner = self.lock()?;
            let settings = settings::load(&inner.profile_db)?;
            (
                inner.active_profile_id.clone(),
                settings.music_folders,
                settings.scan_subfolders,
            )
        };
        let conn = db::open(&self.paths.profile_db(&profile_id)?, db::PROFILE_MIGRATIONS)?;
        artwork::backfill_thumbs(&self.paths.artwork_dir());
        let summary = scanner::scan(
            &conn,
            &folders,
            recursive,
            &self.paths.artwork_dir(),
            now_unix(),
            progress,
        )?;
        // Tracks that left the library must not linger in the queue.
        {
            let mut playback = self.lock_playback()?;
            let inner = self.lock()?;
            if inner.active_profile_id == profile_id
                && playback.drop_unavailable(&inner.profile_db)?
            {
                let _ = playback.save(&inner.profile_db);
            }
        }
        let cleaned = self.clean_artwork_cache();
        if cleaned > 0 {
            log::info!("removed {cleaned} unused artwork cache file(s)");
        }
        log::info!(
            "scan finished: {} added, {} updated, {} moved, {} unchanged, {} missing, {} issues",
            summary.added,
            summary.updated,
            summary.moved,
            summary.unchanged,
            summary.missing,
            summary.issues
        );
        Ok(summary)
    }

    // ── Playback ─────────────────────────────────────────────────────────

    pub fn playback_state(&self) -> Result<PlaybackState> {
        let playback = self.lock_playback()?;
        let inner = self.lock()?;
        playback.state(&inner.profile_db)
    }

    /// Runs a playback change, saves the queue, and returns the new state.
    fn with_playback(
        &self,
        change: impl FnOnce(&mut Controller, &Connection, i64) -> Result<()>,
    ) -> Result<PlaybackState> {
        let mut playback = self.lock_playback()?;
        let inner = self.lock()?;
        change(&mut playback, &inner.profile_db, now_unix())?;
        if let Err(error) = playback.save(&inner.profile_db) {
            log::warn!("the queue could not be saved: {error}");
        }
        playback.state(&inner.profile_db)
    }

    pub fn play_tracks(&self, track_ids: Vec<i64>, start: usize) -> Result<PlaybackState> {
        self.with_playback(|p, conn, now| p.play_tracks(conn, track_ids, start, now))
    }

    pub fn toggle_playback(&self) -> Result<PlaybackState> {
        self.with_playback(|p, conn, now| p.toggle(conn, now))
    }

    pub fn next_track(&self) -> Result<PlaybackState> {
        self.with_playback(|p, conn, now| p.next(conn, now))
    }

    pub fn previous_track(&self) -> Result<PlaybackState> {
        self.with_playback(|p, conn, now| p.previous(conn, now))
    }

    pub fn seek(&self, position_ms: u64) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| p.seek(position_ms))
    }

    pub fn set_shuffle(&self, on: bool) -> Result<PlaybackState> {
        self.with_playback(|p, _, now| {
            p.set_shuffle(on, now);
            Ok(())
        })
    }

    pub fn set_repeat(&self, repeat: Repeat) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| {
            p.set_repeat(repeat);
            Ok(())
        })
    }

    pub fn queue_play_next(&self, track_id: i64) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| {
            p.play_next(track_id);
            Ok(())
        })
    }

    pub fn queue_add(&self, track_id: i64) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| {
            p.add_to_queue(track_id);
            Ok(())
        })
    }

    /// Adds many tracks to the queue in one step.
    pub fn queue_add_many(&self, track_ids: Vec<i64>) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| {
            p.add_many_to_queue(&track_ids);
            Ok(())
        })
    }

    pub fn queue_move(&self, from: usize, to: usize) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| {
            p.move_upcoming(from, to);
            Ok(())
        })
    }

    /// Changes the volume at once without writing settings; the interface
    /// saves the final value when the slider comes to rest.
    pub fn set_volume_live(&self, percent: u32) -> Result<()> {
        self.lock_playback()?.set_volume(percent);
        Ok(())
    }

    pub fn queue_remove(&self, offset: usize) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| {
            p.remove_upcoming(offset);
            Ok(())
        })
    }

    pub fn queue_clear(&self) -> Result<PlaybackState> {
        self.with_playback(|p, _, _| {
            p.clear_upcoming();
            Ok(())
        })
    }

    pub fn queue_play(&self, offset: usize) -> Result<PlaybackState> {
        self.with_playback(|p, conn, now| p.play_upcoming(conn, offset, now))
    }

    /// Called a few times a second by the shell. Returns whether the track or
    /// queue changed, and the current position.
    pub fn tick(&self) -> Result<(bool, Progress)> {
        let mut playback = self.lock_playback()?;
        let inner = self.lock()?;
        let changed = playback.tick(&inner.profile_db, now_unix())?;
        let progress = playback.progress();
        let tick = self.ticks.fetch_add(1, Ordering::Relaxed);
        if changed {
            let _ = playback.save(&inner.profile_db);
        } else if progress.playing && tick.is_multiple_of(POSITION_SAVE_TICKS) {
            // So a crash loses at most a few seconds of position.
            let _ = playback.save_position(&inner.profile_db);
        }
        Ok((changed, progress))
    }

    /// Records a clean shutdown so the next start does not report a crash.
    pub fn shutdown(&self) {
        if let (Ok(mut playback), Ok(inner)) = (self.playback.lock(), self.inner.lock()) {
            let _ = playback.save(&inner.profile_db);
        }
        log::info!("clean shutdown");
        diagnostics::end_session(&self.paths.session_marker());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn patch(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn first_start_then_restart_persists_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("BitNot");
        {
            let core = AppCore::init(root.clone(), None).unwrap();
            let boot = core.bootstrap().unwrap();
            assert!(!boot.setup_complete);
            assert!(!boot.previous_session_crashed);
            assert_eq!(boot.profiles.len(), 1);
            assert_eq!(boot.settings, Settings::default());

            core.update_settings(&patch(json!({"theme": "light", "accentColor": "#AA33CC"})))
                .unwrap();
            assert!(core.complete_setup().unwrap().setup_complete);
            core.shutdown();
        }
        let core = AppCore::init(root, None).unwrap();
        let boot = core.bootstrap().unwrap();
        assert!(boot.setup_complete);
        assert!(!boot.previous_session_crashed);
        assert_eq!(boot.settings.theme, settings::Theme::Light);
        assert_eq!(boot.settings.accent_color, "#AA33CC");
        assert_eq!(boot.profiles.len(), 1);
    }

    #[test]
    fn unclean_exit_is_reported_once() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        drop(AppCore::init(root.clone(), None).unwrap()); // no shutdown()
        let core = AppCore::init(root.clone(), None).unwrap();
        assert!(core.bootstrap().unwrap().previous_session_crashed);
        core.shutdown();
        drop(core);
        let core = AppCore::init(root, None).unwrap();
        assert!(!core.bootstrap().unwrap().previous_session_crashed);
    }

    #[test]
    fn each_profile_has_its_own_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::init(tmp.path().to_path_buf(), None).unwrap();
        let main_id = core.bootstrap().unwrap().active_profile_id;
        core.update_settings(&patch(json!({"accentColor": "#111111"})))
            .unwrap();

        let boot = core.create_profile("Guest").unwrap();
        let guest = boot
            .profiles
            .iter()
            .find(|p| p.name == "Guest")
            .unwrap()
            .id
            .clone();
        assert_eq!(boot.active_profile_id, main_id, "creating does not switch");

        let boot = core.switch_profile(&guest).unwrap();
        assert_eq!(boot.active_profile_id, guest);
        assert_eq!(boot.settings.accent_color, settings::DEFAULT_ACCENT);
        core.update_settings(&patch(json!({"accentColor": "#222222"})))
            .unwrap();

        let boot = core.switch_profile(&main_id).unwrap();
        assert_eq!(boot.settings.accent_color, "#111111");
        assert!(core.switch_profile("not-a-profile").is_err());
        assert_eq!(core.bootstrap().unwrap().active_profile_id, main_id);
    }

    #[test]
    fn deleting_the_active_profile_switches_to_main_and_restore_keeps_data() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::init(tmp.path().to_path_buf(), None).unwrap();
        let main_id = core.bootstrap().unwrap().active_profile_id;
        assert!(core.delete_profile(&main_id).is_err());

        let boot = core.create_profile("Guest").unwrap();
        let guest = boot
            .profiles
            .iter()
            .find(|p| p.name == "Guest")
            .unwrap()
            .id
            .clone();
        core.switch_profile(&guest).unwrap();
        core.update_settings(&patch(json!({"displayScale": 125})))
            .unwrap();

        let boot = core.delete_profile(&guest).unwrap();
        assert_eq!(boot.active_profile_id, main_id);
        assert_eq!(boot.profiles.len(), 1);
        assert_eq!(boot.deleted_profiles.len(), 1);
        assert_eq!(boot.deleted_profiles[0].days_remaining, 90);

        let boot = core.restore_profile(&guest).unwrap();
        assert_eq!(boot.profiles.len(), 2);
        assert!(boot.deleted_profiles.is_empty());
        let boot = core.switch_profile(&guest).unwrap();
        assert_eq!(boot.settings.display_scale, 125);
    }

    #[test]
    fn main_can_be_replaced_then_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::init(tmp.path().to_path_buf(), None).unwrap();
        let old_main = core.bootstrap().unwrap().active_profile_id;
        let boot = core.create_profile("Next").unwrap();
        let next = boot
            .profiles
            .iter()
            .find(|p| p.name == "Next")
            .unwrap()
            .id
            .clone();

        core.set_main_profile(&next).unwrap();
        let boot = core.delete_profile(&old_main).unwrap();
        assert_eq!(boot.active_profile_id, next);
        assert!(boot.profiles.iter().all(|p| p.is_main));
    }

    #[test]
    fn scanning_fills_the_library_and_removing_the_folder_empties_it() {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        fs::create_dir_all(music.join("Album")).unwrap();
        crate::scanner::tests::write_wav(&music.join("one.wav"), 1);
        crate::scanner::tests::write_wav(&music.join("Album").join("two.wav"), 1);
        crate::scanner::tests::tag_wav(&music.join("one.wav"), "One", "Artist", "Record", 1);

        let core = AppCore::init(tmp.path().join("data"), None).unwrap();
        let folder = music.to_string_lossy().into_owned();
        core.update_settings(&patch(json!({"musicFolders": [folder]})))
            .unwrap();

        // Subfolders are excluded unless the setting is on.
        assert_eq!(core.scan_library(&mut |_, _| {}).unwrap().added, 1);
        core.update_settings(&patch(json!({"scanSubfolders": true})))
            .unwrap();
        assert_eq!(core.scan_library(&mut |_, _| {}).unwrap().added, 1);

        let overview = core.library_overview().unwrap();
        assert_eq!(overview.track_count, 2);
        assert!(overview.albums.iter().any(|a| a.title == "Record"));
        assert_eq!(core.library_search("one").unwrap().songs.len(), 1);

        // Queue a track, then remove the folder: library and queue both empty.
        let id = core.library_tracks().unwrap()[0].id;
        core.queue_add(id).unwrap();
        assert_eq!(core.playback_state().unwrap().next_up.len(), 1);
        core.update_settings(&patch(json!({"musicFolders": []})))
            .unwrap();
        core.scan_library(&mut |_, _| {}).unwrap();
        assert_eq!(core.library_overview().unwrap().track_count, 0);
        assert!(core.playback_state().unwrap().next_up.is_empty());
        assert!(music.join("one.wav").exists(), "music files are untouched");
    }

    #[test]
    fn queue_can_be_saved_as_a_playlist_and_removed_tracks_leave_it() {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        fs::create_dir_all(&music).unwrap();
        for (name, seconds) in [("a", 1), ("b", 2), ("c", 3)] {
            crate::scanner::tests::write_wav(&music.join(format!("{name}.wav")), seconds);
        }
        let core = AppCore::init(tmp.path().join("data"), None).unwrap();
        let folder = music.to_string_lossy().into_owned();
        core.update_settings(&patch(json!({"musicFolders": [folder]})))
            .unwrap();
        core.scan_library(&mut |_, _| {}).unwrap();
        let ids: Vec<i64> = core
            .library_tracks()
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect();

        assert!(core.save_queue_as_playlist("Empty").is_err());
        core.queue_add_many(ids.clone()).unwrap();
        core.queue_move(2, 0).unwrap();
        let playlist = core.save_queue_as_playlist("From queue").unwrap();
        let saved: Vec<i64> = core
            .library_collection(&playlist)
            .unwrap()
            .tracks
            .iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(saved, vec![ids[2], ids[0], ids[1]], "queue order is kept");

        assert_eq!(core.remove_from_library(&[ids[2]]).unwrap(), 1);
        assert_eq!(core.playback_state().unwrap().next_up.len(), 2);
        assert_eq!(core.library_overview().unwrap().track_count, 2);
        assert!(music.join("c.wav").exists(), "the audio file is untouched");

        // Restoring from the Recycle Bin brings it back.
        let bin = core.trash().unwrap();
        assert_eq!(bin.len(), 1);
        core.trash_restore(bin[0].id).unwrap();
        assert_eq!(core.library_overview().unwrap().track_count, 3);
    }

    #[test]
    fn a_deleted_profile_can_be_purged_early_with_its_internal_data_only() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::init(tmp.path().to_path_buf(), None).unwrap();
        let boot = core.create_profile("Guest").unwrap();
        let guest = boot
            .profiles
            .iter()
            .find(|p| p.name == "Guest")
            .unwrap()
            .id
            .clone();
        core.switch_profile(&guest).unwrap();
        let guest_dir = tmp.path().join("profiles").join(&guest);
        assert!(guest_dir.exists());

        assert!(
            core.purge_profile(&guest).is_err(),
            "must be in the recycle bin first"
        );
        core.delete_profile(&guest).unwrap();
        let boot = core.purge_profile(&guest).unwrap();
        assert!(boot.deleted_profiles.is_empty());
        assert!(!guest_dir.exists());
        assert_eq!(boot.profiles.len(), 1);
    }

    #[test]
    fn unused_artwork_is_cleaned_only_when_no_profile_refers_to_it() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::init(tmp.path().to_path_buf(), None).unwrap();
        let cache = tmp.path().join("artwork");
        let used = artwork::store(&cache, &artwork::tests::png(50, 50, 1)).unwrap();
        let unused = artwork::store(&cache, &artwork::tests::png(50, 50, 2)).unwrap();
        let playlist = core.create_playlist("Covered").unwrap();
        library::set_playlist_cover(&core.lock().unwrap().profile_db, &playlist, Some(&used))
            .unwrap();

        // Too new to be removed by the automatic clean-up.
        assert_eq!(core.clean_artwork_cache(), 0);
        let referenced = library::referenced_artwork(&core.lock().unwrap().profile_db).unwrap();
        assert_eq!(artwork::cleanup(&cache, &referenced, Duration::ZERO), 2);
        assert!(cache.join(&used).exists());
        assert!(!cache.join(&unused).exists());
    }

    #[test]
    fn a_damaged_profile_database_is_rebuilt_at_startup_and_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let profile_db = {
            let core = AppCore::init(root.clone(), None).unwrap();
            core.update_settings(&patch(json!({"theme": "light"})))
                .unwrap();
            let id = core.bootstrap().unwrap().active_profile_id;
            {
                let inner = core.lock().unwrap();
                for i in 0..600 {
                    inner
                        .profile_db
                        .execute(
                            "INSERT INTO tracks (path, folder, title, artist, album, album_artist,
                                 duration_ms, file_size, file_mtime, added_at)
                             VALUES (?1, 'f', 't', 'a', 'b', 'a', 1, 1, 1, 1)",
                            [format!("C:/m/{i:04}-{}", "x".repeat(200))],
                        )
                        .unwrap();
                }
                inner
                    .profile_db
                    .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                    .unwrap();
            }
            core.shutdown();
            root.join("profiles").join(id).join("profile.db")
        };
        let mut bytes = fs::read(&profile_db).unwrap();
        let middle = bytes.len() / 2 / 4096 * 4096;
        for byte in &mut bytes[middle..middle + 4096 * 3] {
            *byte = 0xA5;
        }
        fs::write(&profile_db, &bytes).unwrap();

        let core = AppCore::init(root, None).unwrap();
        let boot = core.bootstrap().unwrap();
        assert_eq!(boot.recovery_notices.len(), 1);
        assert!(boot.recovery_notices[0].contains("damaged-"));
        assert_eq!(
            boot.settings.theme,
            settings::Theme::Light,
            "readable data survives"
        );
        let kept = fs::read_dir(profile_db.parent().unwrap())
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().contains(".damaged-"))
            .expect("the damaged original is kept");
        assert_eq!(fs::read(kept.path()).unwrap(), bytes);
    }

    #[test]
    fn corrupt_app_database_fails_startup_without_overwriting() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let garbage = vec![7u8; 4096];
        fs::write(root.join("app.db"), &garbage).unwrap();
        assert!(AppCore::init(root.clone(), None).is_err());
        assert_eq!(fs::read(root.join("app.db")).unwrap(), garbage);
    }
}
