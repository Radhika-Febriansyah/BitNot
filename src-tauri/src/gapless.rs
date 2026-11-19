//! Gapless playback: one rodio source plays a run of tracks back to back.
//!
//! The Player gets a single [`TrackChain`] per run. It plays the current
//! track and, the moment that track's decoder runs dry, continues with the
//! next track that the controller prepared in advance, within the same call
//! on the audio thread. Nothing waits for the controller's ticker, and no
//! decoder is opened at the boundary, so no silence is inserted.
//!
//! The next track is prepared on the controller's thread (file opened, format
//! probed) and handed over through [`ChainShared`]. Each preparation is tagged
//! with the number of transitions made so far; the chain only takes one meant
//! for the track it is finishing, so a preparation the controller made before
//! it noticed a transition can never be played in the wrong place. Without a
//! valid preparation the chain simply ends, and the controller loads the next
//! track the ordinary way (as before gapless playback existed).

use std::num::NonZero;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;

/// A decoded track, ready to play.
pub type Track = Box<dyn Source + Send>;

/// State the chain shares with the output that created it.
#[derive(Default)]
pub struct ChainShared {
    /// The prepared next track, and the transition count it was prepared at.
    next: Mutex<Option<(u64, Track)>>,
    /// Samples of the current track delivered so far.
    samples: AtomicU64,
    /// `channels << 32 | rate` of the current track.
    format: AtomicU64,
}

impl ChainShared {
    /// Replaces the prepared next track (or clears it with `None`). The old
    /// one is dropped after the lock is released.
    pub fn prepare(&self, next: Option<(u64, Track)>) {
        let old = std::mem::replace(&mut *self.next.lock().unwrap(), next);
        drop(old);
    }

    /// Whether a track is prepared.
    #[cfg(test)]
    pub fn is_prepared(&self) -> bool {
        self.next.lock().unwrap().is_some()
    }

    /// Position in the current track.
    pub fn position(&self) -> Duration {
        let format = self.format.load(Ordering::Relaxed);
        let (channels, rate) = ((format >> 32).max(1) as f64, (format as u32).max(1) as f64);
        Duration::from_secs_f64(self.samples.load(Ordering::Relaxed) as f64 / channels / rate)
    }

    fn set_format(&self, track: &Track) {
        let format = ((track.channels().get() as u64) << 32) | track.sample_rate().get() as u64;
        self.format.store(format, Ordering::Relaxed);
    }
}

/// See the module documentation.
pub struct TrackChain {
    current: Track,
    shared: Arc<ChainShared>,
    /// Transitions made, counted across every chain of one output.
    transitions: Arc<AtomicU64>,
    samples: u64,
}

impl TrackChain {
    pub fn new(first: Track, shared: Arc<ChainShared>, transitions: Arc<AtomicU64>) -> Self {
        shared.set_format(&first);
        shared.samples.store(0, Ordering::Relaxed);
        TrackChain {
            current: first,
            shared,
            transitions,
            samples: 0,
        }
    }

    /// Takes the prepared track if it was prepared for this transition.
    fn take_next(&mut self) -> Option<Track> {
        let made = self.transitions.load(Ordering::Acquire);
        let prepared = self.shared.next.lock().unwrap().take();
        match prepared {
            Some((at, track)) if at == made => Some(track),
            // Prepared before the controller caught up: never play it here.
            _ => None,
        }
    }
}

impl Iterator for TrackChain {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        loop {
            if let Some(sample) = self.current.next() {
                self.samples += 1;
                self.shared.samples.store(self.samples, Ordering::Relaxed);
                return Some(sample);
            }
            // The current track is done: continue with the prepared one.
            let next = self.take_next()?;
            self.shared.set_format(&next);
            self.samples = 0;
            self.shared.samples.store(0, Ordering::Relaxed);
            // Replacing the finished track drops it (its file closes here).
            self.current = next;
            self.transitions.fetch_add(1, Ordering::AcqRel);
        }
    }
}

