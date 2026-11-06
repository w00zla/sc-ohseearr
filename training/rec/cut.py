"""Cuts a PaddleOCR recognizer's output layer to the recipe's charset.

PP-OCRv6 tiny rec reads 6906 classes (blank, a dictionary of 6904 characters, mostly CJK, and the space). The
cut keeps the blank, the characters the recipe's `charset` names and the space: the last MatMul's weight and
its bias lose the other columns, the softmax after them spreads over the kept classes, their order (argmax)
stays. The result is the fine-tune's starting point (the same columns of the PyTorch head, `keep`), its teacher
and its baseline.

Recipe keys: `base.onnx`, `base.yml` (repo, revision, file, sha256 of the original ONNX and its inference.yml
holding the dictionary), `charset.ranges` (inclusive code point ranges), `charset.chars` (single characters).
"""
import os

import numpy as np
import onnx
import onnxruntime as ort
from onnx import numpy_helper

from ..common import hub


def dict_of_yml(path: str) -> list[str]:
    """The `character_dict` list of a PaddleX recognizer's inference.yml (plain or single-quoted scalars; parsed
    by hand: a YAML parser would turn entries like `~` or `1` into other types)."""
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()
    start = next(i for i, line in enumerate(lines) if line.strip() == "character_dict:") + 1
    chars = []
    for line in lines[start:]:
        if not line.startswith("  - "):
            break
        v = line[4:]
        chars.append(v[1:-1].replace("''", "'") if v.startswith("'") else v)
    if any(len(c) != 1 for c in chars):
        raise ValueError(f"{path}: an entry of character_dict is no single character")
    return chars


def base_files(recipe) -> tuple[str, list[str]]:
    """The original ONNX and its dictionary (downloaded, checked)."""
    b = recipe.get("base")
    onnx_path = hub.fetch(b["onnx"]["repo"], b["onnx"]["revision"], b["onnx"]["file"], b["onnx"].get("sha256"))
    yml = hub.fetch(b["yml"]["repo"], b["yml"]["revision"], b["yml"]["file"], b["yml"].get("sha256"))
    return onnx_path, dict_of_yml(yml)


def kept(recipe, ch: str) -> bool:
    o = ord(ch)
    return any(lo <= o <= hi for lo, hi in recipe.get("charset.ranges", [])) or ch in recipe.get("charset.chars", "")


def plan(recipe) -> tuple[list[str], list[int]]:
    """The kept characters (the new dictionary) and the original class indices to keep (blank, characters,
    space)."""
    _, full = base_files(recipe)
    chars = [c for c in full if kept(recipe, c)]
    keep = [0] + [i + 1 for i, c in enumerate(full) if kept(recipe, c)] + [len(full) + 1]
    return chars, keep


def run(recipe, out: str) -> tuple[str, str]:
    """Writes `<out>/cut/dict.txt` and `<out>/cut/model.onnx`; returns their paths."""
    src, full = base_files(recipe)
    chars, keep = plan(recipe)
    n = len(full) + 2
    d = f"{out}/cut"
    os.makedirs(d, exist_ok=True)
    m = onnx.load(src)
    sliced = 0
    for init in m.graph.initializer:
        a = numpy_helper.to_array(init)
        if a.ndim >= 1 and a.shape[-1] == n:
            init.CopyFrom(numpy_helper.from_array(a[..., keep].copy(), init.name))
            sliced += 1
    if sliced != 2:
        raise RuntimeError(f"expected the head's weight and bias with {n} classes, found {sliced} tensors")
    # The class count in the stored shapes of intermediate values too (else ONNX Runtime warns)
    for v in list(m.graph.value_info) + list(m.graph.output):
        dims = v.type.tensor_type.shape.dim
        if dims and dims[-1].dim_value == n:
            dims[-1].dim_value = len(keep)
    model_path = f"{d}/model.onnx"
    onnx.save(m, model_path)
    dict_path = f"{d}/dict.txt"
    with open(dict_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("".join(c + "\n" for c in chars))

    # Check: the cut's probabilities = the original's over the kept classes, renormalized
    x = np.random.default_rng(0).random((2, 3, 48, 320), np.float32) * 2 - 1
    run_ = lambda p: ort.InferenceSession(p, providers=["CPUExecutionProvider"]).run(None, {"x": x})[0]
    full_p, cut_p = run_(src), run_(model_path)
    ref = full_p[..., keep] / full_p[..., keep].sum(-1, keepdims=True)
    diff = float(np.abs(cut_p - ref).max())
    assert diff < 1e-5, diff
    print(f"cut: {len(keep)} classes ({len(chars)} characters), max deviation {diff:.2e}", flush=True)
    return dict_path, model_path
