<p align="center"><img src="docs/logo.jpg" alt="ohseearr" width="800"></p>

# sc-ohseearr

OCR for Star Citizen screens, in Rust: an engine on ONNX Runtime (CPU, or GPU via DirectML on Windows), a
collection of model packages, readers of real game screens, the game's language tables, a window capture and the
training pipelines behind the models. It started as the OCR core of the Quartermaster companion app and is meant for
any app that reads the game's UI.

> Status: early, APIs will change until 1.0. Decisions and the plan: [`docs/adr/`](docs/adr/README.md). The
> acceptance test of the layering, a mining HUD reader built only from these parts, is still to come.

## Layout

| Path | Crate | What |
|---|---|---|
| `crates/core` | `sc-ohseearr-core` | engine: model families, model packages, `Ocr`, line grouping, image helpers |
| `crates/sc` | `sc-ohseearr-sc` | the game: `Locale` (texts from `global.ini` by loc key, names → loc keys), `CenterUi`, value parsers, the `Reader` interface |
| `crates/capture-win` | `sc-ohseearr-capture-win` | live capture of a game window on Windows (DXGI), cursor rest, change gate |
| `crates/test-support` | `sc-ohseearr-test-support` | dev-only (not published): the data root, arguments, frames and OCR the readers' tests and `regress` examples share |
| `models/*` | `sc-ohseearr-model-*` | model packages (below) |
| `readers/*` | `sc-ohseearr-<reader>` | readers with their cards and regression setup: `warehouse`, `gear-inventory`, `refinery` |
| `training/` | — | training pipelines and recipes that produce model packages |
| `scripts/` | — | `extract-global-ini.{ps1,sh}`: the game's English texts from `Data.p4k` with a StarBreaker CLI (any tool that extracts files from `Data.p4k` does the same) |

## Readers

| Reader | Reads | Pattern |
|---|---|---|
| [`warehouse`](readers/warehouse/README.md) | freight manager's warehouse grid and tooltip | a learned layout model, then targeted reads |
| [`gear-inventory`](readers/gear-inventory/README.md) | character inventory: hovered item's tooltip, page, tile | calibrated geometry around the frame's centre |
| [`refinery`](readers/refinery/README.md) | refinery terminal's work orders | text anchors by loc key (needs the game's texts) |

Each implements `sc::Reader` (`probe`, `read`, `reset`, `set_diagnostics`) on a `Frame { id, image, cursor,
locale }` and returns neutral output: what the game shows, uncertain values as `Field` (value, raw text, score,
ambiguous), names as `Name` with candidate loc keys. Which screen is visible is the app's business.

The game's texts: readers never carry them. Pass a `Locale` built from the player's language pack and/or the English
table extracted from the game: `Data/Localization/english/global.ini` in `Data.p4k`, taken out with any tool that
extracts files from it (there are several), e.g. with StarBreaker by our script
(`scripts/extract-global-ini.ps1 -StarBreaker <starbreaker.exe> -P4k <Data.p4k>`).

## Model families

| Family | Type | What it does |
|---|---|---|
| `db-det` | `DbDetector` | finds text lines (PaddleOCR detector, DB post-processing) |
| `ctc-rec` | `CtcRecognizer` | reads one text line (PaddleOCR recognizer, CTC decoding) |
| `nanodet` | `NanoDet` | finds a screen's parts by class (a layout model trained per screen) |

`Ocr` pairs a detector and a recognizer.

## Model packages

Each folder under `models/` is a package and a crate at once: `model.onnx`, its manifest `model.toml` (family,
input normalization, thresholds, classes or dictionary, license, source), `MODEL.md` (what it is for, how it was
made, measurements) and a crate whose `package()` returns it compiled in. The same folder loads at runtime with
`ModelPackage::load(dir)`, so a model can be swapped without changing code. `examples/tests/models.rs` runs every
package on a synthetic image and checks its output shapes against the manifest (no data needed).

