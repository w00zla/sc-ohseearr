//! The Windows capture: process and window detection by the executable's file name, DXGI Desktop
//! Duplication of the window's monitor, cut to its client area.

use std::fmt;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIFactory1, IDXGIOutput1,
    IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::Graphics::Gdi::{ClientToScreen, HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
};
use windows::core::{Interface, PWSTR};

use crate::geometry::{Crop, crop, cursor_in_crop, unpad};
use crate::{Pixels, Rect};

// Waiting for a desktop frame: the game presents continuously, a timeout means nothing changed
const ACQUIRE_TIMEOUT_MS: u32 = 200;
// A grab slower than this is logged (debug)
const SLOW_GRAB: Duration = Duration::from_millis(150);

/// A capture failure (a Win32/DXGI call, the desktop format). After one, the duplication is closed and
/// opened again by the next grab: mode change, fullscreen switch, secure desktop (UAC).
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

trait Context<T> {
    fn ctx(self, what: &str) -> Result<T, CaptureError>;
}

impl<T> Context<T> for windows::core::Result<T> {
    fn ctx(self, what: &str) -> Result<T, CaptureError> {
        self.map_err(|e| CaptureError(format!("{what}: {e}")))
    }
}

/// Makes the process per-monitor DPI aware (v2), so window and monitor coordinates are physical pixels.
/// Call it before any window exists, once per process; fails (harmlessly) when the awareness is already
/// set, e.g. by a manifest or by Tauri.
pub fn set_per_monitor_dpi_aware() -> Result<(), CaptureError> {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        .ctx("SetProcessDpiAwarenessContext")
}

/// A top-level window (its handle as a plain integer: `Send`, comparable, usable with other Win32 code
/// via [`Window::raw`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window(isize);

impl Window {
    pub fn from_raw(hwnd: isize) -> Self {
        Self(hwnd)
    }

    /// The `HWND` value.
    pub fn raw(self) -> isize {
        self.0
    }

    fn hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }

    /// The window's client area in screen coordinates (physical pixels).
    pub fn client_rect(self) -> Result<Rect, CaptureError> {
        unsafe {
            let hwnd = self.hwnd();
            let mut r = RECT::default();
            GetClientRect(hwnd, &mut r).ctx("GetClientRect")?;
            let mut origin = POINT::default();
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return Err("ClientToScreen failed".into());
            }
            Ok(Rect { left: origin.x, top: origin.y, right: origin.x + r.right, bottom: origin.y + r.bottom })
        }
    }

    /// Whether the window still exists and is not minimized.
    pub fn is_up(self) -> bool {
        unsafe { IsWindow(Some(self.hwnd())).as_bool() && !IsIconic(self.hwnd()).as_bool() }
    }
}

/// The mouse cursor in screen coordinates (physical pixels).
pub(crate) fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
}

/// A window of the watched executable and whether it is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub window: Window,
    /// False: the process's own window is in front (the user works in the app, not in the watched one),
    /// `window` is the one seen in front before. Nothing should be captured then.
    pub in_front: bool,
}

/// Why a grab brought no new frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// No new image within the acquire timeout (200 ms)
    Timeout,
    /// Only the mouse pointer moved (or the black first frame)
    Pointer,
    /// The window is not on the duplicated monitor
    Outside,
}

/// How the frame came about: the grab's duration and the grabs without a new image since the frame before.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct CaptureInfo {
    /// The grab's duration (desktop image to the CPU), ms
    pub grab_ms: u32,
    /// Grabs without a new image: none within the timeout, only the pointer moved, the window
    /// outside the monitor (the next try comes a full interval later)
    pub skipped_timeout: u32,
    pub skipped_pointer: u32,
    pub skipped_outside: u32,
}

/// A grabbed frame.
pub struct Captured {
    /// The window's client area, BGRA (`Pixels::Bgra`), physical pixels
    pub pixels: Pixels,
    /// The mouse cursor in frame pixels; None when outside the frame (the duplicated image has no pointer)
    pub cursor: Option<(f32, f32)>,
    /// The grab's duration and the grabs without an image before it (counted since the last frame)
    pub info: CaptureInfo,
    pub window: Window,
}

/// The outcome of one grab.
pub enum Grab {
    Frame(Captured),
    /// No new image, counted into the next frame's `info`
    Skipped(Skip),
}

/// A frame with the cursor's position in it.
type Shot = (Pixels, Option<(f32, f32)>);

/// The duplication of one monitor, kept while the window stays on it.
struct Duplication {
    monitor: HMONITOR,
    /// The monitor's place on the virtual desktop
    desktop: Rect,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    dup: IDXGIOutputDuplication,
    /// The CPU-readable copy target, kept while the crop's size stays
    staging: Option<(ID3D11Texture2D, u32, u32)>,
}