impl Source for TrackChain {
    fn current_span_len(&self) -> Option<usize> {
        self.current.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.current.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.current.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.current.try_seek(position)?;
        let frames =
            (position.as_secs_f64() * self.current.sample_rate().get() as f64).round() as u64;
        self.samples = frames * self.current.channels().get() as u64;
        self.shared.samples.store(self.samples, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::sync::atomic::AtomicUsize;

    const TWO: NonZero<u16> = NonZero::new(2).unwrap();

    /// A track whose samples say where they come from: track `t`, frame `n`
    /// → left `t * 1000 + n`, right negative (scaled into ±1).
    fn track(t: u32, frames: usize, rate: u32) -> Track {
        let data: Vec<f32> = (0..frames)
            .flat_map(|n| {
                let v = (t as f32 * 1000.0 + n as f32) / 1.0e6;
                [v, -v]
            })
            .collect();
        Box::new(SamplesBuffer::new(TWO, NonZero::new(rate).unwrap(), data))
    }

    fn new_chain(first: Track) -> (TrackChain, Arc<ChainShared>, Arc<AtomicU64>) {
        let shared = Arc::new(ChainShared::default());
        let transitions = Arc::new(AtomicU64::new(0));
        (
            TrackChain::new(first, shared.clone(), transitions.clone()),
            shared,
            transitions,
        )
    }

    #[test]
    fn the_prepared_track_follows_on_the_very_next_sample() {
        let (mut chain, shared, transitions) = new_chain(track(1, 100, 48_000));
        shared.prepare(Some((0, track(2, 50, 48_000))));
        let out: Vec<f32> = chain.by_ref().collect();
        let want: Vec<f32> = track(1, 100, 48_000).chain(track(2, 50, 48_000)).collect();
        assert_eq!(
            out, want,
            "A then B, nothing missing, repeated, or inserted"
        );
        assert_eq!(transitions.load(Ordering::Relaxed), 1);
        assert!(!shared.is_prepared());
    }

    #[test]
    fn position_is_per_track_and_follows_seeks() {
        let (mut chain, shared, _) = new_chain(track(1, 48_000, 48_000));
        shared.prepare(Some((0, track(2, 48_000, 48_000))));
        let _ = chain.by_ref().take(24_000 * 2).count();
        assert_eq!(shared.position(), Duration::from_millis(500));
        let _ = chain.by_ref().take(24_000 * 2 + 4_800 * 2).count();
        // Into the second track: 100 ms in.
        assert_eq!(shared.position(), Duration::from_millis(100));
        chain.try_seek(Duration::from_millis(750)).unwrap();
        assert_eq!(shared.position(), Duration::from_millis(750));
    }

    #[test]
    fn a_preparation_made_for_an_earlier_transition_is_never_played() {
        let (mut chain, shared, transitions) = new_chain(track(1, 10, 48_000));
        // Prepared as if after one transition, but none has happened.
        shared.prepare(Some((1, track(9, 10, 48_000))));
        assert_eq!(
            chain.by_ref().count(),
            20,
            "only track 1 plays; the chain then ends"
        );
        assert_eq!(transitions.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_preparation_can_be_replaced_or_withdrawn_until_the_boundary() {
        let (mut chain, shared, _) = new_chain(track(1, 10, 48_000));
        shared.prepare(Some((0, track(2, 10, 48_000))));
        shared.prepare(Some((0, track(3, 10, 48_000))));
        let out: Vec<f32> = chain.by_ref().collect();
        assert_eq!(
            out[20],
            3000.0 / 1.0e6,
            "the replacement plays, not the first choice"
        );

        let (mut chain, shared, _) = new_chain(track(1, 10, 48_000));
        shared.prepare(Some((0, track(2, 10, 48_000))));
        shared.prepare(None);
        assert_eq!(chain.by_ref().count(), 20);
    }

    /// Counts how many tracks are alive, to show finished ones are released.
    struct Counted {
        inner: Track,
        alive: Arc<AtomicUsize>,
    }
    impl Drop for Counted {
        fn drop(&mut self) {
            self.alive.fetch_sub(1, Ordering::Relaxed);
        }
    }
    impl Iterator for Counted {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            self.inner.next()
        }
    }
    impl Source for Counted {
        fn current_span_len(&self) -> Option<usize> {
            self.inner.current_span_len()
        }
        fn channels(&self) -> NonZero<u16> {
            self.inner.channels()
        }
        fn sample_rate(&self) -> NonZero<u32> {
            self.inner.sample_rate()
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    #[test]
    fn a_long_run_keeps_at_most_two_tracks_alive_and_loses_nothing() {
        let alive = Arc::new(AtomicUsize::new(0));
        let counted = |t: u32| -> Track {
            alive.fetch_add(1, Ordering::Relaxed);
            Box::new(Counted {
                inner: track(t, 1_000 + t as usize, 48_000),
                alive: alive.clone(),
            })
        };
        let (mut chain, shared, transitions) = new_chain(counted(0));
        let mut most_alive = 0;
        let mut samples = 0usize;
        // The controller's role: after each transition, prepare the next track.
        for t in 1..200u32 {
            shared.prepare(Some((t as u64 - 1, counted(t))));
            while transitions.load(Ordering::Relaxed) < t as u64 {
                chain.next().unwrap();
                samples += 1;
                most_alive = most_alive.max(alive.load(Ordering::Relaxed));
            }
        }
        samples += chain.by_ref().count();
        let expected: usize = (0..200usize).map(|t| (1_000 + t) * 2).sum();
        assert_eq!(samples, expected, "every sample of every track, once");
        assert_eq!(transitions.load(Ordering::Relaxed), 199);
        assert!(
            most_alive <= 2,
            "at most the playing and the prepared track ({most_alive})"
        );
        drop(chain);
        assert_eq!(alive.load(Ordering::Relaxed), 0);
    }
}
