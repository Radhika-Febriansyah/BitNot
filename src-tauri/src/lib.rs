//! BitNot desktop shell. This file only wires the application core to the
//! window: every command is a thin wrapper around [`core::AppCore`].

mod artwork;
mod core;
mod db;
mod diagnostics;
mod error;
mod library;
mod paths;
mod playback;
mod profiles;
mod queue;
mod scanner;
mod settings;
mod textfold;
mod watcher;

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

use crate::core::{AppCore, Bootstrap};
use crate::diagnostics::LogFile;
use crate::error::{AppError, Result};
use crate::library::{
    CollectionDetail, HistoryEntry, Overview, ScanIssue, SearchResults, Track, TrackEdit, TrashItem,
};
use crate::playback::{DeviceOutput, Output, PlaybackState};
use crate::queue::Repeat;
use crate::scanner::ScanSummary;
use crate::settings::Settings;

const EVENT_PROGRESS: &str = "playback:progress";
const EVENT_PLAYBACK_CHANGED: &str = "playback:changed";
/// Payload: which part of the library changed, so views reload only what they show.
const EVENT_LIBRARY_CHANGED: &str = "library:changed";
const EVENT_SCAN_PROGRESS: &str = "scan:progress";
const EVENT_SCAN_FINISHED: &str = "scan:finished";
const TICK: Duration = Duration::from_millis(250);
/// Folder changes are scanned once things have been quiet for this long.
const WATCH_QUIET: Duration = Duration::from_millis(1500);

const SCOPE_TRACKS: &str = "tracks";
const SCOPE_PLAYLISTS: &str = "playlists";
const SCOPE_FAVORITES: &str = "favorites";
const SCOPE_HISTORY: &str = "history";
const SCOPE_TRASH: &str = "trash";

/// The core, or the reason it could not start. Keeping the failure lets the
/// window open and explain the problem instead of the app vanishing.
struct Managed(std::result::Result<Arc<AppCore>, String>);

fn core_of(managed: &Managed) -> Result<&AppCore> {
    managed
        .0
        .as_deref()
        .map_err(|message| AppError::Startup(message.clone()))
}

fn changed(app: &AppHandle, scopes: &[&str]) {
    for scope in scopes {
        let _ = app.emit(EVENT_LIBRARY_CHANGED, *scope);
    }
}

fn playback_changed(app: &AppHandle) {
    let _ = app.emit(EVENT_PLAYBACK_CHANGED, ());
}

// ── App, settings, profiles ──────────────────────────────────────────────

#[tauri::command(async)]
fn bootstrap(state: State<'_, Managed>) -> Result<Bootstrap> {
    core_of(&state)?.bootstrap()
}

#[tauri::command(async)]
fn settings_update(state: State<'_, Managed>, patch: Map<String, Value>) -> Result<Settings> {
    core_of(&state)?.update_settings(&patch)
}

#[tauri::command(async)]
fn setup_complete(state: State<'_, Managed>) -> Result<Bootstrap> {
    core_of(&state)?.complete_setup()
}

#[tauri::command(async)]
fn profile_create(state: State<'_, Managed>, name: String) -> Result<Bootstrap> {
    core_of(&state)?.create_profile(&name)
}

#[tauri::command(async)]
fn profile_rename(state: State<'_, Managed>, id: String, name: String) -> Result<Bootstrap> {
    core_of(&state)?.rename_profile(&id, &name)
}

#[tauri::command(async)]
fn profile_switch(state: State<'_, Managed>, id: String) -> Result<Bootstrap> {
    core_of(&state)?.switch_profile(&id)
}

#[tauri::command(async)]
fn profile_set_main(state: State<'_, Managed>, id: String) -> Result<Bootstrap> {
    core_of(&state)?.set_main_profile(&id)
}

#[tauri::command(async)]
fn profile_delete(state: State<'_, Managed>, id: String) -> Result<Bootstrap> {
    core_of(&state)?.delete_profile(&id)
}

#[tauri::command(async)]
fn profile_restore(state: State<'_, Managed>, id: String) -> Result<Bootstrap> {
    core_of(&state)?.restore_profile(&id)
}

#[tauri::command(async)]
fn profile_purge(state: State<'_, Managed>, id: String) -> Result<Bootstrap> {
    core_of(&state)?.purge_profile(&id)
}

#[tauri::command(async)]
fn diagnostics_list(state: State<'_, Managed>) -> Result<Vec<LogFile>> {
    core_of(&state)?.list_logs()
}

