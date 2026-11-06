"""Image files as the pipelines read them."""
import cv2
import numpy as np


def imread(path: str):
    """A BGR image (None when unreadable); paths with non-ASCII characters work too (unlike `cv2.imread` on
    Windows)."""
    return cv2.imdecode(np.fromfile(path, np.uint8), cv2.IMREAD_COLOR)
