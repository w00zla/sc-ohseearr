//! Live capture of an executable's window on Linux.
//!
//! A Wayland compositor shows no window to another process unasked, so the window is not found by its
//! executable as on Windows: the ScreenCast portal (xdg-desktop-portal) lets the user pick it once, and its
//! restore token brings the same window back without the dialog from then on. The frames come as a PipeWire
//! stream of the window's own content (nothing lying over it, no border), as shared memory, with the mouse
//! cursor's position as the stream's metadata. They are `sc-ohseearr-capture`'s `Pixels`.
//!
//! The stream is limited to [`FRAME_RATE`] images a second, and the cursor's position comes only with an
//! image. So its rest is told from the images: one whose cursor is where it was in the image before
//! ([`WindowCapture::grab_rested`]).
//!
//! The capture (`WindowCapture` and friends) builds only on Linux; elsewhere the crate is empty but for
//! [`FRAME_RATE`].

mod process;
mod still;

#[cfg(target_os = "linux")]
mod capture;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "linux")]
mod stream;

/// Images per second the compositor is asked for: one per `sc-ohseearr-capture`'s `REST` (200 ms), so an image
/// whose cursor did not move since the image before shows a cursor that rested that long. Unlimited, a
/// compositor copies every frame the game presents to memory.
pub const FRAME_RATE: u32 = 5;

#[cfg(target_os = "linux")]
pub use capture::{CaptureError, CaptureInfo, Captured, OpenError, WindowCapture};
