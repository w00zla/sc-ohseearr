# 0013. "Reader" as the one term: `readers/` and the `Reader` trait

- Status: accepted
- Date: 2026-11-08
- Scope: repository layout, `crates/sc` (`reader`), docs
- Supersedes: the folder name `usecases/` of [0004](0004-layered-structure.md) and the trait name `ScreenReader` of
  [0010](0010-screen-reader-interface.md)

## Context
The per-screen folders were called `usecases/` and their trait `ScreenReader`. "Use case" reads like requirements
and test specs, not like a folder of code. "Screen reader" is the established name of assistive software that reads
the screen aloud for blind users (NVDA, JAWS): for a public repository a search and a first look point there. And
the planned mining reader (ADR 0009) reads in-universe HUD elements over the 3D scene, not a screen.

## Decision
- One term: a **reader** reads one part of the game's UI (a terminal tab, an inventory panel, a HUD element).
- `usecases/<id>/` becomes `readers/<id>/` (reader, models, recipe, regression manifest, card); the card is the
  reader's card.
- The trait `sc::ScreenReader` becomes `sc::Reader`; its methods and types stay (`probe`, `read`, `reset`,
  `set_diagnostics`, `Frame`, `Reading`, `Field`, `Name`, `ReadError`).
- Concrete readers name what they read (`WarehouseReader`, `GearInventoryReader`, `RefineryReader`, later e.g.
  `MiningHudReader`). No sub-kinds ("screen reader", "HUD reader") as types or terms.
- Crate names stay (`sc-ohseearr-warehouse`, …).
- "Use case" in ADRs 0001–0012 means a reader in this sense.

## Alternatives
- `screens/` with "screen" defined broadly (any readable UI): cheapest (folder only), but keeps the collision with
  assistive screen readers and stretches "screen" over HUD elements.
- `features/` or `game-features/`: collides with Cargo features; vague.
- Sub-kinds `ScreenReader`/`HudReader` on a base `Reader`: brings the collision back one level down, and designing
  for the mining HUD before it is built goes against ADR 0009; whether a HUD reader needs anything else shows when
  it is built, as a finding.
- A descriptive `kind` field in each card (terminal, inventory, hud): not needed now.

## Consequences
- Consumers change their path dependencies to `readers/<id>` and `ScreenReader` to `Reader` (the companion: three
  paths in `Cargo.toml`, the imports of its adapters). No change in behaviour or output.
