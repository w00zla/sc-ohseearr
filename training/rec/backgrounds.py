"""Builds a background bank (a folder of images without text) from screenshots, for banks whose recipe entry has
a `build` section:

  build: {method: screen-tiles, images: [glob..], skip: [substring..], size: [w, h], rect: [x0, y0, x1, y1],
          tile: [w, h], top_hat: 30, black_hat: 40, kernel: 25}

`screen-tiles`: the screen area `rect` of every screenshot of the given size; bright (text) and dark (text on bars)
structures found by morphological top-hat/black-hat are inpainted, the result is cut into tiles. Banks built
from other sources (tooltip areas around known text boxes) are written by `training/tools/tooltip_backgrounds.py`.
"""
import glob
import os

import cv2
import numpy as np

from ..common.images import imread


def screen_tiles(recipe, out_dir: str, b: dict) -> int:
    os.makedirs(out_dir, exist_ok=True)
    files = sorted(f for pat in b["images"] for f in recipe.glob(pat) if not any(s in f for s in b.get("skip", [])))
    x0, y0, x1, y1 = b["rect"]
    tw, th = b["tile"]
    k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (b.get("kernel", 25), b.get("kernel", 25)))
    n = 0
    for f in files:
        img = imread(f)
        if img is None or (b.get("size") and img.shape[1::-1] != tuple(b["size"])):
            continue
        scr = img[y0:y1, x0:x1].copy()
        g = cv2.cvtColor(scr, cv2.COLOR_BGR2GRAY)
        mask = (cv2.morphologyEx(g, cv2.MORPH_TOPHAT, k) > b.get("top_hat", 30)) | \
               (cv2.morphologyEx(g, cv2.MORPH_BLACKHAT, k) > b.get("black_hat", 40))
        mask = cv2.dilate(mask.astype(np.uint8) * 255, np.ones((5, 5), np.uint8))
        small = cv2.resize(scr, None, fx=0.5, fy=0.5, interpolation=cv2.INTER_AREA)
        msmall = cv2.resize(mask, None, fx=0.5, fy=0.5, interpolation=cv2.INTER_NEAREST)
        out = cv2.resize(cv2.inpaint(small, msmall, 3, cv2.INPAINT_TELEA), (scr.shape[1], scr.shape[0]),
                         interpolation=cv2.INTER_LINEAR)
        out = np.where(mask[..., None] > 0, out, scr)
        name = os.path.splitext(os.path.basename(f))[0]
        for ty in range(0, out.shape[0] - th + 1, th):
            for tx in range(0, out.shape[1] - tw + 1, tw):
                cv2.imencode(".jpg", out[ty:ty + th, tx:tx + tw], [cv2.IMWRITE_JPEG_QUALITY, 92])[1].tofile(
                    f"{out_dir}/{name}_{ty}_{tx}.jpg")
                n += 1
    return n


def build(recipe, only: list[str] | None = None, force: bool = False):
    """Builds the banks that have a `build` section and no images yet (or all of them with `force`)."""
    for name, b in recipe.get("synth.backgrounds").items():
        if "build" not in b or (only and name not in only):
            continue
        pattern = b["glob"] if isinstance(b["glob"], str) else b["glob"][0]
        out_dir = os.path.dirname(recipe.path(pattern))
        if recipe.glob(pattern) and not force:
            continue
        if force:
            for f in glob.glob(recipe.path(pattern)):
                os.remove(f)
        if b["build"]["method"] != "screen-tiles":
            raise ValueError(f"bank {name}: unknown method {b['build']['method']}")
        print(f"background bank {name}: {screen_tiles(recipe, out_dir, b['build'])} tiles -> {out_dir}", flush=True)
