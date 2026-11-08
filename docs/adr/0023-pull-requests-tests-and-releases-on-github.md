# 0023. Pull requests only, tests on pull requests, releases by tag

- Status: accepted
- Date: 2026-11-08
- Scope: `main`, `.github/workflows/`, releases, `docs/releases/`

## Context
Until the public release every change was committed straight to `main` by its maintainer. A public repository gets
outside contributions and is consumed by tag (ADR 0001), so `main` needs checks before a change lands and a
release needs a repeatable way from a version to a tag. The sibling project packshot runs the same setup for a
Rust workspace on Windows and Linux.

## Decision
- **Pull requests only:** every change branches from `main`, lands through a pull request and is squash merged.
  A ruleset on `main` enforces it: no direct pushes, no force pushes, no deletion, squash as the only merge method,
  the test jobs as required checks, no bypass.
- **Tests on pull requests** (`.github/workflows/test.yml`, also started by hand): on Ubuntu and Windows,
  `cargo fmt --check`, `cargo test --release --locked` and `cargo clippy --release --locked --all-targets -- -D
  warnings`, the submodule `fixtures/` checked out (ADR 0021); a `shellcheck` job over `scripts/*.sh`. No run on
  pushes to `main`: `main` only changes through pull requests, a run after the merge would test the same code again.
- **Releases by tag** (`.github/workflows/release.yml`): a tag `v<version>` on a commit of `main` whose workspace
  version in `Cargo.toml` is `<version>`; the workflow checks both and publishes a GitHub release with the notes of
  `docs/releases/v<version>.md`. The release has no assets: consumers depend on the tag (ADR 0001).
- A release is a pull request that sets the version and adds its notes file, then the tag on the merged commit.

## Alternatives
- Commits straight to `main`, tests after the push: a broken `main` is found only after it is there.
- Tests on every push to every branch as well: twice the runs for the same code.
- Release assets (the model files, a source archive): GitHub makes the source archive anyway, and the models are
  part of the crates.

## Consequences
- The local full check (each reader's `regress` over its regression set, the recognizer's eval sets) stays the
  maintainer's: its data is not public. CI covers what the repository and the fixtures hold.
- The fixtures repository has no CI and no ruleset: it holds data only.
