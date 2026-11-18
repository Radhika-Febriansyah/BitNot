//! Playback: the audio output (decoding and the Windows audio device) and the
//! controller that drives it from the queue.
//!
//! The controller talks to the output through the [`Output`] trait, so its
//! behaviour is tested without an audio device.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{AppError, Result};
use crate::library::{self, Track};
use crate::queue::{Queue, Repeat};

const NEXT_UP_LIMIT: usize = 100;
/// Pressing Previous later than this restarts the current track instead.
const RESTART_THRESHOLD: Duration = Duration::from_secs(3);

/// What the controller needs from an audio output.
pub trait Output: Send {
    /// Loads a file and positions it at `start`. Playback begins unless `paused`.
    fn load(
        &mut self,
        path: &Path,
        start: Duration,
        paused: bool,
    ) -> std::result::Result<(), String>;
    fn play(&mut self);
    fn pause(&mut self);
    fn seek(&mut self, position: Duration) -> std::result::Result<(), String>;
    fn set_volume(&mut self, linear: f32);
    fn position(&self) -> Duration;
    /// True when the loaded audio has played to its end.
    fn finished(&self) -> bool;
    fn stop(&mut self);
}

/// The real output: Symphonia decoding through rodio to the default Windows
/// playback device. The device stream lives on its own thread for the life of
/// the process.
pub struct DeviceOutput {
    player: rodio::Player,
}

impl DeviceOutput {
    pub fn open() -> std::result::Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("bitnot-audio".into())
            .spawn(
                move || match rodio::DeviceSinkBuilder::open_default_sink() {
                    Ok(mut sink) => {
                        sink.log_on_drop(false);
                        let player = rodio::Player::connect_new(sink.mixer());
                        if sender.send(Ok(player)).is_ok() {
                            // Keep the device stream alive.
                            loop {
                                std::thread::park();
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                    }
                },
            )
            .map_err(|e| e.to_string())?;
        let player = receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "The audio device did not respond.".to_string())??;
        Ok(DeviceOutput { player })
    }
}

impl Output for DeviceOutput {
    fn load(
        &mut self,
        path: &Path,
        start: Duration,
        paused: bool,
    ) -> std::result::Result<(), String> {
        let file = File::open(path).map_err(|_| "The file could not be opened.".to_string())?;
        let length = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut builder = rodio::Decoder::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(length)
            .with_seekable(true);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            builder = builder.with_hint(ext);
        }
        let source = builder
            .build()
            .map_err(|_| "This file could not be decoded.".to_string())?;
        self.player.stop();
        self.player.append(source);
        if !start.is_zero() {
            // A failed seek leaves playback at the start, which is acceptable.
            let _ = self.player.try_seek(start);
        }
        if paused {
            self.player.pause();
        } else {
            self.player.play();
        }
        Ok(())
    }

    fn play(&mut self) {
        self.player.play();
    }

    fn pause(&mut self) {
        self.player.pause();
    }

    fn seek(&mut self, position: Duration) -> std::result::Result<(), String> {
        self.player
            .try_seek(position)
            .map_err(|_| "This file does not support seeking.".to_string())
    }

    fn set_volume(&mut self, linear: f32) {
        self.player.set_volume(linear);
    }

    fn position(&self) -> Duration {
        self.player.get_pos()
    }

    fn finished(&self) -> bool {
        self.player.empty()
    }

    fn stop(&mut self) {
        self.player.stop();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackState {
    pub track: Option<Track>,
    pub next_up: Vec<Track>,
    /// Total upcoming tracks; `next_up` holds at most the first hundred.
    pub next_up_total: usize,
    pub position_ms: u64,
    pub playing: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// 0–100.
    pub volume: u32,
    /// False when no audio output device could be opened.
    pub output_available: bool,
    /// Why the last track could not be played, if it could not.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub track_id: Option<i64>,
    pub position_ms: u64,
    pub playing: bool,
}

/// Saved between sessions so the queue can be restored.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedQueue {
    queue: Queue,
    position_ms: u64,
}

