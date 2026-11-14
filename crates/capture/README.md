# sc-ohseearr-capture

The platform independent parts of a live capture. The capture crates of the platforms
([`sc-ohseearr-capture-win`](../capture-win), [`sc-ohseearr-capture-linux`](../capture-linux)) return their frames
as this crate's `Pixels`; an app's capture loop uses the rest. No capture and no platform code here.

- `Pixels`: a captured image, either the capture's BGRA rows (converted only when read) or an RGB image (test
  images). `Pixels::into_rgb()` for the reader; `unpad` packs a capture's padded rows.
- `Gate` on `Pixels::thumb()` (a 96×40 grey box average, cheap on 4K frames): idle, a view that came to rest
  after it moved passes once; during a visit also when it changed or after 2 s.
- `Rest`: since when the cursor stays within 3 px (`REST_PX`), fed with the cursor's position
  (`Rest::observe`). A pulling reader waits for 200 ms of rest (`REST`): the game then shows the tooltip of the
  tile under the cursor. `PULL_POLL` and `PULL_TIMEOUT_MS` are that pull's timing.
