# CLAUDE.md

Guidance for Claude Code in this repository.

## Project

`sc-ohseearr`: OCR for Star Citizen screens in Rust: engine (`crates/core`), model packages (`models/*`, each a
package folder and a crate), examples (`examples/`). Planned: the game layer `crates/sc` (language tables from
`global.ini`, UI scale, parsers), screen readers in `usecases/*` (warehouse, gear inventory, refinery, then the
mining HUD as the acceptance test), training pipelines in `training/` (uv/PyTorch), `crates/capture-win`. The
plan and every decision: `docs/adr/` (read `docs/adr/README.md` before proposing a design; never re-propose a
declined alternative unless its forces changed).

First consumer: the Quartermaster companion (`../sc-quartermaster/companion/src-tauri`), which uses this repo
as a path dependency during development (sibling checkout `../sc-ohseearr`).

## Rules

- **English everywhere**: code, comments, docs, UI texts, commit messages.
- **ADRs** in `docs/adr/` for architecture decisions, written only after the user decided (skill `adr`).
- **No game data in git**: no screenshots, recordings, extracted game files or fonts (Blender Pro is commercial).
  Local data goes to `local/` (gitignored). Model weights trained on such data are fine (ADR 0002).
- **No compiled-in game texts**: anything the game shows comes from its `global.ini` at runtime (ADR 0007).
- **Readers' output is neutral**: what the game shows, typed; nothing shaped for one app (ADR 0006).
- **Results must not change silently**: a change to the engine or a model must keep the consumer's regression
  dumps identical, or the difference is shown to and accepted by the user.
- `ort` stays pinned (`=2.0.0-rc.13`); changing it changes every consumer.
- Commits on `main`, with the attribution trailer.

## Commands

```sh
cargo test --release --workspace
cargo clippy --release --workspace --all-targets --features sc-ohseearr-core/directml
cargo run --release --example ocr_dump -- <image> [max_height] [threads] [--det <dir>] [--rec <dir>] [--gpu]
cargo run --release --example layout_dump -- <image> [x y w h] [--layout <dir>]
```