pub struct Controller {
    output: Option<Box<dyn Output>>,
    queue: Queue,
    /// Track currently loaded in the output.
    loaded: Option<i64>,
    playing: bool,
    /// Where to resume a restored track that has not been loaded yet.
    resume_ms: u64,
    volume: u32,
    error: Option<String>,
    seed: u64,
    /// The queue as last written to the database, to skip identical writes.
    saved_queue: String,
}

fn linear_volume(percent: u32) -> f32 {
    // Squared so the slider feels even to the ear.
    let v = percent.min(100) as f32 / 100.0;
    v * v
}

impl Controller {
    pub fn new(output: Option<Box<dyn Output>>, volume: u32) -> Self {
        let mut controller = Controller {
            output,
            queue: Queue::default(),
            loaded: None,
            playing: false,
            resume_ms: 0,
            volume: volume.min(100),
            error: None,
            seed: 0x9E37_79B9_7F4A_7C15,
            saved_queue: String::new(),
        };
        if let Some(output) = controller.output.as_mut() {
            output.set_volume(linear_volume(controller.volume));
        }
        controller
    }

    fn next_seed(&mut self, now: i64) -> u64 {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(now as u64 | 1);
        self.seed
    }

    fn output(&mut self) -> Result<&mut Box<dyn Output>> {
        self.output.as_mut().ok_or_else(|| {
            AppError::invalid(
                "No audio output device is available. Connect one and restart BitNot.",
            )
        })
    }

    fn position_ms(&self) -> u64 {
        match (&self.output, self.loaded) {
            (Some(output), Some(_)) => output.position().as_millis() as u64,
            _ => self.resume_ms,
        }
    }

    /// Loads and starts the queue's current track. Tracks that cannot be
    /// played are skipped, with the reason kept for the interface.
    fn start_current(&mut self, conn: &Connection, now: i64, start_ms: u64) -> Result<()> {
        self.error = None;
        let attempts = self.queue.upcoming().len() + 1;
        for attempt in 0..attempts {
            let Some(id) = self.queue.current_track() else {
                break;
            };
            let track = library::track(conn, id)?;
            let start = Duration::from_millis(if attempt == 0 { start_ms } else { 0 });
            let outcome = match &track {
                None => Err("The track is no longer in your library.".to_string()),
                Some(track) => {
                    // This file first, then any other file in the library that
                    // is confidently the same recording (CLAUDE.md §10).
                    let mut paths: Vec<String> = Vec::new();
                    if track.available {
                        paths.push(track.path.clone());
                    }
                    for source in library::track_sources(conn, id)? {
                        if source.id != id {
                            paths.push(source.path);
                        }
                    }
                    let mut result = Err("The file is missing.".to_string());
                    for path in &paths {
                        result = self.output()?.load(Path::new(path), start, false);
                        if result.is_ok() {
                            break;
                        }
                    }
                    result
                }
            };
            match outcome {
                Ok(()) => {
                    self.loaded = Some(id);
                    self.playing = true;
                    self.resume_ms = 0;
                    library::record_play(conn, id, now)?;
                    return Ok(());
                }
                Err(reason) => {
                    let title = track
                        .map(|t| t.title)
                        .unwrap_or_else(|| "A track".to_string());
                    log::warn!("a track could not be played: {reason}");
                    self.error = Some(format!("“{title}” was skipped. {reason}"));
                    if self.queue.advance(false).is_none() {
                        break;
                    }
                }
            }
        }
        self.halt();
        Ok(())
    }

    fn halt(&mut self) {
        if let Some(output) = self.output.as_mut() {
            output.stop();
        }
        self.loaded = None;
        self.playing = false;
        self.resume_ms = 0;
    }