#[tauri::command(async)]
fn diagnostics_read(state: State<'_, Managed>, name: String) -> Result<String> {
    core_of(&state)?.read_log(&name)
}

#[tauri::command(async)]
fn diagnostics_clear(state: State<'_, Managed>) -> Result<usize> {
    core_of(&state)?.clear_logs()
}

// ── Library ──────────────────────────────────────────────────────────────

#[tauri::command(async)]
fn library_overview(state: State<'_, Managed>) -> Result<Overview> {
    core_of(&state)?.library_overview()
}

#[tauri::command(async)]
fn library_tracks(state: State<'_, Managed>) -> Result<Vec<Track>> {
    core_of(&state)?.library_tracks()
}

#[tauri::command(async)]
fn library_recent(state: State<'_, Managed>) -> Result<Vec<Track>> {
    core_of(&state)?.library_recent()
}

#[tauri::command(async)]
fn library_collection(state: State<'_, Managed>, id: String) -> Result<CollectionDetail> {
    core_of(&state)?.library_collection(&id)
}

#[tauri::command(async)]
fn library_favorites(state: State<'_, Managed>) -> Result<Vec<Track>> {
    core_of(&state)?.library_favorites()
}

#[tauri::command(async)]
fn library_history(state: State<'_, Managed>) -> Result<Vec<HistoryEntry>> {
    core_of(&state)?.library_history()
}

#[tauri::command(async)]
fn history_remove(app: AppHandle, state: State<'_, Managed>, id: i64) -> Result<()> {
    core_of(&state)?.remove_history_entry(id)?;
    changed(&app, &[SCOPE_HISTORY]);
    Ok(())
}

#[tauri::command(async)]
fn history_clear(app: AppHandle, state: State<'_, Managed>) -> Result<()> {
    core_of(&state)?.clear_history()?;
    changed(&app, &[SCOPE_HISTORY]);
    Ok(())
}

#[tauri::command(async)]
fn library_search(state: State<'_, Managed>, query: String) -> Result<SearchResults> {
    core_of(&state)?.library_search(&query)
}

#[tauri::command(async)]
fn library_issues(state: State<'_, Managed>) -> Result<Vec<ScanIssue>> {
    core_of(&state)?.scan_issues()
}

#[tauri::command(async)]
fn library_genres(state: State<'_, Managed>) -> Result<Vec<String>> {
    Ok(core_of(&state)?.genres())
}

#[tauri::command(async)]
fn track_sources(state: State<'_, Managed>, id: i64) -> Result<Vec<Track>> {
    core_of(&state)?.track_sources(id)
}

#[tauri::command(async)]
fn track_prefer_source(app: AppHandle, state: State<'_, Managed>, id: i64) -> Result<()> {
    core_of(&state)?.prefer_source(id)?;
    changed(&app, &[SCOPE_TRACKS]);
    Ok(())
}

#[tauri::command(async)]
fn track_edit(
    app: AppHandle,
    state: State<'_, Managed>,
    id: i64,
    edit: TrackEdit,
) -> Result<Track> {
    let track = core_of(&state)?.edit_track(id, edit)?;
    changed(&app, &[SCOPE_TRACKS, SCOPE_TRASH]);
    playback_changed(&app);
    Ok(track)
}

#[tauri::command(async)]
fn track_set_artwork(
    app: AppHandle,
    state: State<'_, Managed>,
    track_ids: Vec<i64>,
    image: Option<String>,
) -> Result<()> {
    core_of(&state)?.set_track_artwork(&track_ids, image.as_deref())?;
    changed(&app, &[SCOPE_TRACKS]);
    playback_changed(&app);
    Ok(())
}

#[tauri::command(async)]
fn tracks_remove(app: AppHandle, state: State<'_, Managed>, track_ids: Vec<i64>) -> Result<usize> {
    let removed = core_of(&state)?.remove_from_library(&track_ids)?;
    changed(&app, &[SCOPE_TRACKS, SCOPE_TRASH]);
    playback_changed(&app);
    Ok(removed)
}

#[derive(Clone, Serialize)]
struct ScanProgress {
    done: usize,
    total: usize,
}

/// Runs a scan and tells the window about progress and the result.
fn run_scan(app: &AppHandle, core: &AppCore) -> Result<ScanSummary> {
    let mut last = Instant::now() - Duration::from_secs(1);
    let summary = core.scan_library(&mut |done, total| {
        if done == total || last.elapsed() >= Duration::from_millis(150) {
            last = Instant::now();
            let _ = app.emit(EVENT_SCAN_PROGRESS, ScanProgress { done, total });
        }
    })?;
    changed(app, &[SCOPE_TRACKS]);
    playback_changed(app);
    Ok(summary)
}

