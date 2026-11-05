<p align="center"><img src="docs/logo.jpg" alt="ohseearr" width="800"></p>

# sc-ohseearr

OCR for Star Citizen screens, in Rust: an engine on ONNX Runtime (CPU, or GPU via DirectML on Windows), a
collection of model packages, and (later) the readers of real game screens and the training pipelines behind
the models. It started as the OCR core of the Quartermaster companion app and is meant for any app that reads
the game's UI.

> Status: early. The engine and the first three model packages are here; the game-specific layer, the screen
> readers, the training pipelines and the capture are planned (`docs/adr/`). APIs will change until 1.0.

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
`ModelPackage::load(dir)`, so a model can be swapped without changing code.

| Package | Family | License | |
|---|---|---|---|
| `paddle-det-tiny` | `db-det` | Apache-2.0 | PP-OCRv6 tiny detector, unmodified |
| `sc-rec-latin` | `ctc-rec` | Apache-2.0 | PP-OCRv6 tiny recognizer cut to Latin, fine-tuned on the game's font |
| `sc-warehouse-layout` | `nanodet` | MIT | the freight manager's warehouse grid and tooltip |

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
cargo test --release --workspace
```

## License

Code: MIT (`LICENSE`). Each model package states its own license; see `NOTICE`. Star Citizen is a trademark of
Cloud Imperium Rights LLC; this project is not affiliated with Cloud Imperium Games.
