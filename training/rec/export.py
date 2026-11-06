"""Exports a fine-tuned recognizer to ONNX like the original: input `x` [N, 3, H, W] with named dynamic dimensions
(`batch`, `width`: the engine fixes them by name for DirectML), output probabilities [N, T, classes]; checks the
ONNX against PyTorch at three widths."""
import os

import numpy as np
import onnxruntime as ort
import torch

from . import model as M


def export(model: torch.nn.Module, out: str, height: int, opset: int = 17) -> float:
    model = M.Exported(model.cpu().eval()).eval()
    x = torch.from_numpy(np.random.default_rng(0).random((2, 3, height, 640), np.float32) * 2 - 1)
    torch.onnx.export(model, (x,), out, input_names=["x"], output_names=["softmax"], opset_version=opset, dynamo=False,
                      dynamic_axes={"x": {0: "batch", 3: "width"}, "softmax": {0: "batch", 1: "steps"}})
    s = ort.InferenceSession(out, providers=["CPUExecutionProvider"])
    worst = 0.0
    for w in (320, 640, 1280):
        xi = torch.from_numpy(np.random.default_rng(w).random((3, 3, height, w), np.float32) * 2 - 1)
        with torch.no_grad():
            ref = model(xi).numpy()
        got = s.run(None, {"x": xi.numpy()})[0]
        worst = max(worst, float(np.abs(got - ref).max()))
    print(f"export: {out} ({os.path.getsize(out)} bytes), max |ONNX - PyTorch| {worst:.2e}", flush=True)
    return worst
