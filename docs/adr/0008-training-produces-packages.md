# 0008. Training produces model packages through recipes; data stays local

- Status: accepted; its last decision bullet superseded by [0011](0011-dataset-converters-without-python-in-the-app.md); the bullet "Data never goes into git" superseded by [0015](0015-small-game-data-excerpts-in-git.md) (small excerpts allowed)
- Date: 2026-11-05
- Scope: `training/`, models, use cases

## Context
The companion's training is tied to it: the layout detector's labels come from its debug recordings (bootstrapped
from the read that preceded the model), the recognizer's evaluation labels come from Quartermaster's catalog
(`label.ts`, Node), the synthetic renderer is shaped for gear tooltips. A new use case (other HUD parts, other
fonts, colours, backgrounds) must be trainable without touching code.

## Decision
- `training/` is one uv project (PyTorch) with a pipeline per family (`rec`: synthetic rendering, teacher crops,
  fine-tune, ONNX export with named dynamic dimensions, evaluation; `layout`: labels, NanoDet training, export,
  evaluation) and recipes per use case (`recipes/<id>.yml`): text sources, fonts, sizes, backgrounds, data.
- A recipe ends in a model package (`model.onnx`, `model.toml`, metrics for `MODEL.md`).
- Neutral dataset formats: COCO for layout data, line crops with a TSV of labels for recognizers; labels from a
  labelling tool or proposed by the existing models.
- The two existing models must be reproducible from their recipes (same metrics) before the pipelines count as
  done.
- Data never goes into git: game fonts (extracted from the user's own `Data.p4k`), screenshots and recordings stay
  in `local/`. Regression data stays local too; only manifests (file lists, expected outputs) are versioned.
- Converting companion recordings into the dataset format is a standalone tool in the Quartermaster repo (it
  knows that format), not here.

## Consequences
- The pipelines are the largest part of the move; Quartermaster-specific labelling (`label.ts`) stays there as one
  possible labeller.
