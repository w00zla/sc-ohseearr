"""The recognizer (family `ctc-rec`): its charset, the pre-processing the engine uses, the PyTorch model with its
head cut to the charset, CTC decoding.

Classes: 0 = CTC blank, 1..n = the dictionary (`dict.txt`), n+1 = the space (`space_char = true`).
Pre-processing like the engine: BGR, scaled to the input height keeping the aspect ratio (width rounded up),
bilinear, `x/127.5 - 1`, padded with 0 on the right to the widest line of the batch (at least the nominal width).
"""
import cv2
import numpy as np
import torch
import torch.nn.functional as F


class Charset:
    def __init__(self, chars: list[str]):
        self.chars = chars                       # the dictionary, without blank and space
        self.classes = [""] + chars + [" "]
        self.index = {c: i for i, c in enumerate(self.classes) if c}

    @classmethod
    def load(cls, path: str) -> "Charset":
        with open(path, encoding="utf-8") as f:
            return cls([line.rstrip("\r\n") for line in f])

    def encode(self, text: str) -> list[int] | None:
        """Class indices of a label; None when a character is not in the charset."""
        try:
            return [self.index[c] for c in text]
        except KeyError:
            return None

    def decode(self, probs: np.ndarray) -> tuple[str, float]:
        """Greedy CTC decoding of [T, C] probabilities: the text and the mean probability of its characters."""
        best = probs.argmax(-1)
        out, scores, last = [], [], 0
        for t, i in enumerate(best):
            if i != last and i != 0:
                out.append(self.classes[i])
                scores.append(float(probs[t, i]))
            last = i
        return "".join(out), (sum(scores) / len(scores) if scores else 0.0)


class Input:
    """The engine's input geometry (`[input]` height and nominal width in model.toml)."""

    def __init__(self, height: int, min_width: int, max_width: int):
        self.height, self.min_width, self.max_width = height, min_width, max_width

    def prep(self, img_bgr: np.ndarray) -> np.ndarray:
        h, w = img_bgr.shape[:2]
        nw = int(min(self.max_width, max(1, np.ceil(self.height * w / h))))
        x = cv2.resize(img_bgr, (nw, self.height), interpolation=cv2.INTER_LINEAR).astype(np.float32) / 127.5 - 1
        return x.transpose(2, 0, 1)

    def batch(self, arrays: list[np.ndarray]) -> torch.Tensor:
        w = max(self.min_width, max(a.shape[2] for a in arrays))
        out = np.zeros((len(arrays), 3, self.height, w), np.float32)
        for i, a in enumerate(arrays):
            out[i, :, :, : a.shape[2]] = a
        return torch.from_numpy(out)


def _patch_head():
    """PP-OCRv6's HF head ends in a softmax; training wants logits (the softmax comes back in `Exported`)."""
    from transformers.models.pp_ocrv6_tiny_rec import modeling_pp_ocrv6_tiny_rec as hf

    def logits_forward(self, hidden_states, **kwargs):
        hidden_states = hidden_states.squeeze(2)
        hidden_states = self.act_fn(self.norm1(self.conv1(hidden_states)))
        hidden_states = self.act_fn(self.norm2(self.conv2(hidden_states)))
        return self.fc2(self.fc1(hidden_states.transpose(1, 2)))

    hf.PPOCRV6TinyRecHead.forward = logits_forward


def load(repo: str, revision: str, keep: list[int], checkpoint: str | None = None) -> torch.nn.Module:
    """The Hugging Face model with its last layer cut to the columns `keep` (class indices of the original
    head); with `checkpoint` its fine-tuned weights (a state_dict)."""
    from transformers import PPOCRV6TinyRecForTextRecognition

    _patch_head()
    m = PPOCRV6TinyRecForTextRecognition.from_pretrained(repo, revision=revision)
    fc2 = m.head.fc2
    new = torch.nn.Linear(fc2.in_features, len(keep))
    with torch.no_grad():
        new.weight.copy_(fc2.weight[keep])
        new.bias.copy_(fc2.bias[keep])
    m.head.fc2 = new
    if checkpoint:
        m.load_state_dict(torch.load(checkpoint, map_location="cpu"))
    return m


def logits(model, x):
    return model(x).last_hidden_state


class Exported(torch.nn.Module):
    """For the ONNX export: probabilities like the original model."""

    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, x):
        return F.softmax(logits(self.model, x), dim=2)

