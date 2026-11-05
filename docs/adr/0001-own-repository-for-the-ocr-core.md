# 0001. Own repository for the OCR core

- Status: accepted
- Date: 2026-11-05
- Scope: repository, distribution

## Context
The Quartermaster companion's OCR (PP-OCRv6 tiny on ONNX Runtime, a recognizer fine-tuned on the game's font, a
layout detector of our own, DXGI capture) was built and tuned for Star Citizen screens. Other developers and apps
(first: a mining HUD reader) should build on it for other use cases, which the companion's code does not allow:
the OCR sits in `companion/src-tauri/src/ocr.rs` next to Tauri glue and app-specific readers.

## Decision
- A public repository `sc-ohseearr` (MIT), public only after the user's acceptance; local and private until then.
- Its own repository from the first phase: a sibling checkout `../sc-ohseearr`; the companion uses it as a path
  dependency during development, git tags for releases.
- Distribution through git tags only, no crates.io.
- Rust only.
- `ort` stays pinned (`=2.0.0-rc.13`); consumers inherit the pin (one ONNX Runtime per build).

## Alternatives
- A workspace inside the Quartermaster repo first, moved later: the code would grow in a German-documented repo
  and move again; ADRs would have no home until then.
- crates.io: its 10 MB crate limit and the release ceremony bring nothing while the API is young; git tags pin
  as well.
- A C ABI with a .NET binding (NuGet): declined; Rust only. A C# port of the pre/post-processing and readers
  would drift from the Rust code.

## Consequences
- Every fix to the OCR is made here; the companion picks it up through the path dependency at once.
- Both checkouts must sit side by side wherever the companion is built.
