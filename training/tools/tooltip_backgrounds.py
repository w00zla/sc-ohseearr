"""Tooltip areas around known text lines, text removed -> a background bank for synthetic lines (recipe
sc-rec-latin: banks `tooltip-1080` = set 2503, `tooltip-1600` = set 8932, gear tooltips of debug recordings).

Reads a line-crop index with the line boxes per frame (`index.tsv`: columns set, frame, x0, y0, w, h; the gear
lines of the Quartermaster companion's `components_dump --dataset`), takes up to `--per-set` frames per set spread
over the folder, cuts the tooltip around its lines, inpaints the boxes and small bright leftovers (unmatched
lines, ▲▼) and writes `<out>/<set>_<frame>.jpg`.

  PY training/tools/tooltip_backgrounds.py [--data-root <dir>] [--index <root>/ocr-eval/index.tsv]
      [--out <root>/ocr-train/bg] [--per-set 90] [--set <name>=<frame folder>]...
PY = local/detector/.venv/Scripts/python.exe -I (from the repository root). Relative paths are taken from the data
root (`--data-root`, else $SC_OHSEEARR_DATA, else <repo>/local). Without --set: 2503 and 8932 (the 1080p and
3840×1600 gear recordings of 2026-10-31).
"""
import argparse
import csv
import os
import re
import sys

import cv2
import numpy as np

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REC = "sources/qm-inventory-debug-"
DEFAULT_SETS = {"2503": REC + "1793452503", "8932": REC + "1793448932"}


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--data-root", default=os.environ.get("SC_OHSEEARR_DATA") or os.path.join(REPO, "local"))
    ap.add_argument("--index", default="ocr-eval/index.tsv")
    ap.add_argument("--out", default="ocr-train/bg")
    ap.add_argument("--per-set", type=int, default=90)
    ap.add_argument("--set", action="append", default=[], metavar="NAME=FOLDER")
    a = ap.parse_args()
    a.index, a.out = (os.path.join(a.data_root, p) for p in (a.index, a.out))
    sets = {k: os.path.join(a.data_root, v) for k, v in (dict(s.split("=", 1) for s in a.set) or DEFAULT_SETS).items()}
    os.makedirs(a.out, exist_ok=True)
    frames = {}
    for r in csv.DictReader(open(a.index, encoding="utf-8"), delimiter="\t", quoting=csv.QUOTE_NONE):
        if r["set"] in sets:
            frames.setdefault((r["set"], r["frame"]), []).append(
                (float(r["x0"]), float(r["y0"]), float(r["w"]), float(r["h"])))
    n = 0
    for st in sets:
        keys = sorted((k for k in frames if k[0] == st), key=lambda k: int(re.sub(r"\D", "", k[1]) or 0))
        step = max(1, len(keys) // a.per_set)
        for key in keys[::step][:a.per_set]:
            img = cv2.imdecode(np.fromfile(f"{sets[st]}/{key[1]}.jpg", np.uint8), cv2.IMREAD_COLOR)
            if img is None:
                continue
            H, W = img.shape[:2]
            k = H / 1080 if W / H < 2 else H / 1180
            boxes = frames[key]
            x0 = min(b[0] for b in boxes) - 16 * k
            y0 = min(b[1] for b in boxes) - 8 * k
            x1 = max(b[0] + b[2] for b in boxes) + 16 * k
            y1 = max(b[1] + b[3] for b in boxes) + 12 * k
            x0, y0, x1, y1 = max(0, int(x0)), max(0, int(y0)), min(W, int(x1)), min(H, int(y1))
            if x1 - x0 < 100 or y1 - y0 < 60:
                continue
            panel = img[y0:y1, x0:x1].copy()
            mask = np.zeros(panel.shape[:2], np.uint8)
            for bx, by, bw, bh in boxes:
                cv2.rectangle(mask, (int(bx - x0) - 2, int(by - y0) - 2), (int(bx + bw - x0) + 2, int(by + bh - y0) + 2), 255, -1)
            # Leftovers without a box (unmatched lines, ▲▼): mask small bright structures too
            g = cv2.cvtColor(panel, cv2.COLOR_BGR2GRAY)
            th = cv2.morphologyEx(g, cv2.MORPH_TOPHAT, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (int(15 * k), int(15 * k))))
            mask |= cv2.dilate((th > 70).astype(np.uint8) * 255, np.ones((3, 3), np.uint8))
            out = cv2.inpaint(panel, mask, 5, cv2.INPAINT_TELEA)
            cv2.imencode(".jpg", out, [cv2.IMWRITE_JPEG_QUALITY, 95])[1].tofile(f"{a.out}/{st}_{key[1]}.jpg")
            n += 1
    print(f"{n} backgrounds -> {a.out}")


if __name__ == "__main__":
    main()
