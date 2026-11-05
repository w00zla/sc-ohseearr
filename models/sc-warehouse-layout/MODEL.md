# sc-warehouse-layout

Layout detector for the freight manager's warehouse view (SC 4.x); family `nanodet` (`NanoDet`). The reference
example of a screen-specific layout model.

- License: MIT (our weights); architecture NanoDet-Plus-m (RangiLyu/nanodet, Apache-2.0), ShuffleNetV2 1.0×
  backbone, GhostPAN, ~1.2 M parameters.
- Classes: `badge` (a tile's count badge, the light box "X 14"), `quality` (a tile's quality text "526Q"),
  `title` (the tooltip's first line), `tipline` (the tooltip's other lines).
- Input: a fixed 480×320 window, BGR with the training's mean/std; callers cut the frame (scaled to 720 px
  height) into windows of that aspect ratio and merge their boxes (NMS 0.5 per class).
- ~4 ms per window on the CPU, where the text detector took ~75 ms for the same area.
- Training data: frames of warehouse recordings (each scaled to 720 px height and cut into 480×320 windows),
  labelled by the text-detector-based read that preceded this model; screenshots of other screens as windows
  without labels (without them it took other screens' "0/0" texts for badges).
