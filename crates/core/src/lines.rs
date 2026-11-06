//! Text boxes into lines: a detector's boxes split a line where its words stand apart (a label and its value in
//! a column of their own); readers want the line.

use crate::geom::{Rect, TextBox};

/// One text line: its boxes joined left to right with a space.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Line {
    pub text: String,
    pub rect: Rect,
}

/// The boxes grouped into lines, top to bottom: a box joins the line above it when its vertical centre lies within
/// half a box height (the smaller of the two) of any box already in that line. Filter the boxes first (score,
/// height): stray boxes would merge lines.
pub fn group_lines(boxes: &[&TextBox]) -> Vec<Line> {
    let mut sorted: Vec<&TextBox> = boxes.to_vec();
    sorted.sort_by(|a, b| a.rect.cy().total_cmp(&b.rect.cy()).then(a.rect.x0.total_cmp(&b.rect.x0)));
    let mut lines: Vec<Vec<&TextBox>> = Vec::new();
    for b in sorted {
        match lines.last_mut() {
            Some(line) if line.iter().any(|o| (o.rect.cy() - b.rect.cy()).abs() < 0.5 * o.rect.h().min(b.rect.h())) => {
                line.push(b)
            }
            _ => lines.push(vec![b]),
        }
    }
    lines
        .into_iter()
        .map(|mut line| {
            line.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
            let rect = line.iter().skip(1).fold(line[0].rect, |r, b| r.union(b.rect));
            Line { text: line.iter().map(|b| b.text.trim()).collect::<Vec<_>>().join(" "), rect }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x0: f32, y0: f32, x1: f32, y1: f32, text: &str) -> TextBox {
        TextBox { rect: Rect { x0, y0, x1, y1 }, text: text.into(), score: 1.0 }
    }

    #[test]
    fn label_and_value_share_a_line() {
        let boxes = [
            b(300.0, 101.0, 360.0, 121.0, " 60.00 "),
            b(10.0, 100.0, 200.0, 120.0, "Coolant Rating:"),
            b(10.0, 140.0, 90.0, 160.0, "Size 2"),
        ];
        let refs: Vec<&TextBox> = boxes.iter().collect();
        let lines = group_lines(&refs);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Coolant Rating: 60.00");
        assert_eq!(lines[0].rect, Rect { x0: 10.0, y0: 100.0, x1: 360.0, y1: 121.0 });
        assert_eq!(lines[1].text, "Size 2");
    }
}
