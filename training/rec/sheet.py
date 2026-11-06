"""A contact sheet of synthetic lines next to real ones, for judging the renderer by eye."""
import math
import random

import cv2
from PIL import Image, ImageDraw, ImageFont

from ..common.datasets import read_lines
from ..common.images import imread


def sheet(path: str, synth_dir: str, real_dir: str | None, k: int = 30, seed: int = 0):
    """A contact sheet: k synthetic lines (left) next to k real ones (right), each at height 48, with labels."""
    rng = random.Random(seed)
    syn = rng.sample(read_lines(synth_dir), k)
    real = rng.sample(read_lines(real_dir), k) if real_dir else []
    colw = 760
    font = ImageFont.load_default()

    def cell(p, text):
        img = imread(p)
        h, w = img.shape[:2]
        img = cv2.resize(img, (max(1, math.ceil(48 * w / h)), 48), interpolation=cv2.INTER_LINEAR)[:, :colw]
        c = Image.new("RGB", (colw, 68), (255, 255, 255))
        c.paste(Image.fromarray(img[..., ::-1]), (0, 0))
        ImageDraw.Draw(c).text((2, 52), f"{text}   [{h}px]", fill=(0, 110, 0), font=font)
        return c
    out = Image.new("RGB", (2 * colw + 20, 68 * k), (255, 255, 255))
    for i, r in enumerate(syn):
        out.paste(cell(r["file"], r["text"]), (0, 68 * i))
    for i, r in enumerate(real):
        out.paste(cell(r["file"], r["text"]), (colw + 20, 68 * i))
    out.save(path)
    print("sheet:", path)