/// Live capture of one executable's window.
///
/// The pieces a capture loop needs: [`window`](Self::window) (which window, in front or not),
/// [`is_running`](Self::is_running), [`grab_window`](Self::grab_window); or all in one,
/// [`grab`](Self::grab). Keep one value per thread (the duplication and its D3D device live in it).
pub struct WindowCapture {
    exe: String,
    dup: Option<Duplication>,
    /// The window last seen in front: it still counts while the process itself is in front
    last: Option<Window>,
    /// Skips since the last frame
    skipped: CaptureInfo,
}

impl WindowCapture {
    /// Watches the executable with this file name (case-insensitive, e.g. `StarCitizen.exe`).
    pub fn new(exe_name: impl Into<String>) -> Self {
        Self { exe: exe_name.into(), dup: None, last: None, skipped: CaptureInfo::default() }
    }

    pub fn exe_name(&self) -> &str {
        &self.exe
    }

    /// The window to capture and whether it is in front: the foreground window if it belongs to the
    /// executable and is not minimized (remembered; in front), or, while the calling process itself is
    /// in front (the user works in the app), the window seen in front before as long as it is up and not
    /// minimized (`in_front: false`: the executable still counts as seen, no frames should be taken).
    /// Any other window in front: None (and the duplication is closed).
    pub fn window(&mut self) -> Option<Found> {
        let found = self.find();
        if found.is_none() {
            self.dup = None;
        }
        found
    }

    fn find(&mut self) -> Option<Found> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() || IsIconic(hwnd).as_bool() {
                return None;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == GetCurrentProcessId() {
                return self.last.filter(|w| w.is_up()).map(|window| Found { window, in_front: false });
            }
            let window = is_exe_process(pid, &self.exe).then_some(Window(hwnd.0 as isize));
            if window.is_some() {
                self.last = window;
            }
            window.map(|window| Found { window, in_front: true })
        }
    }

    /// The foreground window if it is the executable's and not minimized (an overlay follows it; unlike
    /// [`window`](Self::window), the process itself in front does not count).
    pub fn foreground_window(&self) -> Option<Window> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() || IsIconic(hwnd).as_bool() {
                return None;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            is_exe_process(pid, &self.exe).then_some(Window(hwnd.0 as isize))
        }
    }

    /// Whether a process of the executable runs (in the background too).
    pub fn is_running(&self) -> bool {
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
            let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            let mut found = false;
            let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
            while more && !found {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                found = exe_matches(&String::from_utf16_lossy(&entry.szExeFile[..len]), &self.exe);
                more = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
            found
        }
    }

    /// Closes the duplication (opened again by the next grab).
    pub fn reset(&mut self) {
        self.dup = None;
    }

    /// One frame of the executable's window if it is in front: `window()` plus `grab_window`. None when
    /// the window is not found, not in front, or there is no new image (counted in the next frame's
    /// `info`). An error closes the duplication; the next call tries again.
    pub fn grab(&mut self) -> Result<Option<Captured>, CaptureError> {
        let Some(Found { window, in_front: true }) = self.window() else { return Ok(None) };
        Ok(match self.grab_window(window)? {
            Grab::Frame(c) => Some(c),
            Grab::Skipped(_) => None,
        })
    }

    /// One frame of this window's client area with the mouse cursor's position in it, or the skip
    /// reason when there is no new image. Skips are added to the `info` of the next frame. An error
    /// closes the duplication.
    pub fn grab_window(&mut self, window: Window) -> Result<Grab, CaptureError> {
        let started = Instant::now();
        let grabbed = self.grab_inner(window);
        match grabbed {
            Err(e) => {
                self.dup = None;
                Err(e)
            }
            Ok(Err(skip)) => {
                match skip {
                    Skip::Timeout => self.skipped.skipped_timeout += 1,
                    Skip::Pointer => self.skipped.skipped_pointer += 1,
                    Skip::Outside => self.skipped.skipped_outside += 1,
                }
                Ok(Grab::Skipped(skip))
            }
            Ok(Ok((pixels, cursor))) => {
                // A slow copy to the CPU (the GPU busy with the game) delays every frame
                if started.elapsed() > SLOW_GRAB {
                    log::debug!("screen capture: grab took {:?}", started.elapsed());
                }
                let info =
                    CaptureInfo { grab_ms: started.elapsed().as_millis() as u32, ..std::mem::take(&mut self.skipped) };
                Ok(Grab::Frame(Captured { pixels, cursor, info, window }))
            }
        }
    }

    fn grab_inner(&mut self, window: Window) -> Result<Result<Shot, Skip>, CaptureError> {
        let monitor = unsafe { MonitorFromWindow(window.hwnd(), MONITOR_DEFAULTTONEAREST) };
        if self.dup.as_ref().is_none_or(|d| d.monitor != monitor) {
            self.dup = None;
            let d = open(monitor)?;
            let r = d.desktop;
            log::debug!(
                "screen capture on the monitor at ({}, {}), {}×{}",
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top
            );
            self.dup = Some(d);
        }
        let d = self.dup.as_mut().expect("opened above");
        let Some(crop) = crop(window.client_rect()?, d.desktop) else {
            log::debug!("screen capture: frame skipped (window outside the monitor)");
            return Ok(Err(Skip::Outside));
        };
        let pixels = match d.frame(&crop)? {
            Ok(p) => p,
            Err(skip) => return Ok(Err(skip)),
        };
        // The cursor relative to the frame's top left (screen position of the crop)
        let cursor = cursor_pos().and_then(|p| cursor_in_crop(p, d.desktop, &crop));
        Ok(Ok((pixels, cursor)))
    }
}

