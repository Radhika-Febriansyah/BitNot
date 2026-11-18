//! The play queue: a temporary playback order, separate from playlists.
//! Pure data and logic, with no audio or database access.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Queue {
    /// Order as queued, used to undo shuffle.
    original: Vec<i64>,
    /// Order that will actually play.
    items: Vec<i64>,
    current: Option<usize>,
    pub shuffle: bool,
    pub repeat: Repeat,
}

fn shuffle_slice(slice: &mut [i64], seed: u64) {
    // xorshift64*: small, deterministic for a given seed, good enough for shuffling.
    let mut state = seed | 1;
    let mut next = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    for i in (1..slice.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        slice.swap(i, j);
    }
}

impl Queue {
    /// Replaces the queue with `tracks`, starting at `start`.
    pub fn set(&mut self, tracks: Vec<i64>, start: usize, seed: u64) {
        self.original = tracks.clone();
        self.items = tracks;
        if self.items.is_empty() {
            self.current = None;
            return;
        }
        let start = start.min(self.items.len() - 1);
        if self.shuffle {
            self.items.swap(0, start);
            shuffle_slice(&mut self.items[1..], seed);
            self.current = Some(0);
        } else {
            self.current = Some(start);
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn current_track(&self) -> Option<i64> {
        self.current.and_then(|i| self.items.get(i).copied())
    }

    pub fn upcoming(&self) -> &[i64] {
        match self.current {
            Some(i) => &self.items[(i + 1).min(self.items.len())..],
            None => &self.items,
        }
    }

    /// Moves to the next track. `auto` is true when the current track ended by
    /// itself, which is the only case where Repeat One replays it.
    /// Returns `None` at the end of the queue (the position does not change).
    pub fn advance(&mut self, auto: bool) -> Option<i64> {
        if self.items.is_empty() {
            return None;
        }
        let Some(current) = self.current else {
            self.current = Some(0);
            return self.current_track();
        };
        if auto && self.repeat == Repeat::One {
            return self.current_track();
        }
        if current + 1 < self.items.len() {
            self.current = Some(current + 1);
        } else if self.repeat == Repeat::All {
            self.current = Some(0);
        } else {
            return None;
        }
        self.current_track()
    }

    /// Moves to the previous track; stays on the first unless Repeat All wraps.
    pub fn previous(&mut self) -> Option<i64> {
        if self.items.is_empty() {
            return None;
        }
        self.current = Some(match self.current {
            Some(0) | None if self.repeat == Repeat::All => self.items.len() - 1,
            Some(0) | None => 0,
            Some(i) => i - 1,
        });
        self.current_track()
    }

    /// Jumps to an upcoming track by its offset in [`Queue::upcoming`].
    pub fn skip_to_upcoming(&mut self, offset: usize) -> Option<i64> {
        let index = self.current.map_or(0, |c| c + 1) + offset;
        if index < self.items.len() {
            self.current = Some(index);
        }
        self.current_track()
    }

    pub fn play_next(&mut self, track: i64) {
        let at = self.current.map_or(0, |c| c + 1);
        let anchor = self
            .current_track()
            .and_then(|id| self.original.iter().position(|x| *x == id));
        self.items.insert(at, track);
        match anchor {
            Some(i) => self.original.insert(i + 1, track),
            None => self.original.insert(0, track),
        }
    }

    pub fn add(&mut self, track: i64) {
        self.items.push(track);
        self.original.push(track);
    }

    pub fn add_many(&mut self, tracks: &[i64]) {
        self.items.extend_from_slice(tracks);
        self.original.extend_from_slice(tracks);
    }

    /// The playing track followed by everything upcoming.
    pub fn remaining(&self) -> Vec<i64> {
        self.current_track()
            .into_iter()
            .chain(self.upcoming().iter().copied())
            .collect()
    }

    /// Moves an upcoming track from one offset to another (drag-and-drop).
    /// Returns false when either offset is out of range.
    pub fn move_upcoming(&mut self, from: usize, to: usize) -> bool {
        let base = self.current.map_or(0, |c| c + 1);
        let count = self.items.len().saturating_sub(base);
        if from >= count || to >= count {
            return false;
        }
        let id = self.items.remove(base + from);
        self.items.insert(base + to, id);
        if !self.shuffle {
            // Without shuffle the play order is the queue order.
            self.original = self.items.clone();
        }
        true
    }

    /// Removes one upcoming track. Returns false when the offset is out of range.
    pub fn remove_upcoming(&mut self, offset: usize) -> bool {
        let index = self.current.map_or(0, |c| c + 1) + offset;
        if index >= self.items.len() {
            return false;
        }
        let id = self.items.remove(index);
        if let Some(i) = self.original.iter().rposition(|x| *x == id) {
            self.original.remove(i);
        }
        true
    }

    pub fn clear_upcoming(&mut self) {
        match self.current {
            Some(i) => self.items.truncate(i + 1),
            None => self.items.clear(),
        }
        self.original = self.items.clone();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.original.clear();
        self.current = None;
    }

    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        if on {
            let from = self.current.map_or(0, |c| c + 1).min(self.items.len());
            shuffle_slice(&mut self.items[from..], seed);
        } else {
            let playing = self.current_track();
            self.items = self.original.clone();
            self.current = playing.and_then(|id| self.items.iter().position(|x| *x == id));
        }
    }

    /// Drops every occurrence of tracks that no longer exist.
    pub fn retain(&mut self, keep: impl Fn(i64) -> bool) {
        let playing = self.current_track().filter(|id| keep(*id));
        self.items.retain(|id| keep(*id));
        self.original.retain(|id| keep(*id));
        self.current = playing.and_then(|id| self.items.iter().position(|x| *x == id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(tracks: &[i64], start: usize) -> Queue {
        let mut q = Queue::default();
        q.set(tracks.to_vec(), start, 1);
        q
    }

    #[test]
    fn plays_in_order_and_stops_at_the_end() {
        let mut q = queue(&[1, 2, 3], 0);
        assert_eq!(q.current_track(), Some(1));
        assert_eq!(q.upcoming(), &[2, 3]);
        assert_eq!(q.advance(true), Some(2));
        assert_eq!(q.advance(true), Some(3));
        assert_eq!(q.advance(true), None);
        assert_eq!(q.current_track(), Some(3), "position is kept at the end");
    }

    #[test]
    fn starts_at_the_selected_track_and_continues() {
        let mut q = queue(&[1, 2, 3, 4], 2);
        assert_eq!(q.current_track(), Some(3));
        assert_eq!(q.upcoming(), &[4]);
        assert_eq!(q.previous(), Some(2));
        assert_eq!(q.previous(), Some(1));
        assert_eq!(q.previous(), Some(1), "stays on the first track");
    }

    #[test]
    fn repeat_modes() {
        let mut q = queue(&[1, 2], 1);
        q.repeat = Repeat::All;
        assert_eq!(q.advance(true), Some(1), "wraps to the start");
        assert_eq!(q.previous(), Some(2), "wraps to the end");

        q.repeat = Repeat::One;
        assert_eq!(q.advance(true), Some(2), "a finished track repeats");
        assert_eq!(q.advance(false), None, "pressing Next does not repeat it");
    }

    #[test]
    fn empty_queue_is_safe() {
        let mut q = Queue::default();
        assert_eq!(q.current_track(), None);
        assert_eq!(q.advance(true), None);
        assert_eq!(q.previous(), None);
        assert!(!q.remove_upcoming(0));
        q.set(vec![], 5, 1);
        assert!(q.is_empty());
    }

    #[test]
    fn play_next_and_add() {
        let mut q = queue(&[1, 2, 3], 0);
        q.play_next(9);
        q.add(8);
        assert_eq!(q.upcoming(), &[9, 2, 3, 8]);
        assert_eq!(q.advance(false), Some(9));

        let mut empty = Queue::default();
        empty.add(5);
        assert_eq!(empty.current_track(), None);
        assert_eq!(
            empty.advance(false),
            Some(5),
            "first advance starts the queue"
        );
    }

    #[test]
    fn remove_skip_and_clear_upcoming() {
        let mut q = queue(&[1, 2, 3, 4], 0);
        assert!(q.remove_upcoming(1));
        assert_eq!(q.upcoming(), &[2, 4]);
        assert!(!q.remove_upcoming(7));
        assert_eq!(q.skip_to_upcoming(1), Some(4));
        q.set(vec![1, 2, 3], 0, 1);
        q.clear_upcoming();
        assert_eq!(q.upcoming(), &[] as &[i64]);
        assert_eq!(q.current_track(), Some(1));
    }

    #[test]
    fn shuffle_keeps_current_first_and_every_track_once() {
        let mut q = Queue {
            shuffle: true,
            ..Queue::default()
        };
        let tracks: Vec<i64> = (1..=50).collect();
        q.set(tracks.clone(), 10, 12345);
        assert_eq!(q.current_track(), Some(11));
        let mut all: Vec<i64> = std::iter::once(11)
            .chain(q.upcoming().iter().copied())
            .collect();
        assert_ne!(all, tracks, "order is shuffled");
        all.sort_unstable();
        assert_eq!(all, tracks);
    }

    #[test]
    fn turning_shuffle_off_restores_order_around_the_current_track() {
        let mut q = queue(&[1, 2, 3, 4, 5], 1);
        q.set_shuffle(true, 99);
        assert_eq!(q.current_track(), Some(2));
        let mut upcoming = q.upcoming().to_vec();
        upcoming.sort_unstable();
        assert_eq!(upcoming, vec![3, 4, 5], "only upcoming tracks are shuffled");

        q.advance(false);
        let playing = q.current_track().unwrap();
        q.set_shuffle(false, 0);
        assert_eq!(q.current_track(), Some(playing));
        let expected: Vec<i64> = (playing + 1..=5).collect();
        assert_eq!(q.upcoming(), expected.as_slice());
    }

    #[test]
    fn upcoming_tracks_can_be_reordered_and_added_in_bulk() {
        let mut q = queue(&[1, 2, 3, 4], 0);
        assert!(q.move_upcoming(0, 2));
        assert_eq!(q.upcoming(), &[3, 4, 2]);
        assert!(q.move_upcoming(2, 0));
        assert_eq!(q.upcoming(), &[2, 3, 4]);
        assert!(!q.move_upcoming(0, 3));
        assert!(!q.move_upcoming(9, 0));
        assert_eq!(q.current_track(), Some(1), "the playing track never moves");

        q.add_many(&[7, 8]);
        assert_eq!(q.remaining(), vec![1, 2, 3, 4, 7, 8]);

        // Reordering sticks when shuffle is later turned on and off again.
        q.move_upcoming(4, 0);
        q.set_shuffle(true, 5);
        q.set_shuffle(false, 5);
        assert_eq!(q.upcoming(), &[8, 2, 3, 4, 7]);
    }

    #[test]
    fn retain_drops_removed_tracks() {
        let mut q = queue(&[1, 2, 3, 4], 1);
        q.retain(|id| id != 3);
        assert_eq!(q.current_track(), Some(2));
        assert_eq!(q.upcoming(), &[4]);
        q.retain(|id| id != 2);
        assert_eq!(q.current_track(), None);
    }

    #[test]
    fn survives_serialisation() {
        let mut q = queue(&[1, 2, 3], 1);
        q.repeat = Repeat::All;
        let restored: Queue = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
        assert_eq!(restored, q);
    }
}
