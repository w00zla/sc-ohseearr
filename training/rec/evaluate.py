"""Measures a recognizer on line datasets (labels.tsv): lines exactly right, right ignoring spaces, character
error rate; per group (a column of the TSV) and for subsets (lines containing a string).

Recipe keys: `eval.sets` (name -> {path, group_by, subsets: {name: substring}}), `eval.normalize` (characters
treated as equal, e.g. μ and µ), `eval.headline` (the set the MODEL.md block leads with).
"""
import random

import numpy as np
import onnxruntime as ort
import torch

from ..common.datasets import read_lines
from ..common.images import imread
from . import model as M


def rows_of(folder: str, charset: M.Charset, normalize: dict, limit: int | None = None, seed: int = 0) -> list[dict]:
    """The rows whose label the charset can write (after `normalize`), optionally a random sample."""
    rows = []
    for r in read_lines(folder):
        r["text"] = fix(r["text"], normalize)
        if r["text"] and charset.encode(r["text"]) is not None:
            rows.append(r)
    if limit and limit < len(rows):
        random.Random(seed).shuffle(rows)
        rows = rows[:limit]
    return rows


def fix(s: str, normalize: dict) -> str:
    for a, b in normalize.items():
        s = s.replace(a, b)
    return s


def onnx_reader(path: str, charset: M.Charset, inp: M.Input, threads: int = 0):
    o = ort.SessionOptions()
    if threads:
        o.intra_op_num_threads = threads
    s = ort.InferenceSession(path, o, providers=["CPUExecutionProvider"])
    name = s.get_inputs()[0].name
    return lambda xs: [charset.decode(p) for p in s.run(None, {name: inp.batch(xs).numpy()})[0]]


def torch_reader(model, charset: M.Charset, inp: M.Input, dev: str):
    def read(xs):
        with torch.no_grad():
            p = torch.softmax(M.logits(model, inp.batch(xs).to(dev)), 2).cpu().numpy()
        return [charset.decode(q) for q in p]
    return read


def run(read, rows: list[dict], inp: M.Input, bs: int = 32) -> list[tuple[str, float]]:
    """Reads the rows in batches of similar width (like the engine); keeps the order of rows."""
    arrays = [inp.prep(imread(r["file"])) for r in rows]
    order = sorted(range(len(rows)), key=lambda i: arrays[i].shape[2])
    out = [("", 0.0)] * len(rows)
    for k in range(0, len(order), bs):
        idx = order[k:k + bs]
        for i, t in zip(idx, read([arrays[i] for i in idx])):
            out[i] = t
    return out


def lev(a: str, b: str) -> int:
    prev = list(range(len(b) + 1))
    for i in range(1, len(a) + 1):
        cur = [i]
        for j in range(1, len(b) + 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] != b[j - 1])))
        prev = cur
    return prev[-1]


def metrics(pairs: list[tuple[str, str]]) -> dict:
    """pairs: (label, read). Lines exactly right, right ignoring spaces, character error rate (spaces ignored)."""
    n = len(pairs)
    exact = sum(lab == r for lab, r in pairs)
    nospace = sum(lab.replace(" ", "") == r.replace(" ", "") for lab, r in pairs)
    errs = sum(lev(lab.replace(" ", ""), r.replace(" ", "")) for lab, r in pairs)
    chars = sum(len(lab.replace(" ", "")) for lab, _ in pairs)
    return {"n": n, "exact": exact / max(1, n), "nospace": nospace / max(1, n), "cer": errs / max(1, chars)}


def report(rows: list[dict], reads: list[str], spec: dict, normalize: dict) -> dict:
    pairs = [(r["text"], fix(t, normalize)) for r, t in zip(rows, reads)]
    res = {"all": metrics(pairs)}
    g = spec.get("group_by")
    if g:
        for v in sorted({r.get(g, "") for r in rows}):
            res[v] = metrics([p for p, r in zip(pairs, rows) if r.get(g, "") == v])
    for name, sub in (spec.get("subsets") or {}).items():
        sel = [p for p in pairs if sub in p[0]]
        if sel:
            res[name] = metrics(sel)
    return res


def show(res: dict) -> str:
    return "\n".join(f"  {k:8s} n {v['n']:6d}  exact {v['exact']:.3f}  nospace {v['nospace']:.3f}  cer {v['cer']:.4f}"
                     for k, v in res.items())


def evaluate(recipe, read, charset: M.Charset, inp: M.Input, sets: list[str] | None = None) -> dict:
    """{set: {group: metrics}} over the recipe's eval sets."""
    normalize = recipe.get("eval.normalize", {})
    out = {}
    for name, spec in recipe.get("eval.sets").items():
        if sets and name not in sets:
            continue
        rows = rows_of(recipe.path(spec["path"]), charset, normalize)
        reads = [t for t, _ in run(read, rows, inp)]
        out[name] = report(rows, reads, spec, normalize)
        print(f"{name}:\n{show(out[name])}", flush=True)
    return out


def markdown(recipe, res: dict, baseline: dict | None) -> str:
    """The MODEL.md block: per set and group, exact lines and CER (baseline -> this model when given)."""
    def cell(name, g, key, pct):
        v = res[name][g][key]
        f = (lambda x: f"{100 * x:.1f} %") if pct else (lambda x: f"{100 * x:.2f} %")
        if baseline and name in baseline and g in baseline[name]:
            return f"{f(baseline[name][g][key])} → {f(v)}"
        return f(v)
    lines = ["| Set | Group | Lines | Exactly right | Character error rate |", "|---|---|---|---|---|"]
    order = [recipe.get("eval.headline", None)] + [n for n in res if n != recipe.get("eval.headline", None)]
    for name in [n for n in order if n in res]:
        for g in res[name]:
            lines.append(f"| {name} | {g} | {res[name][g]['n']} | {cell(name, g, 'exact', True)} | "
                         f"{cell(name, g, 'cer', False)} |")
    if baseline:
        lines.append("")
        lines.append("Baseline (left of →): the original model with its head cut to the charset, before the fine-tune.")
    return "\n".join(lines)
