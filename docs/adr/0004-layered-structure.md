# 0004. Layered structure: core, sc, capture, models, use cases, training

- Status: accepted; the folder name `usecases/` superseded by [0013](0013-readers-instead-of-use-cases-and-screen-readers.md) (`readers/`)
- Date: 2026-11-05
- Scope: repository layout

## Context
The companion has three readers built on three patterns: the warehouse (a learned layout model, then targeted
reads), the character inventory (calibrated geometry: a panel in perspective scaled by a factor read from the
page number, fixed areas, line grouping, pixel heuristics) and the refinery terminal (text anchors: panels and
columns found by their localized labels). Much repeats across them (height-relative UI coordinates, crop and
scale small areas before reading, reads restricted to a charset, line grouping, language tables); some is
specific to the game but not to a screen, some to one screen.

## Decision
- `crates/core`: game-agnostic engine: ONNX Runtime sessions (CPU, DirectML), the model families (`db-det`,
  `ctc-rec`, `nanodet`), the manifest, image helpers, reading building blocks.
- `crates/sc`: Star Citizen specific, screen-agnostic: language tables from `global.ini` (ADR 0007), UI scale
  and calibration, parsers (SCU, quality, durations, counts).
- `crates/capture-win`: live capture of any executable's window on Windows (DXGI duplication, cursor, rest
  detection, change gate), no Tauri.
- `models/<id>/`: one model package per folder, which is also its crate (ADR 0005).
- `usecases/<id>/`: one screen each: its reader, its models, its training recipe, its regression manifest, a card
  (screen, pattern, verified resolutions, game version, measurements).
- `training/`: one uv project with the pipelines per family and the recipes per use case (ADR 0008).
- Phases: 1 core and models (the companion on them, its dumps identical); 2 `sc`; 3 readers (ADR 0006);
  4 training; 5 `capture-win`; 6 the mining HUD (ADR 0009).

## Alternatives
- A frame recorder tool for collecting samples: declined; hand-picked screenshots served every screen so far,
  and recordings drown in near-identical frames. A use case that needs volume records its own reads.
- A screen router (which screen is visible) in the library: left to the apps.

## Consequences
- A new screen is a new folder in `usecases/` (and maybe in `models/`), not a change to `core` or `sc`.
