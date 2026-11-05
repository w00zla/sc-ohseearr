# paddle-det-tiny

PaddleOCR's PP-OCRv6 tiny text detector (PaddlePaddle's own ONNX export), unmodified. Finds text lines as
axis-aligned boxes; family `db-det` (`DbDetector`).

- License: Apache-2.0 (PaddlePaddle). Source and checksum: `model.toml`.
- Input: the image scaled so its height is at most the caller's `max_height`, both sides rounded to multiples of
  32; BGR, ImageNet mean/std applied in that order (PaddleX's own pre-processing).
- Small texts (a few pixels high) are found only scaled up: crop the area and scale it before detecting.
- On the GPU, every input shape holds its own DirectML session (~13 MB VRAM for a small one, ~150–315 MB for a
  whole 1080p–3840×1600 frame); `DbDetector::cpu_max_height` and `detect_on_cpu` keep small and rare inputs on
  the CPU.
