//! The change gate: which live frames are worth reading.

use image::GrayImage;

use crate::pixels::THUMB;

// Mean absolute difference of the thumbnails (0–255): below STILL the view did not move, above NEW it
// changed. Changes on the terminal are small (a message over the panel going away: 0.3), so during a
// visit it is read again every REREAD_MS anyway.
const STILL: f32 = 3.0;
const NEW: f32 = 1.5;
const REREAD_MS: u64 = 2_000;

/// Decides which live frames are worth reading, from their grey thumbnails ([`crate::Pixels::thumb`]).
pub struct Gate {
    previous: Option<GrayImage>,
    last_read: Option<GrayImage>,
    last_read_ms: u64,
    /// The view moved since the last read (idle: only a view that came to rest is checked)
    moved: bool,
    /// At a refinery terminal: read again every REREAD_MS
    visit: bool,
    /// How long after a read a still view is read again during a visit
    reread_ms: u64,
}

impl Default for Gate {
    fn default() -> Self {
        // The first still view is checked
        Self { previous: None, last_read: None, last_read_ms: 0, moved: true, visit: false, reread_ms: REREAD_MS }
    }
}

fn diff(a: &GrayImage, b: &GrayImage) -> f32 {
    let sum: u64 = a.pixels().zip(b.pixels()).map(|(p, q)| p[0].abs_diff(q[0]) as u64).sum();
    sum as f32 / (THUMB.0 * THUMB.1) as f32
}

impl Gate {
    /// Whether to read the frame with this thumbnail (taken at `at_ms`), still since the last frame: idle,
    /// once the view came to rest after it moved; during a visit, when it changed since the last read or
    /// REREAD_MS after it.
    pub fn pass(&mut self, t: GrayImage, at_ms: u64) -> bool {
        let still = self.previous.as_ref().is_some_and(|p| diff(p, &t) < STILL);
        if self.previous.is_some() && !still {
            self.moved = true;
        }
        let pass = still
            && if self.visit {
                let new = self.last_read.as_ref().is_none_or(|l| diff(l, &t) > NEW);
                new || at_ms.saturating_sub(self.last_read_ms) >= self.reread_ms
            } else {
                self.moved
            };
        if pass {
            self.moved = false;
            self.last_read = Some(t.clone());
            self.last_read_ms = at_ms;
        }
        self.previous = Some(t);
        pass
    }

    /// Whether a visit is going on after the read of the passed frame.
    pub fn read(&mut self, visit: bool) {
        self.visit = visit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pixels;
    use image::RgbImage;

    fn flat(v: u8) -> GrayImage {
        Pixels::Rgb(RgbImage::from_pixel(192, 80, image::Rgb([v, v, v]))).thumb()
    }

    #[test]
    fn gate_checks_a_view_once_it_came_to_rest() {
        let mut gate = Gate::default();
        let (a, b) = (flat(40), flat(200));
        // First frame: nothing to compare with yet; then still: checked once
        assert!(!gate.pass(a.clone(), 0));
        assert!(gate.pass(a.clone(), 1_000));
        gate.read(false);
        assert!(!gate.pass(a.clone(), 2_000));
        assert!(!gate.pass(a.clone(), 10_000));
        // Small changes while standing (the scene's animations) are no movement: nothing
        assert!(!gate.pass(flat(42), 11_000));
        assert!(!gate.pass(a.clone(), 12_000));
        // Moving (not still), then at rest again: checked once
        assert!(!gate.pass(b.clone(), 13_000));
        assert!(gate.pass(b.clone(), 14_000));
        gate.read(false);
        assert!(!gate.pass(b, 15_000));
    }

    #[test]
    fn gate_reads_again_every_two_seconds_during_a_visit() {
        let mut gate = Gate::default();
        let a = flat(40);
        assert!(!gate.pass(a.clone(), 0));
        assert!(gate.pass(a.clone(), 1_000));
        gate.read(true);
        // Unchanged (a small change on the terminal looks the same in the thumbnail): again after 2 s
        assert!(!gate.pass(a.clone(), 2_000));
        assert!(gate.pass(a.clone(), 3_000));
        gate.read(false);
        // The visit ended: once per change again
        assert!(!gate.pass(a, 6_000));
    }
}
