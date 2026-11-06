# Refinery terminal

Reads the refinery terminal's work order overview: the station and every work order panel shown side by side
(running or completed) with its materials, qualities, yields and time remaining. Ported from the Quartermaster
companion's `refinery.rs`; the algorithm is unchanged, only the labels differ (below).

| | |
|---|---|
| Screen | the refinery terminal ("Refinement Center") with its work order cards |
| Pattern | **text anchors by loc key** (ADR 0007): table head, panel header, time label and column heads are found by their texts; columns by the head's words, rows by the material names below it. No layout detector. |
| Game version | SC 4.x (layout of the screenshots 2026-10-07/08, recordings 2026-10-19) |
| Verified resolutions | 1080p and 3840×1600 (frames are scaled by their height) |
| Read time | probe 30–110 ms at 2 threads on 3840×1600 (320 px height); full read ~0.9 s on 3840×1600 (companion docs; 800 px height, only the wide boxes first: frames without a table head skip the rest) |
| Reader | `RefineryReader` (`ScreenReader`): `probe` = a refinery title is in view, `read` = the overview |
| Output | `Refinery { screen: Overview \| Terminal \| Nothing, station: Option<Name>, orders: Vec<WorkOrder> }`; `WorkOrder { status, time_remaining: Field<u32> (s), rows: Vec<Row { material: Name, quality: Field<u32>, yield_cscu: Field<u32> }> }` |
| Diagnostics | `RefineryDiagnostics { boxes, panels }`: all text boxes read; per panel (after `set_diagnostics(true)`) the columns, the row boxes and the rows dropped for lacking a quality |

A panel cut by the screen's edge is left out (the terminal scrolls sideways); a row without a quality is dropped;
a value cell the full pass missed is read again on its own (3× upscaled, digits only).

## Locale requirement

The reader needs `Frame::locale`; without it `read` and `probe` return `ReadError::Locale(NoLanguageTable)`.
It knows only the 9 loc keys in `KEYS` (table head, "processing", "completed", time remaining, quality, yield,
to do, done, refinement center). Per frame (cached while the texts stay the same) it takes for each key
`Locale::texts(key)` (the language pack's text, then the English table's), cuts it at `(` ("Materials Yielded
(cSCU)": the parenthesis is often a box of its own), normalizes it (`norm`) and drops duplicates.

Behaviour change against the companion: it compared against compiled-in English plus the pack. Here it is the
pack plus the English table, whichever the locale has: with only a pack, English labels are no longer tried;
with only the English table, a player with a language pack is not matched. Give the locale both tables.
`quality` uses `ui_quality`, which is not confirmed as the terminal's own key.

## Regression

`examples/regress.rs` prints one JSON line per frame (folder, file, probe, output, diagnostics) over folders of
screenshots or debug recordings; compare the lines before and after a change (no expected outputs are kept in
git, the images stay local, ADR 0008). `regression.toml` lists the data folders, relative to the data root
(`$SC_OHSEEARR_DATA`, else `<repo>/local`):

```sh
cargo run --release -p sc-ohseearr-refinery --example regress -- --english <global.ini> [--pack <global.ini>] [--diagnostics] [--threads n] [folder...]
```

`cargo test -p sc-ohseearr-refinery --release` also runs the screenshot tests over `refinery flow`, `… 2`, `… 3`
under the data root (skipped silently when absent), with a small test ini holding just the 9 keys.