#[tauri::command(async)]
fn library_scan(app: AppHandle, state: State<'_, Managed>) -> Result<ScanSummary> {
    run_scan(&app, core_of(&state)?)
}

#[tauri::command(async)]
fn favorite_set(app: AppHandle, state: State<'_, Managed>, track_id: i64, on: bool) -> Result<()> {
    core_of(&state)?.set_favorite(track_id, on)?;
    changed(&app, &[SCOPE_FAVORITES, SCOPE_TRASH]);
    Ok(())
}

#[tauri::command(async)]
fn playlist_create(app: AppHandle, state: State<'_, Managed>, name: String) -> Result<String> {
    let id = core_of(&state)?.create_playlist(&name)?;
    changed(&app, &[SCOPE_PLAYLISTS]);
    Ok(id)
}

#[tauri::command(async)]
fn playlist_update(
    app: AppHandle,
    state: State<'_, Managed>,
    id: String,
    name: String,
    description: String,
) -> Result<()> {
    core_of(&state)?.update_playlist(&id, &name, &description)?;
    changed(&app, &[SCOPE_PLAYLISTS]);
    Ok(())
}

#[tauri::command(async)]
fn playlist_set_cover(
    app: AppHandle,
    state: State<'_, Managed>,
    id: String,
    image: Option<String>,
) -> Result<()> {
    core_of(&state)?.set_playlist_cover(&id, image.as_deref())?;
    changed(&app, &[SCOPE_PLAYLISTS]);
    Ok(())
}

#[tauri::command(async)]
fn playlist_delete(app: AppHandle, state: State<'_, Managed>, id: String) -> Result<()> {
    core_of(&state)?.delete_playlist(&id)?;
    changed(&app, &[SCOPE_PLAYLISTS, SCOPE_TRASH]);
    Ok(())
}

#[tauri::command(async)]
fn playlist_add(
    app: AppHandle,
    state: State<'_, Managed>,
    id: String,
    track_ids: Vec<i64>,
) -> Result<()> {
    core_of(&state)?.add_to_playlist(&id, &track_ids)?;
    changed(&app, &[SCOPE_PLAYLISTS]);
    Ok(())
}

#[tauri::command(async)]
fn playlist_remove(
    app: AppHandle,
    state: State<'_, Managed>,
    id: String,
    index: usize,
) -> Result<()> {
    core_of(&state)?.remove_from_playlist(&id, index)?;
    changed(&app, &[SCOPE_PLAYLISTS]);
    Ok(())
}

#[tauri::command(async)]
fn playlist_move(
    app: AppHandle,
    state: State<'_, Managed>,
    id: String,
    from: usize,
    to: usize,
) -> Result<()> {
    core_of(&state)?.move_in_playlist(&id, from, to)?;
    changed(&app, &[SCOPE_PLAYLISTS]);
    Ok(())
}

#[tauri::command(async)]
fn playlist_from_queue(app: AppHandle, state: State<'_, Managed>, name: String) -> Result<String> {
    let id = core_of(&state)?.save_queue_as_playlist(&name)?;
    changed(&app, &[SCOPE_PLAYLISTS]);
    Ok(id)
}

// ── Recycle Bin ──────────────────────────────────────────────────────────

const ALL_SCOPES: &[&str] = &[
    SCOPE_TRACKS,
    SCOPE_PLAYLISTS,
    SCOPE_FAVORITES,
    SCOPE_HISTORY,
    SCOPE_TRASH,
];

#[tauri::command(async)]
fn trash_list(state: State<'_, Managed>) -> Result<Vec<TrashItem>> {
    core_of(&state)?.trash()
}

#[tauri::command(async)]
fn trash_restore(app: AppHandle, state: State<'_, Managed>, id: i64) -> Result<()> {
    core_of(&state)?.trash_restore(id)?;
    changed(&app, ALL_SCOPES);
    playback_changed(&app);
    Ok(())
}

#[tauri::command(async)]
fn trash_purge(app: AppHandle, state: State<'_, Managed>, id: i64) -> Result<()> {
    core_of(&state)?.trash_purge(id)?;
    changed(&app, ALL_SCOPES);
    Ok(())
}

