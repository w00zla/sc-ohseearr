//! Since when the mouse cursor stays where it is.

use std::time::{Duration, Instant};

/// While a reader pulls frames: how often the cursor is looked at
pub const PULL_POLL: Duration = Duration::from_millis(15);
/// How far the cursor may move and still rest (physical pixels)
pub const REST_PX: i32 = 3;
/// How long it must rest before a grab (the game then shows the tooltip of the tile under it)
pub const REST: Duration = Duration::from_millis(200);

/// Since when the mouse cursor stays where it is.
pub struct Rest {
    at: (i32, i32),
    since: Instant,
}

impl Default for Rest {
    fn default() -> Self {
        Self { at: (0, 0), since: Instant::now() }
    }
}

impl Rest {
    /// Looks at the cursor now (Windows; a failing `GetCursorPos` changes nothing).
    #[cfg(windows)]
    pub fn update(&mut self) {
        if let Some(p) = crate::capture::cursor_pos() {
            self.observe(p, Instant::now());
        }
    }

    /// The cursor is at `p` (screen pixels) at `now`: it rests while it stays within `REST_PX`.
    pub fn observe(&mut self, p: (i32, i32), now: Instant) {
        if (p.0 - self.at.0).abs() > REST_PX || (p.1 - self.at.1).abs() > REST_PX {
            *self = Self { at: p, since: now };
        }
    }

    /// How long the cursor has rested.
    pub fn rested(&self) -> Duration {
        self.since.elapsed()
    }

    /// How long the cursor rested at `now`.
    pub fn rested_at(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.since)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_moves_keep_the_rest_big_ones_restart_it() {
        let t0 = Instant::now();
        let mut r = Rest { at: (100, 100), since: t0 };
        // Within REST_PX on both axes: still resting
        r.observe((103, 97), t0 + Duration::from_millis(50));
        assert_eq!(r.since, t0);
        assert_eq!(r.rested_at(t0 + Duration::from_millis(250)), Duration::from_millis(250));
        // Beyond it: restarts at the new place
        let t1 = t0 + Duration::from_millis(300);
        r.observe((104, 100), t1);
        assert_eq!((r.at, r.since), ((104, 100), t1));
        assert_eq!(r.rested_at(t1 + Duration::from_millis(10)), Duration::from_millis(10));
    }
}
