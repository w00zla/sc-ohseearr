//! The game's UI geometry.
//!
//! The UI scales with the frame's height (16:9 and ultrawide alike), so positions and sizes are measured as
//! shares of the height. Screens drawn as a panel in perspective around the frame's centre (the character
//! inventory) also scale with the FOV and aspect ratio as a whole about the centre: a factor `k` the reader
//! calibrates from a known element's position (1.0 where the shares were measured).

use image::RgbImage;

/// A panel around the frame's centre: x as a share of the height from the centre, y as a share of the height from
/// the top (0.5 = the centre), lengths as shares of the height, everything scaled by `k` about the centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CenterUi {
    cx: f32,
    cy: f32,
    s: f32,
    pub k: f32,
}

impl CenterUi {
    pub fn new(frame: &RgbImage, k: f32) -> Self {
        let (w, h) = frame.dimensions();
        Self::of_size(w, h, k)
    }

    pub fn of_size(width: u32, height: u32, k: f32) -> Self {
        Self { cx: width as f32 / 2.0, cy: height as f32 / 2.0, s: height as f32, k }
    }

    /// Frame x of a share measured from the centre.
    pub fn x(&self, share: f32) -> f32 {
        self.cx + self.k * share * self.s
    }

    /// Frame y of a share measured from the top.
    pub fn y(&self, share: f32) -> f32 {
        self.cy + self.k * (share - 0.5) * self.s
    }

    /// Frame pixels of a length given as a share.
    pub fn len(&self, share: f32) -> f32 {
        self.k * share * self.s
    }

    /// The frame's height (the unit of the shares before `k`).
    pub fn height(&self) -> f32 {
        self.s
    }

    pub fn center(&self) -> (f32, f32) {
        (self.cx, self.cy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_about_the_centre() {
        let ui = CenterUi::of_size(3840, 1600, 1.0);
        assert_eq!(ui.x(0.0), 1920.0);
        assert_eq!(ui.y(0.5), 800.0);
        assert_eq!(ui.len(0.1), 160.0);
        let k = CenterUi::of_size(1920, 1080, 1.05);
        assert!((k.x(0.1) - (960.0 + 1.05 * 108.0)).abs() < 1e-3);
    }
}
