"""Measures a layout detector (a `nanodet` model package) on a COCO split: hits per class against the labels
(IoU ≥ `eval.iou`, default 0.5; recall and precision) and the time per run on the CPU (`eval.threads`).
The model's numbers (normalization, strides, bins, classes, score, NMS) come from its model.toml."""
import os
import time
import tomllib

import cv2
import numpy as np
import onnxruntime as ort

from ..common.datasets import read_coco


class Package:
    """A `nanodet` package folder: its session and decoding."""

    def __init__(self, folder: str, threads: int = 3, onnx: str | None = None):
        with open(os.path.join(folder, "model.toml"), "rb") as f:
            m = tomllib.load(f)
        if m["family"] != "nanodet":
            raise ValueError(f"{folder}: family {m['family']}, not nanodet")
        self.manifest = m
        i, n = m["input"], m["nanodet"]
        order = i.get("order", "bgr")
        self.mean = np.array(i["mean"], np.float32) * 255
        self.std = np.array(i["std"], np.float32) * 255
        self.rgb = order == "rgb"
        self.strides, self.reg_max, self.classes = n["strides"], n["reg_max"], n["classes"]
        self.score, self.nms = n["score"], n["nms"]
        o = ort.SessionOptions()
        o.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        o.intra_op_num_threads = threads
        self.sess = ort.InferenceSession(onnx or os.path.join(folder, "model.onnx"), o, providers=["CPUExecutionProvider"])
        self.inp = self.sess.get_inputs()[0]
        _, _, self.H, self.W = self.inp.shape
        self.priors = self._priors(self.W, self.H)

    def _priors(self, w, h):
        out = []
        for s in self.strides:
            fw, fh = -(-w // s), -(-h // s)
            ys, xs = np.mgrid[0:fh, 0:fw]
            out.append(np.stack([xs.ravel() * s, ys.ravel() * s, np.full(fw * fh, s)], 1))
        return np.concatenate(out).astype(np.float32)

    def tensor(self, img_bgr: np.ndarray) -> np.ndarray:
        x = img_bgr[..., ::-1] if self.rgb else img_bgr
        return ((x.astype(np.float32) - self.mean) / self.std).transpose(2, 0, 1)[None]

    def detect(self, img_bgr: np.ndarray) -> list[tuple]:
        """A window of the model's size (smaller ones padded with black) -> [(class, score, x0, y0, x1, y1)] after
        NMS per class."""
        if img_bgr.shape[:2] != (self.H, self.W):
            canvas = np.zeros((self.H, self.W, 3), np.uint8)
            canvas[:min(img_bgr.shape[0], self.H), :min(img_bgr.shape[1], self.W)] = img_bgr[:self.H, :self.W]
            img_bgr = canvas
        out =self.sess.run(None, {self.inp.name: self.tensor(img_bgr)})[0][0]
        return self.decode(out)

    def decode(self, out: np.ndarray) -> list[tuple]:
        nc = out.shape[1] - 4 * (self.reg_max + 1)
        scores, reg = out[:, :nc], out[:, nc:].reshape(-1, 4, self.reg_max + 1)
        cls, sc = scores.argmax(1), scores.max(1)
        keep = sc >= self.score
        if not keep.any():
            return []
        reg = reg[keep]
        e = np.exp(reg - reg.max(2, keepdims=True))
        dist = (e / e.sum(2, keepdims=True) * np.arange(self.reg_max + 1)).sum(2) * self.priors[keep, 2:3]
        cx, cy = self.priors[keep, 0], self.priors[keep, 1]
        boxes = np.stack([cx - dist[:, 0], cy - dist[:, 1], cx + dist[:, 2], cy + dist[:, 3]], 1)
        res = []
        for c in range(nc):
            m = cls[keep] == c
            if not m.any():
                continue
            b, s = boxes[m], sc[keep][m]
            idx = cv2.dnn.NMSBoxes([[float(x0), float(y0), float(x1 - x0), float(y1 - y0)] for x0, y0, x1, y1 in b],
                                   s.tolist(), self.score, self.nms)
            for i in np.array(idx).ravel():
                res.append((c, float(s[i]), *map(float, b[i])))
        return res


def iou(a, b):
    ix = max(0, min(a[2], b[2]) - max(a[0], b[0]))
    iy = max(0, min(a[3], b[3]) - max(a[1], b[1]))
    i = ix * iy
    return i / ((a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - i + 1e-9)


def ground_truth(coco: dict) -> dict:
    gts = {}
    for an in coco["annotations"]:
        x, y, w, h = an["bbox"]
        gts.setdefault(an["image_id"], []).append((an["category_id"] - 1, x, y, x + w, y + h))
    return gts


def match(dets, gt, thr):
    """(tp/fp per detection, matched gt indices)."""
    matched, hits = set(), []
    for c, s, *b in sorted(dets, key=lambda d: -d[1]):
        best = max(((iou(b, g[1:]), j) for j, g in enumerate(gt) if g[0] == c and j not in matched), default=(0, None))
        if best[0] >= thr:
            matched.add(best[1])
            hits.append((c, s, b, True))
        else:
            hits.append((c, s, b, False))
    return hits, matched


def evaluate(pkg: Package, data: str, split: str = "val", iou_thr: float = 0.5) -> dict:
    coco = read_coco(data, split)
    names = [c["name"] for c in sorted(coco["categories"], key=lambda c: c["id"])]
    if names != pkg.classes:
        raise ValueError(f"dataset classes {names} differ from the package's {pkg.classes}")
    gts = ground_truth(coco)
    tp, fp, fn = np.zeros(len(names), int), np.zeros(len(names), int), np.zeros(len(names), int)
    times = []
    for im in coco["images"]:
        img = cv2.imread(os.path.join(data, split, im["file_name"]))
        canvas = np.zeros((pkg.H, pkg.W, 3), np.uint8)
        canvas[:min(img.shape[0], pkg.H), :min(img.shape[1], pkg.W)] = img[:pkg.H, :pkg.W]
        t0 = time.perf_counter()
        dets = pkg.detect(canvas)
        times.append(time.perf_counter() - t0)
        gt = gts.get(im["id"], [])
        hits, matched = match(dets, gt, iou_thr)
        for c, _, _, ok in hits:
            (tp if ok else fp)[c] += 1
        for j, g in enumerate(gt):
            if j not in matched:
                fn[g[0]] += 1
    t = np.array(times[3:] or times) * 1000
    res = {"images": len(times), "ms_median": round(float(np.median(t)), 2), "ms_p90": round(float(np.percentile(t, 90)), 2),
           "classes": {}}
    for c, n in enumerate(names):
        res["classes"][n] = {"recall": tp[c] / max(tp[c] + fn[c], 1), "precision": tp[c] / max(tp[c] + fp[c], 1),
                             "tp": int(tp[c]), "fp": int(fp[c]), "fn": int(fn[c])}
    return res


def show(res: dict, threads: int) -> str:
    lines = [f"{res['images']} images, {res['ms_median']:.1f} ms median, {res['ms_p90']:.1f} ms p90 ({threads} threads)"]
    for n, v in res["classes"].items():
        lines.append(f"  {n:8s} recall {v['recall']:.3f}  precision {v['precision']:.3f}  (tp {v['tp']}, fp {v['fp']}, fn {v['fn']})")
    return "\n".join(lines)


def markdown(res: dict, iou_thr: float, threads: int) -> str:
    lines = [f"Validation ({res['images']} windows, IoU ≥ {iou_thr}):", "",
             "| Class | Recall | Precision |", "|---|---|---|"]
    for n, v in res["classes"].items():
        lines.append(f"| {n} | {100 * v['recall']:.1f} % | {100 * v['precision']:.1f} % |")
    lines += ["", f"CPU time per window: {res['ms_median']:.1f} ms median ({threads} threads)."]
    return "\n".join(lines)
