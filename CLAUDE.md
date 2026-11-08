# CLAUDE.md

Guidance for Claude Code in this repository.

## Project

`sc-ohseearr`: OCR for Star Citizen screens in Rust, extracted from the Quartermaster companion (2026-11-05/06). The
crates, model packages, readers and pipelines: README.md, "Layout"; each reader's card in `readers/<name>/README.md`.

Status: private on GitHub (w00zla/sc-ohseearr), public after the user's acceptance. Phases 1–5 of ADR 0004 are built; phase 6, the mining HUD
reader as acceptance test (ADR 0009), is not. Every decision is an ADR in `docs/adr/`: read `docs/adr/README.md` before
proposing a design and never re-propose a declined alternative unless its forces changed.

Consumers: the Quartermaster companion `../sc-qm-companion` (path dependencies on core, sc, capture-win, the readers
and model crates). Its `scripts/regression-dumps.sh`, compared byte for byte with a baseline, is the gate for any change
here that could move results.

## Rules

- **English everywhere**: code, comments, docs, UI texts, commit messages.
- **READMEs (all of them, also `MODEL.md`) describe the current state only**: no ADR numbers or links, no plans,
  open points, phases or "to come"/"until replaced". Decisions live in `docs/adr/`, plans in `.claude/open-points.md`.
- **ADRs** in `docs/adr/` for architecture decisions, written only after the user decided (skill `adr`).
- **Game data in git only as small excerpts** (ADR 0015): what a test, eval or example concretely needs (the loc keys
  a test reads, a few records, single crops or frames; a file up to ~1 MB, a set a few MB), with its source noted.
  Never whole files: no `global.ini`, fonts (Blender Pro is commercial), `Data.p4k` extracts, complete recordings or
  datasets; the English `global.ini` comes from `Data.p4k` with any extracting tool (`scripts/extract-global-ini.*`: StarBreaker), fonts and
  translations users get themselves (`training/README.md`, ADR 0016). Local data lives
  in `local/` (gitignored: screenshots, recordings, training data, the venv with the NanoDet checkout). Model weights
  trained on such data are fine (ADR 0002).
- **No compiled-in game texts**: readers anchor on loc keys and get texts from `Frame::locale` (ADR 0007); test fixtures
  may hold the few keys a test needs.
- **Readers' output is neutral** (ADR 0006, 0010): what the game shows, typed; diagnostics separate.
- **KISS and explicit** (ADR 0017): no hidden shortcuts (a reader building its own model, lazy rebuilds on changed
  terms); the app makes every model and passes it in. A little more boilerplate beats logic nobody checks.
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
cargo run --release -p sc-ohseearr-<reader> --example regress-<reader> -- [folders] [--data <root>] [--threads n (3)] [--gpu]   # reader: warehouse, gear-inventory, refinery (needs --english <global.ini> and/or --pack <ini>)
cargo run --release -p sc-ohseearr-capture-win --example grab -- [exe] [out.png] [delay_s]
scripts/extract-global-ini.ps1 -StarBreaker <starbreaker.exe> -P4k <Data.p4k> -Out <dir>   # or .sh; -> global-english-<version>.ini
```

Data root for tests, `regress` and training: README.md, "Data root".
Training: `training/README.md`.
