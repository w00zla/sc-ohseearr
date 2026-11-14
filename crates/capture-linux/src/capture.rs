//! The Linux capture: the portal's session, the stream's thread, and the grabs on top of them.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ashpd::desktop::Session;
use ashpd::desktop::screencast::Screencast;
use sc_ohseearr_capture::Pixels;

use crate::{portal, process, stream};

/// A capture failure: the stream ended (the window was closed, the session revoked) or cannot be read. After
/// one, [`WindowCapture::open`] starts over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureError(pub String);

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CaptureError {}

impl From<&str> for CaptureError {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Why [`WindowCapture::open`] opened nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// No process of the executable runs: the portal was not asked (it would show its dialog, and the restore
    /// token would be spent)
    NotRunning,
    /// The user closed the portal's dialog without picking a window. Asking again shows it again: wait for
    /// the user to want it
    Cancelled,
    /// The portal or its answer failed
    Portal(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => f.write_str("the executable does not run"),
            Self::Cancelled => f.write_str("the window selection was cancelled"),
            Self::Portal(e) => write!(f, "screen cast portal: {e}"),
        }
    }
}

impl std::error::Error for OpenError {}

/// How the frame came about: the grab's duration and the grabs and buffers without an image since the frame
/// before.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct CaptureInfo {
    /// The grab's duration (waiting for the image, its copy), ms
    pub grab_ms: u32,
    /// Grabs that brought no image within their timeout
    pub skipped_timeout: u32,
    /// Buffers without an image: only the pointer moved
    pub skipped_pointer: u32,
}

/// A grabbed frame.
pub struct Captured {
    /// The window's content, BGRA (`Pixels::Bgra`)
    pub pixels: Pixels,
    /// The mouse cursor in frame pixels; None when it is not in the window (the image has no pointer)
    pub cursor: Option<(f32, f32)>,
    /// How long the cursor has stayed where it is; zero when it moved since the image before
    pub rested: Duration,
    /// The grab's duration and what came without an image before it (counted since the last frame)
    pub info: CaptureInfo,
}

/// Which image a grab waits for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Want {
    /// The next one
    Next,
    /// The next one whose cursor is where it was in the image before
    Rested,
}

/// An image the stream's thread hands to a grab.
pub(crate) struct Shot {
    pub(crate) pixels: Pixels,
    pub(crate) cursor: Option<(f32, f32)>,
    pub(crate) rested: Duration,
}

#[derive(Default)]
pub(crate) struct State {
    /// A grab waits for an image
    pub(crate) want: Option<Want>,
    pub(crate) shot: Option<Shot>,
    /// Why the stream ended
    pub(crate) ended: Option<String>,
    /// Buffers without an image since the last frame
    pub(crate) skipped_pointer: u32,
}

/// What a grab and the stream's thread share.
#[derive(Default)]
pub(crate) struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    /// The thread's loop ends
    pub(crate) stop: AtomicBool,
}

impl Shared {
    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wakes the waiting grab: an image is there.
    pub(crate) fn deliver(&self, mut state: MutexGuard<'_, State>, shot: Shot) {
        state.shot = Some(shot);
        state.want = None;
        drop(state);
        self.changed.notify_all();
    }

    /// The stream is over (the first reason stays); the thread's loop ends and a waiting grab fails.
    pub(crate) fn end(&self, why: impl Into<String>) {
        self.state().ended.get_or_insert_with(|| why.into());
        self.stop.store(true, Ordering::Relaxed);
        self.changed.notify_all();
    }
}

/// An open capture: the portal's session and the thread reading its stream.
struct Open {
    session: Session<Screencast>,
    shared: Arc<Shared>,
    thread: JoinHandle<()>,
}

/// Live capture of one executable's window.
///
/// A capture loop checks [`is_running`](Self::is_running), opens the capture once it does
/// ([`open`](Self::open): the portal's dialog, or none with a restore token), grabs
/// ([`grab`](Self::grab), [`grab_rested`](Self::grab_rested)) and closes it when the executable is gone or a
/// grab failed.
pub struct WindowCapture {
    exe: String,
    open: Option<Open>,
    /// Grabs without an image since the last frame
    skipped_timeout: u32,
}

impl WindowCapture {
    /// Watches the executable with this file name (case-insensitive, e.g. `StarCitizen.exe`: a Windows
    /// program under Wine counts).
    pub fn new(exe_name: impl Into<String>) -> Self {
        Self { exe: exe_name.into(), open: None, skipped_timeout: 0 }
    }

