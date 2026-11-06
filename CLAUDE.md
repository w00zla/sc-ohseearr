# CLAUDE.md

Guidance for Claude Code in this repository.

## Project

`sc-ohseearr`: OCR for Star Citizen screens in Rust, extracted from the Quartermaster companion (2026-11-05/06):
- `crates/core`: the engine (ONNX Runtime on CPU or DirectML; model families `DbDetector`, `CtcRecognizer`, `NanoDet`;
  model packages with a `model.toml` manifest; `Ocr`; `group_lines`; typed `Error`).
- `crates/sc`: the game layer (`Locale` from `global.ini`, `CenterUi`, parsers, the `ScreenReader` interface).
- `crates/capture-win`: DXGI capture of any executable's window, cursor rest, change gate (Windows; stubs elsewhere).
- `models/*`: model packages, each a folder and a crate (`package()` compiled in, `ModelPackage::load` at runtime).
- `usecases/*`: screen readers `warehouse`, `gear-inventory`, `refinery`, each with `README.md` (the card),
  `regression.toml` and `examples/regress.rs`.
- `training/`: pipelines (rec, layout) and recipes producing model packages (uv/PyTorch).
- `scripts/extract-global-ini.{ps1,sh}`: the game's English texts from `Data.p4k` via the StarBreaker CLI.

Status: local only (public after the user's acceptance). Phases 1–5 of ADR 0004 are built; phase 6, the mining HUD
reader as acceptance test (ADR 0009), is not. Every decision is an ADR in `docs/adr/`: read `docs/adr/README.md` before
proposing a design and never re-propose a declined alternative unless its forces changed.

Consumers: the Quartermaster companion `../sc-qm-companion` (path dependencies on core, sc, capture-win, the use cases
and model crates). Its `scripts/regression-dumps.sh`, compared byte for byte with a baseline, is the gate for any change
here that could move results.

## Rules

- **English everywhere**: code, comments, docs, UI texts, commit messages.
- **ADRs** in `docs/adr/` for architecture decisions, written only after the user decided (skill `adr`).
- **No game data in git**: no screenshots, recordings, extracted game files, game texts, crops or fonts (Blender Pro is
  commercial). Local data lives in `local/` (gitignored; here a junction to `../sc-quartermaster/local`). Model weights
  trained on such data are fine (ADR 0002).
- **No compiled-in game texts**: readers anchor on loc keys and get texts from `Frame::locale` (ADR 0007); test fixtures
  may hold the few keys a test needs.
- **Readers' output is neutral** (ADR 0006, 0010): what the game shows, typed; diagnostics separate.
- **Results must not change silently**: a change to the engine, a model or a reader keeps the consumer's regression
  dumps identical, or the difference is shown to and accepted by the user.
- `ort` stays pinned (`=2.0.0-rc.13`); `rustfmt.toml` (max width 120) applies; clippy clean.
- Commits on `main`, with the attribution trailer.
- Python only through the venv (`local/detector/.venv/Scripts/python.exe -I`), never bare `python`; no WSL.

## Commands

```sh
cargo test --release --workspace
cargo clippy --release --workspace --all-targets --features sc-ohseearr-core/directml
cargo fmt --all
cargo run --release --example ocr_dump -- <image> [max_height] [threads] [--det <dir>] [--rec <dir>] [--gpu]
cargo run --release --example layout_dump -- <image> [x y w h] [--layout <dir>]
cargo run --release -p sc-ohseearr-<warehouse|gear-inventory|refinery> --example regress -- [folders] [--data <root>]   # refinery: --english <global.ini> [--pack <ini>]
cargo run --release -p sc-ohseearr-capture-win --example grab -- [exe] [out.png] [delay_s]
scripts/extract-global-ini.ps1 -StarBreaker <starbreaker.exe> -P4k <Data.p4k> -Out <dir>   # or .sh; -> global-english-<version>.ini
```

Data root for tests and `regress`: `$SC_OHSEEARR_DATA`, else `local/` (`local/screenshots/…`, `local/stock_scan_debug/…`).
Training: `training/README.md`.