| Package | Family | License | |
|---|---|---|---|
| [`paddle-det-tiny`](models/paddle-det-tiny/MODEL.md) | `db-det` | Apache-2.0 | PP-OCRv6 tiny detector, unmodified |
| [`sc-rec-latin`](models/sc-rec-latin/MODEL.md) | `ctc-rec` | Apache-2.0 | PP-OCRv6 tiny recognizer cut to Latin, fine-tuned on the game's font |
| [`sc-warehouse-layout`](models/sc-warehouse-layout/MODEL.md) | `nanodet` | MIT | the freight manager's warehouse grid and tooltip |

## Use

Git dependency (no crates.io releases), pinned by tag:

```toml
[dependencies]
sc-ohseearr-core = { git = "https://github.com/w00zla/sc-ohseearr", tag = "v0.1.0" }
sc-ohseearr-model-paddle-det-tiny = { git = "https://github.com/w00zla/sc-ohseearr", tag = "v0.1.0" }
sc-ohseearr-model-sc-rec-latin = { git = "https://github.com/w00zla/sc-ohseearr", tag = "v0.1.0" }
```

```rust
let det = sc_ohseearr_model_paddle_det_tiny::package();
let rec = sc_ohseearr_model_sc_rec_latin::package();
let mut ocr = sc_ohseearr_core::Ocr::new(&det, &rec, 3, false)?;
for line in ocr.read(&image, 800)? {
    println!("{} ({:.2})", line.text, line.score);
}
```

- `ort` is pinned to `=2.0.0-rc.13`: a build has one ONNX Runtime, so a consumer uses that version too.
- GPU: feature `directml` (Windows) and Microsoft's redistributable `DirectML.dll` 1.15 next to the executable;
  Windows 10's own DirectML refuses the sessions.
- The models want the UI at its native look: scale frames by their height (the game's UI scales with it, on 16:9
  like on ultrawide) and scale small texts up before detecting them.

Examples (`examples/`): `ocr_dump` (detect and read an image, with package folders to try other models) and
`layout_dump` (the warehouse layout model plus targeted reads).

```sh
cargo run --release --example ocr_dump -- <image> [max_height] [threads] [--det <dir>] [--rec <dir>]
cargo run --release --example layout_dump -- <image> [x y w h]
cargo run --release -p sc-ohseearr-warehouse --example regress-warehouse -- [folders] [--data <root>] [--threads n]   # likewise regress-gear-inventory, regress-refinery (--english <global.ini>)
cargo test --release --workspace
```

## Data root

Test, regression and training data (screenshots, recordings, line crops, fonts) are not in the repository. Everything
that reads them, the tests, the `regress` examples (`--data <root>`) and the training pipelines (`--data-root
<dir>`), looks in the **data root**: `$SC_OHSEEARR_DATA`, else `local/` in the repository (gitignored). Below it:
`regression/<reader>/<set>/` (the regression sets: the apps' debug recordings, the only data the readers'
`regression.toml` and the apps' regression dumps refer to, ADR 0014), `screenshots/…` and `stock_scan_debug/…`
(scratch for analysis and prototyping, no test fixture; the readers' old screenshot tests still read
`screenshots/` until they are replaced), `training/<id>/` (training runs). Tests skip what is
missing.

## Game data and fan content

This repository holds game data only as small excerpts that tests concretely need (a few loc keys, single crops or
frames, each with its source noted; ADR 0015), never whole game files, fonts or recordings. The readers take the
game's texts from your own installation; the training pipelines read data exported from your own copy of the game.

sc-ohseearr is an unofficial Star Citizen fan project, not endorsed by or affiliated with the Cloud Imperium group of
companies. Game content and materials are copyright Cloud Imperium Rights LLC and Cloud Imperium Rights Ltd. Star
Citizen® is a registered trademark of Cloud Imperium Rights LLC.

## License

Code: MIT (`LICENSE`). Each model package states its own license; see `NOTICE`.
