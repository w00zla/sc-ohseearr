# 0020. Training sources live in `sources/`; scratch is scratch for training too

- Status: accepted
- Date: 2026-11-08
- Scope: the data root, `training/recipes/*.yml`, `training/tools/`
- Supersedes: in [0014](0014-regression-data-only-from-regression-folders.md) the bullet "Training sources … are not
  regression data and are not affected"

## Context
ADR 0014 made `screenshots/` and `stock_scan_debug/` scratch that may be tidied or deleted at any time, but left the
training sources out of it. These sources lay in exactly those folders: the recipe `sc-rec-latin` built its terminal
background bank from `screenshots/refinery flow*`, `training/tools/teacher_lines.py` read the refinery screenshots and
the warehouse recordings …6676 and …6994 from `screenshots/` and `stock_scan_debug/`, and
`training/tools/tooltip_backgrounds.py` the gear recordings …2503 and …8932 from `stock_scan_debug/`. Tidying the
scratch folders would have made both models irreproducible without notice.

## Decision
- Every screenshot or recording a recipe or a tool in `training/tools/` reads lives in the data root under
  `sources/<set>/`, under its original folder name (`sources/refinery flow/`, `sources/qm-inventory-debug-<id>/`), so
  the teacher dumps that name their frames by folder stay valid.
- Recipes and tools reference only `sources/` for such inputs; nothing in them refers to `screenshots/` or
  `stock_scan_debug/`. Those stay scratch without exception.
- A frame that should become a training source is copied into `sources/` explicitly.
- Regression sets stay in `regression/<reader>/` (ADR 0014). A recording may be both a regression set and a
  training source; it is then copied into both folders, each folder owning its copy.

## Alternatives
- Narrow 0014 to "scratch, except what a recipe or tool references": no copy, but whoever tidies must grep the
  recipes first; the folders stay mixed.
- Train from the regression sets: ties training to the regression folders' life cycle and blurs which recording
  serves which purpose.

## Consequences
- The seven sets (~1.4 GB) were copied into `sources/`; the rebuilt datasets match the existing ones (teacher lines
  train and eval byte for byte, tooltip bank byte for byte, terminal bank 261 of 261 tiles equal in content).
- The warehouse recordings …6676 and …6994 exist twice (regression and source).
