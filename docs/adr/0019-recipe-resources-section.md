# 0019. The machine's settings live in a recipe's `resources` section

- Status: accepted
- Date: 2026-11-08
- Scope: `training/` (both pipelines, the recipes)

## Context
Settings that change speed and memory but not the model were mixed into the recipes' `train` and `synth` sections
or hard-coded: the device ("CUDA when available"), data loader workers, the evaluation's batch size and threads.
A recognizer run on a 16 GB GPU grew PyTorch's cache (batches of varying widths) past the GPU; Windows spilled 5 GB
into system RAM and an epoch took 800 s instead of 80–90 s, with no way to limit it from the recipe.

## Decision
- Every recipe has a `resources` section: `device` (`auto`, `cuda`, `cuda:<n>`, `cpu`), `memory_fraction` (the share
  of the GPU memory PyTorch may take), `workers`, `pin_memory`, `synth_workers`, `eval_threads`, `eval_batch`.
- Their defaults live in one place (`training/common/recipe.py`, `RESOURCES`); `--set resources.<key>=<value>`
  overrides them per machine. An unknown key is an error.
- What changes the trained model (batch size, epochs, learning rate, data) stays in `train`.
- `sc-rec-latin` sets `memory_fraction: 0.85`.

## Alternatives
- Command line flags per setting: a second place next to the recipe's `--set`.
- A separate machine profile file: one more file to find; `--set` covers the per-machine case.

## Consequences
- The layout training accepts only GPU devices (NanoDet's training here runs on a GPU).
