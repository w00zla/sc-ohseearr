//! Rectangles and the crop of a window out of its monitor's image.

/// A rectangle in screen coordinates (physical pixels), like Win32's `RECT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// A part of the monitor's image, in its pixels.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct Crop {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// The window's client area (screen coordinates) within the monitor (`desktop`); None when outside.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn crop(client: Rect, desktop: Rect) -> Option<Crop> {
    let left = client.left.max(desktop.left);
    let top = client.top.max(desktop.top);
    let right = client.right.min(desktop.right);
    let bottom = client.bottom.min(desktop.bottom);
    (right > left && bottom > top).then(|| Crop {
        x: (left - desktop.left) as u32,
        y: (top - desktop.top) as u32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

/// The cursor (screen coordinates) relative to the crop's top left; None when outside the crop.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn cursor_in_crop(cursor: (i32, i32), desktop: Rect, crop: &Crop) -> Option<(f32, f32)> {
    let (x, y) = (cursor.0 - desktop.left - crop.x as i32, cursor.1 - desktop.top - crop.y as i32);
    (x >= 0 && y >= 0 && (x as u32) < crop.width && (y as u32) < crop.height).then_some((x as f32, y as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect { left, top, right, bottom }
    }

    #[test]
    fn crop_to_the_monitor() {
        // Fullscreen on the second monitor (right of the first)
        let second = rect(3840, 0, 7680, 1600);
        assert_eq!(crop(rect(3840, 0, 7680, 1600), second), Some(Crop { x: 0, y: 0, width: 3840, height: 1600 }));
        // A window partly off the monitor: only the visible part
        assert_eq!(crop(rect(3700, 100, 4840, 900), second), Some(Crop { x: 0, y: 100, width: 1000, height: 800 }));
        // Monitor left of the primary (negative coordinates)
        assert_eq!(
            crop(rect(-1800, 50, -200, 950), rect(-1920, 0, 0, 1080)),
            Some(Crop { x: 120, y: 50, width: 1600, height: 900 })
        );
        // Outside
        assert_eq!(crop(rect(0, 0, 100, 100), second), None);
    }

    #[test]
    fn cursor_relative_to_the_crop() {
        let desktop = rect(-1920, 0, 0, 1080);
        let c = Crop { x: 120, y: 50, width: 1600, height: 900 };
        // The window's top left on screen is (-1800, 50)
        assert_eq!(cursor_in_crop((-1800, 50), desktop, &c), Some((0.0, 0.0)));
        assert_eq!(cursor_in_crop((-1000, 500), desktop, &c), Some((800.0, 450.0)));
        // Left of, below, and at the far edge (exclusive)
        assert_eq!(cursor_in_crop((-1801, 50), desktop, &c), None);
        assert_eq!(cursor_in_crop((-1800, 950), desktop, &c), None);
        assert_eq!(cursor_in_crop((-200, 100), desktop, &c), None);
    }
}
