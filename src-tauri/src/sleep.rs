//! Sleep timer (CLAUDE.md §20): pause after 15, 30, 45, or 60 minutes, or
//! when the current track ends.
//!
//! A fixed-length timer is an end time on the monotonic clock ([`Instant`]),
//! not a countdown. Nothing has to tick for it to stay accurate, any reader
//! works out the time left for itself, and changing the Windows clock or time
//! zone cannot move it. The playback ticker already runs four times a second,
//! so checking the end time costs one comparison per tick and no extra thread.
//!
//! The timer lives only in memory. It is deliberately not restored after a
//! restart: the specification does not ask for it, and a timer that silently
//! came back would pause music the user had since started on purpose.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// The lengths offered (CLAUDE.md §20).
pub const PRESET_MINUTES: [u32; 4] = [15, 30, 45, 60];

/// What the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SleepRequest {
    /// Pause after this many minutes of real time.
    Minutes { minutes: u32 },
    /// Let the current track finish, then stop instead of starting the next.
    EndOfTrack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SleepKind {
    Minutes,
    EndOfTrack,
}

/// The armed timer as the interface sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepStatus {
    pub kind: SleepKind,
    /// The chosen length, for a fixed-length timer.
    pub minutes: Option<u32>,
    /// Time left when this status was produced, for a fixed-length timer.
    pub remaining_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Armed {
    Until { deadline: Instant, minutes: u32 },
    EndOfTrack,
}

#[derive(Debug, Default)]
pub struct SleepTimer {
    armed: Option<Armed>,
}

impl SleepTimer {
    /// Arms the timer, replacing any timer already set.
    pub fn set(&mut self, request: SleepRequest, now: Instant) -> Result<()> {
        self.armed = Some(match request {
            SleepRequest::Minutes { minutes } => {
                if !PRESET_MINUTES.contains(&minutes) {
                    return Err(AppError::invalid(
                        "The sleep timer can be set to 15, 30, 45, or 60 minutes.",
                    ));
                }
                let deadline = now
                    .checked_add(Duration::from_secs(u64::from(minutes) * 60))
                    .ok_or_else(|| AppError::invalid("The sleep timer could not be set."))?;
                Armed::Until { deadline, minutes }
            }
            SleepRequest::EndOfTrack => Armed::EndOfTrack,
        });
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.armed = None;
    }

    #[cfg(test)]
    pub fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    pub fn status(&self, now: Instant) -> Option<SleepStatus> {
        self.armed.map(|armed| match armed {
            Armed::Until { deadline, minutes } => SleepStatus {
                kind: SleepKind::Minutes,
                minutes: Some(minutes),
                remaining_ms: Some(deadline.saturating_duration_since(now).as_millis() as u64),
            },
            Armed::EndOfTrack => SleepStatus {
                kind: SleepKind::EndOfTrack,
                minutes: None,
                remaining_ms: None,
            },
        })
    }

    /// True once, when a fixed-length timer has reached its end time; the
    /// timer is disarmed in the same step, so it can never stop playback twice.
    pub fn take_expired(&mut self, now: Instant) -> bool {
        match self.armed {
            Some(Armed::Until { deadline, .. }) if now >= deadline => {
                self.armed = None;
                true
            }
            _ => false,
        }
    }

    /// True once, when an end-of-track timer is armed; disarms it.
    pub fn take_end_of_track(&mut self) -> bool {
        if self.armed == Some(Armed::EndOfTrack) {
            self.armed = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    fn minutes(minutes: u32) -> SleepRequest {
        SleepRequest::Minutes { minutes }
    }

    #[test]
    fn every_preset_expires_exactly_at_its_end_time() {
        let start = Instant::now();
        for preset in PRESET_MINUTES {
            let mut timer = SleepTimer::default();
            timer.set(minutes(preset), start).unwrap();
            let end = start + MIN * preset;
            let status = timer.status(start).unwrap();
            assert_eq!(status.kind, SleepKind::Minutes);
            assert_eq!(status.minutes, Some(preset));
            assert_eq!(status.remaining_ms, Some(u64::from(preset) * 60_000));

            let just_before = end - Duration::from_millis(1);
            assert!(!timer.take_expired(just_before), "{preset} min fired early");
            assert_eq!(timer.status(just_before).unwrap().remaining_ms, Some(1));
            assert!(timer.take_expired(end), "{preset} min did not fire");
            assert!(!timer.is_armed());
        }
    }

    #[test]
    fn fires_only_once() {
        let start = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set(minutes(15), start).unwrap();
        assert!(timer.take_expired(start + MIN * 20));
        assert!(!timer.take_expired(start + MIN * 21));
        assert!(!timer.take_expired(start + MIN * 600));
        assert_eq!(timer.status(start + MIN * 21), None);
    }

    #[test]
    fn only_the_offered_lengths_are_accepted() {
        let start = Instant::now();
        let mut timer = SleepTimer::default();
        for bad in [0, 1, 14, 16, 90, u32::MAX] {
            assert!(timer.set(minutes(bad), start).is_err(), "{bad}");
        }
        assert!(!timer.is_armed());
        // A rejected request leaves an existing timer alone.
        timer.set(minutes(30), start).unwrap();
        assert!(timer.set(minutes(7), start).is_err());
        assert_eq!(timer.status(start).unwrap().minutes, Some(30));
    }

    #[test]
    fn a_new_choice_replaces_the_old_one_and_cancel_disarms() {
        let start = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set(minutes(60), start).unwrap();
        timer.set(minutes(15), start + MIN * 10).unwrap();
        assert_eq!(
            timer.status(start + MIN * 10).unwrap().remaining_ms,
            Some(15 * 60_000)
        );
        timer
            .set(SleepRequest::EndOfTrack, start + MIN * 11)
            .unwrap();
        assert!(
            !timer.take_expired(start + MIN * 100),
            "end-of-track never expires by time"
        );
        assert_eq!(timer.status(start).unwrap().kind, SleepKind::EndOfTrack);
        timer.cancel();
        assert!(!timer.is_armed());
        assert!(!timer.take_end_of_track());
    }

    #[test]
    fn end_of_track_is_taken_once() {
        let mut timer = SleepTimer::default();
        timer.set(SleepRequest::EndOfTrack, Instant::now()).unwrap();
        assert!(timer.take_end_of_track());
        assert!(!timer.take_end_of_track());
    }

    #[test]
    fn requests_read_from_the_interface() {
        let parsed: SleepRequest =
            serde_json::from_str(r#"{"kind":"minutes","minutes":45}"#).unwrap();
        assert_eq!(parsed, minutes(45));
        let parsed: SleepRequest = serde_json::from_str(r#"{"kind":"endOfTrack"}"#).unwrap();
        assert_eq!(parsed, SleepRequest::EndOfTrack);
        assert!(serde_json::from_str::<SleepRequest>(r#"{"kind":"forever"}"#).is_err());
        let status = serde_json::to_value(SleepStatus {
            kind: SleepKind::EndOfTrack,
            minutes: None,
            remaining_ms: None,
        })
        .unwrap();
        assert_eq!(status["kind"], "endOfTrack");
    }
}
