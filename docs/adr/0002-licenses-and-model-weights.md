# 0002. MIT for the code, per-package licenses for models, our weights distributed

- Status: accepted
- Date: 2026-11-05
- Scope: licensing, models

## Context
The code is ours. The models are not uniform: PaddleOCR's detector is Apache-2.0 and unmodified; our recognizer
is derived from PaddleOCR's (Apache-2.0) and fine-tuned on synthetic lines in the game's commercial font
(Blender Pro, extracted from the user's `Data.p4k`, used locally only) and on real game lines; the warehouse
layout model has our own weights on the NanoDet-Plus architecture (Apache-2.0). Distributing weights trained on
a commercial font and game screenshots is a grey area, as is most of the game's modding scene.

## Decision
- Code: MIT (`LICENSE`).
- Every model package states its license in `model.toml` (SPDX) and `MODEL.md`; `NOTICE` lists them:
  `paddle-det-tiny` Apache-2.0, `sc-rec-latin` Apache-2.0 (derived), `sc-warehouse-layout` MIT.
- Our fine-tuned and trained weights are distributed.
- The font, screenshots, recordings and any extracted game file never go into the repository.

## Alternatives
- Keeping our weights private (only the code public): the weights are what makes the OCR good on game screens.

## Consequences
- A new model package must state its license and source before it is merged.
