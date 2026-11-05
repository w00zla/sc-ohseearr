# sc-rec-latin

Text line recognizer for Star Citizen's UI; family `ctc-rec` (`CtcRecognizer`). PaddleOCR's PP-OCRv6 tiny
recognizer with its output layer cut to Latin and then fine-tuned on the game's font.

- License: Apache-2.0 (derived from PaddlePaddle's weights).
- Charset (`dict.txt`, 318 characters plus the space): ASCII, Latin-1, Latin Extended-A (Polish and Czech
  language packs), μ, ▲▼. No Cyrillic, no CJK. The cut removed the CJK look-alikes the original read in game
  texts ("日" for a thin "6") and shrank the GPU sessions (outputs over 320 instead of 6906 classes).
- Fine-tune (2026-10-31): the game draws its UI in Blender Pro, whose "&" looks like an "Ɛ"; the original read
  "Klaus & Werner" right in 0.9 % of such lines. Trained on 100k synthetic lines (Blender Pro and other game
  fonts at the measured cap heights, backgrounds from real tooltips and terminals, item names and descriptions
  of a language pack, stat lines) plus 2525 real warehouse and refinery lines labelled by the uncut model
  (confidence ≥ 0.95). The font itself is commercial and was used for local training only; it is not in this
  repository.
- Evaluation on 21,881 real character inventory tooltip lines (labels from independent sources): lines exactly
  right 90.4 → 99.0 %, character error rate 0.26 → 0.04 %, "&" lines 0.9 → 98.6 %.
- Input: one line at height 48, nominal width 320 (wider lines widen the input), BGR, `x/127.5 - 1`; batches of 6
  lines of similar shape; on the GPU padded to 6 lines of a width in 320–2560 (few DirectML sessions).
