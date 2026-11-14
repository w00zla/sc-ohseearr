# sc-ohseearr-capture-linux

Live capture of an executable's window on Linux, without Tauri. Linux only; on other platforms the crate builds
without the capture. The frames are [`sc-ohseearr-capture`](../capture)'s `Pixels`.

A Wayland compositor shows no window to another process unasked. So the window is not found by its executable
as on Windows: the ScreenCast portal (xdg-desktop-portal) lets the user pick it once, and the portal's restore
token brings the same window back without the dialog from then on, also after the program was restarted. The
frames come as a PipeWire stream of the window's own content, as shared memory, with the mouse cursor's
position as the stream's metadata.

- `WindowCapture::new("StarCitizen.exe")`, `is_running()`: whether a process of the executable runs (`/proc`;
  a Windows program under Wine counts).
- `open(restore_token)`: asks the portal for one window and starts the stream; returns the restore token for
  the next `open` (each is good for one). Blocks while the portal's dialog is open. Only while the executable
  runs (`OpenError::NotRunning` otherwise); `OpenError::Cancelled` when the user closed the dialog.
- `grab(timeout)`: the next image of the window as `Pixels::Bgra`, the mouse cursor's position in it, the
  grab's duration, and what came without an image before it.
- `grab_rested(timeout)`: the next image whose cursor is where it was in the image before; `Captured::rested`
  tells how long it has stayed (a pulling reader waits for that: the game then shows the tooltip of the tile
  under the cursor).
- `is_open()`, `close()`: the stream ends when the window is closed or the session revoked; a grab then
  returns an error, and `open` starts over.

```sh
cargo run --release -p sc-ohseearr-capture-linux --example grab-linux -- StarCitizen.exe grab.png restore-token 5
```

## Five images a second, the cursor's rest from the images

Unlimited, a compositor copies every frame the program presents from the GPU to memory. The stream is limited
to `FRAME_RATE` (5) images a second: one per `REST` (200 ms) of `sc-ohseearr-capture`. The cursor's position
comes only with an image then, so it cannot be polled as on Windows: an image whose cursor did not move since
the image before shows a cursor that rested for an interval at least. A read on a resting cursor comes up to
one interval later than on Windows.

## The portal's dialog

- Without a restore token, or with one that is spent or unknown, the portal shows its dialog. Nothing tells
  whether the user picked the executable's window.
- With a token and without the window (the program does not run), the portal shows the dialog too, and closing
  it spends the token. So `open` asks the portal only while the executable runs.
- After `OpenError::Cancelled` an app must not ask again on its own: the dialog would come back each time.
- The token outlives the process (the user can revoke it in the desktop's settings); the app stores it.

## Building

Needs libpipewire's headers (`libpipewire-0.3-dev`, `pipewire-devel`) and clang (bindgen) on Linux. An app
that also uses the portals elsewhere (file dialogs) gets one `zbus` for all of them: this crate runs it on
its own executor (`async-io`), not on tokio.

## Limits

- **Verified on KDE Plasma 6 (Wayland)** with the game under Wine on XWayland, 3840×1600. Other desktops
  (GNOME, wlroots) are untested; the stream must offer BGRx or BGRA as shared memory.
- **Scaled displays and HDR are untested.**
- **No "in front":** the capture does not know whether the window is in front, minimized or covered. The
  image is the window's own content either way.
- The process is found by its command line in `/proc` (one of the first three arguments ends in the
  executable's file name).
- One `WindowCapture` holds one session; the stream is read on its own thread.