#[tauri::command(async)]
fn trash_empty(app: AppHandle, state: State<'_, Managed>) -> Result<usize> {
    let removed = core_of(&state)?.trash_empty()?;
    changed(&app, ALL_SCOPES);
    Ok(removed)
}

// ── Playback ─────────────────────────────────────────────────────────────

#[tauri::command(async)]
fn playback_state(state: State<'_, Managed>) -> Result<PlaybackState> {
    core_of(&state)?.playback_state()
}

/// A new track starting changes play counts and history.
fn after_start(app: &AppHandle, result: Result<PlaybackState>) -> Result<PlaybackState> {
    if result.is_ok() {
        changed(app, &[SCOPE_HISTORY]);
    }
    result
}

#[tauri::command(async)]
fn play_tracks(
    app: AppHandle,
    state: State<'_, Managed>,
    track_ids: Vec<i64>,
    start: usize,
) -> Result<PlaybackState> {
    after_start(&app, core_of(&state)?.play_tracks(track_ids, start))
}

#[tauri::command(async)]
fn playback_toggle(app: AppHandle, state: State<'_, Managed>) -> Result<PlaybackState> {
    after_start(&app, core_of(&state)?.toggle_playback())
}

#[tauri::command(async)]
fn playback_next(app: AppHandle, state: State<'_, Managed>) -> Result<PlaybackState> {
    after_start(&app, core_of(&state)?.next_track())
}

#[tauri::command(async)]
fn playback_previous(app: AppHandle, state: State<'_, Managed>) -> Result<PlaybackState> {
    after_start(&app, core_of(&state)?.previous_track())
}

#[tauri::command(async)]
fn playback_seek(state: State<'_, Managed>, position_ms: u64) -> Result<PlaybackState> {
    core_of(&state)?.seek(position_ms)
}

#[tauri::command(async)]
fn playback_set_volume(state: State<'_, Managed>, percent: u32) -> Result<()> {
    core_of(&state)?.set_volume_live(percent)
}

#[tauri::command(async)]
fn playback_set_shuffle(state: State<'_, Managed>, on: bool) -> Result<PlaybackState> {
    core_of(&state)?.set_shuffle(on)
}

#[tauri::command(async)]
fn playback_set_repeat(state: State<'_, Managed>, repeat: Repeat) -> Result<PlaybackState> {
    core_of(&state)?.set_repeat(repeat)
}

#[tauri::command(async)]
fn queue_play_next(state: State<'_, Managed>, track_id: i64) -> Result<PlaybackState> {
    core_of(&state)?.queue_play_next(track_id)
}

#[tauri::command(async)]
fn queue_add(state: State<'_, Managed>, track_id: i64) -> Result<PlaybackState> {
    core_of(&state)?.queue_add(track_id)
}

#[tauri::command(async)]
fn queue_add_many(state: State<'_, Managed>, track_ids: Vec<i64>) -> Result<PlaybackState> {
    core_of(&state)?.queue_add_many(track_ids)
}

#[tauri::command(async)]
fn queue_move(state: State<'_, Managed>, from: usize, to: usize) -> Result<PlaybackState> {
    core_of(&state)?.queue_move(from, to)
}

#[tauri::command(async)]
fn queue_remove(state: State<'_, Managed>, offset: usize) -> Result<PlaybackState> {
    core_of(&state)?.queue_remove(offset)
}

#[tauri::command(async)]
fn queue_clear(state: State<'_, Managed>) -> Result<PlaybackState> {
    core_of(&state)?.queue_clear()
}

#[tauri::command(async)]
fn queue_play(app: AppHandle, state: State<'_, Managed>, offset: usize) -> Result<PlaybackState> {
    after_start(&app, core_of(&state)?.queue_play(offset))
}

/// Advances the queue when a track ends and reports the position to the window.
fn spawn_ticker(app: AppHandle, core: Arc<AppCore>) {
    let spawned = std::thread::Builder::new()
        .name("bitnot-ticker".into())
        .spawn(move || {
            let mut was_playing = false;
            loop {
                std::thread::sleep(TICK);
                let Ok((track_changed, progress)) = core.tick() else {
                    continue;
                };
                if track_changed {
                    playback_changed(&app);
                    changed(&app, &[SCOPE_HISTORY]);
                }
                if progress.playing || was_playing {
                    let _ = app.emit(EVENT_PROGRESS, progress);
                }
                was_playing = progress.playing;
            }
        });
    if spawned.is_err() {
        log::error!("the playback ticker could not be started");
    }
}

