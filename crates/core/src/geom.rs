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

    /// The smallest box holding both.
    pub fn union(self, b: Rect) -> Rect {
        Rect { x0: self.x0.min(b.x0), y0: self.y0.min(b.y0), x1: self.x1.max(b.x1), y1: self.y1.max(b.y1) }
    }

    /// The box shifted by (dx, dy).
    pub fn offset(self, dx: f32, dy: f32) -> Rect {
        Rect { x0: self.x0 + dx, y0: self.y0 + dy, x1: self.x1 + dx, y1: self.y1 + dy }
    }

    /// Whether (x, y) lies in the box (x1, y1 exclusive).
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }

    /// The share of `b` that this box covers (`b`'s area taken as at least 1 px²).
    pub fn share_of(self, b: Rect) -> f32 {
        let w = (self.x1.min(b.x1) - self.x0.max(b.x0)).max(0.0);
        let h = (self.y1.min(b.y1) - self.y0.max(b.y0)).max(0.0);
        w * h / (b.w() * b.h()).max(1.0)
    }

    /// A part of the box given in fractions of its size, (x0, y0, x1, y1); fractions below 0 or above 1 reach
    /// beyond it.
    pub fn part(self, (fx0, fy0, fx1, fy1): (f32, f32, f32, f32)) -> Rect {
        Rect {
            x0: self.x0 + fx0 * self.w(),
            y0: self.y0 + fy0 * self.h(),
            x1: self.x0 + fx1 * self.w(),
            y1: self.y0 + fy1 * self.h(),
        }
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

impl TextBox {
    /// The boxes of recognized lines: each rect with its (text, score), in their order; lines read empty (or as
    /// blanks only) are dropped.
    pub fn from_reads(rects: impl IntoIterator<Item = Rect>, texts: Vec<(String, f32)>) -> Vec<TextBox> {
        rects
            .into_iter()
            .zip(texts)
            .filter(|(_, (t, _))| !t.trim().is_empty())
            .map(|(rect, (text, score))| TextBox { rect, text, score })
            .collect()
    }
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
pub(crate) fn iou(a: Rect, b: Rect) -> f32 {
    let w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let i = w * h;
    i / (a.w() * a.h() + b.w() * b.h() - i).max(1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    #[test]
    fn contains_share_and_part() {
        let a = r(10.0, 10.0, 30.0, 20.0);
        assert!(a.contains(10.0, 10.0) && a.contains(29.9, 19.9));
        assert!(!a.contains(30.0, 15.0) && !a.contains(20.0, 20.0));
        // Half of b lies in a; a tiny b counts as 1 px²
        assert_eq!(a.share_of(r(20.0, 10.0, 40.0, 20.0)), 0.5);
        assert_eq!(a.share_of(r(10.0, 10.0, 10.5, 10.5)), 0.25);
        assert_eq!(a.share_of(r(50.0, 50.0, 60.0, 60.0)), 0.0);
        assert_eq!(a.part((0.5, -0.5, 1.0, 1.5)), r(20.0, 5.0, 30.0, 25.0));
    }

    #[test]
    fn reads_without_text_are_dropped() {
        let texts = vec![("a".to_string(), 0.9), (" ".to_string(), 0.5), ("b".to_string(), 0.8)];
        let boxes = TextBox::from_reads([r(0.0, 0.0, 1.0, 1.0), r(1.0, 0.0, 2.0, 1.0), r(2.0, 0.0, 3.0, 1.0)], texts);
        assert_eq!(boxes.iter().map(|b| (b.text.as_str(), b.rect.x0)).collect::<Vec<_>>(), [("a", 0.0), ("b", 2.0)]);
    }
}
