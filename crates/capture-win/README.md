# sc-ohseearr-capture-win

Live capture of any executable's window on Windows, extracted from the Quartermaster companion, without
Tauri. Windows only; on other platforms the crate builds with its pure parts (pixels, gate, rest logic, crop
math) and no capture.

- `WindowCapture::new("StarCitizen.exe")`: finds the process's window by the executable's file name
  (`window()`: in front or not, minimized ones do not count; `is_running()`; `foreground_window()`).
- `grab()` / `grab_window(window)`: one frame of the window's client area as `Pixels::Bgra`, the mouse
  cursor's position in it, the grab's duration, and why a grab brought no frame (`Skip`: no new image within
  200 ms, only the pointer moved, the window outside the monitor).
- `Rest`: since when the cursor stays within 3 px (a pulling reader waits for 200 ms of rest, the game then
  shows the tooltip of the tile under it).
- `Gate` on `Pixels::thumb()` (a 96×40 grey box average, cheap on 4K frames): idle, a view that came to rest
  after it moved passes once; during a visit also when it changed or after 2 s.
- `Pixels::into_rgb()` for the reader.

```sh
cargo run --release -p sc-ohseearr-capture-win --example grab -- StarCitizen.exe grab.png 5
```

## DXGI Desktop Duplication, not Windows.Graphics.Capture

The capture is the image of the window's monitor (DXGI Desktop Duplication), cut to the window's client area.
Windows.Graphics.Capture would capture the window itself, but on Windows 10 it draws a capture border around
it that Windows 10 cannot turn off. DXGI has no border. Consequences of capturing the
monitor: what lies over the window is captured too (so a caller should capture only while the window is in
front), and only the part of the window on the duplicated monitor is seen.

## Limits

- **DPI awareness:** window and monitor coordinates are physical pixels, so the process must be per-monitor
  DPI aware. Tauri does that itself; any other app calls `set_per_monitor_dpi_aware()` first (or declares it
  in its manifest). Without it the crop is wrong on scaled monitors.
- **HDR and rotated monitors are untested.** HDR desktops deliver a format other than BGRA8, which is
  rejected with an error ("desktop format … not supported"); rotated outputs are not handled.
- Exclusive fullscreen, mode changes and the secure desktop (UAC) end the duplication: `grab` returns an
  error and opens it again on the next call.
- One `WindowCapture` per thread (the D3D device lives in it).