/// Scans after the watched music folders change, once the changes settle.
fn spawn_watch_scanner(app: AppHandle, core: Arc<AppCore>) {
    let (sender, receiver) = mpsc::channel::<()>();
    let listener: core::ChangeListener = Arc::new(move || {
        let _ = sender.send(());
    });
    if let Err(error) = core.set_change_listener(listener) {
        log::warn!("folder watching could not be set up: {error}");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("bitnot-watch".into())
        .spawn(move || {
            while receiver.recv().is_ok() {
                // Wait for a burst of changes (a copy, an unzip) to finish.
                loop {
                    match receiver.recv_timeout(WATCH_QUIET) {
                        Ok(()) => continue,
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
                match run_scan(&app, &core) {
                    Ok(summary) => {
                        let _ = app.emit(EVENT_SCAN_FINISHED, summary);
                    }
                    // A scan was already running; it will see these files too.
                    Err(error) => log::info!("watch-triggered scan skipped: {error}"),
                }
            }
        });
    if spawned.is_err() {
        log::error!("the folder-watch thread could not be started");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let started = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("The AppData folder could not be located: {e}"))
                .and_then(|root| {
                    let logger =
                        diagnostics::install(&root.join("logs"), Some(root.join("session.lock")));
                    // The window may load cover images from the artwork cache, and nothing else.
                    let artwork = root.join("artwork");
                    let _ = std::fs::create_dir_all(&artwork);
                    // Requests are matched against a file's fully resolved path. That can
                    // differ from the folder's plain path (junctions, or AppData that
                    // Windows redirects for a packaged parent process), so resolve a
                    // real file inside the folder and allow the folder it lands in too.
                    let probe = artwork.join(".access-probe");
                    let resolved = std::fs::write(&probe, b"")
                        .ok()
                        .and_then(|_| std::fs::canonicalize(&probe).ok())
                        .and_then(|path| path.parent().map(|dir| dir.to_path_buf()));
                    let scope = app.asset_protocol_scope();
                    for dir in std::iter::once(&artwork).chain(resolved.iter()) {
                        if let Err(error) = scope.allow_directory(dir, false) {
                            log::warn!("cover images cannot be shown: {error}");
                        }
                    }
                    if !scope.is_allowed(&probe) {
                        log::warn!(
                            "the window cannot read the artwork cache; covers will not show"
                        );
                    }
                    let _ = std::fs::remove_file(&probe);
                    let output: Option<Box<dyn Output>> = match DeviceOutput::open() {
                        Ok(output) => Some(Box::new(output)),
                        Err(reason) => {
                            log::warn!("audio output unavailable: {reason}");
                            None
                        }
                    };
                    AppCore::init_with_output(root, logger, output)
                        .map(Arc::new)
                        .map_err(|e| {
                            log::error!("startup failed: {e}");
                            e.to_string()
                        })
                });
            if let Ok(core) = &started {
                spawn_ticker(app.handle().clone(), Arc::clone(core));
                spawn_watch_scanner(app.handle().clone(), Arc::clone(core));
            }
            app.manage(Managed(started));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            settings_update,
            setup_complete,
            profile_create,
            profile_rename,
            profile_switch,
            profile_set_main,
            profile_delete,
            profile_restore,
            profile_purge,
            diagnostics_list,
            diagnostics_read,
            diagnostics_clear,
            library_overview,
            library_tracks,
            library_recent,
            library_collection,
            library_favorites,
            library_history,
            history_remove,
            history_clear,
            library_search,
            library_issues,
            library_genres,
            library_scan,
            track_sources,
            track_prefer_source,
            track_edit,
            track_set_artwork,
            tracks_remove,
            favorite_set,
            playlist_create,
            playlist_update,
            playlist_set_cover,
            playlist_delete,
            playlist_add,
            playlist_remove,
            playlist_move,
            playlist_from_queue,
            trash_list,
            trash_restore,
            trash_purge,
            trash_empty,
            playback_state,
            play_tracks,
            playback_toggle,
            playback_next,
            playback_previous,
            playback_seek,
            playback_set_volume,
            playback_set_shuffle,
            playback_set_repeat,
            queue_play_next,
            queue_add,
            queue_add_many,
            queue_move,
            queue_remove,
            queue_clear,
            queue_play,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the BitNot application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(managed) = handle.try_state::<Managed>() {
                if let Ok(core) = managed.0.as_ref() {
                    core.shutdown();
                }
            }
        }
    });
}