    /// Replaces the queue with `tracks` and plays from `start`.
    pub fn play_tracks(
        &mut self,
        conn: &Connection,
        tracks: Vec<i64>,
        start: usize,
        now: i64,
    ) -> Result<()> {
        self.output()?;
        let seed = self.next_seed(now);
        self.queue.set(tracks, start, seed);
        self.start_current(conn, now, 0)
    }

    pub fn toggle(&mut self, conn: &Connection, now: i64) -> Result<()> {
        if self.playing {
            self.output()?.pause();
            self.playing = false;
            return Ok(());
        }
        if self.loaded.is_some() {
            self.output()?.play();
            self.playing = true;
            return Ok(());
        }
        // Nothing loaded: resume a restored queue, or start one that was only added to.
        if self.queue.current_track().is_none() && self.queue.advance(false).is_none() {
            return Ok(());
        }
        self.output()?;
        let resume = self.resume_ms;
        self.start_current(conn, now, resume)
    }

    pub fn next(&mut self, conn: &Connection, now: i64) -> Result<()> {
        if self.queue.advance(false).is_some() {
            self.output()?;
            self.start_current(conn, now, 0)?;
        }
        Ok(())
    }

    pub fn previous(&mut self, conn: &Connection, now: i64) -> Result<()> {
        if self.loaded.is_some() && self.output()?.position() > RESTART_THRESHOLD {
            return self.seek(0);
        }
        if self.queue.previous().is_some() {
            self.output()?;
            self.start_current(conn, now, 0)?;
        }
        Ok(())
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<()> {
        if self.loaded.is_none() {
            self.resume_ms = position_ms;
            return Ok(());
        }
        self.output()?
            .seek(Duration::from_millis(position_ms))
            .map_err(AppError::Invalid)
    }

    pub fn set_volume(&mut self, percent: u32) {
        self.volume = percent.min(100);
        if let Some(output) = self.output.as_mut() {
            output.set_volume(linear_volume(self.volume));
        }
    }

    pub fn set_shuffle(&mut self, on: bool, now: i64) {
        let seed = self.next_seed(now);
        self.queue.set_shuffle(on, seed);
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.queue.repeat = repeat;
    }

    pub fn play_next(&mut self, track: i64) {
        self.queue.play_next(track);
    }

    pub fn add_to_queue(&mut self, track: i64) {
        self.queue.add(track);
    }

    pub fn add_many_to_queue(&mut self, tracks: &[i64]) {
        self.queue.add_many(tracks);
    }

    pub fn move_upcoming(&mut self, from: usize, to: usize) {
        self.queue.move_upcoming(from, to);
    }

    /// The playing track followed by everything upcoming (Save Queue as Playlist).
    pub fn remaining(&self) -> Vec<i64> {
        self.queue.remaining()
    }

    pub fn remove_upcoming(&mut self, offset: usize) {
        self.queue.remove_upcoming(offset);
    }

    pub fn clear_upcoming(&mut self) {
        self.queue.clear_upcoming();
    }

    pub fn play_upcoming(&mut self, conn: &Connection, offset: usize, now: i64) -> Result<()> {
        self.output()?;
        if self.queue.skip_to_upcoming(offset).is_some() {
            self.start_current(conn, now, 0)?;
        }
        Ok(())
    }

    /// Stops playback and empties the queue (used when switching profile).
    pub fn reset(&mut self) {
        self.halt();
        self.queue.clear();
        self.error = None;
    }

    /// Called a few times a second. Moves on when a track ends.
    /// Returns true when the track or queue changed.
    pub fn tick(&mut self, conn: &Connection, now: i64) -> Result<bool> {
        let ended = self.playing
            && self.loaded.is_some()
            && self.output.as_ref().is_some_and(|o| o.finished());
        if !ended {
            return Ok(false);
        }
        if self.queue.advance(true).is_some() {
            self.start_current(conn, now, 0)?;
        } else {
            self.halt();
        }
        Ok(true)
    }

    pub fn progress(&self) -> Progress {
        Progress {
            track_id: self.queue.current_track(),
            position_ms: self.position_ms(),
            playing: self.playing,
        }
    }

    pub fn state(&self, conn: &Connection) -> Result<PlaybackState> {
        let upcoming = self.queue.upcoming();
        let shown = &upcoming[..upcoming.len().min(NEXT_UP_LIMIT)];
        Ok(PlaybackState {
            track: match self.queue.current_track() {
                Some(id) => library::track(conn, id)?,
                None => None,
            },
            next_up: library::tracks_by_ids(conn, shown)?,
            next_up_total: upcoming.len(),
            position_ms: self.position_ms(),
            playing: self.playing,
            shuffle: self.queue.shuffle,
            repeat: self.queue.repeat,
            volume: self.volume,
            output_available: self.output.is_some(),
            error: self.error.clone(),
        })
    }

    /// Removes tracks that are no longer available (file missing, or their
    /// folder was removed) from the queue. Stops playback if the current track
    /// was one of them. Returns true when the queue changed.
    pub fn drop_unavailable(&mut self, conn: &Connection) -> Result<bool> {
        let available: std::collections::HashSet<i64> = conn
            .prepare("SELECT id FROM tracks WHERE status = 'ok'")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let before = (self.queue.current_track(), self.queue.upcoming().len());
        self.queue.retain(|id| available.contains(&id));
        if self.queue.current_track() != before.0 {
            self.halt();
        }
        Ok((self.queue.current_track(), self.queue.upcoming().len()) != before)
    }

    /// Saves the playback position: one small row, cheap enough to write often.
    pub fn save_position(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "INSERT INTO playback_position (id, position_ms) VALUES (1, ?1)
             ON CONFLICT (id) DO UPDATE SET position_ms = excluded.position_ms",
            [self.position_ms() as i64],
        )?;
        Ok(())
    }

