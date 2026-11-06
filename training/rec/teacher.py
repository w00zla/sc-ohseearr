"""Teacher labels: reads line crops with an existing recognizer and keeps the lines it reads with a mean character
probability of at least `min_score` (and at least `min_len` characters) as a labelled dataset. Used against
forgetting: real lines of other screens, labelled by the model before the fine-tune, trained alongside the
synthetic ones.

Input: a folder with `labels.tsv` (the text column may be empty) or a folder of images (every *.png/*.jpg).
Output: `<out>/labels.tsv` (file, text, score) pointing at the input images (paths relative to `<out>`).
"""
import glob
import os

from ..common.datasets import read_lines, write_lines
from . import evaluate as ev
from . import model as M


def label(model_path: str, dict_path: str, inp: M.Input, src: str, out: str, min_score: float = 0.95,
          min_len: int = 2) -> int:
    charset = M.Charset.load(dict_path)
    if os.path.exists(os.path.join(src, "labels.tsv")):
        rows = read_lines(src)
    else:
        rows = [{"file": p.replace("\\", "/")} for ext in ("png", "jpg") for p in sorted(glob.glob(f"{src}/*.{ext}"))]
    reads = ev.run(ev.onnx_reader(model_path, charset, inp), rows, inp)
    keep = []
    for r, (text, score) in zip(rows, reads):
        text = text.strip()
        if score >= min_score and len(text) >= min_len:
            keep.append({"file": os.path.relpath(r["file"], out).replace("\\", "/"), "text": text,
                         "score": f"{score:.4f}"})
    os.makedirs(out, exist_ok=True)
    write_lines(out, keep, ["score"])
    print(f"teacher: {len(keep)} of {len(rows)} lines with score >= {min_score} -> {out}", flush=True)
    return len(keep)
