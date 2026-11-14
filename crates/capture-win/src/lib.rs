//! Live capture of any executable's window on Windows.
//!
//! DXGI Desktop Duplication of the window's monitor, cut to the window's client area: no capture border
//! (unlike Windows.Graphics.Capture on Windows 10). Finds the window by the executable's file name,
//! knows whether it is in front or minimized, and returns BGRA frames (`sc-ohseearr-capture`'s `Pixels`)
//! with the mouse cursor's position in them. [`cursor_pos`] is the cursor on the screen, for that crate's
//! rest detector.
//!
//! All coordinates are physical pixels: the process must be per-monitor DPI aware
//! (`set_per_monitor_dpi_aware`, or a manifest; Tauri does it itself).
//!
//! The capture (`WindowCapture` and friends) builds only on Windows; elsewhere the crate is empty but for
//! [`Rect`].

mod geometry;

#[cfg(windows)]
mod capture;

pub use geometry::Rect;

#[cfg(windows)]
pub use capture::{
    CaptureError, CaptureInfo, Captured, Found, Grab, Skip, Window, WindowCapture, cursor_pos,
    set_per_monitor_dpi_aware,
};