    /// Saves the queue and position so the next session can restore them.
    /// The queue itself is written only when it actually changed.
    pub fn save(&mut self, conn: &Connection) -> Result<()> {
        let saved = SavedQueue {
            queue: self.queue.clone(),
            position_ms: 0,
        };
        let json = serde_json::to_string(&saved)?;
        if json != self.saved_queue {
            conn.execute(
                "INSERT INTO queue_state (id, data) VALUES (1, ?1)
                 ON CONFLICT (id) DO UPDATE SET data = excluded.data",
                [&json],
            )?;
            self.saved_queue = json;
        }
        self.save_position(conn)
    }

    /// Restores the saved queue, paused. Tracks that no longer exist are dropped.
    pub fn restore(&mut self, conn: &Connection) -> Result<()> {
        self.halt();
        self.queue.clear();
        let raw: Option<String> = {
            use rusqlite::OptionalExtension;
            conn.query_row("SELECT data FROM queue_state WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?
        };
        let Some(saved) = raw
            .as_deref()
            .and_then(|raw| serde_json::from_str::<SavedQueue>(raw).ok())
        else {
            self.saved_queue.clear();
            return Ok(());
        };
        self.saved_queue = raw.unwrap_or_default();
        // Position has its own row; queues saved by older versions carry it inline.
        let position: Option<i64> = {
            use rusqlite::OptionalExtension;
            conn.query_row(
                "SELECT position_ms FROM playback_position WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
        };
        let before = saved.queue.current_track();
        self.queue = saved.queue;
        self.drop_unavailable(conn)?;
        self.resume_ms = if self.queue.current_track() == before {
            position.map_or(saved.position_ms, |ms| ms.max(0) as u64)
        } else {
            0
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::{add_track, conn};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeState {
        loaded: Option<String>,
        position: Duration,
        paused: bool,
        finished: bool,
        volume: f32,
        fail_paths: Vec<String>,
    }

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<FakeState>>);

    impl Output for Fake {
        fn load(
            &mut self,
            path: &Path,
            start: Duration,
            paused: bool,
        ) -> std::result::Result<(), String> {
            let mut s = self.0.lock().unwrap();
            let path = path.to_string_lossy().into_owned();
            if s.fail_paths.contains(&path) {
                return Err("This file could not be decoded.".into());
            }
            s.loaded = Some(path);
            s.position = start;
            s.paused = paused;
            s.finished = false;
            Ok(())
        }
        fn play(&mut self) {
            self.0.lock().unwrap().paused = false;
        }
        fn pause(&mut self) {
            self.0.lock().unwrap().paused = true;
        }
        fn seek(&mut self, position: Duration) -> std::result::Result<(), String> {
            self.0.lock().unwrap().position = position;
            Ok(())
        }
        fn set_volume(&mut self, linear: f32) {
            self.0.lock().unwrap().volume = linear;
        }
        fn position(&self) -> Duration {
            self.0.lock().unwrap().position
        }
        fn finished(&self) -> bool {
            self.0.lock().unwrap().finished
        }
        fn stop(&mut self) {
            let mut s = self.0.lock().unwrap();
            s.loaded = None;
            s.position = Duration::ZERO;
        }
    }

    fn setup() -> (Connection, Vec<i64>, Fake, Controller) {
        let conn = conn();
        let ids = vec![
            add_track(&conn, "One", "A", "X", 1),
            add_track(&conn, "Two", "A", "X", 2),
            add_track(&conn, "Three", "A", "X", 3),
        ];
        let fake = Fake::default();
        let controller = Controller::new(Some(Box::new(fake.clone())), 80);
        (conn, ids, fake, controller)
    }

    fn current(c: &Controller, conn: &Connection) -> Option<String> {
        c.state(conn).unwrap().track.map(|t| t.title)
    }

    #[test]
    fn playing_from_a_track_continues_through_the_rest_and_stops() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids.clone(), 1, 100).unwrap();
        assert_eq!(current(&c, &conn).as_deref(), Some("Two"));
        assert!(c.state(&conn).unwrap().playing);
        assert_eq!(c.state(&conn).unwrap().next_up.len(), 1);

        fake.0.lock().unwrap().finished = true;
        assert!(c.tick(&conn, 200).unwrap());
        assert_eq!(current(&c, &conn).as_deref(), Some("Three"));

        fake.0.lock().unwrap().finished = true;
        assert!(c.tick(&conn, 300).unwrap());
        let state = c.state(&conn).unwrap();
        assert!(!state.playing, "stops at the end of the queue");
        assert!(!c.tick(&conn, 400).unwrap(), "nothing more happens");
    }

    #[test]
    fn each_started_track_is_counted_once_and_added_to_history() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids.clone(), 0, 100).unwrap();
        c.toggle(&conn, 110).unwrap(); // pause
        c.toggle(&conn, 120).unwrap(); // resume: not a new play
        fake.0.lock().unwrap().finished = true;
        c.tick(&conn, 200).unwrap();

