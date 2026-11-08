# 0017. The app makes every model; a reader gets the engines it needs

- Status: accepted
- Date: 2026-11-08
- Scope: `readers/*`, the reader constructors, consumers

## Context
The warehouse reader made its layout detector itself: on its first read it built a `NanoDet` from the compiled-in
`sc-warehouse-layout` package, on the threads and device of the `Ocr` it was given, and built it again when these
changed. The detector and recognizer, by contrast, are made by the app (`Ocr::new` from packages it picks). The
reader crate therefore depended on its model crate, a retrained layout model could not be passed in, and the
hidden rebuild was logic nobody checks from the outside.

## Decision
- The app makes every model engine itself, like the `Ocr`: `NanoDet::new(&package, threads, gpu)`.
- A reader that needs an engine of its own gets it in its constructor and owns it:
  `WarehouseReader::new(detector: NanoDet) -> Result<Self, Error>` (an error when the detector has a class the read
  does not know). No reader makes, loads or rebuilds a model.
- Reader crates do not depend on model crates (only their tests and examples do).
- The app keeps the engines of one device together (the companion: `Ocr` and warehouse reader loaded and dropped as
  one set).
- Prefer explicit code over shortcuts: a little more boilerplate in the app is fine when it makes the flow plain.

## Alternatives
- The reader takes a `ModelPackage` and builds the detector itself on the `Ocr`'s terms: still a hidden build and
  rebuild inside the reader.
- The reader keeps building its model (as before): consumers cannot swap the model, and the dependency stays.

## Consequences
- A consumer of the warehouse reader adds `sc-ohseearr-model-sc-warehouse-layout` (or loads a package folder) and
  builds the detector next to its `Ocr`.
- Switching the device means building the reader again; its prior goes with it.
