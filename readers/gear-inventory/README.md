# gear-inventory

Reader card: the character inventory reader.

| | |
|---|---|
| Screen | The character inventory ("Storage Access"): the storage pane on the right (a grid 4 units wide, 6 rows in view, scrolled row by row, pages "2/3"), the tooltip of the hovered tile at a fixed place right of the character |
| Reads | The pane's page (index, total); the tile under the mouse cursor (row in the page's content, first unit, span, area, stack count "2x"); the tooltip (title with loc keys, body lines with text and area, area) |
| Pattern | Calibrated geometry: positions measured on 3840×1600 as shares of the frame's height around the centre, scaled by a factor `k` the page number's place tells; the lit frame of the hovered tile and the scrollbar thumb found by pixel runs; OCR only on the page number's and the tooltip's areas, plus the tile's corner for the count |
| Game version | SC 4.x (measured 2026-10-18 to 2026-10-31) |
| Verified resolutions | 3840×1600 and 1920×1080 (also seen: 3840×2160) |
| Read time | The app's read (page where the known scale puts it, tooltip, stack count) ~57 ms on the CPU (3 threads) at 1080p and 3840×1600, the tooltip ~50 ms of it; ~30 ms for the page number alone (the probe); in game on the GPU median 55 ms at 1080p, ~195 ms at 3840×1600 (it waits for the GPU the game holds) |
| Needs | `Frame::cursor` (hover-driven: without a cursor the tooltip is read anyway, a tile is not found); `Frame::locale` only to resolve the title to loc keys |
| Does not read | What the lines mean (stat names, values): the app matches them against the game's texts |

## API

- `GearInventoryReader`: `Reader` with `Output = GearInventory`, `Diagnostics = GearDiagnostics`.
  - `probe`: reads the page number (around where the kept scale puts it; over every place without one). It also
    tells this screen from the freight manager's terminal. Kept for `read` of the same `Frame::id`.
  - `read`: the page, the tile under the cursor, the tooltip, the tile's count. With a cursor that is on no lit tile
    the tooltip is not read. Keeps the UI scale `k` of the last frame that showed the page number (`scale()`).
  - `probe_with(.., hinted: false)`: the probe over every place the page number can have, whatever scale is kept.
  - `reset`: drops `k` (the screen was left).
- `read_components(ocr, image, cursor, locale)`: one frame, no state, always the full page search, the tooltip
  always read. The reference the regression compares against.
- `parse_page`.

`GearInventory { page, cell, tooltip }`. `Cell.count` is a `Field<u32>` (value, the box's text, its score).
`Tooltip.title` is a `Name` (text and loc keys). `GearDiagnostics { boxes, scale }`: every text box read (page area,
tooltip area, count area, in that order, frame pixels) and the scale `k` the read used.

## Tests

```sh
cargo test -p sc-ohseearr-gear-inventory --release
cargo clippy -p sc-ohseearr-gear-inventory --release --all-targets
```

The frame tests read six frames of debug recordings from the submodule `fixtures/gear-inventory` (3840×1600 and
1920×1080, with their cursors) and check title, stat lines, page and the hovered tile's span; they fail when the
submodule is not checked out (`git submodule update --init`).

## Regression

`regression.toml` lists the data folders (debug recordings of the character inventory, relative to the data root).
`examples/regress.rs` (`regress-gear-inventory`; `--threads` defaults to 3, as in the companion's dumps) prints one
JSON line per frame (`frame`, `cursor`, `output`, `diagnostics`). No expected
outputs are kept in git: run it before and after a change (or an OCR/model change) and diff the files.

```sh
cargo run -p sc-ohseearr-gear-inventory --release --example regress-gear-inventory > after.jsonl
# other folders instead of regression.toml's; --data <root> for the data root; --threads n (default 3); --gpu;
# --stateful runs a GearInventoryReader over each folder's frames (probe + read, kept scale) instead of read_components
```
