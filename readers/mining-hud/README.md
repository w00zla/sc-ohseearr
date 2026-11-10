# mining-hud

Reader card: the mining HUD's reader. It reads the signature a ship's scan shows for an ore or harvestable spot.

| | |
|---|---|
| Screen | a ship's HUD in scan mode: the small plate above the heading tape, a pin icon and a number ("42,000") |
| Reads | the signature |
| Pattern | **a window around the HUD's centre, read as a whole**: the plate sways and tilts with the HUD, so the window (0.5 × 0.23 of the frame's height, around the centre's x) is cut out, upscaled (2× at 1600 px height, 3× at 1080) and read; the signature is the line written with thousands separators. No layout detector, no text anchor. |
| Game version | SC 4.x (screenshots of 2026-11-08, MISC Prospector) |
| Verified resolutions | 3840×1600 and 1920×1080 |
| Read time | 45–60 ms on the CPU (3 threads) at both resolutions |
| Reader | `MiningHudReader` (`Reader`): `probe` = a signature is in view (it is the full read, kept for `read` of the same `Frame::id`), `read` = the signature |
| Output | `MiningHud { signature: Field<u32>, rect: Option<Rect> }`: `Field::none()` without a plate; a `raw` text without a `value` for a plate whose number could not be read. `rect`: the number's box in frame pixels, without the pin icon, for an app that draws next to it (where the icon was read into the line: a box of a number's size at the line's right edge and middle height); `None` without a plate |
| Diagnostics | `MiningHudDiagnostics { boxes, window }`: the text boxes read in the window and the window, in frame pixels |
| Needs | nothing but the image (`Frame::locale` and `Frame::cursor` are not used) |

The signature always has thousands separators (1,000 and up): a number without one (a heading, a distance) is never
taken. The pin icon is often read as a character before the number; only the text after the last blank counts. A
line of a signature's shape with a misread digit ("54,00D") is read again as digits and commas only.

Nothing but the number's form tells the plate from other texts: any number with thousands separators in the window
is taken for the signature. Read only while a ship's scan HUD is up.

On the regression sets, 25 screenshots (13 at 3840×1600, 12 at 1920×1080, one of them without a plate; daylight,
night side and the red tint of a hot HUD), 22 of 24 plates gave a value; two motion-blurred ones at 1080p gave none.

## Regression

`examples/regress.rs` (`regress-mining-hud`) prints one JSON line per frame (folder, file, probe, output,
diagnostics) over folders of screenshots or recordings; compare the lines before and after a change (no expected
outputs are kept in git, the images stay local). `regression.toml` lists the data folders, relative to the
[data root](../../README.md#data-root).

```sh
cargo run --release -p sc-ohseearr-mining-hud --example regress-mining-hud -- [--data <root>] [--threads n] [--gpu] [folder...]   # threads default 3
```

`cargo test -p sc-ohseearr-mining-hud --release` runs the tests of the number's form, the window and a frame without
the HUD.
