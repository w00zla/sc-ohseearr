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
| [0014](0014-regression-data-only-from-regression-folders.md) | Regression data only from `regression/<reader>/`; screenshots are scratch | accepted (training-sources bullet superseded by 0020, test replacement decided in 0021) | 2026-11-08 |
| [0015](0015-small-game-data-excerpts-in-git.md) | Small game-data excerpts may go into git; the rest is exported by a script | accepted (translation fetching superseded by 0016, test frames by 0021) | 2026-11-08 |
| [0016](0016-translations-are-fetched-by-the-user.md) | Translation files are the user's to get; the script exports only the English table | accepted | 2026-11-08 |
| [0017](0017-apps-make-every-model.md) | The app makes every model; a reader gets the engines it needs | accepted | 2026-11-08 |
| [0018](0018-refinery-digits-no-synthetic-retrain.md) | Small refinery digits at 1080p: no synthetic retrain; apps gate on the score | accepted | 2026-11-08 |
| [0019](0019-recipe-resources-section.md) | The machine's settings live in a recipe's `resources` section | accepted | 2026-11-08 |
| [0020](0020-training-sources-in-their-own-folder.md) | Training sources live in `sources/`; scratch is scratch for training too | accepted | 2026-11-08 |
| [0021](0021-reader-test-frames-in-a-fixtures-repository.md) | The readers' tests assert on recording frames from a separate fixtures repository | accepted | 2026-11-08 |
| [0022](0022-public-release-before-the-mining-hud.md) | The public release does not wait for the mining HUD reader | accepted | 2026-11-08 |
| [0023](0023-pull-requests-tests-and-releases-on-github.md) | Pull requests only, tests on pull requests, releases by tag | accepted | 2026-11-08 |