        assert_eq!(
            library::track(&conn, ids[0]).unwrap().unwrap().play_count,
            1
        );
        assert_eq!(
            library::track(&conn, ids[1]).unwrap().unwrap().play_count,
            1
        );
        assert_eq!(
            library::track(&conn, ids[2]).unwrap().unwrap().play_count,
            0
        );
        assert_eq!(library::history(&conn).unwrap().len(), 2);
    }

    #[test]
    fn pause_resume_seek_and_volume_reach_the_output() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids, 0, 1).unwrap();
        c.toggle(&conn, 2).unwrap();
        assert!(fake.0.lock().unwrap().paused);
        assert!(!c.progress().playing);
        c.toggle(&conn, 3).unwrap();
        assert!(!fake.0.lock().unwrap().paused);

        c.seek(42_000).unwrap();
        assert_eq!(c.progress().position_ms, 42_000);

        c.set_volume(50);
        assert!((fake.0.lock().unwrap().volume - 0.25).abs() < 1e-6);
        c.set_volume(500);
        assert_eq!(c.state(&conn).unwrap().volume, 100);
    }

    #[test]
    fn previous_restarts_after_three_seconds_otherwise_goes_back() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids, 1, 1).unwrap();
        fake.0.lock().unwrap().position = Duration::from_secs(10);
        c.previous(&conn, 2).unwrap();
        assert_eq!(current(&c, &conn).as_deref(), Some("Two"));
        assert_eq!(c.progress().position_ms, 0);

        c.previous(&conn, 3).unwrap();
        assert_eq!(current(&c, &conn).as_deref(), Some("One"));
    }

    #[test]
    fn unplayable_and_missing_tracks_are_skipped_with_a_reason() {
        let (conn, ids, fake, mut c) = setup();
        let bad_path: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [ids[0]], |r| {
                r.get(0)
            })
            .unwrap();
        fake.0.lock().unwrap().fail_paths.push(bad_path);
        conn.execute(
            "UPDATE tracks SET status = 'missing' WHERE id = ?1",
            [ids[1]],
        )
        .unwrap();

        c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
        let state = c.state(&conn).unwrap();
        assert_eq!(state.track.unwrap().title, "Three");
        assert!(state.playing);
        assert!(state.error.unwrap().contains("Two"));
        assert_eq!(
            library::track(&conn, ids[0]).unwrap().unwrap().play_count,
            0
        );
    }

    #[test]
    fn when_nothing_can_play_playback_stops_cleanly() {
        let (conn, ids, _fake, mut c) = setup();
        conn.execute("UPDATE tracks SET status = 'missing'", [])
            .unwrap();
        c.play_tracks(&conn, ids, 0, 1).unwrap();
        let state = c.state(&conn).unwrap();
        assert!(!state.playing);
        assert!(state.error.is_some());
    }

    #[test]
    fn without_an_output_device_playback_reports_it_and_the_rest_works() {
        let conn = conn();
        let id = add_track(&conn, "One", "A", "X", 1);
        let mut c = Controller::new(None, 80);
        assert!(c.play_tracks(&conn, vec![id], 0, 1).is_err());
        c.add_to_queue(id);
        let state = c.state(&conn).unwrap();
        assert!(!state.output_available);
        assert_eq!(state.next_up.len(), 1);
        assert!(!c.tick(&conn, 2).unwrap());
    }

    #[test]
    fn queue_edits_and_jumping_to_an_upcoming_track() {
        let (conn, ids, _fake, mut c) = setup();
        c.play_tracks(&conn, vec![ids[0]], 0, 1).unwrap();
        c.add_to_queue(ids[1]);
        c.play_next(ids[2]);
        let titles = |c: &Controller| -> Vec<String> {
            c.state(&conn)
                .unwrap()
                .next_up
                .into_iter()
                .map(|t| t.title)
                .collect()
        };
        assert_eq!(titles(&c), vec!["Three", "Two"]);
        c.remove_upcoming(0);
        assert_eq!(titles(&c), vec!["Two"]);
        c.add_to_queue(ids[2]);
        c.play_upcoming(&conn, 1, 2).unwrap();
        assert_eq!(current(&c, &conn).as_deref(), Some("Three"));
        c.clear_upcoming();
        assert!(titles(&c).is_empty());
    }

    #[test]
    fn queue_and_position_are_restored_paused() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids.clone(), 1, 1).unwrap();
        fake.0.lock().unwrap().position = Duration::from_secs(33);
        c.save(&conn).unwrap();

        let fake2 = Fake::default();
        let mut restored = Controller::new(Some(Box::new(fake2.clone())), 80);
        restored.restore(&conn).unwrap();
        let state = restored.state(&conn).unwrap();
        assert_eq!(state.track.unwrap().title, "Two");
        assert!(!state.playing);
        assert_eq!(state.position_ms, 33_000);
        assert_eq!(state.next_up.len(), 1);
        assert!(
            fake2.0.lock().unwrap().loaded.is_none(),
            "nothing plays until asked"
        );

        restored.toggle(&conn, 5).unwrap();
        assert!(restored.state(&conn).unwrap().playing);
        assert_eq!(fake2.0.lock().unwrap().position, Duration::from_secs(33));
    }

    #[test]
    fn a_missing_file_falls_back_to_another_copy_of_the_same_recording() {
        let conn = conn();
        let first = crate::library::tests::add_file(&conn, "Song", "A", "X", 1, "mp3", 180_000);
        let copy = crate::library::tests::add_file(&conn, "Song", "A", "X", 1, "flac", 180_500);
        let copy_path: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [copy], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "UPDATE tracks SET status = 'missing' WHERE id = ?1",
            [first],
        )
        .unwrap();

        let fake = Fake::default();
        let mut c = Controller::new(Some(Box::new(fake.clone())), 80);
        // A playlist can still point at the missing copy.
        c.play_tracks(&conn, vec![first], 0, 1).unwrap();
        let state = c.state(&conn).unwrap();
        assert!(state.playing);
        assert!(state.error.is_none());
        assert_eq!(
            fake.0.lock().unwrap().loaded.as_deref(),
            Some(copy_path.as_str())
        );
    }

    #[test]
    fn the_queue_is_written_only_when_it_changes_and_position_always() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
        c.save(&conn).unwrap();
        // Mark the stored queue; an unchanged queue must not be written again.
        conn.execute("UPDATE queue_state SET data = 'sentinel'", [])
            .unwrap();
        fake.0.lock().unwrap().position = Duration::from_secs(12);
        c.save(&conn).unwrap();
        let data: String = conn
            .query_row("SELECT data FROM queue_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(data, "sentinel");
        let position: i64 = conn
            .query_row("SELECT position_ms FROM playback_position", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(position, 12_000);

        c.add_many_to_queue(&[ids[0], ids[1]]);
        c.move_upcoming(3, 0);
        c.save(&conn).unwrap();
        let data: String = conn
            .query_row("SELECT data FROM queue_state", [], |r| r.get(0))
            .unwrap();
        assert_ne!(data, "sentinel");
        assert_eq!(c.remaining(), vec![ids[0], ids[1], ids[1], ids[2], ids[0]]);
    }

    #[test]
    fn tracks_that_leave_the_library_leave_the_queue() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();

        // An upcoming track disappears: playback continues.
        conn.execute(
            "UPDATE tracks SET status = 'detached' WHERE id = ?1",
            [ids[2]],
        )
        .unwrap();
        assert!(c.drop_unavailable(&conn).unwrap());
        let state = c.state(&conn).unwrap();
        assert!(state.playing);
        assert_eq!(state.next_up.len(), 1);
        assert!(!c.drop_unavailable(&conn).unwrap(), "nothing more to drop");

        // The playing track disappears: playback stops.
        conn.execute(
            "UPDATE tracks SET status = 'missing' WHERE id = ?1",
            [ids[0]],
        )
        .unwrap();
        assert!(c.drop_unavailable(&conn).unwrap());
        let state = c.state(&conn).unwrap();
        assert!(!state.playing);
        assert!(state.track.is_none());
        assert!(fake.0.lock().unwrap().loaded.is_none());
    }

    #[test]
    fn restore_drops_tracks_that_no_longer_exist() {
        let (conn, ids, _fake, mut c) = setup();
        c.play_tracks(&conn, ids.clone(), 1, 1).unwrap();
        c.save(&conn).unwrap();
        conn.execute("DELETE FROM tracks WHERE id = ?1", [ids[1]])
            .unwrap();

        let mut restored = Controller::new(Some(Box::new(Fake::default())), 80);
        restored.restore(&conn).unwrap();
        let state = restored.state(&conn).unwrap();
        assert!(state.track.is_none());
        assert_eq!(state.position_ms, 0);
        assert_eq!(state.next_up.len(), 2);
    }
}
