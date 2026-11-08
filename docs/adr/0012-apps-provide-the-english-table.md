# 0012. Each app decides how it provides the English table

- Status: accepted
- Date: 2026-11-08
- Scope: `crates/sc` (`Locale`), consumers
- Supersedes: the example "Quartermaster: its server ships it per build" in the last decision bullet of
  [0007](0007-game-texts-from-global-ini.md)

## Context
ADR 0007 makes every reader take the game's texts from `global.ini` and leaves the English table (the game's own
texts inside `Data.p4k`) to the apps. Its last bullet also named how one app would do it: the Quartermaster server
ships the table per build. That is a decision of one consumer recorded in the platform, and it no longer holds: the
companion is to become independent of the Quartermaster web app (a standalone version is planned), so a server
cannot be its only source.

## Decision
- This repository decides nothing about how an app obtains the English table. The library takes a `Locale` built
  from a pack and/or a table at a path the app passes; `scripts/extract-global-ini.{ps1,sh}` stay as the documented
  way to extract it.
- How the Quartermaster companion obtains it is decided in that repository, not here.

## Alternatives
- Keep the Quartermaster example in 0007: a platform record that is wrong for its own first consumer.

## Consequences
- ADR 0007's other decisions stand unchanged (loc keys as anchors, no compiled-in texts, no p4k code in the
  library, an explicit error without any table).
- The companion's way to the table is an open decision there (its `.claude/open-points.md`).
