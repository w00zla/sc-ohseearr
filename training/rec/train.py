"""Fine-tunes the recognizer (PyTorch, CTC) on line datasets.

Recipe keys under `train`: `data` (a list of {path, repeat}; `@synth` = the run's synthetic set), `epochs`,
`lr`, `weight_decay`, `batch`, `workers`, `train_bn` (false: BatchNorm statistics and weights stay as
pre-trained), `clip`, `warmup` (epochs of linear warm-up), `select` (`last`, or `best`: the lowest sum of the
monitored sets' CER), `monitor` (a list of {set, limit, seed}: eval sets measured after every epoch; `@synth-val`
= the run's synthetic validation set). Writes `<run>/train/{last,best}.pt` and `log.json`.
"""
import json
import math
import os
import time

import numpy as np
import torch
from torch.utils.data import DataLoader, Dataset

from ..common.datasets import read_lines
from ..common.images import imread
from . import evaluate as ev
from . import model as M


class Lines(Dataset):
    def __init__(self, items: list[tuple[str, list[int]]], inp: M.Input):
        self.items, self.inp = items, inp

    def __len__(self):
        return len(self.items)

    def __getitem__(self, i):
        path, ids = self.items[i]
        return self.inp.prep(imread(path)), ids


def items_of(folders: list[tuple[str, int]], charset: M.Charset, normalize: dict) -> list[tuple[str, list[int]]]:
    out = []
    for folder, repeat in folders:
        n0 = len(out)
        for r in read_lines(folder):
            ids = charset.encode(ev.fix(r["text"], normalize))
            if ids:
                out.extend([(r["file"], ids)] * repeat)
        print(f"train data {folder}: {len(out) - n0} lines (×{repeat})", flush=True)
    return out


class Collate:
    """A batch: images padded like the engine, the targets concatenated, their lengths (picklable for workers)."""

    def __init__(self, inp: M.Input):
        self.inp = inp

    def __call__(self, b):
        x = self.inp.batch([a for a, _ in b])
        targets = torch.tensor([i for _, ids in b for i in ids], dtype=torch.long)
        lengths = torch.tensor([len(ids) for _, ids in b], dtype=torch.long)
        return x, targets, lengths


def freeze_bn(model):
    for m in model.modules():
        if isinstance(m, (torch.nn.BatchNorm1d, torch.nn.BatchNorm2d)):
            m.eval()
            for p in m.parameters():
                p.requires_grad_(False)


def train(recipe, model: torch.nn.Module, charset: M.Charset, inp: M.Input, data: list[tuple[str, int]],
          monitor: list[tuple[str, list[dict], dict]], out: str) -> str:
    """Trains `model` in place; returns the path of the selected checkpoint."""
    t = recipe.get("train")
    normalize = recipe.get("eval.normalize", {})
    os.makedirs(out, exist_ok=True)
    dev = "cuda" if torch.cuda.is_available() else "cpu"
    model.to(dev)

    loader = DataLoader(Lines(items_of(data, charset, normalize), inp), batch_size=t["batch"], shuffle=True,
                        num_workers=t["workers"], collate_fn=Collate(inp), drop_last=True,
                        persistent_workers=t["workers"] > 0)
    opt = torch.optim.AdamW(list(model.parameters()), lr=t["lr"], weight_decay=t["weight_decay"])
    steps = t["epochs"] * len(loader)
    warm = max(1, int(len(loader) * t.get("warmup", 0.5)))
    sched = torch.optim.lr_scheduler.LambdaLR(
        opt, lambda s: min(1.0, (s + 1) / warm) * 0.5 * (1 + math.cos(math.pi * min(1.0, s / steps))))
    ctc = torch.nn.CTCLoss(blank=0, zero_infinity=True)

    def evaluate():
        model.eval()
        read = ev.torch_reader(model, charset, inp, dev)
        res = {}
        for name, rows, spec in monitor:
            res[name] = ev.report(rows, [x for x, _ in ev.run(read, rows, inp)], spec, normalize)
        return res

    def show(res):
        return "\n".join(f"  {n}: " + "  ".join(f"{k} n {v['n']} exact {v['exact']:.3f} cer {v['cer']:.4f}"
                                                 for k, v in r.items()) for n, r in res.items())
    score = lambda res: sum(r["all"]["cer"] for r in res.values())
    res = evaluate()
    log = [{"epoch": 0, "eval": res}]
    print(f"epoch 0\n{show(res)}", flush=True)
    best = score(res)
    best_path, last_path = f"{out}/best.pt", f"{out}/last.pt"
    for epoch in range(1, t["epochs"] + 1):
        model.train()
        if not t.get("train_bn", False):
            freeze_bn(model)
        t0, total, n = time.time(), 0.0, 0
        for x, targets, lengths in loader:
            x = x.to(dev, non_blocking=True)
            lp = M.logits(model, x).log_softmax(2).permute(1, 0, 2)  # T, N, C
            in_len = torch.full((x.shape[0],), lp.shape[0], dtype=torch.long)
            loss = ctc(lp, targets.to(dev), in_len.to(dev), lengths.to(dev))
            opt.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), t.get("clip", 5.0))
            opt.step()
            sched.step()
            total += loss.item()
            n += 1
        secs = time.time() - t0
        res = evaluate()
        log.append({"epoch": epoch, "loss": total / n, "seconds": secs, "eval": res})
        print(f"epoch {epoch}  loss {total / n:.4f}  {secs:.0f} s  lr {sched.get_last_lr()[0]:.2e}\n{show(res)}",
              flush=True)
        torch.save(model.state_dict(), last_path)
        if score(res) < best:
            best = score(res)
            torch.save(model.state_dict(), best_path)
            print(f"  best so far (monitored CER sum {best:.4f})", flush=True)
        with open(f"{out}/log.json", "w", encoding="utf-8") as f:
            json.dump(log, f, indent=1)
    if t.get("select", "last") == "best" and os.path.exists(best_path):
        return best_path
    return last_path
