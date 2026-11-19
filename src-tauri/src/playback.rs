//! Playback: the audio output (decoding and the Windows audio device) and the
//! controller that drives it from the queue.
//!
//! The controller talks to the output through the [`Output`] trait, so its
//! behaviour is tested without an audio device.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{AppError, Result};
use crate::gapless::{ChainShared, TrackChain};
use crate::library::{self, Track};
use crate::queue::{Queue, Repeat};
use crate::resample::{Marked, OutputStage, TrackStarts};
use crate::sleep::{SleepRequest, SleepStatus, SleepTimer};

const NEXT_UP_LIMIT: usize = 100;
/// Pressing Previous later than this restarts the current track instead.
const RESTART_THRESHOLD: Duration = Duration::from_secs(3);

/// Milliseconds on the core's monotonic clock: time since the first reading in
/// this process. Progress readings carry it so the interface can tell how old a
/// reading is and estimate the position between readings (see
/// `src/lib/playbackClock.ts`). Wall-clock time is not used, so changing the
/// Windows clock cannot disturb it.
pub fn monotonic_ms(at: Instant) -> u64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    // The real time of the first use, never a caller's (possibly future) instant.
    let origin = *ORIGIN.get_or_init(Instant::now);
    at.saturating_duration_since(origin).as_millis() as u64
}

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
    /// True when the loaded audio has played to its end (with nothing
    /// prepared to follow it).
    fn finished(&self) -> bool;
    fn stop(&mut self);
    /// Prepares the track to follow the loaded one with no gap, replacing any
    /// prepared before; `None` clears it. `after` is the number of
    /// transitions the caller has seen: the preparation is only used for the
    /// transition after that many.
    fn prepare_next(&mut self, path: Option<&Path>, after: u64) -> std::result::Result<(), String>;
    /// How many times playback has moved on to a prepared track by itself.
    fn transitions(&self) -> u64;
}

