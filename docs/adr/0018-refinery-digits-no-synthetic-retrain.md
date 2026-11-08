# 0018. Small refinery digits at 1080p: no synthetic retrain; apps gate on the score

- Status: accepted
- Date: 2026-11-08
- Scope: `training/recipes/sc-rec-latin.yml`, the refinery reader's consumers

## Context
In the first in-game refinery test (2026-11-08) the 1080p recording read "SILICON 782" as 762 in 3 of 33 frames,
twice in a row, always the same cell; such a read can book a wrong quality. Every misread scored 0.85–0.92, 607 of 614
right rows 0.97 or more. A retrain with 4,754 synthetic terminal digits at the 1080p cap height (kind `term_num`)
did not help: the last epoch read the cell wrong five times (the current model three, the uncut base model never,
but "852" as "B52"); the epochs in between had 0–5 errors. The eval set (1,232 real number crops with true values,
one session per resolution) holds only about 25 distinct numbers: one borderline glyph, not a weakness synthetic
lines target.

## Decision
- No synthetic retrain for the terminal digits; `term_num` is not in the recipe. The current `sc-rec-latin` stays.
- The protection is the app's: the reader passes each value's score (`Field::score`), and the companion's refinery
  sync does not take an order with a row below 0.95 as read.
- The refinery numbers are measured with every recognizer run: eval set and monitor `refinery`.
- The next lever, when wanted, is real data: several 1080p refinery recordings with many different numbers (6, 8, 9,
  0, 3), true labels, one session as training lines and another kept for the eval. Not scheduled.

## Alternatives
- Synthetic digits with other weights or cap heights: the renders do not match the game's glyph closely enough to
  move a single borderline case; tuning them against a set of ~25 numbers would fit the eval, not the game.
- Reading the cell again (upscaled, digits only): measured worse (609 of 617 rows right against the reader's 614).

## Consequences
- A misread with a low score delays an order by a read instead of booking it wrong; a confident misread would still
  pass (none seen).
