//! Whether the mouse cursor stayed where it was from one image to the next.

use std::time::{Duration, Instant};

use sc_ohseearr_capture::REST_PX;

/// Follows the cursor over the stream's buffers and tells, for each image, whether it moved since the image
/// before. The cursor's position comes only with the buffers, so this stands in for polling it: at one image
/// per `REST`, an image at rest shows a cursor that stayed for at least that long.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct Still {
    /// Where the cursor is (None: not in the window)
    at: Option<(i32, i32)>,
    /// Since when it is there
    since: Instant,
    /// It moved since the last image
    moved: bool,
    /// An image came already (the first one has none before it)
    image_seen: bool,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl Still {
    pub(crate) fn new(now: Instant) -> Self {
        Self { at: None, since: now, moved: false, image_seen: false }
    }

    /// The cursor of a buffer that came at `now`, with or without an image (None: not in the window). It stays
    /// while it is within `REST_PX`; leaving or entering the window is a move.
    pub(crate) fn cursor(&mut self, at: Option<(i32, i32)>, now: Instant) {
        let moved = match (self.at, at) {
            (Some(a), Some(b)) => (a.0 - b.0).abs() > REST_PX || (a.1 - b.1).abs() > REST_PX,
            (None, None) => false,
            _ => true,
        };
        if moved {
            self.at = at;
            self.since = now;
            self.moved = true;
        }
    }

    /// An image came at `now` (its buffer's cursor was given to [`cursor`](Self::cursor) before): how long the
    /// cursor has stayed where it is, or None when it moved since the image before or this is the first image.
    pub(crate) fn image(&mut self, now: Instant) -> Option<Duration> {
        let still = self.image_seen && !self.moved;
        self.image_seen = true;
        self.moved = false;
        still.then(|| now.saturating_duration_since(self.since))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: Duration = Duration::from_millis(200);

    /// One buffer with an image.
    fn image(still: &mut Still, at: Option<(i32, i32)>, now: Instant) -> Option<Duration> {
        still.cursor(at, now);
        still.image(now)
    }

    #[test]
    fn an_image_is_at_rest_when_the_cursor_is_where_it_was_in_the_image_before() {
        let t0 = Instant::now();
        let mut still = Still::new(t0);
        // The first image has none before it
        assert_eq!(image(&mut still, Some((100, 100)), t0), None);
        // Within REST_PX on both axes: at rest, since the first image
        assert_eq!(image(&mut still, Some((103, 97)), t0 + STEP), Some(STEP));
        assert_eq!(image(&mut still, Some((100, 100)), t0 + 2 * STEP), Some(2 * STEP));
        // Beyond it: moved; the image after it is at rest again, since the move
        assert_eq!(image(&mut still, Some((104, 100)), t0 + 3 * STEP), None);
        assert_eq!(image(&mut still, Some((104, 100)), t0 + 4 * STEP), Some(STEP));
    }

    #[test]
    fn a_move_between_two_images_counts() {
        let t0 = Instant::now();
        let mut still = Still::new(t0);
        image(&mut still, Some((100, 100)), t0);
        // A buffer without an image: the cursor went away and came back
        still.cursor(Some((500, 500)), t0 + STEP / 4);
        still.cursor(Some((100, 100)), t0 + STEP / 2);
        assert_eq!(image(&mut still, Some((100, 100)), t0 + STEP), None);
        assert_eq!(image(&mut still, Some((100, 100)), t0 + 2 * STEP), Some(STEP + STEP / 2));
    }

    #[test]
    fn leaving_the_window_is_a_move_staying_outside_is_rest() {
        let t0 = Instant::now();
        let mut still = Still::new(t0);
        // No cursor from the start: the first image has none before it, the next is at rest
        assert_eq!(image(&mut still, None, t0), None);
        assert_eq!(image(&mut still, None, t0 + STEP), Some(STEP));
        assert_eq!(image(&mut still, Some((10, 10)), t0 + 2 * STEP), None);
        assert_eq!(image(&mut still, None, t0 + 3 * STEP), None);
        assert_eq!(image(&mut still, None, t0 + 4 * STEP), Some(STEP));
    }
}
