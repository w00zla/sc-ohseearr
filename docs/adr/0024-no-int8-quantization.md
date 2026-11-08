# 0024. No int8 quantization of the detector and the recognizer

- Status: accepted
- Date: 2026-11-08
- Scope: `models/paddle-det-tiny`, `models/sc-rec-latin`, the CPU path of `crates/core`

## Context
int8 quantization was noted as an option to speed up the OCR on the CPU (1.5–2× expected on CPUs with VNNI) at a risk
to accuracy. It was measured on a CPU with AVX-512 VNNI (Ryzen 7 9800X3D), ONNX Runtime 1.31.1 on the CPU with 3
intra-op threads as the apps run it, calibration from training data only (teacher and synthetic lines, frames of the
training sources), evaluation on the recognizer's eval sets and on frames of the regression sets:

| Model | Variant | Time vs fp32 | Accuracy |
|---|---|---|---|
| detector | static QDQ, Conv only | 0.63–0.65× | not measured |
| detector | static QOperator, all ops | 0.78× | not measured |
| detector | static QDQ, all ops | 0.75× | not measured |
| recognizer | static QDQ (Conv, MatMul, Gemm) | 0.90× | gear 99.0 → 97.3 %, titles 98.3 → 95.3 %, refinery 99.8 → 97.5 %, teacher 91.2 → 85.5 % |
| recognizer | dynamic (MatMul, Gemm) | 0.93× | as fp32 |

The models are small: the conversions between int8 and float at the quantized ops' borders cost more than the int8
arithmetic saves, on the best case of a CPU for it.

## Decision
- The detector and the recognizer stay fp32; no int8 packages, no int8 path in `core`.
- Not to be re-proposed unless its forces change: a much larger model, an ONNX Runtime whose int8 kernels make
  these models faster on the CPU, or an execution provider built for int8.

## Alternatives
- Static int8 with a better calibration (percentile, more data): slower already before any accuracy question.
- Dynamic int8 of the recognizer: as accurate, but slower.

## Consequences
- The CPU path's speed comes from the existing levers: the reads' crop sizes and heights, the batch widths, the
  threads.