    pub fn exe_name(&self) -> &str {
        &self.exe
    }

    /// Whether a process of the executable runs (in the background too).
    pub fn is_running(&self) -> bool {
        process::is_running(&self.exe)
    }

    /// Opens the capture of one window through the ScreenCast portal and returns the restore token for the
    /// next `open`, if the portal gave one: keep it, each is good for one `open`.
    ///
    /// Without a (valid) `restore_token` the portal shows its dialog and the user picks the window; nothing
    /// tells whether it is the executable's. With one, the window of the session before comes back without a
    /// dialog, also after the program was restarted. Blocks until the portal answered, so as long as the
    /// dialog is open.
    ///
    /// Only while the executable runs ([`OpenError::NotRunning`] otherwise): without its window, the portal
    /// shows the dialog despite the token, and closing that dialog spends the token. After
    /// [`OpenError::Cancelled`] do not ask again on your own: the dialog would come back each time.
    pub fn open(&mut self, restore_token: Option<&str>) -> Result<Option<String>, OpenError> {
        self.close();
        if !self.is_running() {
            return Err(OpenError::NotRunning);
        }
        let portal::Opened { session, node, remote, restore_token } = portal::open(restore_token)?;
        let shared = Arc::new(Shared::default());
        let thread = std::thread::Builder::new().name("capture-stream".into()).spawn({
            let shared = shared.clone();
            move || {
                if let Err(e) = stream::run(node, remote, &shared) {
                    shared.end(format!("pipewire: {e}"));
                }
                // However the loop ended: nothing comes any more
                shared.end("the stream is closed");
            }
        });
        let thread = match thread {
            Ok(thread) => thread,
            Err(e) => {
                portal::close(&session);
                return Err(OpenError::Portal(format!("cannot start the stream's thread: {e}")));
            }
        };
        self.open = Some(Open { session, shared, thread });
        self.skipped_timeout = 0;
        Ok(restore_token)
    }

    /// Whether the capture is open and its stream still runs.
    pub fn is_open(&self) -> bool {
        self.open.as_ref().is_some_and(|open| open.shared.state().ended.is_none())
    }

    /// Closes the stream and the portal's session (the desktop's sharing indicator goes away).
    pub fn close(&mut self) {
        let Some(open) = self.open.take() else { return };
        open.shared.end("the capture was closed");
        let _ = open.thread.join();
        portal::close(&open.session);
    }

    /// The next image of the window, with the mouse cursor's position in it. None when none came within
    /// `timeout` (counted in the next frame's `info`): the stream brings at most [`FRAME_RATE`](crate::FRAME_RATE)
    /// a second, and none while the window shows nothing new. An error when the capture is not open or its
    /// stream ended.
    pub fn grab(&mut self, timeout: Duration) -> Result<Option<Captured>, CaptureError> {
        self.take(Want::Next, timeout)
    }

    /// As [`grab`](Self::grab), but the next image whose cursor is where it was in the image before (within
    /// `sc-ohseearr-capture`'s `REST_PX`; not in the window both times counts too): it rested for an image's
    /// interval at least, `Captured::rested` tells how long.
    pub fn grab_rested(&mut self, timeout: Duration) -> Result<Option<Captured>, CaptureError> {
        self.take(Want::Rested, timeout)
    }

    fn take(&mut self, want: Want, timeout: Duration) -> Result<Option<Captured>, CaptureError> {
        let Some(open) = &self.open else { return Err("the capture is not open".into()) };
        let started = Instant::now();
        let mut state = open.shared.state();
        if let Some(why) = &state.ended {
            return Err(CaptureError(why.clone()));
        }
        // Only an image that comes from now on
        state.shot = None;
        state.want = Some(want);
        let (mut state, _) = open
            .shared
            .changed
            .wait_timeout_while(state, timeout, |s| s.shot.is_none() && s.ended.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        state.want = None;
        if let Some(Shot { pixels, cursor, rested }) = state.shot.take() {
            let info = CaptureInfo {
                grab_ms: started.elapsed().as_millis() as u32,
                skipped_timeout: std::mem::take(&mut self.skipped_timeout),
                skipped_pointer: std::mem::take(&mut state.skipped_pointer),
            };
            return Ok(Some(Captured { pixels, cursor, rested, info }));
        }
        if let Some(why) = &state.ended {
            return Err(CaptureError(why.clone()));
        }
        self.skipped_timeout += 1;
        Ok(None)
    }
}

impl Drop for WindowCapture {
    fn drop(&mut self) {
        self.close();
    }
}
