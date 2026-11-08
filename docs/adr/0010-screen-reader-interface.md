# 0010. The screen reader interface

- Status: accepted; the trait name `ScreenReader` superseded by [0013](0013-readers-instead-of-use-cases-and-screen-readers.md) (`Reader`)
- Date: 2026-11-06
- Scope: `crates/sc` (`reader`), `usecases/*`

## Context
ADR 0006 asks for a neutral reader API designed before the three references move. The companion's readers are
free functions whose callers carry their state (the warehouse's layout prior, the inventory's UI scale), take
English labels compiled in, return debug fields next to the results and leave the order of probing to
`ocrwatch`. The user asked to run phases 2–5 through without stopping; this design is recorded here for review
rather than approved up front.

## Decision
- `sc::reader`: `Frame { id, image, cursor, locale }`, the trait `ScreenReader { probe, read, reset,
  set_diagnostics }` with associated `Output` and `Diagnostics` (every diagnostics carries the text boxes read),
  `Reading { output, diagnostics }`, `ReadError` (OCR or locale).
- Readers are objects that keep their own state across frames; `reset` forgets it. `probe` may cache work that
  `read` of the same frame id reuses.
- Which screen is visible, in which order to probe, how often to read: the app's (no router in the library).
- Uncertain values are `Field<T> { value, raw, score, ambiguous }`; names the game shows are
  `Name { text, loc_keys }`, resolved through the frame's `Locale` (empty without one).
- Anchors on game texts come from `Frame::locale` by loc key; a reader that needs them and gets no locale returns
  `ReadError::Locale`.
- Diagnostics that cost extra reads (the warehouse reading every badge) only after `set_diagnostics(true)`.
- Every use case keeps a regression example that prints its output per frame, so an app can verify a change
  against its own recordings.

## Alternatives
- One `read` without `probe`: the app's screen detection would pay a full read per candidate screen.
- Diagnostics as a JSON blob: typed structs keep the app's debug views honest.

## Consequences
- The companion keeps its screen state machine and maps the neutral output to its payloads in an adapter.
