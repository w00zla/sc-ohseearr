//! Boxes in image pixels and what the models return.

/// An axis-aligned box in image pixels (x1, y1 exclusive).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
    pub fn cx(&self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
    pub fn cy(&self) -> f32 {
        (self.y0 + self.y1) / 2.0
    }

    /// The box scaled by (sx, sy) and clamped to a `w`×`h` image.
    pub(crate) fn scaled_into(self, sx: f32, sy: f32, w: u32, h: u32) -> Rect {
        Rect {
            x0: (self.x0 * sx).clamp(0.0, w as f32),
            y0: (self.y0 * sy).clamp(0.0, h as f32),
            x1: (self.x1 * sx).clamp(0.0, w as f32),
            y1: (self.y1 * sy).clamp(0.0, h as f32),
        }
    }
}

/// A recognized text line.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TextBox {
    pub rect: Rect,
    pub text: String,
    pub score: f32,
}

/// A box a layout detector found: its class (an index into the model's classes), its score (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub class: usize,
    pub score: f32,
    pub rect: Rect,
}

/// Of boxes of one class overlapping more than `limit` (IoU) only the best scored, best first.
pub fn nms(mut boxes: Vec<Detection>, limit: f32) -> Vec<Detection> {
    boxes.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for d in boxes {
        if !kept.iter().any(|k| k.class == d.class && iou(k.rect, d.rect) > limit) {
            kept.push(d);
        }
    }
    kept
}

/// Intersection over union of two boxes.
pub fn iou(a: Rect, b: Rect) -> f32 {
    let w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let i = w * h;
    i / (a.w() * a.h() + b.w() * b.h() - i).max(1e-6)
}
