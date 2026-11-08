# warehouse

Reader card: the freight manager's warehouse reader.

| | |
|---|---|
| Screen | The freight elevator terminal's tab Warehouse: the pane right of the screen centre, a grid of 4 columns of tiles scrolled without pages, every tile with a count badge top right ("X14") and, for materials, a quality bottom left ("572Q"); the tooltip of the hovered tile opens at the cursor (title; materials also "Capacity: …", the chip "<name>" "<quality>" and the SCU). The terminal is a screen in the world: the grid's size and place change with where the player stands |
| Reads | Every tile of the fitted grid (rect, column, row, hovered; the hovered tile's quality and count, every tile's quality without a cursor, every tile's count with diagnostics on); the tooltip (name with loc keys, the chip's quality, the SCU, its area) |
| Pattern | Learned layout, then targeted reads: the layout detector `sc-warehouse-layout` finds badges, quality texts and the tooltip's lines as boxes of their kind; the grid (pitch, phase, 4 columns, rows) is fitted to the badges and quality texts; only the tooltip's lines, the hovered tile's quality text and badge are recognized. A strong fit is kept as the prior: the next frames with a cursor run the detector on one window around it, weaker fits keep its pitch |
| Game version | SC 4.x (screenshots and recordings of 2026-10-18 to 2026-10-30) |
| Verified resolutions | 3840×1600 and 1920×1080 (pitch 0.10–0.20 of the frame's height) |
| Read time | Recordings of 2026-10-30 with every tile's badge read (diagnostics on): 26 ms per read at 3840×1600, 25 ms at 1080p on the CPU (3 threads), 24 ms on the GPU; the layout detector ~4 ms of it, the rest the recognizer. The regression set here: 22 ms per read (740 frames, 3 threads, 2026-11-06). The detector over the whole pane (the first read, no prior) takes 6 windows at 3840×1600 |
| Models | the layout detector it is given (`sc-warehouse-layout`, NanoDet-Plus; the app makes it like its `Ocr`, on the same threads and device); the `Ocr`'s recognizer (`sc-rec-latin`; the badges with the characters "Xx× 0123456789" only). The `Ocr`'s text detector is not used |
| Needs | `Frame::cursor` for the hovered tile (without one: the tile with the chip's quality or whose quality the tooltip hides, else the one under the tooltip's cursor point; every tile's quality is read then); `Frame::locale` only to resolve the name to loc keys |
| Does not read | Which item or material a name is (loc keys only); the scroll offset (rows count from the top of the frame); the elevator pane on the left |

## API

- `WarehouseReader::new(detector)`: the layout detector, made by the app:
  `NanoDet::new(&sc_ohseearr_model_sc_warehouse_layout::package(), threads, gpu)`; an error when it has a class
  the read does not know.
- `WarehouseReader`: `Reader` with `Output = Warehouse`, `Diagnostics = WarehouseDiagnostics`.
  - `probe`: the whole read; true when a grid is fitted. Kept for `read` of the same `Frame::id`.
  - `read`: the grid and the tooltip. A fit with weight ≥ `STRONG_FIT_WEIGHT` becomes the prior (`prior()`).
  - `reset`: drops the prior (the screen was left, the scan stopped: the next terminal may be seen from elsewhere).
  - `set_diagnostics(true)`: every tile's badge is read, not only the hovered tile's (the costly part).
  - `take_timings()`: the layout detector's timings (the `Ocr`'s own stay with it). Drop the reader to free the
    detector's memory.
- `parse_badge`.

`Warehouse { tiles, tooltip }`; no tiles: no grid in the frame. `Tile { rect, col, row, hovered, quality, count,
badge_covered }`: `quality` and `count` are `Field<u32>` (`quality.raw` the text read, `ambiguous` "1000" = 100Q or
1000; `count.raw` the read the count came from, else every read joined by " | ", `ambiguous` a bare "X" taken as
1); `badge_covered`: the tooltip covers the badge, so it was not read. `Tooltip { name: Name, name_rect, quality:
Field<u32>, scu: Field<String>, rect }` (`raw`: the line the value came from). `WarehouseDiagnostics { boxes, cells,
layout }`: the detector's anchors as "[badge]"/"[quality]" and the tooltip's lines as read; every tile part
recognized (`Cell { row, col, kind, rect, text, score }`); the fitted `Layout { cols, rows, pitch_x, pitch_y, weight
}`.

## Tests

```sh
cargo test -p sc-ohseearr-warehouse --release
cargo clippy -p sc-ohseearr-warehouse --release --all-targets
```

The frame test reads seven frames of debug recordings from the submodule `fixtures/warehouse` (3840×1600 and
1920×1080, with their cursors), each on its own without a prior, and checks the tooltip's name, quality and SCU and
the hovered tile's count; it fails when the submodule is not checked out (`git submodule update --init`).

## Regression

`regression.toml` lists the data folders (relative to the data root). `examples/regress.rs` (`regress-warehouse`;
`--threads` defaults to 3, as in the companion's dumps) runs one
`WarehouseReader` over each folder's frames in order (`reset` per folder, diagnostics on, the cursor of a recording's
`read-N.json`) and prints one JSON line per frame (`frame`, `cursor`, `output`, `diagnostics`; the timing to
stderr). No expected outputs are kept in git: run it before and after a change (or an OCR/model change) and diff the
files.

```sh
cargo run -p sc-ohseearr-warehouse --release --example regress-warehouse > after.jsonl
# other folders instead of regression.toml's; --data <root> for the data root; --threads n (default 3); --gpu
```
