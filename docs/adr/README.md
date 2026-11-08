# Architecture Decision Records

| # | Title | Status | Date |
|---|---|---|---|
| [0001](0001-own-repository-for-the-ocr-core.md) | Own repository for the OCR core | accepted | 2026-11-05 |
| [0002](0002-licenses-and-model-weights.md) | MIT for the code, per-package licenses for models, our weights distributed | accepted (data bullet superseded by 0015) | 2026-11-05 |
| [0003](0003-english-everywhere.md) | English everywhere | accepted | 2026-11-05 |
| [0004](0004-layered-structure.md) | Layered structure: core, sc, capture, models, use cases, training | accepted (folder name superseded by 0013) | 2026-11-05 |
| [0005](0005-model-packages-with-a-manifest.md) | Model packages with a manifest, compiled in or loaded | accepted | 2026-11-05 |
| [0006](0006-neutral-reader-output.md) | Neutral reader output; the companion's readers move over completely | accepted | 2026-11-05 |
| [0007](0007-game-texts-from-global-ini.md) | Game texts only from global.ini, no compiled-in texts | accepted (Quartermaster example superseded by 0012) | 2026-11-05 |
| [0008](0008-training-produces-packages.md) | Training produces model packages through recipes; data stays local | accepted (last bullet superseded by 0011, data bullet by 0015) | 2026-11-05 |
| [0009](0009-mining-hud-as-acceptance-test.md) | The mining HUD reader is the acceptance test | accepted | 2026-11-05 |
| [0010](0010-screen-reader-interface.md) | The screen reader interface | accepted (trait name superseded by 0013) | 2026-11-06 |
| [0011](0011-dataset-converters-without-python-in-the-app.md) | The app writes datasets from its recordings in Rust; tools over generic inputs live here | accepted | 2026-11-07 |
| [0012](0012-apps-provide-the-english-table.md) | Each app decides how it provides the English table | accepted | 2026-11-08 |
| [0013](0013-readers-instead-of-use-cases-and-screen-readers.md) | "Reader" as the one term: `readers/` and the `Reader` trait | accepted | 2026-11-08 |
| [0014](0014-regression-data-only-from-regression-folders.md) | Regression data only from `regression/<reader>/`; screenshots are scratch | accepted | 2026-11-08 |
| [0015](0015-small-game-data-excerpts-in-git.md) | Small game-data excerpts may go into git; the rest is exported by a script | accepted | 2026-11-08 |
