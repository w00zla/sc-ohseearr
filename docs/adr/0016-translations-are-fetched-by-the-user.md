# 0016. Translation files are the user's to get; the script exports only the English table

- Status: accepted
- Date: 2026-11-08
- Scope: `training/README.md`, `scripts/`, the recipes' language files
- Supersedes: in [0015](0015-small-game-data-excerpts-in-git.md) the bullet that has the export script fetch
  community content (the German language pack) pinned by version and SHA-256

## Context
ADR 0015 planned one script that exports the game-derived training inputs and also downloads the German language
pack the recognizer's recipe uses, pinned by tag and hash. Translations are community projects with their own
releases and licences (StarCitizen-Deutsch-INI: CC BY-NC-SA 4.0, a release per patch); pinning one of them in this
repository ties the recipe to a version nobody maintains here.

## Decision
- The repository fetches no translation file. `training/README.md` suggests sources (e.g. StarCitizen-Deutsch-INI),
  says to get the file yourself and explains how the recipe uses it (`synth.packs`, the corpora drawn from it).
- The script for game data is `scripts/extract-global-ini.{ps1,sh}`: it exports the English `global.ini` from the
  user's own `Data.p4k` with starbreaker-ps.

## Alternatives
- A pinned download of the German pack: a version and licence to track here for a file that changes every patch.

## Consequences
- A recipe run needs the user to place a translation file; without one, its `packs` entry must point elsewhere.
