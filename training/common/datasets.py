"""The neutral dataset formats.

Line crops (recognizers): a folder with images and `labels.tsv`, UTF-8, tab-separated, no quoting. Either
without a header (columns: file, text) or with a header row whose first column is `file` and which has a
`text` (or `label`) column; further columns are kept (e.g. `source` for per-group metrics, `score` for proposed
labels). `file` is relative to the folder.

Layout (detectors): COCO. A folder with `train.json` and `val.json` (images, annotations with `bbox` as
x, y, w, h in pixels, categories with 1-based ids) and the images in `train/` and `val/`.
"""
import csv
import json
import os


def read_lines(folder: str) -> list[dict]:
    """The rows of `<folder>/labels.tsv` as dicts with `file` (absolute path), `text` and any further columns."""
    path = os.path.join(folder, "labels.tsv")
    with open(path, encoding="utf-8", newline="") as f:
        rows = list(csv.reader(f, delimiter="\t", quoting=csv.QUOTE_NONE))
    if not rows:
        return []
    if rows[0] and rows[0][0] == "file":
        head = rows[0]
        text_col = "text" if "text" in head else "label"
        out = []
        for r in rows[1:]:
            d = dict(zip(head, r))
            d["text"] = d.pop(text_col, "")
            out.append(d)
    else:
        out = [{"file": r[0], "text": r[1] if len(r) > 1 else ""} for r in rows if r]
    for d in out:
        d["file"] = os.path.join(folder, d["file"]).replace("\\", "/")
    return out


def write_lines(folder: str, rows: list[dict], extra: list[str] | None = None):
    """Writes `<folder>/labels.tsv`; rows hold `file` (relative to the folder) and `text`; `extra` columns get a
    header row."""
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, "labels.tsv"), "w", encoding="utf-8", newline="\n") as f:
        if extra:
            f.write("\t".join(["file", "text", *extra]) + "\n")
        for r in rows:
            cells = [r["file"], r["text"], *(str(r.get(c, "")) for c in extra or [])]
            f.write("\t".join(c.replace("\t", " ").replace("\n", " ") for c in cells) + "\n")


def read_coco(folder: str, split: str) -> dict:
    with open(os.path.join(folder, f"{split}.json"), encoding="utf-8") as f:
        return json.load(f)
