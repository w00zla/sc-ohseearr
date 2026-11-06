//! How a frame came about.

/// A pulling reader (the companion's `FastCapture`) lets the capture grab the next frame at
/// `now + PULL_TIMEOUT_MS` after a grab (Unix ms), in case no read answers (the screen changed, the scan
/// stopped).
pub const PULL_TIMEOUT_MS: u64 = 3000;

/// The capture side of how the frame behind a read came about (debug view, recordings; remote diagnosis
/// of slow reads). The counts cover everything since the frame before.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct CaptureInfo {
    /// The grab's duration (desktop image to the CPU), ms
    pub grab_ms: u32,
    /// Grabs without a new image: none within the timeout, only the pointer moved, the window
    /// outside the monitor (the next try comes a full interval later)
    pub skipped_timeout: u32,
    pub skipped_pointer: u32,
    pub skipped_outside: u32,
    /// How long the mouse cursor had rested when the frame was grabbed (pulled frames; 0: not waited
    /// for). Set by the caller from its [`crate::Rest`], ms.
    pub rest_ms: u32,
}
