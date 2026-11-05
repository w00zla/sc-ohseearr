# 0005. Model packages with a manifest, compiled in or loaded

- Status: accepted
- Date: 2026-11-05
- Scope: models, core API

## Context
In the companion each model's properties (input normalization, thresholds, classes, dictionary, sizes) were
constants next to the code, and the models were compiled in with `include_bytes!` (the Windows release is a bare
exe). A collection of models for many use cases needs models that can be swapped and added without code changes.

## Decision
- A model package is a folder `models/<id>/`: `model.onnx`, `model.toml`, `MODEL.md`, a dictionary for
  recognizers, and a crate `sc-ohseearr-model-<id>` whose `package()` returns it compiled in.
- `model.toml`: `id`, `version`, `family` (`db-det`, `ctc-rec`, `nanodet`), `license` (SPDX), `description`,
  `source`; `[input]` channel order, `mean`/`std` in that order (normalization `(x/255 - mean) / std`), fixed or
  nominal `width`/`height`; the family's section (`[db]` thresholds and unclip, `[ctc]` dictionary and space,
  `[nanodet]` strides, bins, classes, score, NMS). Unknown fields are refused.
- `ModelPackage::embedded(...)` (compiled in) and `ModelPackage::load(dir)` (runtime) yield the same thing; the
  families (`DbDetector`, `CtcRecognizer`, `NanoDet`) take a package.
- Runtime policies stay code options, not manifest fields: threads, GPU, the recognizer's batch size and GPU
  widths, the detector's CPU fallback height.
- All model files are in git (no LFS; ~10 MB today).

## Alternatives
- Models fetched at build time (the companion's Node script with pinned SHA-256): an extra toolchain and network
  access per build for files that fit in git.
- One crate with all models behind features: one crate per model reads plainly and keeps a binary to what it uses.

## Consequences
- Trying another model is `ModelPackage::load` of a folder; the examples take `--det`, `--rec`, `--layout`.
- A package's numbers must match how it was trained; the manifest is their single source.
