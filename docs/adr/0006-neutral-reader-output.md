# 0006. Neutral reader output; the companion's readers move over completely

- Status: accepted
- Date: 2026-11-05
- Scope: use cases, reader API

## Context
The companion's readers return what its frontend and debug view need: debug fields (`cells`, raw badge and
quality texts, an `all_badges` flag), the grid fit's internals the caller hands back next frame, an inventory read
orchestrated outside the reader, camelCase JSON for the frontend. Their meaning (which item, stat, material) is
resolved in the companion's TypeScript against Quartermaster's catalog.

## Decision
- The three readers (warehouse, gear inventory, refinery) move here in full as references, with a neutral API
  designed before the move and approved by the user:
  - reader objects that keep their own state (the warehouse's layout prior, the inventory's sequence);
  - a common trait: `probe(frame)` (is my screen visible) and `read(frame)`;
  - input: a frame and an optional cursor;
  - output: only what the game shows, typed; uncertainty in one pattern (value, confidence, raw text);
  - diagnostics (all text boxes, fit internals, timings) separate and opt-in;
  - read names come with candidate loc keys (ADR 0007).
- The companion maps the neutral output to its own payloads in an adapter; its frontend and recording format stay.

## Alternatives
- Moving the readers with their current, companion-shaped output: declined; it contradicts publishing a platform.
- Slim example readers here while the real ones stay in the companion: the references would drift at once.

## Consequences
- Fixes to a reader happen here; the sibling checkout keeps that quick while the companion still changes a lot.
- Regression manifests (file lists and expected outputs) live with each use case; the images stay local (ADR 0008).
