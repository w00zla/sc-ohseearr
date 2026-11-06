//! Live capture of any executable's window on Windows.
//!
//! DXGI Desktop Duplication of the window's monitor, cut to the window's client area: no capture border
//! (unlike Windows.Graphics.Capture on Windows 10). Finds the window by the executable's file name,
//! knows whether it is in front or minimized, and returns BGRA frames with the mouse cursor's position in
//! them. Around that: the cursor-rest detector ([`Rest`]) and the change gate ([`Gate`]) on a grey
//! thumbnail ([`Pixels::thumb`]).
//!
//! All coordinates are physical pixels: the process must be per-monitor DPI aware
//! (`set_per_monitor_dpi_aware`, or a manifest; Tauri does it itself).
//!
//! The pure parts (pixels, gate, rest logic, crop math) build everywhere; the capture itself
//! (`WindowCapture` and friends) only on Windows.

mod gate;
mod geometry;
mod pixels;
mod rest;

#[cfg(windows)]
mod capture;

pub use gate::Gate;
pub use geometry::Rect;
pub use pixels::Pixels;
pub use rest::{PULL_POLL, PULL_TIMEOUT_MS, REST, Rest};

#[cfg(windows)]
pub use capture::{
    CaptureError, CaptureInfo, Captured, Found, Grab, Skip, Window, WindowCapture, set_per_monitor_dpi_aware,
};
