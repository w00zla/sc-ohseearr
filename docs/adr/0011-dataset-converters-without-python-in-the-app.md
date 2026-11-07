# 0011. The app writes datasets from its recordings in Rust; tools over generic inputs live here

- Status: accepted
- Date: 2026-11-07
- Scope: `training/tools/`, dataset formats, the Quartermaster companion's dump examples
- Supersedes: the last bullet of [0008](0008-training-produces-packages.md) ("Converting companion recordings … is a
  standalone tool in the Quartermaster repo")

## Context
After 0008 the companion kept four Python converters (`tools/dataset-export/`). They ran with this repository's
venv, read and wrote this repository's data root, and found both only through nine directory junctions from the
companion's `local/` into `../sc-ohseearr/local`. Two of them did not convert companion recordings at all:
`teacher_lines.py` cuts crops from box dumps (half of them `ocr_dump` output over refinery screenshots) and
`tooltip_backgrounds.py` inpaints tooltip areas around boxes of a line index (`cv2.inpaint`, nothing comparable in
Rust without OpenCV). The companion should need no Python.

## Decision
- What reads an app's own recording format stays with the app, in its own language: the companion's
  `components_dump --dataset <dir>` writes the gear tooltip lines as line crops plus `index.tsv` straight from the
  read (formerly `gear_lines.py` over the dump's text output).
- Tools whose inputs are generic (box dumps `x y w h score text` over frames, a line index with boxes) live in
  `training/tools/` here: `teacher_lines.py`, `tooltip_backgrounds.py`. Paths come from the data root
  (`--data-root`, `$SC_OHSEEARR_DATA`, `<repo>/local`), as in the pipelines.
- `warehouse_coco.py` (old recordings up to companion 1.3.1 → COCO) is dropped; its dataset exists. A COCO export of
  the current recording format goes into the companion's `inventory_dump` when it is needed.
- The companion's `local/` links into this data root no more; its tools take the data root as an argument.

## Alternatives
- Keep the junctions: hides which repository owns what; a recursive delete there deletes training data.
- Port all four to Rust in the companion: `teacher_lines` and `tooltip_backgrounds` are not about its recordings,
  and inpainting would need OpenCV or an own implementation.
- Rust instead of Python for the gear export, bit for bit: not possible, the JPEG decoders differ.

## Consequences
- The moved scripts reproduce the existing datasets byte for byte (`ocr-train/teacher`, `ocr-eval-teacher`,
  `ocr-train/bg`). The Rust gear export over the same reads gives the same `index.tsv` byte for byte (26,445
  lines); its crops differ in the pixels (98.3 % of the channel values equal, at most 3 off).
- A re-export of the gear lines gives the same boxes but slightly different pixels; a retrained `sc-rec-latin` must
  be measured again before its package replaces the one in `models/`.
