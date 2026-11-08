<p align="center"><img src="docs/logo.jpg" alt="ohseearr" width="800"></p>

<h1 align="center">sc-ohseearr</h1>

<p align="center">
  <a href="https://github.com/w00zla/sc-ohseearr/actions/workflows/test.yml"><img src="https://github.com/w00zla/sc-ohseearr/actions/workflows/test.yml/badge.svg?event=pull_request" alt="Tests"></a>
  <a href="https://github.com/w00zla/sc-ohseearr/releases"><img src="https://img.shields.io/github/v/release/w00zla/sc-ohseearr?color=54adf7" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-54adf7" alt="License: MIT"></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/rust-1.88%2B-54adf7?logo=rust" alt="Rust 1.88+"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-54adf7" alt="Windows | Linux">
</p>

<p align="center">
  OCR for Star Citizen in Rust: read what the game shows, from a frame of its window, as typed values. An engine on
  ONNX Runtime, models tuned to the game's UI, ready readers for real screens and the pipelines that train the
  models. It started as the OCR core of the Quartermaster companion app.
</p>

- **Engine:** PP-OCRv6 tiny detection and recognition on ONNX Runtime, on the CPU or the GPU (DirectML), batched.
- **Models for the game:** a recognizer fine-tuned on the game's UI font, a layout detector trained on a screen.
- **Readers:** the freight manager's warehouse, the character inventory and the refinery terminal, each one call
  per frame with neutral, typed output.
- **The game's language:** texts come from the player's `global.ini` by loc key, so readers work with language packs.
- **Capture:** a live grab of any game window, with the cursor, its rest and a change gate (Windows; Linux is work in
  progress).
- **Training included:** one recipe per model turns local data into a model package; a new screen is a recipe and
  data, no engine code.

> **Status:** early, APIs change until 1.0. Linux: OCR and the readers run, the window capture is work in progress.

## Requirements

