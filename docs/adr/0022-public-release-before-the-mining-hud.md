# 0022. The public release does not wait for the mining HUD reader

- Status: accepted
- Date: 2026-11-08
- Scope: release (ADR 0001), the mining HUD reader (ADR 0009)

## Context
ADR 0001 keeps the repository private until the user's acceptance; ADR 0009 makes the mining HUD reader, built
last, the test of the layering. The plan coupled the two: public only after the mining HUD test. The mining HUD needs
material that does not exist yet (the user's screenshots of several ships, scan states and light), while the three
readers, the training pipelines and the scripts for the game data are built and tested.

## Decision
- The public release (git tags, the repository public) comes first, when the user accepts it; it does not wait for the
  mining HUD reader.
- The mining HUD reader follows after the release and stays the test of the layering with ADR 0009's rules (no change
  to `core` or `sc`, training through a recipe only, the companion untouched, the API not designed toward mining in
  advance).

## Alternatives
- Release after the mining HUD test, as planned: holds a working platform back for material not yet recorded.

## Consequences
- The API may still change after the release when the mining HUD shows a missing building block; such a change is a
  finding of ADR 0009 and a new tag, not a reason to delay.
- Before the release the visibility of sc-ohseearr-fixtures must be decided (ADR 0021): while it stays private, a
  public clone cannot check out `fixtures/` and the readers' tests fail there.