/// The real output: Symphonia decoding through rodio to the default Windows
/// playback device. The device stream lives on its own thread for the life of
/// the process.
pub struct DeviceOutput {
    player: rodio::Player,
    /// Shared with the output stage so it knows where each track begins.
    starts: std::sync::Arc<TrackStarts>,
    /// The chain playing now (see gapless.rs).
    chain: Option<std::sync::Arc<ChainShared>>,
    /// Transitions made by every chain of this output.
    transitions: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl DeviceOutput {
    /// Connects a Player to `mixer` through the output stage (resample.rs),
    /// which hands the mixer stereo at `rate`: the mixer's own rate. The
    /// mixer therefore never converts the rate (rodio's converter is low
    /// quality) and never sees the empty queue's mono format, which made the
    /// first track after start-up play its first 10 ms wrongly.
    pub fn on_mixer(mixer: &rodio::mixer::Mixer, rate: std::num::NonZero<u32>) -> Self {
        crate::resample::prepare_kernel();
        let (player, queue) = rodio::Player::new();
        let starts = std::sync::Arc::new(TrackStarts::default());
        mixer.add(OutputStage::new(
            queue,
            rate,
            std::sync::Arc::clone(&starts),
        ));
        DeviceOutput {
            player,
            starts,
            chain: None,
            transitions: Default::default(),
        }
    }

    /// Opens and probes a file, ready to play (the decoder reads lazily).
    fn open_track(&self, path: &Path) -> std::result::Result<crate::gapless::Track, String> {
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
        Ok(Box::new(Marked::new(source, &self.starts)))
    }

    pub fn open() -> std::result::Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("bitnot-audio".into())
            .spawn(
                move || match rodio::DeviceSinkBuilder::open_default_sink() {
                    Ok(mut sink) => {
                        sink.log_on_drop(false);
                        // The rate of the stream actually opened, not an assumption.
                        let config = sink.config();
                        log::info!(
                            "audio output: {} Hz, {} channels, {:?}",
                            config.sample_rate(),
                            config.channel_count(),
                            config.sample_format()
                        );
                        let output = DeviceOutput::on_mixer(sink.mixer(), config.sample_rate());
                        if sender.send(Ok(output)).is_ok() {
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
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "The audio device did not respond.".to_string())?
    }
}

impl Output for DeviceOutput {
    fn load(
        &mut self,
        path: &Path,
        start: Duration,
        paused: bool,
    ) -> std::result::Result<(), String> {
        let track = self.open_track(path)?;
        let shared = std::sync::Arc::new(ChainShared::default());
        let chain = TrackChain::new(
            track,
            std::sync::Arc::clone(&shared),
            std::sync::Arc::clone(&self.transitions),
        );
        self.player.stop();
        self.player.append(chain);
        self.chain = Some(shared);
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
        // Per track: rodio's own position would run on across a whole chain.
        self.chain
            .as_ref()
            .map(|c| c.position())
            .unwrap_or_default()
    }

    fn finished(&self) -> bool {
        self.player.empty()
    }

    fn stop(&mut self) {
        self.player.stop();
        self.chain = None;
    }

    fn prepare_next(&mut self, path: Option<&Path>, after: u64) -> std::result::Result<(), String> {
        let Some(chain) = self.chain.clone() else {
            return Ok(());
        };
        match path {
            None => chain.prepare(None),
            Some(path) => {
                // Cleared first, so a failure never leaves an older choice in place.
                chain.prepare(None);
                let track = self.open_track(path)?;
                chain.prepare(Some((after, track)));
            }
        }
        Ok(())
    }

    fn transitions(&self) -> u64 {
        self.transitions.load(std::sync::atomic::Ordering::Acquire)
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
    /// The armed sleep timer, if any.
    pub sleep: Option<SleepStatus>,
    /// When `position_ms` was read, on the monotonic clock ([`monotonic_ms`]).
    pub clock_ms: u64,
}

/// What one tick of the ticker did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tick {
    /// The track, queue, or playing state changed.
    pub changed: bool,
    /// The sleep timer stopped playback (or ran out while paused).
    pub slept: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub track_id: Option<i64>,
    pub position_ms: u64,
    pub playing: bool,
    /// When `position_ms` was read, on the monotonic clock ([`monotonic_ms`]).
    pub clock_ms: u64,
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
    sleep: SleepTimer,
    /// Track prepared in the output to follow the loaded one without a gap.
    prepared: Option<i64>,
    /// Track whose preparation failed (not retried until the plan changes).
    prepare_failed: Option<i64>,
    /// Output transitions already accounted for.
    seen_transitions: u64,
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
            sleep: SleepTimer::default(),
            prepared: None,
            prepare_failed: None,
            seen_transitions: 0,
        };
        if let Some(output) = controller.output.as_mut() {
            output.set_volume(linear_volume(controller.volume));
            controller.seen_transitions = output.transitions();
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
            let (track, paths) = Self::track_paths(conn, id)?;
            let start = Duration::from_millis(if attempt == 0 { start_ms } else { 0 });
            let outcome = match &track {
                None => Err("The track is no longer in your library.".to_string()),
                Some(_) => {
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
                    self.prepared = None;
                    self.prepare_failed = None;
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
        self.prepared = None;
        self.prepare_failed = None;
    }

    /// Files to try for a track: its own, then any other file in the library
    /// that is confidently the same recording (CLAUDE.md §10).
    fn track_paths(conn: &Connection, id: i64) -> Result<(Option<Track>, Vec<String>)> {
        let track = library::track(conn, id)?;
        let mut paths = Vec::new();
        if let Some(track) = &track {
            if track.available {
                paths.push(track.path.clone());
            }
            for source in library::track_sources(conn, id)? {
                if source.id != id {
                    paths.push(source.path);
                }
            }
        }
        Ok((track, paths))
    }

    /// Accounts for tracks the output moved on to by itself: the queue
    /// advances, the new track becomes current, and its play is counted now
    /// (when it starts), not when it was prepared. Returns true if anything changed.
    pub fn catch_up(&mut self, conn: &Connection, now: i64) -> Result<bool> {
        let Some(made) = self.output.as_ref().map(|o| o.transitions()) else {
            return Ok(false);
        };
        let mut changed = false;
        while self.seen_transitions < made {
            self.seen_transitions += 1;
            let expected = self.prepared.take();
            let current = self.queue.advance(true);
            if current != expected {
                log::warn!("gapless transition reached an unexpected track");
            }
            self.loaded = current;
            self.resume_ms = 0;
            self.error = None;
            self.prepare_failed = None;
            if let Some(id) = current {
                library::record_play(conn, id, now)?;
            }
            changed = true;
        }
        Ok(changed)
    }

    /// Makes sure the output has the right track prepared to follow the
    /// loaded one: the track automatic advance would play next (following
    /// repeat and shuffle), or none at the end of the queue or when the sleep
    /// timer will stop after this track. Cheap when nothing changed.
    pub fn sync_preload(&mut self, conn: &Connection) {
        if self.output.is_none() || self.loaded.is_none() {
            return;
        }
        let wanted = if self.sleep.stops_after_track() {
            None
        } else {
            let mut queue = self.queue.clone();
            queue.advance(true)
        };
        if wanted == self.prepared {
            return;
        }
        let after = self.seen_transitions;
        let Some(id) = wanted else {
            if let Some(output) = self.output.as_mut() {
                let _ = output.prepare_next(None, after);
            }
            self.prepared = None;
            return;
        };
        if self.prepare_failed == Some(id) {
            return;
        }
        self.prepared = None;
        let paths = match Self::track_paths(conn, id) {
            Ok((_, paths)) => paths,
            Err(error) => {
                log::warn!("the next track could not be looked up: {error}");
                Vec::new()
            }
        };
        let output = self.output.as_mut().unwrap();
        let _ = output.prepare_next(None, after);
        for path in &paths {
            match output.prepare_next(Some(Path::new(path)), after) {
                Ok(()) => {
                    self.prepared = Some(id);
                    return;
                }
                Err(reason) => log::warn!("the next track could not be prepared: {reason}"),
            }
        }
        // Not playable: the track ends without a prepared successor, and the
        // ordinary path (skip with a message) takes over at the boundary.
        self.prepare_failed = Some(id);
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
    /// A sleep timer belongs to what was playing, so it is cancelled too.
    pub fn reset(&mut self) {
        self.halt();
        self.queue.clear();
        self.error = None;
        self.sleep.cancel();
    }

    /// Arms the sleep timer. Stopping after the current track needs a track.
    pub fn set_sleep(&mut self, request: SleepRequest, at: Instant) -> Result<()> {
        if request == SleepRequest::EndOfTrack && self.queue.current_track().is_none() {
            return Err(AppError::invalid(
                "Nothing is playing, so there is no track to stop after.",
            ));
        }
        self.sleep.set(request, at)
    }

    pub fn cancel_sleep(&mut self) {
        self.sleep.cancel();
    }

    /// [`Self::tick_at`] with the current monotonic time; used by tests.
    #[cfg(test)]
    pub fn tick(&mut self, conn: &Connection, now: i64) -> Result<bool> {
        Ok(self.tick_at(conn, now, Instant::now())?.changed)
    }

    /// One tick. `now` is wall-clock time for play history; `at` is the
    /// monotonic time the sleep timer is measured against.
    pub fn tick_at(&mut self, conn: &Connection, now: i64, at: Instant) -> Result<Tick> {
        let moved_on = self.catch_up(conn, now)?;
        let mut tick = self.tick_inner(conn, now, at)?;
        tick.changed |= moved_on;
        self.sync_preload(conn);
        Ok(tick)
    }

    fn tick_inner(&mut self, conn: &Connection, now: i64, at: Instant) -> Result<Tick> {
        if self.sleep.take_expired(at) {
            // Paused, not stopped: the track and position stay where they are.
            if self.playing {
                if let Some(output) = self.output.as_mut() {
                    output.pause();
                }
                self.playing = false;
            }
            return Ok(Tick {
                changed: true,
                slept: true,
            });
        }
        let ended = self.playing
            && self.loaded.is_some()
            && self.output.as_ref().is_some_and(|o| o.finished());
        if !ended {
            return Ok(Tick::default());
        }
        if self.sleep.take_end_of_track() {
            // Stop instead of starting the next track. The queue still moves on
            // (following repeat and shuffle), so Play continues with the track
            // that would have come next; at the end of the queue it stays put.
            self.queue.advance(true);
            self.halt();
            return Ok(Tick {
                changed: true,
                slept: true,
            });
        }
        if self.queue.advance(true).is_some() {
            self.start_current(conn, now, 0)?;
        } else {
            self.halt();
        }
        Ok(Tick {
            changed: true,
            slept: false,
        })
    }

    pub fn progress(&self) -> Progress {
        self.progress_at(Instant::now())
    }

    /// The position, dated: read at `at` on the monotonic clock.
    pub fn progress_at(&self, at: Instant) -> Progress {
        Progress {
            track_id: self.queue.current_track(),
            position_ms: self.position_ms(),
            playing: self.playing,
            clock_ms: monotonic_ms(at),
        }
    }

    pub fn state(&self, conn: &Connection) -> Result<PlaybackState> {
        self.state_at(conn, Instant::now())
    }

    pub fn state_at(&self, conn: &Connection, at: Instant) -> Result<PlaybackState> {
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
            sleep: self.sleep.status(at),
            clock_ms: monotonic_ms(at),
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
        // A sleep timer is never carried over (see sleep.rs).
        self.sleep.cancel();
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
        /// Set by a test: the loaded track's audio has run out.
        finished: bool,
        volume: f32,
        fail_paths: Vec<String>,
        /// Prepared next track and the transition count it was prepared at.
        prepared: Option<(String, u64)>,
        transitions: u64,
        /// Every path ever prepared, in order.
        prepared_log: Vec<String>,
    }

    impl FakeState {
        /// Behaves like the track chain: at the end of the loaded track,
        /// move on to a valid prepared one.
        fn settle(&mut self) {
            if self.finished {
                if let Some((path, at)) = self.prepared.take() {
                    if at == self.transitions && self.loaded.is_some() {
                        self.loaded = Some(path);
                        self.position = Duration::ZERO;
                        self.finished = false;
                        self.transitions += 1;
                    }
                }
            }
        }
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
            // A new chain: nothing is prepared to follow it yet.
            s.prepared = None;
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
            let mut s = self.0.lock().unwrap();
            s.settle();
            s.finished
        }
        fn stop(&mut self) {
            let mut s = self.0.lock().unwrap();
            s.loaded = None;
            s.position = Duration::ZERO;
            s.prepared = None;
        }
        fn prepare_next(
            &mut self,
            path: Option<&Path>,
            after: u64,
        ) -> std::result::Result<(), String> {
            let mut s = self.0.lock().unwrap();
            if s.loaded.is_none() {
                return Ok(());
            }
            s.prepared = None;
            if let Some(path) = path {
                let path = path.to_string_lossy().into_owned();
                if s.fail_paths.contains(&path) {
                    return Err("This file could not be decoded.".into());
                }
                s.prepared_log.push(path.clone());
                s.prepared = Some((path, after));
            }
            Ok(())
        }
        fn transitions(&self) -> u64 {
            let mut s = self.0.lock().unwrap();
            s.settle();
            s.transitions
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
    fn readings_are_dated_on_the_monotonic_clock() {
        let (conn, ids, fake, mut c) = setup();
        c.play_tracks(&conn, ids, 0, 1).unwrap();
        monotonic_ms(Instant::now()); // the origin is set before t0
        let t0 = Instant::now();
        fake.0.lock().unwrap().position = Duration::from_millis(1_500);
        let first = c.progress_at(t0);
        let later = c.progress_at(t0 + Duration::from_millis(250));
        assert_eq!(later.clock_ms - first.clock_ms, 250);
        assert_eq!(first.position_ms, 1_500);
        assert!(first.playing);

        // The full state is dated on the same clock, so either can be compared.
        let state = c.state_at(&conn, t0 + Duration::from_millis(400)).unwrap();
        assert_eq!(state.clock_ms - first.clock_ms, 400);

        // Paused, stopped, and track changes are all visible in a reading.
        c.toggle(&conn, 2).unwrap();
        assert!(!c.progress_at(t0).playing);
        c.next(&conn, 3).unwrap();
        let next = c.progress_at(t0);
        assert!(next.playing);
        assert_ne!(next.track_id, first.track_id);
        c.reset();
        let stopped = c.progress_at(t0);
        assert!(!stopped.playing);
        assert_eq!((stopped.track_id, stopped.position_ms), (None, 0));

        // Never earlier than the clock's origin, and serialised for the window.
        assert_eq!(monotonic_ms(t0 - Duration::from_secs(3600)), 0);
        let json = serde_json::to_value(c.progress_at(t0)).unwrap();
        assert!(json["clockMs"].is_u64());
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

    // ── Sleep timer ───────────────────────────────────────────────────────

    mod sleep_timer {
        use super::*;
        use crate::sleep::{SleepKind, SleepRequest, PRESET_MINUTES};

        const MIN: Duration = Duration::from_secs(60);

        fn minutes(minutes: u32) -> SleepRequest {
            SleepRequest::Minutes { minutes }
        }

        fn paused(fake: &Fake) -> bool {
            fake.0.lock().unwrap().paused
        }

        fn finish_track(fake: &Fake) {
            fake.0.lock().unwrap().finished = true;
        }

        #[test]
        fn each_length_pauses_at_its_end_time_and_keeps_the_place() {
            for preset in PRESET_MINUTES {
                let (conn, ids, fake, mut c) = setup();
                c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
                c.seek(42_000).unwrap();
                let t0 = Instant::now();
                c.set_sleep(minutes(preset), t0).unwrap();
                let end = t0 + MIN * preset;

                let early = c.tick_at(&conn, 2, end - Duration::from_millis(1)).unwrap();
                assert_eq!(early, Tick::default(), "{preset} min fired early");
                assert!(c.state(&conn).unwrap().playing);

                let fired = c.tick_at(&conn, 3, end).unwrap();
                assert!(fired.changed && fired.slept, "{preset} min did not fire");
                let state = c.state_at(&conn, end).unwrap();
                assert!(!state.playing);
                assert!(paused(&fake));
                assert_eq!(state.track.unwrap().title, "One");
                assert_eq!(state.position_ms, 42_000);
                assert_eq!(state.sleep, None);
            }
        }

        #[test]
        fn it_stops_playback_only_once() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            let t0 = Instant::now();
            c.set_sleep(minutes(15), t0).unwrap();
            assert!(c.tick_at(&conn, 2, t0 + MIN * 15).unwrap().slept);
            // Music started again afterwards keeps playing.
            c.toggle(&conn, 3).unwrap();
            for later in [16, 30, 600] {
                assert_eq!(
                    c.tick_at(&conn, 4, t0 + MIN * later).unwrap(),
                    Tick::default()
                );
            }
            assert!(c.state(&conn).unwrap().playing);
            assert!(!paused(&fake));
        }

        #[test]
        fn wall_clock_changes_do_not_move_it() {
            let (conn, ids, _fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1_700_000_000).unwrap();
            let t0 = Instant::now();
            c.set_sleep(minutes(30), t0).unwrap();
            // The Windows clock jumps a day ahead, then a year back; the
            // monotonic time has only moved ten minutes.
            for wall in [1_700_086_400, 1_668_000_000, 0, -1] {
                assert!(!c.tick_at(&conn, wall, t0 + MIN * 10).unwrap().slept);
            }
            assert_eq!(
                c.state_at(&conn, t0 + MIN * 10)
                    .unwrap()
                    .sleep
                    .unwrap()
                    .remaining_ms,
                Some(20 * 60_000)
            );
            assert!(
                c.tick_at(&conn, 1_668_000_000, t0 + MIN * 30)
                    .unwrap()
                    .slept
            );
        }

        #[test]
        fn it_counts_real_time_through_a_pause() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            let t0 = Instant::now();
            c.set_sleep(minutes(15), t0).unwrap();
            c.toggle(&conn, 2).unwrap(); // pause
            c.toggle(&conn, 3).unwrap(); // resume
            assert!(!c.tick_at(&conn, 4, t0 + MIN * 14).unwrap().slept);
            assert!(c.state(&conn).unwrap().playing);

            // Paused when the time runs out: it is used up, and nothing else changes.
            c.toggle(&conn, 5).unwrap();
            let fired = c.tick_at(&conn, 6, t0 + MIN * 20).unwrap();
            assert!(fired.slept);
            let state = c.state(&conn).unwrap();
            assert!(!state.playing && state.sleep.is_none());
            assert_eq!(state.track.unwrap().title, "One");
            c.toggle(&conn, 7).unwrap();
            assert!(c.state(&conn).unwrap().playing);
            assert!(!paused(&fake));
        }

        #[test]
        fn cancelling_keeps_the_music_playing() {
            let (conn, ids, _fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            let t0 = Instant::now();
            c.set_sleep(minutes(45), t0).unwrap();
            c.cancel_sleep();
            assert_eq!(c.state(&conn).unwrap().sleep, None);
            assert_eq!(c.tick_at(&conn, 2, t0 + MIN * 90).unwrap(), Tick::default());
            assert!(c.state(&conn).unwrap().playing);
        }

        #[test]
        fn seeking_and_skipping_do_not_change_a_fixed_length_timer() {
            let (conn, ids, _fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            let t0 = Instant::now();
            c.set_sleep(minutes(60), t0).unwrap();
            c.seek(170_000).unwrap();
            c.next(&conn, 2).unwrap();
            c.previous(&conn, 3).unwrap();
            let state = c.state_at(&conn, t0 + MIN * 5).unwrap();
            assert_eq!(state.sleep.unwrap().remaining_ms, Some(55 * 60_000));
            assert!(c.tick_at(&conn, 4, t0 + MIN * 60).unwrap().slept);
        }

        #[test]
        fn end_of_track_finishes_the_song_and_does_not_start_the_next() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            let t0 = Instant::now();
            c.set_sleep(SleepRequest::EndOfTrack, t0).unwrap();
            assert_eq!(
                c.state(&conn).unwrap().sleep.unwrap().kind,
                SleepKind::EndOfTrack
            );

            // Time alone never ends it, and neither does a seek.
            c.seek(150_000).unwrap();
            assert_eq!(
                c.tick_at(&conn, 2, t0 + MIN * 600).unwrap(),
                Tick::default()
            );
            assert!(c.state(&conn).unwrap().playing);

            finish_track(&fake);
            let fired = c.tick_at(&conn, 3, t0 + MIN * 601).unwrap();
            assert!(fired.changed && fired.slept);
            let state = c.state(&conn).unwrap();
            assert!(!state.playing);
            assert_eq!(state.sleep, None);
            assert!(
                fake.0.lock().unwrap().loaded.is_none(),
                "nothing new was loaded"
            );
            // The queue is ready on the next song, which was not played or counted.
            let next = state.track.unwrap();
            assert_eq!(next.title, "Two");
            assert_eq!(next.play_count, 0);
            assert_eq!(state.position_ms, 0);
            assert_eq!(
                c.tick_at(&conn, 4, t0 + MIN * 602).unwrap(),
                Tick::default()
            );

            // Play continues from there.
            c.toggle(&conn, 5).unwrap();
            let state = c.state(&conn).unwrap();
            assert!(state.playing);
            assert_eq!(state.track.unwrap().title, "Two");
        }

        #[test]
        fn end_of_track_follows_repeat_and_shuffle() {
            // Repeat One: the same song is ready again, not restarted.
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 1, 1).unwrap();
            c.set_repeat(Repeat::One);
            c.set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .unwrap();
            finish_track(&fake);
            assert!(c.tick_at(&conn, 2, Instant::now()).unwrap().slept);
            let state = c.state(&conn).unwrap();
            assert!(!state.playing);
            assert_eq!(state.track.unwrap().title, "Two");
            assert!(fake.0.lock().unwrap().loaded.is_none());

            // Repeat All on the last song: it would wrap, so the first is ready.
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 2, 1).unwrap();
            c.set_repeat(Repeat::All);
            c.set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .unwrap();
            finish_track(&fake);
            assert!(c.tick_at(&conn, 2, Instant::now()).unwrap().slept);
            let state = c.state(&conn).unwrap();
            assert!(!state.playing);
            assert_eq!(state.track.unwrap().title, "One");

            // Repeat off on the last song: playback ends there as it normally would.
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 2, 1).unwrap();
            c.set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .unwrap();
            finish_track(&fake);
            assert!(c.tick_at(&conn, 2, Instant::now()).unwrap().slept);
            let state = c.state(&conn).unwrap();
            assert!(!state.playing);
            assert_eq!(state.track.unwrap().title, "Three");

            // Shuffle: the song shuffle had lined up next is the one left ready.
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            c.set_shuffle(true, 12_345);
            let lined_up = c.state(&conn).unwrap().next_up[0].id;
            c.set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .unwrap();
            finish_track(&fake);
            assert!(c.tick_at(&conn, 2, Instant::now()).unwrap().slept);
            let state = c.state(&conn).unwrap();
            assert!(!state.playing);
            assert_eq!(state.track.unwrap().id, lined_up);
            assert!(state.shuffle);
        }

        #[test]
        fn end_of_track_applies_to_whichever_song_is_playing_when_it_ends() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            c.set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .unwrap();
            c.next(&conn, 2).unwrap();
            let state = c.state(&conn).unwrap();
            assert!(state.playing);
            assert_eq!(state.track.unwrap().title, "Two");
            assert!(state.sleep.is_some());
            finish_track(&fake);
            assert!(c.tick_at(&conn, 3, Instant::now()).unwrap().slept);
            assert_eq!(c.state(&conn).unwrap().track.unwrap().title, "Three");
        }

        #[test]
        fn without_a_track() {
            let (conn, _ids, _fake, mut c) = setup();
            assert!(c
                .set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .is_err());
            assert_eq!(c.state(&conn).unwrap().sleep, None);
            // A fixed length can be set before choosing music; running out with
            // nothing playing changes nothing.
            let t0 = Instant::now();
            c.set_sleep(minutes(15), t0).unwrap();
            let fired = c.tick_at(&conn, 2, t0 + MIN * 15).unwrap();
            assert!(fired.slept);
            let state = c.state(&conn).unwrap();
            assert!(!state.playing && state.track.is_none() && state.sleep.is_none());

            // No audio device: still harmless.
            let mut silent = Controller::new(None, 80);
            silent.set_sleep(minutes(15), t0).unwrap();
            assert!(silent.tick_at(&conn, 2, t0 + MIN * 15).unwrap().slept);
        }

        #[test]
        fn a_restart_or_profile_switch_does_not_carry_the_timer_over() {
            let (conn, ids, _fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            c.set_sleep(minutes(30), Instant::now()).unwrap();
            c.save(&conn).unwrap();

            // A new session restores the queue but not the timer.
            let mut restarted = Controller::new(Some(Box::new(Fake::default())), 80);
            restarted.restore(&conn).unwrap();
            let state = restarted.state(&conn).unwrap();
            assert_eq!(state.track.unwrap().title, "One");
            assert_eq!(state.sleep, None);

            // Restoring in place (profile switch) and resetting both cancel it.
            c.restore(&conn).unwrap();
            assert_eq!(c.state(&conn).unwrap().sleep, None);
            c.set_sleep(minutes(30), Instant::now()).unwrap();
            c.reset();
            assert_eq!(c.state(&conn).unwrap().sleep, None);
        }
    }

    // ── Gapless: preparing the next track ─────────────────────────────────

    mod gapless {
        use super::*;
        use crate::sleep::SleepRequest;

        fn path_of(conn: &Connection, id: i64) -> String {
            library::track(conn, id).unwrap().unwrap().path
        }

        fn prepared(fake: &Fake) -> Option<String> {
            fake.0
                .lock()
                .unwrap()
                .prepared
                .as_ref()
                .map(|p| p.0.clone())
        }

        fn plays(conn: &Connection, id: i64) -> i64 {
            library::track(conn, id).unwrap().unwrap().play_count
        }

        /// The loaded track's audio runs out (the chain then moves on, if it can).
        fn finish(fake: &Fake) {
            fake.0.lock().unwrap().finished = true;
        }

        fn tick(c: &mut Controller, conn: &Connection) -> Tick {
            c.tick_at(conn, 50, Instant::now()).unwrap()
        }

        #[test]
        fn the_next_track_is_prepared_and_becomes_current_at_the_boundary() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[1])));
            // Prepared is not played: Two is not current and not counted.
            assert_eq!(current(&c, &conn).as_deref(), Some("One"));
            assert_eq!(plays(&conn, ids[1]), 0);

            finish(&fake);
            let t = tick(&mut c, &conn);
            assert!(t.changed);
            let state = c.state(&conn).unwrap();
            assert_eq!(state.track.unwrap().title, "Two");
            assert!(state.playing);
            assert_eq!(fake.0.lock().unwrap().loaded, Some(path_of(&conn, ids[1])));
            // Counted once, when it started.
            assert_eq!(plays(&conn, ids[1]), 1);
            assert_eq!(plays(&conn, ids[0]), 1);
            // And the one after it is prepared, for the transition after this one.
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[2])));
            assert_eq!(fake.0.lock().unwrap().prepared.as_ref().unwrap().1, 1);
            // A tick with nothing new changes nothing and counts nothing.
            assert!(!tick(&mut c, &conn).changed);
            assert_eq!(plays(&conn, ids[1]), 1);
        }

        #[test]
        fn repeat_and_the_end_of_the_queue_are_followed() {
            // Repeat One: the same track again, counted again.
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.set_repeat(Repeat::One);
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[0])));
            finish(&fake);
            tick(&mut c, &conn);
            assert_eq!(current(&c, &conn).as_deref(), Some("One"));
            assert_eq!(plays(&conn, ids[0]), 2);

            // Repeat All on the last track: wraps to the first.
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 2, 1).unwrap();
            c.set_repeat(Repeat::All);
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[0])));

            // Repeat off on the last track: nothing to prepare; playback ends.
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 2, 1).unwrap();
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), None);
            finish(&fake);
            tick(&mut c, &conn);
            assert!(!c.state(&conn).unwrap().playing);
        }

        #[test]
        fn shuffle_order_is_followed() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids, 0, 1).unwrap();
            c.set_shuffle(true, 77);
            c.sync_preload(&conn);
            let next = c.state(&conn).unwrap().next_up[0].clone();
            assert_eq!(prepared(&fake), Some(next.path.clone()));
            finish(&fake);
            tick(&mut c, &conn);
            assert_eq!(c.state(&conn).unwrap().track.unwrap().id, next.id);
        }

        #[test]
        fn queue_changes_change_what_is_prepared() {
            let (conn, ids, fake, mut c) = setup();
            let four = add_track(&conn, "Four", "A", "X", 4);
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[1])));

            c.move_upcoming(1, 0); // Three before Two
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[2])));

            c.remove_upcoming(0); // Three removed
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[1])));

            c.play_next(four);
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, four)));

            c.clear_upcoming();
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), None);
            // Unchanged plans are not prepared again.
            let log = fake.0.lock().unwrap().prepared_log.len();
            c.sync_preload(&conn);
            c.sync_preload(&conn);
            assert_eq!(fake.0.lock().unwrap().prepared_log.len(), log);
        }

        #[test]
        fn choosing_another_track_replaces_the_preparation() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            c.next(&conn, 2).unwrap(); // manual: loads Two directly
            assert_eq!(
                prepared(&fake),
                None,
                "a new load starts with nothing prepared"
            );
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[2])));
            c.previous(&conn, 3).unwrap();
            c.sync_preload(&conn);
            assert_eq!(current(&c, &conn).as_deref(), Some("One"));
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[1])));
            c.play_upcoming(&conn, 1, 4).unwrap(); // jump to Three
            c.sync_preload(&conn);
            assert_eq!(current(&c, &conn).as_deref(), Some("Three"));
            assert_eq!(prepared(&fake), None, "end of the queue");
        }

        #[test]
        fn a_next_track_that_cannot_be_prepared_does_not_stall_playback() {
            let (conn, ids, fake, mut c) = setup();
            let two = path_of(&conn, ids[1]);
            fake.0.lock().unwrap().fail_paths.push(two.clone());
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), None);
            // Not retried on every tick.
            let tries = fake.0.lock().unwrap().prepared_log.len();
            tick(&mut c, &conn);
            tick(&mut c, &conn);
            assert_eq!(fake.0.lock().unwrap().prepared_log.len(), tries);
            // At the boundary the ordinary path skips it, as before.
            finish(&fake);
            tick(&mut c, &conn);
            let state = c.state(&conn).unwrap();
            assert_eq!(state.track.unwrap().title, "Three");
            assert!(state.playing);
            assert!(state.error.unwrap().contains("Two"));
        }

        #[test]
        fn pause_keeps_the_preparation_and_stop_clears_it() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            c.toggle(&conn, 2).unwrap(); // pause
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[1])));
            c.toggle(&conn, 3).unwrap(); // resume
            c.reset(); // stop (profile switch)
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), None);
            assert!(fake.0.lock().unwrap().loaded.is_none());
        }

        #[test]
        fn stop_after_this_track_withdraws_the_preparation() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            c.set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .unwrap();
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), None);
            c.cancel_sleep();
            c.sync_preload(&conn);
            assert_eq!(prepared(&fake), Some(path_of(&conn, ids[1])));
            // Armed again: the track ends and playback stops, nothing follows.
            c.set_sleep(SleepRequest::EndOfTrack, Instant::now())
                .unwrap();
            c.sync_preload(&conn);
            finish(&fake);
            let t = tick(&mut c, &conn);
            assert!(t.slept);
            assert!(!c.state(&conn).unwrap().playing);
            assert_eq!(plays(&conn, ids[1]), 0);
        }

        #[test]
        fn seeking_to_the_end_moves_on_cleanly() {
            let (conn, ids, fake, mut c) = setup();
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            c.seek(179_900).unwrap();
            finish(&fake);
            tick(&mut c, &conn);
            assert_eq!(current(&c, &conn).as_deref(), Some("Two"));
            assert_eq!(c.state(&conn).unwrap().position_ms, 0);
        }
    }

    // ── The real output on an offline mixer ─────────────────────────────────

    mod device_output {
        use super::*;
        use crate::resample::test_audio::*;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Instant;

        const RATE: u32 = 48_000;

        /// The device thread's role, played by a thread that pulls the mixer
        /// (about ten times faster than real time) and keeps what it pulled.
        struct Device {
            pulled: Arc<Mutex<Vec<f32>>>,
            stop: Arc<AtomicBool>,
            thread: Option<std::thread::JoinHandle<()>>,
        }

        impl Device {
            fn start(mut mixed: rodio::mixer::MixerSource) -> Self {
                let pulled = Arc::new(Mutex::new(Vec::with_capacity(RATE as usize * 2 * 30)));
                let stop = Arc::new(AtomicBool::new(false));
                let (p, s) = (pulled.clone(), stop.clone());
                let thread = std::thread::spawn(move || {
                    while !s.load(Ordering::Relaxed) {
                        let chunk: Vec<f32> = mixed.by_ref().take(960).collect();
                        p.lock().unwrap().extend_from_slice(&chunk);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                });
                Device {
                    pulled,
                    stop,
                    thread: Some(thread),
                }
            }

            fn frames(&self) -> usize {
                self.pulled.lock().unwrap().len() / 2
            }

            /// Waits until `frames` more frames have been pulled; returns them.
            fn next_frames(&self, frames: usize) -> Vec<f32> {
                let from = self.frames();
                let started = Instant::now();
                while self.frames() < from + frames {
                    assert!(
                        started.elapsed() < Duration::from_secs(10),
                        "the mixer stopped being pulled"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                self.pulled.lock().unwrap()[from * 2..(from + frames) * 2].to_vec()
            }
        }

        impl Drop for Device {
            fn drop(&mut self) {
                self.stop.store(true, Ordering::Relaxed);
                if let Some(t) = self.thread.take() {
                    t.join().ok();
                }
            }
        }

        fn setup() -> (DeviceOutput, Device) {
            let (mixer, mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let output = DeviceOutput::on_mixer(&mixer, nz(RATE));
            (output, Device::start(mixed))
        }

        fn files(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
            let a = dir.join("a-44k1.wav");
            let b = dir.join("b-48k.wav");
            write_wav(&a, 44_100, 2, &tone(997.0, 0.5, 44_100, 0.0, 44_100 * 4));
            write_wav(&b, 48_000, 2, &tone(440.0, 0.5, 48_000, 0.0, 48_000 * 2));
            (a, b)
        }

        fn silent(x: &[f32]) -> bool {
            x.iter().all(|&s| s == 0.0)
        }

        #[test]
        fn rodio_alone_reproduces_the_start_up_defect() {
            // The original condition: a Player connected while its queue is
            // empty. The mixer takes the empty queue's format (mono), and the
            // first stretch of a stereo file plays as mono at half speed.
            let dir = tempfile::tempdir().unwrap();
            let (_, b) = files(dir.path());
            let (mixer, mut mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let player = rodio::Player::connect_new(&mixer);
            player.append(decoder(&b));
            let out: Vec<f32> = mixed.by_ref().take(600 * 2).collect();
            let misread = out
                .chunks(2)
                .filter(|f| f[0] == f[1] && f[0] != 0.0)
                .count();
            assert!(
                misread > 500,
                "rodio no longer shows the defect ({misread} frames); recheck the fix's reason"
            );
        }

        #[test]
        fn the_first_track_plays_correctly_from_its_first_sample() {
            let dir = tempfile::tempdir().unwrap();
            let (a, b) = files(dir.path());
            // 48 kHz: exactly the file, from sample one.
            let (mixer, mut mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let mut output = DeviceOutput::on_mixer(&mixer, nz(RATE));
            output.load(&b, Duration::ZERO, false).unwrap();
            let want: Vec<f32> = decoder(&b).take(48_000 * 2).collect();
            let got: Vec<f32> = mixed.by_ref().take(48_000 * 2).collect();
            assert!(
                got.iter()
                    .zip(&want)
                    .all(|(x, y)| x.to_bits() == y.to_bits()),
                "48 kHz first track is not exact"
            );

            // 44.1 kHz: stereo at the right speed from the start.
            let (mixer, mut mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let mut output = DeviceOutput::on_mixer(&mixer, nz(RATE));
            output.load(&a, Duration::ZERO, false).unwrap();
            let got: Vec<f32> = mixed.by_ref().take(48_000 * 2).collect();
            let reference = ideal(997.0, 0.5, RATE, 48_000);
            // From the first millisecond (the file itself starts abruptly).
            let worst = max_difference(&got[96..], &reference[96..]);
            assert!(worst < 2e-4, "44.1 kHz first track deviates by {worst:e}");
            assert_eq!(
                got.chunks(2)
                    .take(600)
                    .filter(|f| f[0] == f[1] && f[0] != 0.0)
                    .count(),
                0
            );
        }

        #[test]
        fn seek_pause_resume_stop_and_track_changes_still_work() {
            let dir = tempfile::tempdir().unwrap();
            let (a, b) = files(dir.path());
            let (mut output, device) = setup();

            // Play: the position follows what the device has pulled.
            output.load(&a, Duration::ZERO, false).unwrap();
            device.next_frames(24_000);
            let pos = output.position().as_secs_f64();
            let pulled = device.frames() as f64 / RATE as f64;
            assert!(
                (pos - pulled).abs() < 0.05,
                "position {pos:.3} s vs {pulled:.3} s pulled"
            );

            // Seek forward, then back.
            output.seek(Duration::from_secs(3)).unwrap();
            device.next_frames(4_800);
            let pos = output.position().as_secs_f64();
            assert!(
                (3.0..3.2).contains(&pos),
                "after seeking to 3 s: {pos:.3} s"
            );
            output.seek(Duration::from_millis(500)).unwrap();
            let after = device.next_frames(9_600);
            let pos = output.position().as_secs_f64();
            assert!(
                (0.5..0.7).contains(&pos),
                "after seeking to 0.5 s: {pos:.3} s"
            );
            // Still a clean tone after the seek settles.
            assert!(worst_block_thd(&left(&after[2_400 * 2..]), RATE as f64, 997.0) < -80.0);

            // Pause: silence, position held; resume: sound again.
            output.pause();
            device.next_frames(2_400);
            let held = output.position();
            assert!(silent(&device.next_frames(9_600)), "audio while paused");
            assert_eq!(output.position(), held);
            output.play();
            device.next_frames(2_400);
            assert!(
                !silent(&device.next_frames(4_800)),
                "no audio after resuming"
            );
            assert!(output.position() > held);

            // Change to a 48 kHz track: plays exactly once the switch is done.
            output.load(&b, Duration::ZERO, false).unwrap();
            let got = device.next_frames(24_000);
            let want: Vec<f32> = decoder(&b).collect();
            let probe = &want[4_800 * 2..4_800 * 2 + 64];
            assert!(
                got.windows(64)
                    .any(|w| w.iter().zip(probe).all(|(x, y)| x.to_bits() == y.to_bits())),
                "the 48 kHz track is not passed through exactly"
            );

            // It ends: the output reports it.
            let started = Instant::now();
            while !output.finished() {
                assert!(
                    started.elapsed() < Duration::from_secs(10),
                    "never finished"
                );
                std::thread::sleep(Duration::from_millis(2));
            }

            // Load paused at an offset, then play; stop silences it.
            output.load(&a, Duration::from_millis(1_500), true).unwrap();
            device.next_frames(2_400);
            assert!(
                silent(&device.next_frames(4_800)),
                "audio while loaded paused"
            );
            let pos = output.position().as_secs_f64();
            assert!(
                (1.45..1.6).contains(&pos),
                "loaded at 1.5 s, reports {pos:.3} s"
            );
            output.play();
            device.next_frames(2_400);
            assert!(!silent(&device.next_frames(4_800)));
            output.stop();
            device.next_frames(4_800);
            assert!(silent(&device.next_frames(4_800)), "audio after stop");
        }

        // ── Gapless on the real output ──────────────────────────────────────

        /// Writes `frames` of a continuous 997 Hz tone that starts at `t0`
        /// seconds, as WAV or AIFF.
        fn piece(path: &Path, rate: u32, t0: f64, frames: usize) {
            let samples = tone(997.0, 0.5, rate, t0, frames);
            if path.extension().is_some_and(|e| e == "aiff") {
                write_aiff(path, rate, &samples);
            } else {
                write_wav(path, rate, 2, &samples);
            }
        }

        #[test]
        fn consecutive_tracks_join_without_a_gap_at_any_rate() {
            let cases: [(&str, u32, &str, u32); 5] = [
                ("wav", 44_100, "wav", 44_100),
                ("wav", 48_000, "wav", 48_000),
                ("wav", 44_100, "wav", 48_000),
                ("wav", 48_000, "wav", 44_100),
                ("wav", 44_100, "aiff", 44_100),
            ];
            for (ext_a, ra, ext_b, rb) in cases {
                let dir = tempfile::tempdir().unwrap();
                let a = dir.path().join(format!("a.{ext_a}"));
                let b = dir.path().join(format!("b.{ext_b}"));
                let frames_a = ra as usize / 2 + 37;
                piece(&a, ra, 0.0, frames_a);
                piece(&b, rb, frames_a as f64 / ra as f64, rb as usize / 2);

                let (mixer, mut mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
                let mut output = DeviceOutput::on_mixer(&mixer, nz(RATE));
                output.load(&a, Duration::ZERO, false).unwrap();
                output.prepare_next(Some(&b), 0).unwrap();
                let total = RATE as usize - 2_000;
                let out: Vec<f32> = mixed.by_ref().take(total * 2).collect();
                assert_eq!(
                    output.transitions(),
                    1,
                    "{ra} → {rb}: the prepared track did not follow"
                );
                // Into B by about 0.4 s: its own position, not the run's.
                let pos = output.position().as_secs_f64();
                let expected = total as f64 / RATE as f64 - frames_a as f64 / ra as f64;
                assert!(
                    (pos - expected).abs() < 0.01,
                    "{ra} → {rb}: position {pos:.3} s, expected {expected:.3} s"
                );

                let reference = ideal(997.0, 0.5, RATE, total);
                let join = (frames_a as f64 * RATE as f64 / ra as f64).round() as usize;
                let range = (join - 480) * 2..(join + 480) * 2;
                let worst = max_difference(&out[range.clone()], &reference[range.clone()]);
                let silent = out[range]
                    .chunks(2)
                    .filter(|f| f[0] == 0.0 && f[1] == 0.0)
                    .count();
                println!("{ext_a} {ra} Hz → {ext_b} {rb} Hz: largest deviation within 10 ms of the join {worst:.5}, silent frames {silent}");
                assert_eq!(silent, 0, "{ra} → {rb}: silence inserted");
                // Same rate: the 16-bit files' own precision. A change of rate:
                // the stage's measured transition (resample.rs tests).
                let limit = if ra == rb { 2e-4 } else { 0.01 };
                assert!(
                    worst < limit,
                    "{ext_a} {ra} → {ext_b} {rb}: deviation {worst}"
                );
            }
        }

        #[test]
        fn a_mono_track_followed_by_a_stereo_track_switches_on_the_exact_sample() {
            let dir = tempfile::tempdir().unwrap();
            let a = dir.path().join("mono.wav");
            let b = dir.path().join("stereo.wav");
            let mono: Vec<f32> = tone(500.0, 0.4, RATE, 0.0, 4_801)
                .into_iter()
                .step_by(2)
                .collect();
            write_wav(&a, RATE, 1, &mono);
            write_wav(&b, RATE, 2, &tone(700.0, 0.4, RATE, 0.0, 4_800));
            let want_a: Vec<f32> = decoder(&a).collect();
            let want_b: Vec<f32> = decoder(&b).collect();
            let (mixer, mut mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let mut output = DeviceOutput::on_mixer(&mixer, nz(RATE));
            output.load(&a, Duration::ZERO, false).unwrap();
            output.prepare_next(Some(&b), 0).unwrap();
            let out: Vec<f32> = mixed.by_ref().take((4_801 + 4_800) * 2).collect();
            assert!(out[..4_801 * 2]
                .chunks(2)
                .zip(&want_a)
                .all(|(f, m)| f[0] == *m && f[1] == *m));
            assert!(out[4_801 * 2..]
                .iter()
                .zip(&want_b)
                .all(|(x, y)| x.to_bits() == y.to_bits()));
        }

        /// A library with the given files as tracks, in order.
        fn library_of(conn: &Connection, files: &[std::path::PathBuf]) -> Vec<i64> {
            files
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let id = add_track(conn, &format!("Piece {i}"), "A", "X", i as i64 + 1);
                    conn.execute(
                        "UPDATE tracks SET path = ?1 WHERE id = ?2",
                        rusqlite::params![f.to_string_lossy(), id],
                    )
                    .unwrap();
                    id
                })
                .collect()
        }

        #[test]
        fn the_controller_plays_a_run_of_tracks_without_gaps() {
            // Six pieces of one continuous tone, alternating 44.1 and 48 kHz,
            // 0.3 s each, played through the controller as the app does.
            let dir = tempfile::tempdir().unwrap();
            let mut files = Vec::new();
            let mut t = 0.0;
            for i in 0..6 {
                let rate = if i % 2 == 0 { 44_100 } else { 48_000 };
                let frames = (rate as f64 * 0.3) as usize + i * 7;
                let path = dir.path().join(format!("piece-{i}.wav"));
                piece(&path, rate, t, frames);
                t += frames as f64 / rate as f64;
                files.push(path);
            }
            let conn = crate::library::tests::conn();
            let ids = library_of(&conn, &files);
            let (mixer, mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let device = Device::start(mixed);
            let mut c = Controller::new(
                Some(Box::new(DeviceOutput::on_mixer(&mixer, nz(RATE)))),
                100,
            );
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            // The ticker's role, much faster than real time to match the device thread.
            let started = Instant::now();
            let mut seen = vec![ids[0]];
            while c.state(&conn).unwrap().playing {
                assert!(
                    started.elapsed() < Duration::from_secs(20),
                    "playback never ended"
                );
                c.tick_at(&conn, 10, Instant::now()).unwrap();
                if let Some(track) = c.state(&conn).unwrap().track {
                    if *seen.last().unwrap() != track.id {
                        seen.push(track.id);
                    }
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(seen, ids, "each track became current once, in order");
            for id in &ids {
                assert_eq!(library::track(&conn, *id).unwrap().unwrap().play_count, 1);
            }
            // The whole run is one continuous tone: no gap at any of the five joins.
            let out = device.pulled.lock().unwrap().clone();
            let total = (t * RATE as f64) as usize;
            let reference = ideal(997.0, 0.5, RATE, total);
            // Where the run starts: the tone's onset (its power is 0.25 once
            // playing; the filter's faint pre-ringing before it is not the
            // start), refined to the best-fitting frame.
            let onset = out
                .chunks(2)
                .position(|f| f[0] * f[0] + f[1] * f[1] > 0.125)
                .unwrap();
            let start = (onset.saturating_sub(4)..onset + 4)
                .min_by(|&x, &y| {
                    let e = |k: usize| {
                        max_difference(&out[(k + 100) * 2..(k + 2_000) * 2], &reference[200..4_000])
                    };
                    e(x).total_cmp(&e(y))
                })
                .unwrap();
            let played = &out[start * 2..(start + total - 100) * 2];
            let silent = played
                .chunks(2)
                .filter(|f| f[0] == 0.0 && f[1] == 0.0)
                .count();
            // A gap or jump is a sharp break in the waveform: its second
            // difference spikes far above the tone's own smooth curvature.
            let left: Vec<f64> = played.iter().step_by(2).map(|&v| v as f64).collect();
            let curvature = |range: std::ops::Range<usize>| {
                range
                    .map(|n| (left[n] - 2.0 * left[n - 1] + left[n - 2]).abs())
                    .fold(0.0, f64::max)
            };
            let tone_curvature = 0.5 * (std::f64::consts::TAU * 997.0 / RATE as f64).powi(2);
            let worst = curvature(2..left.len()) / tone_curvature;
            // Timing at the end of the run: within two frames of ideal (each
            // change into a 48 kHz piece places it on whole frames).
            let tail = left.len() - 2_100;
            let offset = (-3i64..=3)
                .min_by(|&x, &y| {
                    let e = |k: i64| {
                        (0..2_000)
                            .map(|n| {
                                (left[tail + n]
                                    - reference[((tail + n) as i64 + k) as usize * 2] as f64)
                                    .abs()
                            })
                            .fold(0.0, f64::max)
                    };
                    e(x).total_cmp(&e(y))
                })
                .unwrap();
            println!("six-track run: {silent} silent frames, sharpest break {worst:.2}× the tone's curvature, timing at the end {offset} frames");
            assert_eq!(silent, 0);
            // Every join here is a change of rate, whose measured residue
            // (about 0.005 near 20 kHz, see resample.rs) reads as about 3×.
            // Losing or repeating a single frame would read as about 15×, a
            // gap as 50× or more.
            assert!(worst < 5.0, "a break {worst:.2}× sharper than the tone");
            assert!(offset.abs() <= 2, "timing off by {offset} frames");
        }

        #[test]
        fn next_and_previous_still_work_during_gapless_playback() {
            let dir = tempfile::tempdir().unwrap();
            let files: Vec<_> = (0..3)
                .map(|i| {
                    let p = dir.path().join(format!("t{i}.wav"));
                    piece(&p, 44_100, 0.0, 44_100 * 2);
                    p
                })
                .collect();
            let conn = crate::library::tests::conn();
            let ids = library_of(&conn, &files);
            let (mixer, mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let device = Device::start(mixed);
            let mut c = Controller::new(
                Some(Box::new(DeviceOutput::on_mixer(&mixer, nz(RATE)))),
                100,
            );
            c.play_tracks(&conn, ids.clone(), 0, 1).unwrap();
            c.sync_preload(&conn);
            device.next_frames(9_600);
            c.next(&conn, 2).unwrap();
            c.sync_preload(&conn);
            device.next_frames(9_600);
            let state = c.state(&conn).unwrap();
            assert_eq!(state.track.unwrap().id, ids[1]);
            assert!(state.position_ms < 1_000);
            c.previous(&conn, 3).unwrap();
            c.sync_preload(&conn);
            device.next_frames(9_600);
            assert_eq!(c.state(&conn).unwrap().track.unwrap().id, ids[0]);
            assert!(!silent(&device.next_frames(4_800)));
        }

        /// Not a correctness test: prints what preparing a track costs. Run with
        /// `cargo test --release --lib preparation_cost -- --ignored --nocapture`.
        #[test]
        #[ignore]
        fn preparation_cost() {
            let dir = tempfile::tempdir().unwrap();
            let wav = dir.path().join("long.wav");
            let aiff = dir.path().join("long.aiff");
            piece(&wav, 44_100, 0.0, 44_100 * 180);
            piece(&aiff, 44_100, 0.0, 44_100 * 180);
            let (first_mixer, _unpulled) = rodio::mixer::mixer(STEREO, nz(RATE));
            let mut first = DeviceOutput::on_mixer(&first_mixer, nz(RATE));
            first.load(&wav, Duration::ZERO, false).unwrap();
            for path in [&wav, &aiff] {
                let started = Instant::now();
                for _ in 0..50 {
                    first.prepare_next(Some(path), 0).unwrap();
                }
                println!(
                    "prepare_next ({}, 3-minute file): {:.3} ms on average over 50",
                    path.extension().unwrap().to_string_lossy(),
                    started.elapsed().as_secs_f64() * 1000.0 / 50.0
                );
            }
            // The switch on the audio thread: the slowest single sample pulled
            // around the boundary (includes decoding the new track's first packet).
            // A fresh output: reloading one whose mixer nobody pulls would wait
            // for the old audio to drain (rodio's stop, then append).
            let short = dir.path().join("short.wav");
            piece(&short, 44_100, 0.0, 4_410);
            let (mixer, mut mixed) = rodio::mixer::mixer(STEREO, nz(RATE));
            let mut output = DeviceOutput::on_mixer(&mixer, nz(RATE));
            output.load(&short, Duration::ZERO, false).unwrap();
            output.prepare_next(Some(&wav), 0).unwrap();
            // The short track is 0.1 s: about 9 600 samples at 48 kHz.
            let mut times = Vec::new();
            for _ in 0..20_000 {
                let t = Instant::now();
                mixed.next();
                times.push(t.elapsed());
            }
            let switch = times[9_000..11_000].iter().max().unwrap();
            let steady = times[1_000..8_000].iter().max().unwrap();
            println!(
                "slowest sample: {:.1} µs around the switch, {:.1} µs in steady playback (a 10 ms device callback holds 960 samples)",
                switch.as_secs_f64() * 1e6,
                steady.as_secs_f64() * 1e6
            );
            println!(
                "transitions after the measurement: {}",
                output.transitions()
            );
            // Memory held by a prepared track: 100 held at once.
            let private = || {
                let out = std::process::Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-Command",
                        &format!(
                            "(Get-Process -Id {}).PrivateMemorySize64",
                            std::process::id()
                        ),
                    ])
                    .output()
                    .unwrap();
                String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse::<f64>()
                    .unwrap()
            };
            let before = private();
            let held: Vec<_> = (0..100).map(|_| output.open_track(&wav).unwrap()).collect();
            let after = private();
            println!(
                "memory per prepared track: about {:.0} KB (100 held: {:.1} MB)",
                (after - before) / 100.0 / 1024.0,
                (after - before) / 1048576.0
            );
            drop(held);
        }
    }
}
