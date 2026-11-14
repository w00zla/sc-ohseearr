# 0025. Linux capture through the ScreenCast portal and PipeWire; the pure capture parts in their own crate

- Status: accepted
- Date: 2026-11-14
- Scope: `crates/capture-linux` (new), `crates/capture` (new), `crates/capture-win`

## Context
`capture-win` finds the game's window by its executable, reads the monitor's image and asks Windows for the
cursor. None of that exists on Wayland, by design: no window list, no read of the screen without the user's
consent, no global cursor position. A Linux capture is a different flow, not another API behind the same one.

A probe (the ScreenCast portal with `ashpd`, the stream with `pipewire`) measured what a desktop delivers, on KDE
Plasma 6.7 (Wayland) with the game under Wine on XWayland, borderless at 3840×1600:

| Question | Result |
|---|---|
| Pixels | BGRx as shared memory (MemFd) when no modifier is offered, tightly packed rows, the window's own content; the first image 10–20 ms after connecting |
| Cursor | its position as stream metadata while it is in the window, also while the game shows its own pointer (the image has none); id 0 outside the window |
| Restore token | `Start` answers at once, without the dialog, also after the game was restarted |
| Token without the window | the game not running: the dialog comes despite the token, and cancelling it spends the token |
| Unlimited stream | 93 images a second (the game's rate) of 24.5 MB each, plus buffers with the cursor alone |
| Limited to 2 a second (a fixed `maxFramerate`) | kept exactly; no buffers with the cursor alone any more: the position comes only with an image |

The pure parts of a capture (`Pixels`, the change gate, the rest detector) lived in `capture-win` (ADR 0004), which
built on other platforms for their sake.

## Decision
- `crates/capture-linux`: the capture of one window through the ScreenCast portal (source type window, the cursor
  as metadata, persist mode "until revoked") and its PipeWire stream (BGRx or BGRA, shared memory). The app stores
  the restore token and passes it in; every `open` returns the next one.
- The stream is limited to 5 images a second, one per `REST`. The cursor's rest is told from the images: an image
  whose cursor is where it was in the image before (`grab_rested`). The 15 ms cursor poll of the Windows loop has
  no counterpart.
- `open` asks the portal only while the executable runs (its command line in `/proc`), and reports a cancelled
  dialog as its own error: an app must not ask again on its own after it.
- No "in front": the stream is the window's own content, so the reason for that check on Windows (the monitor's
  image shows what lies over the window) does not apply. The capture does not know it either.
- `crates/capture`: `Pixels` (with `unpad`), `Gate` and `Rest` move there from `capture-win`. Both platform crates
  depend on it and re-export nothing of it. `Rest::update` (Windows' cursor) is gone: `capture-win` exports
  `cursor_pos`, the app passes it to `Rest::observe`.
- Each platform crate keeps its own capture types (`Captured`, `CaptureInfo`, errors): a window found by its
  executable and a session the user opened share no interface worth forcing.
- `ashpd` runs on zbus's own executor (`async-io`), not on tokio: cargo unifies zbus's features across an app's
  build, and its other portal users (file dialogs) run without a tokio runtime.

## Alternatives
- X11 through XWayland (`x11rb`, XComposite/XShm): no dialog, the window by its process, the cursor and the focus
  known. Declined as the path: gone with Wine's Wayland driver or gamescope, and reports of black or frozen images
  of Vulkan windows; not needed since the portal delivers the cursor. Not measured.
- KWin's `ScreenShot2.CaptureWindow` (D-Bus): KDE only, needs an installed desktop file naming the restricted
  interface, one screenshot per call, no cursor.
- The Wayland protocol `ext-image-copy-capture-v1`: no dialog, but KWin does not offer it (declined upstream).
  To be looked at again if it does.
- A Vulkan layer in the game (as obs-vkcapture): the best frame source, but code inside a process the anti-cheat
  protects.
- An unlimited stream for a cursor that can be polled: the compositor copies every frame the game presents.
- Pausing the stream between grabs instead of a fixed rate: not measured; the fixed rate needs no state.
- `capture-linux` depending on `capture-win` for `Pixels`: no move, but a Linux crate on a Windows one, and
  `capture-win` built everywhere for parts that are not its own.
- One capture crate with a module per platform: the two captures share only the pure parts, and their interfaces
  differ.

## Consequences
- Consumers take `Pixels`, `Gate`, `Rest` and the pull's timing from `sc-ohseearr-capture`; on Windows
  `rest.update()` becomes `cursor_pos()` and `Rest::observe`. A breaking change for the next release.
- Building the workspace on Linux needs libpipewire's headers and clang (bindgen); the tests' Linux runner
  installs them. An app on Linux links libpipewire.
- A read on a resting cursor comes up to 200 ms later than on Windows.
- The app's part: where the token is kept, when the dialog may come (the user's action, not a timer), what it
  shows after a cancelled dialog, and closing the capture when the executable is gone.
- Verified on KDE Plasma only; other desktops, scaled displays and HDR are untested.
