# 0021. The readers' tests assert on recording frames from a separate fixtures repository

- Status: accepted
- Date: 2026-11-08
- Scope: the readers' tests, `crates/test-support`, the submodule `fixtures/`, the repository sc-ohseearr-fixtures
- Decides: what replaces the screenshot tests, left open by [0014](0014-regression-data-only-from-regression-folders.md)
- Supersedes: in [0015](0015-small-game-data-excerpts-in-git.md) "single line crops or frames a test asserts on" as
  far as frames: whole frames go into the fixtures repository, not into this one

## Context
ADR 0014 made `screenshots/` scratch and took the readers' screenshot tests away without deciding what replaces them.
They were the only automatic checks on real game content (titles, stats, pages, qualities, counts, work orders, all
checked by hand). The `regress` examples only print lines to compare by hand. ADR 0015 allows small real fixtures in
git, but frames show whole game screens: once committed here they can only be removed by rewriting this repository's
history (tags, clones and the consumers' path dependencies break), and the repository is to go public. GitHub's
limits (a warning at 50 MB per file, 100 MB refused, about 1 GB per repository) are not the constraint at ~11 MB.

## Decision
- The readers' tests assert on **single frames of the apps' debug recordings** (as recorded: JPEG q90 with the
  cursor in a JSON next to it, cut down to `{"cursor": [x, y]}`), never screenshots. Each reader has frames at
  3840×1600 and 1920×1080 where its recordings have them. The asserted values are checked by hand against the
  frames.
- The frames live in a **separate repository, sc-ohseearr-fixtures**, one folder per reader with a `NOTES.md` naming
  recording, date and game build of every frame. This repository includes it as the **git submodule `fixtures/`**
  (relative URL `../sc-ohseearr-fixtures.git`), which pins the commit.
- `test-support::fixture(reader, name)` reads `fixtures/<reader>/<name>.jpg` and its cursor. A test **fails** when the
  submodule is not checked out (the message names `git submodule update --init`); nothing is skipped silently.
- The sizes of ADR 0015 hold there too: a frame up to ~1 MB, a few MB per reader. Today 20 frames, 3.5–4.2 MB per
  reader.
- Frames are picked from the regression sets (ADR 0014). A frame of a recording that is also a training source is
  noted as such (the warehouse's 3840×1600 frames: recording …6676, the only one with materials at that size).
- Small text excerpts (the loc keys a test reads) stay inline in the tests here (ADR 0015).

## Alternatives
- Frames in this repository (`readers/<id>/tests/frames/`): simplest, but their removal means a history rewrite.
- The same assertions on the frames in the local `regression/` folders: no data in any repository, but the tests run
  only where the data root holds the sets and skip silently elsewhere.
- Golden files of the `regress` output, compared by a test: catch every change, but prove no value right.
- A pinned ZIP as a release asset plus a fetch script: the same separation, but each change of the set needs a new
  asset and hash, and a private repository needs authentication to download.

## Consequences
- A checkout needs `git submodule update --init` before `cargo test`; consumers that only build the crates (the
  companion through path dependencies) do not.
- Deleting or rewriting sc-ohseearr-fixtures does not touch this repository's history; only the pinned gitlink dangles.
- `crates/test-support` no longer reads `screenshots/` (`screenshots()` and `shot()` are gone).
