# Training

Pipelines that turn a **recipe** (one YAML file per model) and local data into a **model package** (`model.onnx`,
`model.toml`, metrics) like the ones in `models/` (ADR 0005, ADR 0008). One pipeline per model family:

| Pipeline | Family | Makes | Recipes |
|---|---|---|---|
| `training/rec` | `ctc-rec` | a text line recognizer: PP-OCRv6 tiny rec, its head cut to a charset, fine-tuned on synthetic and real lines | `sc-rec-latin` |
| `training/layout` | `nanodet` | a screen's layout detector: NanoDet-Plus-m trained on a COCO dataset | `sc-warehouse-layout` |

Data never goes into git: game fonts, screenshots, recordings, language files, crops and generated datasets stay
in the data root (README.md, ["Data root"](../README.md#data-root)). Only code and recipes are versioned.

```
training/
  pyproject.toml          the uv project (versions pinned to the venv the models were made with)
  common/                 recipes (paths, overrides), dataset formats, image files, package writer, pinned downloads
  rec/                    cut, texts + render (synthetic lines), sheet, backgrounds, teacher, train, export, evaluate
  layout/                 nanodet-plus-m.yml (the architecture), evaluate, sheets (contact sheets)
  recipes/                <id>.yml per model
```

## Setup

Windows, an NVIDIA GPU with CUDA 12.8 wheels (PaddlePaddle's own training does not run on an RTX 50, hence
PyTorch). Python only through the venv (`-I`; a bare `python` on Windows may be the Store stub):

```sh
uv venv --python 3.11 local/detector/.venv
uv pip install --python local/detector/.venv/Scripts/python.exe --index-url https://download.pytorch.org/whl/cu128 torch==2.11.0 torchvision==0.26.0
uv pip install --python local/detector/.venv/Scripts/python.exe -r training/pyproject.toml --group layout
```

(or `uv sync --project training --group layout`, which makes its own `training/.venv`). Below,
`PY = local/detector/.venv/Scripts/python.exe -I`, run from the repository root.

**NanoDet** (layout only), once per data root:

```sh
git clone https://github.com/RangiLyu/nanodet local/detector/nanodet      # at be9b4a9
uv pip install --python local/detector/.venv/Scripts/python.exe --no-deps -e local/detector/nanodet
```

and two patches for torch 2: `nanodet/data/collate.py`: `string_classes = (str,)` instead of the import from
`torch._six`; `tools/export_onnx.py`: `dynamo=False` in the `torch.onnx.export` call.

**Fonts** (recognizers drawing the game's UI). Star Citizen draws its UI in Blender Pro, a commercial font: it is
taken from the player's own `Data.p4k` for local training only and never goes into git or a build. The fonts in
`local/fonts-sc/ttf/` were made like this:

1. Extract the Scaleform font library `Data/UI/fonts/Shared/fonts_en.swf` (29 fonts; `FontConfig.xml` next to it
   maps them to the UI's font names) from `Data.p4k`, e.g. with StarBreaker:
   `starbreaker p4k extract --p4k <install>/LIVE/Data.p4k --filter "Data/UI/fonts/Shared/*" -o local/fonts-sc/p4k`.
2. Convert its `DefineFont3` tags to TrueType: `local/fonts-sc/swf2ttf.py <fonts_en.swf> local/fonts-sc/ttf`
   (a local script, not in git: it reads the glyph shapes (EM 20480, scaled to 2048 units per em), advances and
   kerning, and writes one TTF per font with fontTools). The files are named `<Family>-<Style>.ttf`, e.g.
   `BlenderProMedium-Regular.ttf`, the names the recipe uses.

**Language files**: the recipe `sc-rec-latin` reads the English `global.ini` the game uses (set
`SC_GLOBAL_INI`, e.g. `<install>/LIVE/data/Localization/english/global.ini`) and a German pack
(`local/localization/sc-deutsch/global.ini`).

**Environment variables**:

| Variable | Used by | What |
|---|---|---|
| `SC_OHSEEARR_DATA` | both pipelines (`--data-root` overrides it) | the data root; unset: `<repo>/local` |
| `SC_GLOBAL_INI` | recipe `sc-rec-latin` (`synth.packs.en`) | the English `global.ini` the game uses |
| `SC_DATA_P4K` | `scripts/extract-global-ini.{ps1,sh}` (`-P4k` overrides it) | the game's `Data.p4k`, for the English texts (and, by hand, the fonts) |

## Running

```sh
PY training/rec    training/recipes/sc-rec-latin.yml          # all steps; --steps cut,synth,... for a part
PY training/layout training/recipes/sc-warehouse-layout.yml
```

Common options: `--data-root <dir>` (default: the data root), `--out <dir>` (default
`<data root>/training/<id>`), `--set key.sub=value` (override a recipe key, YAML value, e.g.
`--set train.epochs=2`). The result is `<out>/package/`: copy `model.onnx` (and `dict.txt`, `model.toml`) into
`models/<id>/` only after checking the metrics and the consumers' regression dumps (`CLAUDE.md`: results must not
change silently); paste `MODEL.metrics.md` into its `MODEL.md`.

### `rec` steps

| Step | What | Output |
|---|---|---|
| `cut` | downloads the original ONNX and its `inference.yml` (pinned revision, SHA-256), cuts the head to `charset` | `cut/dict.txt`, `cut/model.onnx` (teacher and baseline) |
| `backgrounds` | builds background banks that have a `build` section and no images | the bank's folder |
| `synth` | renders `synth.count` lines and `synth.val_count` validation lines (kept when present) | `synth/`, `synth-val/` (line datasets with `kind` and `font` columns) |
| `train` | the Hugging Face port (`PPOCRV6TinyRecForTextRecognition`, pinned revision) with its head cut like the ONNX, CTC, AdamW, cosine; monitored sets after every epoch | `train/last.pt`, `best.pt`, `log.json` |
| `export` | ONNX opset 17, input `x` with named dynamic `batch`/`width`, softmax output, checked against PyTorch | `model.onnx` |
| `eval` | every `eval.sets` entry on `model.onnx` (and on the cut model with `eval.baseline`) | `eval.json` |
| `package` | the package folder: the model, its manifest (input normalization, `[ctc]`), the dictionary, the eval metrics (with the baseline) and data sizes, their MODEL.md block | `package/` (`model.onnx`, `model.toml`, `dict.txt`, `metrics.json`, `MODEL.metrics.md`) |

Tools on the same entry point: `PY training/rec eval <recipe> --model <onnx>` (measure any model; its `dict.txt`
next to it), `teacher <recipe> --src <crops> --dest <dir> [--model <onnx>] [--min-score 0.95]` (label line crops
with a model: lines read with a mean character probability ≥ the threshold become a dataset),
`backgrounds <recipe> [--force]`, `sheet <recipe> [--real <lines>]` (synthetic lines next to real ones, to judge
the renderer by eye).

### `layout` steps

| Step | What | Output |
|---|---|---|
| `config` | `layout/<arch>.yml` with the recipe's classes, input size, data, schedule, augmentation | `nanodet.yml` |
| `train` | NanoDet's `tools/train.py` in the checkout | `nanodet/` (checkpoints, logs, `model_best/`) |
| `export` | NanoDet's `tools/export_onnx.py` (opset 11, onnxsim) from the best (or last) checkpoint | `model.onnx` |
| `eval` | recall/precision per class at IoU `eval.iou` on the eval split, CPU time per window | `eval.json` |
| `package` | the package folder: the model, its manifest (input normalization, `[nanodet]` from the NanoDet config and the recipe's decoding), the eval metrics and data sizes, their MODEL.md block | `package/` (`model.onnx`, `model.toml`, `metrics.json`, `MODEL.metrics.md`) |

Tools: `PY training/layout eval <recipe> --package <dir>`, `labels <recipe>` (windows with their labels),
`errors <recipe> --package <dir>` (false hits red, misses white), `shots <recipe> --package <dir> <image>...`
(the model over screenshots without labels).

## Dataset formats

- **Line crops** (recognizers): a folder of images and `labels.tsv` (UTF-8, tab-separated, no quoting). Without a
  header the columns are `file`, `text`; with a header row starting with `file` it needs a `text` (or `label`)
  column, further columns are kept (`source` for per-group metrics, `score` for proposed labels, `kind`/`font`
  for synthetic ones). `file` is relative to the folder. Training sets list folders with a `repeat` weight.
- **Layout** (detectors): COCO. A folder with `train.json` and `val.json` (`images`, `annotations` with `bbox`
  `[x, y, w, h]` in pixels, `categories` with ids from 1 in the order of the recipe's `classes`) and the images in
  `train/` and `val/`, at the model's input size. Images without annotations are negatives.

Labels come from a labelling tool, from an earlier read, or are proposed by an existing model (`rec teacher`).
Converters from one app's own recordings live with that app: Quartermaster's companion has
`tools/dataset-export/` (`warehouse_coco.py`: warehouse recordings → COCO; `gear_lines.py`: gear tooltip dumps →
line crops + `index.tsv`; `teacher_lines.py`: warehouse/refinery dumps of the cut model → teacher line sets;
`tooltip_backgrounds.py`: tooltip areas → a background bank). The gear evaluation labels come from its
`tools/dataset-export/ocr-eval-label.ts` (Quartermaster's catalog and language pack: `index.tsv` → `labels.tsv`).

## Recipe format

Common keys: `id` (the package id), `family`, `package` (`version`, `license`, `description`, `source`; `{date}`,
`{commit}`, `{recipe}` in `source` are filled in). Paths are relative to the data root; `${VAR}` is taken from the
environment.

**`ctc-rec`** (`recipes/sc-rec-latin.yml` is the commented example):

- `base`: `onnx`, `yml` (repo, revision, file, sha256 of the original ONNX and its dictionary), `weights` (the
  PyTorch weights' repo and revision).
- `charset`: `ranges` (inclusive code point ranges), `chars`: what the cut keeps besides blank and space.
- `input`: `height`, `width` (nominal), `max_width`.
- `synth`: `count`, `seed`, `val_count`, `val_seed`, `workers`; texts: `packs` (name → `key=value` file),
  `char_map`, `reject`, `corpora` (pools from packs: `ini`; derived: `match`, `words`), `lists`; the mix `kinds`
  (`name`, `w`, `style`, `text` spec or `label_value`); `colours` (name → RGB), `fonts` (`dir`, `fallback`,
  `caps_only`, `groups` with `hscale`/`stroke` ranges), `backgrounds` (bank → `glob`, `flip`, `jitter`, optional
  `build`), `styles` (per style: `fonts`, `cap` heights in px, `colours`, `dim`, `split`, `background` rules by cap
  height, `bars`, `flat`, `box`, `neighbours`), `render` (supersampling, tracking, pitch, opacity, blur, noise,
  JPEG, the crop's margins). Text specs and their template functions: `rec/texts.py`.
- `train`: `data` (`path` or `@synth`, `repeat`), `epochs`, `lr`, `weight_decay`, `batch`, `workers`, `train_bn`,
  `clip`, `warmup`, `select` (`last` or `best` by the sum of the monitored CERs), `monitor` (eval sets or
  `@synth-val`, `limit`, `seed`).
- `eval`: `sets` (name → `path`, `group_by`, `subsets` name → substring), `normalize`, `headline`, `baseline`.

**`nanodet`** (`recipes/sc-warehouse-layout.yml`): `data` (the COCO folder), `arch` (`layout/<arch>.yml`),
`input` (`width`, `height`), `classes`, `normalize` (`mean`, `std` in BGR, 0–255), `train` (`nanodet` checkout,
`epochs`, `schedule_epochs` (the cosine's length), `val_interval`, `checkpoint`, `batch`, `workers`, `lr`,
`weight_decay`, `warmup_steps`, `seed`, `augment` (NanoDet's pipeline keys)), `decode` (`score`, `nms`: into
`model.toml`), `eval` (`split`, `iou`, `threads`), `shots` (`height`, `area`, `overlap`).

## A new use case (e.g. the mining HUD)

No code changes, only a recipe and data:

1. **Layout**: collect screenshots (or recordings) of the HUD, label the parts (any COCO-capable labelling tool,
   or a converter from an app's own recordings) into `local/<usecase>/coco/{train,val}.json` with the windows at
   the input size you want; add screenshots of other screens as negatives. Copy `sc-warehouse-layout.yml` to
   `recipes/sc-mining-hud-layout.yml`, change `id`, `package`, `data`, `classes`, `input`, maybe `augment` and
   `epochs`. `PY training/layout labels <recipe>` to check the labels, then run it.
2. **Recognizer**, when the generic one misreads the HUD (another font, colours, numbers only): a recipe that
   names the HUD's font(s) and cap heights (`styles`), its colours, a background bank of HUD areas without text
   (`backgrounds` with `build: screen-tiles` over screenshots and the HUD's `rect`), and its texts as `kinds`
   (e.g. templates `"{float:0:100:.1f} %"`, `"MASS: {int:100:90000:,}"`, a `pick` from a pool of ore names built
   from the packs with `ini`). Real lines: crops plus `labels.tsv`, or `rec teacher` over crops to propose them.
   `PY training/rec sheet <recipe> --real <lines>` compares synthetic and real lines before training; `eval.sets`
   names the real lines that judge it.
3. The package goes to `models/<id>/` once its metrics hold and the reader's regression dumps were checked.

## Reproduction (2026-11-06)

Both models in `models/` were remade from their recipes into `local/training/<id>/` (full runs, RTX 5080), and the
packages in `models/` were measured with the new eval code (the same numbers as the old scripts).

**sc-rec-latin** (21,881 real gear tooltip lines; teacher set 1,266 warehouse/refinery lines). Data: 100,000
synthetic lines rendered from the recipe (146 s, plus 5,000 validation lines), 2,525 teacher lines ×10. Training
10 epochs, 30 min; whole run 34 min.

| | `models/` (2026-10-31) | retrained |
|---|---|---|
| gear lines exactly right | 99.0 % | 98.8 % |
| gear CER | 0.04 % | 0.05 % |
| titles exactly right | 98.3 % | 98.2 % |
| "&" lines exactly right | 98.6 % | 98.9 % |
| teacher set exactly right | 91.2 % | 91.7 % |

The cut step reproduces `dict.txt` and the old cut model byte for byte; the baseline (cut, before the
fine-tune) measures 90.4 % / 0.26 % / "&" 0.9 %, as in `MODEL.md`.

**sc-warehouse-layout** (validation: 304 windows, IoU ≥ 0.5). Data: 1,166 training windows (870 from two
recordings, 296 without labels), the COCO files identical to the old ones when re-exported. 20 epochs, 11 min.

| Class | `models/` recall / precision | retrained, seed 1 | retrained, unseeded |
|---|---|---|---|
| badge | 98.9 / 93.8 % | 98.9 / 93.4 % | 98.8 / 94.1 % |
| quality | 100 / 87.2 % | 100 / 87.5 % | 100 / 84.4 % |
| title | 97.5 / 91.9 % | 97.5 / 91.3 % | 96.3 / 91.2 % |
| tipline | 97.4 / 91.7 % | 97.4 / 95.4 % | 97.4 / 84.0 % |

The unseeded run picked its epoch-10 checkpoint (validation mAP 0.736 vs 0.685 at epoch 20); the recipe now pins
seed 1. Precision moves by several points between runs; most "false" hits are gaps in the labels (badges the old
read did not see) and the held-out inventory's "0/0" texts.
