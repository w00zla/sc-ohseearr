"""Contact sheets for judging a layout dataset and model by eye (class colours in BGR, cycling)."""
import os
import random

import cv2
import numpy as np

from ..common.datasets import read_coco
from ..common.images import imread
from .evaluate import Package, ground_truth, match

COLORS = [(0, 255, 255), (255, 0, 255), (0, 255, 0), (255, 160, 0), (0, 160, 255), (255, 255, 0)]


def _grid(tiles, out):
    if len(tiles) % 2:
        tiles.append(np.zeros_like(tiles[-1]))
    cv2.imwrite(out, np.vstack([np.hstack(tiles[i:i + 2]) for i in range(0, len(tiles), 2)]))
    print("sheet:", out)


def labels(data: str, split: str, out: str, n: int = 6, seed: int = 1):
    """n random windows of a split with their label boxes."""
    coco = read_coco(data, split)
    by_img = {}
    for a in coco["annotations"]:
        by_img.setdefault(a["image_id"], []).append(a)
    tiles = []
    for im in random.Random(seed).sample(coco["images"], min(n, len(coco["images"]))):
        img = cv2.imread(os.path.join(data, split, im["file_name"]))
        for a in by_img.get(im["id"], []):
            x, y, w, h = a["bbox"]
            cv2.rectangle(img, (int(x), int(y)), (int(x + w), int(y + h)), COLORS[(a["category_id"] - 1) % len(COLORS)], 1)
        cv2.putText(img, im["file_name"], (4, 14), cv2.FONT_HERSHEY_SIMPLEX, 0.4, (255, 255, 255), 1)
        tiles.append(img)
    _grid(tiles, out)


def errors(package: str, data: str, split: str, out: str, n: int = 6, iou_thr: float = 0.5):
    """The n windows with the most errors: false hits red (with class and score), misses white, hits thin."""
    p = Package(package)
    coco = read_coco(data, split)
    gts = ground_truth(coco)
    bad = []
    for im in coco["images"]:
        img = cv2.imread(os.path.join(data, split, im["file_name"]))
        gt = gts.get(im["id"], [])
        hits, matched = match(p.detect(img), gt, iou_thr)
        errs = 0
        for c, s, b, ok in hits:
            if ok:
                cv2.rectangle(img, (int(b[0]), int(b[1])), (int(b[2]), int(b[3])), COLORS[c % len(COLORS)], 1)
            else:
                errs += 1
                cv2.rectangle(img, (int(b[0]), int(b[1])), (int(b[2]), int(b[3])), (0, 0, 255), 2)
                cv2.putText(img, f"{p.classes[c]} {s:.2f}", (int(b[0]), int(b[1]) - 2), cv2.FONT_HERSHEY_SIMPLEX, 0.35,
                            (0, 0, 255), 1)
        for j, g in enumerate(gt):
            if j not in matched:
                errs += 1
                cv2.rectangle(img, (int(g[1]), int(g[2])), (int(g[3]), int(g[4])), (255, 255, 255), 2)
                cv2.putText(img, f"miss {p.classes[g[0]]}", (int(g[1]), int(g[4]) + 10), cv2.FONT_HERSHEY_SIMPLEX, 0.35,
                            (255, 255, 255), 1)
        cv2.putText(img, im["file_name"], (4, 14), cv2.FONT_HERSHEY_SIMPLEX, 0.4, (255, 255, 255), 1)
        bad.append((errs, img))
    bad.sort(key=lambda b: -b[0])
    print("errors per window (worst first):", [b[0] for b in bad[:n]], "total", sum(b[0] for b in bad))
    _grid([b[1] for b in bad[:n]], out)


def shots(package: str, images: list[str], out: str, cfg: dict):
    """The model over screenshots without labels: each frame scaled to `cfg.height`, windows of the model's size
    (overlapping by `cfg.overlap` px) over `cfg.area` (x0, y0, x1, y1 as fractions of the frame), hits drawn."""
    p = Package(package)
    hgt, (ax0, ay0, ax1, ay1), ov = cfg["height"], cfg["area"], cfg.get("overlap", 40)
    sheets = []
    for path in images:
        f = imread(path)
        k = hgt / f.shape[0]
        f = cv2.resize(f, (int(f.shape[1] * k), hgt), interpolation=cv2.INTER_AREA)
        x_lo, x_hi, y_lo, y_hi = int(ax0 * f.shape[1]), int(ax1 * f.shape[1]), int(ay0 * hgt), int(ay1 * hgt)
        xs = range(x_lo, x_hi - p.W + 1, p.W - ov) or [x_lo]
        ys = range(y_lo, y_hi - p.H + 1, p.H - ov) or [y_lo]
        for y0 in ys:
            for x0 in xs:
                for c, s, bx0, by0, bx1, by1 in p.detect(f[y0:y0 + p.H, x0:x0 + p.W]):
                    cv2.rectangle(f, (int(x0 + bx0), int(y0 + by0)), (int(x0 + bx1), int(y0 + by1)), COLORS[c % len(COLORS)], 1)
        pane = f[y_lo:y_hi, x_lo:x_hi]
        cv2.putText(pane, os.path.basename(path), (4, 14), cv2.FONT_HERSHEY_SIMPLEX, 0.45, (255, 255, 255), 1)
        sheets.append(pane)
    w = max(s.shape[1] for s in sheets)
    cv2.imwrite(out, np.vstack([cv2.copyMakeBorder(s, 0, 4, 0, w - s.shape[1], cv2.BORDER_CONSTANT) for s in sheets]))
    print("sheet:", out)
