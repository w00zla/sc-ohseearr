# 0015. Small game-data excerpts may go into git; the rest is exported by a script

- Status: accepted; the bullet on fetching community content pinned superseded by [0016](0016-translations-are-fetched-by-the-user.md); frames for the readers' tests superseded by [0021](0021-reader-test-frames-in-a-fixtures-repository.md) (a separate repository)
- Date: 2026-11-08
- Scope: repository content, tests, training data, `scripts/`
- Supersedes: in [0002](0002-licenses-and-model-weights.md) the bullet "The font, screenshots, recordings and any
  extracted game file never go into the repository"; in [0008](0008-training-produces-packages.md) the bullet "Data
  never goes into git …" as far as it bans every piece of game data

## Context
The rule "no game data in git" made the readers' tests depend on local screenshots, left the training recipe
unrunnable without hand-made data (the English `global.ini`, a German language pack, the game's fonts), and kept
fixtures synthetic where real values would test more. Star Citizen tools are a grey area anyway: they cannot do
anything useful without reading the game's data. Whole extracted files (a 10 MB `global.ini`, fonts, recordings)
are still too big, too close to a redistribution of the game, or under a third party's licence (Blender Pro is a
commercial font).

## Decision
- Game-derived data may go into the repository when it is **small and concretely needed** by a test, an eval or an
  example: an excerpt, not a whole file. Examples: the loc keys and texts a test reads, a few records, single line
  crops or frames a test asserts on. Rule of thumb: a file up to ~1 MB, a set up to a few MB.
- **Never whole files**: no `global.ini` (game or language pack), no fonts, no `Data.p4k` or files extracted from it,
  no complete recordings, screenshot folders or datasets.
- Each excerpt states where it comes from (game version or build, loc keys or record names, the recording) next to
  it, e.g. in a comment or a short `.notes.md`.
- What a user needs beyond that, chiefly for training (the English `global.ini`, the game's fonts), is **exported
  from their own `Data.p4k` by a script in this repository** using the StarBreaker fork (starbreaker-ps). Community
  content the recipes use (the German language pack) is fetched from its published source by the same script,
  pinned by version and SHA-256.
- Regression sets (ADR 0014) stay in the data root: recordings are too big; their manifests are versioned.
- The same rule applies to Quartermaster and its companion; each records it in its own docs. packshot and the
  StarBreaker fork keep their own rules.

## Alternatives
- Keep "no game data at all": tests on local data only, training not reproducible without hand work.
- Whole language files and fonts in git: a redistribution of the game's texts and of a commercial font; 10+ MB per
  game patch in the history.
- Git LFS for larger data: not needed while only excerpts go in (ADR 0005: no LFS).

## Consequences
- The readers' tests can ship small real fixtures instead of reading `screenshots/` (ADR 0014 left their
  replacement open).
- The export script is to be built; until then the data is placed by hand as `training/README.md` describes. The
  font conversion (`swf2ttf`) is a local script today and moves into the repository with it.
