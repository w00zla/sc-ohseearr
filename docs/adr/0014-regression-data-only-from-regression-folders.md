# 0014. Regression data only from `regression/<reader>/`; screenshots are scratch

- Status: accepted
- Date: 2026-11-08
- Scope: the data root, `readers/*/regression.toml`, the readers' tests, the apps' regression dumps

## Context
The regression sets mixed debug recordings and hand-taken screenshots, and covered the readers unevenly: the
warehouse with 3 recordings and 88 screenshots, the gear inventory with 2 recordings only, the refinery with 62
screenshots only (no recording held a refinery read). The tests and manifests pointed into `screenshots/` and
`stock_scan_debug/`, folders that also collect material for analysis and prototyping, so tidying them could break a
test without notice. A recording is what a reader gets live (the app's grab with cursor and timing, saved as JPEG
q90); a screenshot is not.

## Decision
- Regression data lives in the data root under `regression/<reader>/<set>/` and nowhere else; `regression.toml`,
  the tests that read local data and the apps' regression dumps reference only these folders.
- The sets are the apps' debug recordings. Every reader gets its own; the refinery's come from new in-game
  recordings.
- `screenshots/` and `stock_scan_debug/` are scratch for analysis and prototyping: nothing in a test or manifest
  refers to them, they may be tidied or deleted at any time.
- A screenshot that should belong to a regression set is copied into `regression/<reader>/` explicitly.
- Training sources (`training/recipes/*.yml`, `training/tools/`) are not regression data and are not affected.

## Alternatives
- Keep the screenshots as regression data next to the recordings: uneven coverage, and the scratch folders turn
  into test fixtures without anyone deciding so.
- Reference the recordings where they lie (`stock_scan_debug/`): the same problem one folder over.

## Consequences
- ADR 0004 declined a frame recorder in this repository ("recordings drown in near-identical frames"); that stands:
  the recordings come from the apps, the sets are picked by hand.
- The screenshot assertion tests of all three readers (titles, stats, pages, work orders per screenshot) go; what
  replaces them is not decided yet.