/// Whether the process runs the executable.
fn is_exe_process(pid: u32, exe: &str) -> bool {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let named = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        named.is_ok() && exe_matches(&String::from_utf16_lossy(&buf[..len as usize]), exe)
    }
}

/// Whether the path (or bare file name) ends in the executable's file name, case-insensitive.
fn exe_matches(exe_path: &str, exe: &str) -> bool {
    exe_path.rsplit(['\\', '/']).next().is_some_and(|name| name.eq_ignore_ascii_case(exe))
}

/// Duplicates the DXGI output showing `monitor`, on a device of the adapter driving it.
fn open(monitor: HMONITOR) -> Result<Duplication, CaptureError> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ctx("CreateDXGIFactory1")?;
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            a += 1;
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                o += 1;
                let desc = output.GetDesc().ctx("GetDesc")?;
                if desc.Monitor != monitor {
                    continue;
                }
                let (mut device, mut context) = (None, None);
                D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
                .ctx("D3D11CreateDevice")?;
                let device = device.ok_or("no D3D11 device")?;
                let context = context.ok_or("no D3D11 context")?;
                let dup = output
                    .cast::<IDXGIOutput1>()
                    .ctx("IDXGIOutput1")?
                    .DuplicateOutput(&device)
                    .ctx("DuplicateOutput")?;
                let c = desc.DesktopCoordinates;
                let desktop = Rect { left: c.left, top: c.top, right: c.right, bottom: c.bottom };
                return Ok(Duplication { monitor, desktop, device, context, dup, staging: None });
            }
        }
        Err("no DXGI output shows the window's monitor".into())
    }
}

impl Duplication {
    fn frame(&mut self, crop: &Crop) -> Result<Result<Pixels, Skip>, CaptureError> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;
            match self.dup.AcquireNextFrame(ACQUIRE_TIMEOUT_MS, &mut info, &mut resource) {
                Ok(()) => {}
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                    log::debug!("screen capture: frame skipped (no new image within {ACQUIRE_TIMEOUT_MS} ms)");
                    return Ok(Err(Skip::Timeout));
                }
                Err(e) => return Err(CaptureError(format!("AcquireNextFrame: {e}"))),
            }
            // No new desktop image (only the mouse pointer moved, or the black first frame)
            if info.LastPresentTime == 0 {
                let _ = self.dup.ReleaseFrame();
                log::debug!("screen capture: frame skipped (pointer only)");
                return Ok(Err(Skip::Pointer));
            }
            let copied = self.copy(resource, crop);
            let _ = self.dup.ReleaseFrame();
            copied.map(Ok)
        }
    }

    /// Copies the cropped part of the desktop image to the CPU (BGRA rows without padding).
    unsafe fn copy(&mut self, resource: Option<IDXGIResource>, c: &Crop) -> Result<Pixels, CaptureError> {
        unsafe {
            let texture: ID3D11Texture2D = resource.ok_or("no desktop image")?.cast().ctx("ID3D11Texture2D")?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
                return Err(CaptureError(format!("desktop format {:?} not supported (HDR?)", desc.Format)));
            }
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: c.width,
                Height: c.height,
                MipLevels: 1,
                ArraySize: 1,
                Format: desc.Format,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            if self.staging.as_ref().is_none_or(|(_, w, h)| (*w, *h) != (c.width, c.height)) {
                let mut staging = None;
                self.device.CreateTexture2D(&staging_desc, None, Some(&mut staging)).ctx("CreateTexture2D")?;
                self.staging = Some((staging.ok_or("no staging texture")?, c.width, c.height));
            }
            let staging = self.staging.as_ref().expect("created above").0.clone();
            let source =
                D3D11_BOX { left: c.x, top: c.y, front: 0, right: c.x + c.width, bottom: c.y + c.height, back: 1 };
            self.context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture, 0, Some(&source));
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).ctx("Map")?;
            let pitch = mapped.RowPitch as usize;
            let data = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * c.height as usize);
            let pixels = Pixels::Bgra { data: unpad(data, pitch, c.width, c.height), width: c.width, height: c.height };
            self.context.Unmap(&staging, 0);
            Ok(pixels)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_by_file_name() {
        let sc = "StarCitizen.exe";
        assert!(exe_matches(r"C:\Games\StarCitizen\LIVE\Bin64\StarCitizen.exe", sc));
        assert!(exe_matches(r"D:\SC\LIVE\Bin64\starcitizen.EXE", sc));
        assert!(exe_matches("StarCitizen.exe", sc));
        assert!(!exe_matches(r"C:\Games\StarCitizen\RSI Launcher.exe", sc));
        assert!(!exe_matches(r"C:\Tools\NotStarCitizen.exe", sc));
    }
}
