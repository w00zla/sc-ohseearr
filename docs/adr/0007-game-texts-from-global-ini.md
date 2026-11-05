# 0007. Game texts only from global.ini, no compiled-in texts

- Status: accepted
- Date: 2026-11-05
- Scope: `crates/sc`, readers

## Context
Every screen shows texts the player's language decides: a language pack in
`data/Localization/<language>/global.ini` (`user.cfg`'s `g_language`; a pack is a complete file, English ones like
StarStrings reword the English texts), otherwise the game's built-in English, which lives inside `Data.p4k`. The
companion's refinery reader carried 9 English labels compiled in, a copy that goes stale with patches and packs.

## Decision
- Readers define their anchors as loc keys only, never as texts.
- Texts come from the active pack's `global.ini`; without a pack from the English `global.ini` extracted from the
  game; with neither, an explicit error (no guessing).
- Read names resolve to candidate loc keys through the same table (game-native identifiers, no app catalog).
- The library has no p4k code. `scripts/extract-global-ini.{ps1,sh}` extract the English table with the
  StarBreaker fork's CLI (`starbreaker p4k extract --filter "Data/Localization/english/global.ini"`; ~1 s, 10 MB,
  tried with the game closed), named by the build in `build_manifest.id`; the docs only point to it. Apps provide
  the table their own way (Quartermaster: its server ships it per build).

## Alternatives
- Compiled-in English labels as a fallback: declined; a stale copy of the game's texts.
- Reading `Data.p4k` in the library (the fork's `starbreaker-p4k` crate behind a feature): declined; a heavy
  dependency for one file per patch.
- Shipping a snapshot of the English table per game version: more grey area and a release per patch.

## Consequences
- Players without a pack need the extracted table; apps must provide it.
- Untested: extracting while the game runs and holds `Data.p4k` open.
