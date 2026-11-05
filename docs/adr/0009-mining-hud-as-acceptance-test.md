# 0009. The mining HUD reader is the acceptance test

- Status: accepted
- Date: 2026-11-05
- Scope: process

## Context
The platform's claim is that a new use case needs no change below `usecases/`. The user plans a mining app that
reads other parts of the HUD: holographic text over the 3D scene, coloured, glowing, possibly in another font.

## Decision
The mining HUD is built last, as the test of the layering:
- no change to `core` or `sc`; anything it needs there is recorded as a finding and added as a general building
  block, not patched in;
- recognizer or layout training only through a new recipe and data;
- the companion stays untouched (its dumps identical);
- the API is not designed toward mining in advance.

Material: the user's screenshots (several ships, scan states, light, 1080p and 3840×1600).

## Consequences
- The readers' API is derived from the three existing references only.