- Rust 1.88+ (edition 2024, let chains) and git.
- Windows 10/11 or Linux. OCR and the readers run on both; the window capture is Windows only so far (Linux: work in
  progress). The GPU (Windows) needs Microsoft's redistributable `DirectML.dll` 1.15
  next to the executable (Windows' own DirectML refuses the sessions).
- For readers that anchor on the game's texts (the refinery): the English `global.ini` of your game (see
  [The game's texts](#the-games-texts)) and/or the player's language pack.
- For training: an NVIDIA GPU, Python 3.11 with uv and your own copy of the game ([`training/README.md`](training/README.md)).

## Use

A git dependency (no crates.io), pinned by a release tag or a revision. Every crate of the repository:

```toml
[dependencies]
# The engine and the models
sc-ohseearr-core = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }  # features = ["directml"] for the GPU
sc-ohseearr-model-paddle-det-tiny = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }
sc-ohseearr-model-sc-rec-latin = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }
sc-ohseearr-model-sc-warehouse-layout = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }  # only to use it directly
# The game: texts, parsers, the reader interface; the readers
sc-ohseearr-sc = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }
sc-ohseearr-warehouse = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }
sc-ohseearr-gear-inventory = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }
sc-ohseearr-refinery = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }
# The window capture (Windows)
sc-ohseearr-capture-win = { git = "https://github.com/w00zla/sc-ohseearr", tag = "<release>" }
```

The engine alone, the text lines of an image:

```rust
use std::path::Path;
use sc_ohseearr_core::{ModelPackage, Ocr};

let det = sc_ohseearr_model_paddle_det_tiny::package();      // compiled in
let rec = ModelPackage::load(Path::new("models/sc-rec-latin"))?; // or a package folder loaded at run time
let mut ocr = Ocr::new(&det, &rec, 3, false)?;                 // 3 threads, CPU (true: GPU)
for line in ocr.read(&image, 800)? {                           // the frame scaled to 800 px height
    println!("{} ({:.2})", line.text, line.score);
}
```

A model on its own, the warehouse layout detector:

```rust
let mut layout = sc_ohseearr_core::NanoDet::new(&sc_ohseearr_model_sc_warehouse_layout::package(), 3, false)?;
for d in layout.detect(&image)? {                              // the classes of `layout.classes()`
    println!("{:?}", d);
}
```

Everything together, the game window read live by all readers:

```rust
use std::path::Path;
use sc_ohseearr_capture_win::{WindowCapture, set_per_monitor_dpi_aware};
use sc_ohseearr_core::Ocr;
use sc_ohseearr_gear_inventory::GearInventoryReader;
use sc_ohseearr_refinery::RefineryReader;
use sc_ohseearr_sc::{Frame, LocTable, Locale, Reader};
use sc_ohseearr_warehouse::WarehouseReader;

let mut ocr = Ocr::new(&sc_ohseearr_model_paddle_det_tiny::package(), &sc_ohseearr_model_sc_rec_latin::package(), 3, false)?;
// The game's texts: the player's language pack (if any) first, then the English table
let locale = Locale::new(None, Some(LocTable::load(Path::new("global-english.ini"))?))?;
// Each reader keeps its own state across frames (the warehouse its layout, with its own model)
let (mut warehouse, mut gear, mut refinery) = (WarehouseReader::new(), GearInventoryReader::new(), RefineryReader::new());

set_per_monitor_dpi_aware()?;                                  // once, before any window exists
let mut capture = WindowCapture::new("StarCitizen.exe");
let mut id = 0;
loop {
    // A frame of the game window while it is in front, with the cursor; None: no new image
    let Some(shot) = capture.grab()? else { continue };
    let image = shot.pixels.into_rgb();
    id += 1;
    let frame = Frame { id, image: &image, cursor: shot.cursor, locale: Some(&locale) };
    // Which screen is visible is the app's business: probe (cheap), then read
    if warehouse.probe(&mut ocr, &frame)? {
        let w = warehouse.read(&mut ocr, &frame)?.output;      // tiles, the hovered tile's tooltip
    } else if gear.probe(&mut ocr, &frame)? {
        let g = gear.read(&mut ocr, &frame)?.output;           // page, hovered cell, tooltip with stats
    } else if refinery.probe(&mut ocr, &frame)? {
        let r = refinery.read(&mut ocr, &frame)?.output;       // station, work orders with their rows
    }
}
```

- `ort` is pinned to `=2.0.0-rc.13`: a build has one ONNX Runtime, so a consumer uses that version too.
- Frames go in at their native size; the readers scale them by their height (the game's UI scales with it, on 16:9
  as on ultrawide).
- Diagnostics (every text box read, fit internals) come with `set_diagnostics(true)`, as `read(..)?.diagnostics`.

## How it works

| Layer | Crate | What |
|---|---|---|
| `crates/core` | `sc-ohseearr-core` | the engine: model families, model packages, `Ocr`, line grouping, image helpers; nothing about the game |
| `crates/sc` | `sc-ohseearr-sc` | the game, any screen: `Locale` (texts by loc key, names → loc keys), `CenterUi`, value parsers, the `Reader` interface |
| `crates/capture-win` | `sc-ohseearr-capture-win` | live capture of a game window on Windows (DXGI), cursor rest, change gate |
| `models/*` | `sc-ohseearr-model-*` | model packages, each also a crate |
| `readers/*` | `sc-ohseearr-<reader>` | one reader each, with its card and regression setup |
| `training/` | — | the pipelines and recipes that make model packages (Python) |
| `scripts/` | — | `extract-global-ini.{ps1,sh}`: the game's English texts from `Data.p4k` |

A new screen is a new folder in `readers/` (and maybe in `models/`), not a change to `core` or `sc`.

### Model families

| Family | Type | What it does |
|---|---|---|
| `db-det` | `DbDetector` | finds text lines (PaddleOCR detector, DB post-processing) |
| `ctc-rec` | `CtcRecognizer` | reads one text line (PaddleOCR recognizer, CTC decoding) |
| `nanodet` | `NanoDet` | finds a screen's parts by class (a layout model trained per screen) |

`Ocr` pairs a detector and a recognizer.

### Model packages

Each folder in `models/` is a package and a crate: `model.onnx`, its manifest `model.toml` (family, input
normalization, thresholds, classes or dictionary, license, source), `MODEL.md` (purpose, making, measurements) and a
crate whose `package()` returns it compiled in. `ModelPackage::load(dir)` loads the same folder at run time, so a model
can be swapped without code changes.

| Package | Family | License | |
|---|---|---|---|
| [`paddle-det-tiny`](models/paddle-det-tiny/MODEL.md) | `db-det` | Apache-2.0 | PP-OCRv6 tiny detector, unmodified |
| [`sc-rec-latin`](models/sc-rec-latin/MODEL.md) | `ctc-rec` | Apache-2.0 | PP-OCRv6 tiny recognizer cut to Latin, fine-tuned on the game's font |
| [`sc-warehouse-layout`](models/sc-warehouse-layout/MODEL.md) | `nanodet` | MIT | the freight manager's warehouse grid and tooltip |

### Readers

| Reader | Reads | Pattern |
|---|---|---|
| [`warehouse`](readers/warehouse/README.md) | the freight manager's warehouse: grid, hovered tile, tooltip | a learned layout model, then targeted reads |
| [`gear-inventory`](readers/gear-inventory/README.md) | the character inventory: the hovered item's tooltip, page, tile | calibrated geometry around the frame's centre |
| [`refinery`](readers/refinery/README.md) | the refinery terminal's work orders | text anchors by loc key |

Each implements `sc::Reader`: `probe` (is my screen in view, cheap), `read`, `reset`, `set_diagnostics`, on a
`Frame { id, image, cursor, locale }`. A reader keeps its own state across frames. The output is neutral, what the
game shows: uncertain values as `Field` (value, raw text, score, ambiguous), names as `Name` with candidate loc keys;
diagnostics (all text boxes, fit internals) come separately. Which screen is visible and how often to read is the
app's business.

### The game's texts

Readers carry no game text. They anchor on loc keys and take the texts from the frame's `Locale`: the player's
language pack (`data/Localization/<language>/global.ini`, picked by `user.cfg`) and the game's English table. The
English table lives inside `Data.p4k` as `Data/Localization/english/global.ini`; any tool that extracts files from
`Data.p4k` gets it (there are several). `scripts/extract-global-ini.{ps1,sh}` does it with StarBreaker and names the
file by the game build:

```powershell
scripts/extract-global-ini.ps1 -StarBreaker <starbreaker.exe> -P4k <install>/LIVE/Data.p4k -Out <dir>   # Linux: scripts/extract-global-ini.sh
```

## Development

```sh
cargo test --release --workspace                  # tests on local data skip what is missing
cargo run --release --example ocr_dump -- <image> [max_height] [threads] [--det <dir>] [--rec <dir>] [--gpu]
cargo run --release --example layout_dump -- <image> [x y w h]
cargo run --release -p sc-ohseearr-refinery --example regress-refinery -- --english <global.ini>   # likewise regress-warehouse, regress-gear-inventory
```

- `examples/tests/models.rs` runs every model package on a synthetic image and checks its outputs against the
  manifest (no data needed).
- Each reader's `regress` example prints its output per frame over its regression set: compare the lines before and
  after a change. Results must not change silently.
- Training: [`training/README.md`](training/README.md) (setup, the fonts and language files you provide, recipes).

**Data root.** Local data stays out of git: `$SC_OHSEEARR_DATA`, else `local/` in the checkout (gitignored). The tests,
the `regress` examples (`--data <root>`) and the training pipelines (`--data-root <dir>`) read from it:

| Folder | What |
|---|---|
| `regression/<reader>/<set>/` | the regression sets: debug recordings the readers' `regression.toml` list |
| `training/<id>/` | training runs |
| `screenshots/`, `stock_scan_debug/` | scratch for analysis and prototyping, never a regression set (the readers' screenshot tests read `screenshots/`) |

## Game data and fan content

The repository holds game data only as small excerpts a test concretely needs (a few loc keys, single crops or
frames, each with its source noted), never whole game files, fonts or recordings. The readers take the game's texts
from your own installation; training uses data from your own copy of the game.

sc-ohseearr is an unofficial Star Citizen fan project, not endorsed by or affiliated with the Cloud Imperium group of
companies. Game content and materials are copyright Cloud Imperium Rights LLC and Cloud Imperium Rights Ltd. Star
Citizen® is a registered trademark of Cloud Imperium Rights LLC.

## Credits

[PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) (PP-OCRv6 detector and recognizer),
[NanoDet](https://github.com/RangiLyu/nanodet) (NanoDet-Plus), [ONNX Runtime](https://onnxruntime.ai/) through
[`ort`](https://github.com/pykeio/ort), [StarBreaker](https://github.com/diogotr7/StarBreaker) for reading `Data.p4k`.

## License

Code: MIT, see [LICENSE](LICENSE). Each model package states its own license; see [NOTICE](NOTICE).
