//! Finding the panels: each table head, its panel's header, station and time remaining, the columns by the head's
//! words and the rows by the material names below it.

use sc_ohseearr_core::{Rect, TextBox};
use sc_ohseearr_sc::norm;
use sc_ohseearr_sc::parse::duration;

use crate::{Labels, Status, is_head};

/// A row's name starts at most this share of the table head's width right of the head's left edge (the
/// material's icon comes first)
const NAME_INDENT: f32 = 0.35;

/// A column of the table: its name (normalized) and center.
#[derive(Debug, Clone)]
pub(crate) struct Column {
    pub(crate) key: String,
    pub(crate) cx: f32,
}

/// The words of the table head with their (estimated) centers: OCR may merge neighbouring heads into one box
/// ("QUALITY YIELD"), each word then gets its share of the box by its characters. A zero is an O (no label holds
/// a digit; PP-OCRv6 tiny read "TO D0").
pub(crate) fn head_words(boxes: &[&TextBox]) -> Vec<Column> {
    let mut out = Vec::new();
    for b in boxes {
        let chars: Vec<char> = b.text.chars().collect();
        let total = chars.len().max(1) as f32;
        let mut start = None;
        for (i, c) in chars.iter().chain(std::iter::once(&' ')).enumerate() {
            match (c.is_whitespace(), start) {
                (false, None) => start = Some(i),
                (true, Some(s)) => {
                    let word: String = chars[s..i].iter().collect();
                    let mid = (s + i) as f32 / 2.0;
                    out.push(Column { key: norm(&word).replace('0', "O"), cx: b.rect.x0 + b.rect.w() * mid / total });
                    start = None;
                }
                _ => {}
            }
        }
    }
    out.sort_by(|a, b| a.cx.total_cmp(&b.cx));
    out
}

/// Where a parse found things, before the cells are read.
pub(crate) struct Layout {
    pub(crate) status: Status,
    pub(crate) station: Option<String>,
    /// The time remaining: the box of the first text right of the label that reads as a duration
    pub(crate) time: Option<TextBox>,
    /// Name boxes of the rows, top to bottom
    pub(crate) rows: Vec<Rect>,
    pub(crate) names: Vec<String>,
    /// The value columns in table order (processing: QUALITY YIELD TODO DONE; completed: QUALITY YIELD)
    pub(crate) columns: Vec<Column>,
}

/// The columns `wanted` (each: its label in every language) among the head's words, left to right; a label may
/// span up to three words ("TO DO"), its center then lies between them.
pub(crate) fn find_columns(words: &[Column], wanted: &[&Vec<String>]) -> Option<Vec<Column>> {
    let mut out: Vec<Column> = Vec::new();
    for labels in wanted {
        let found = (0..words.len()).find_map(|i| {
            (1..=3).filter(|n| i + n <= words.len()).find_map(|n| {
                let joined: String = words[i..i + n].iter().map(|w| w.key.as_str()).collect();
                labels.contains(&joined).then(|| Column { key: joined, cx: (words[i].cx + words[i + n - 1].cx) / 2.0 })
            })
        })?;
        out.push(found);
    }
    out.sort_by(|a, b| a.cx.total_cmp(&b.cx));
    Some(out)
}

/// The work order panels in a frame, left to right.
pub(crate) fn layouts(boxes: &[TextBox], labels: &Labels) -> Vec<Layout> {
    let mut heads: Vec<&TextBox> = boxes.iter().filter(|b| is_head(&b.text, labels)).collect();
    heads.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
    // Each table ends where the next one to its right starts
    (0..heads.len())
        .filter_map(|i| layout_at(boxes, heads[i], heads.get(i + 1).map_or(f32::MAX, |n| n.rect.x0), labels))
        .collect()
}

/// The panel whose table head is `head`, left of `right_limit`.
fn layout_at(boxes: &[TextBox], head: &TextBox, right_limit: f32, labels: &Labels) -> Option<Layout> {
    let find = |pred: &dyn Fn(&str) -> bool| boxes.iter().filter(|b| pred(&norm(&b.text))).collect::<Vec<_>>();
    let unit = head.rect.h();
    // The panel header above the table (the biggest "PROCESSING"/"COMPLETED" there, within this panel)
    let header = find(&|t| labels.processing.iter().chain(&labels.completed).any(|l| l == t))
        .into_iter()
        .filter(|b| b.rect.cy() < head.rect.y0 && b.rect.x0 > head.rect.x0 - unit * 2.0 && b.rect.x1 < right_limit)
        .max_by(|a, b| a.rect.h().total_cmp(&b.rect.h()))?;
    let status = if labels.completed.contains(&norm(&header.text)) { Status::Completed } else { Status::Processing };
    let panel_left = head.rect.x0 - unit;
    let panel_right = (header.rect.x1 + unit).min(right_limit);
    let in_panel = |b: &TextBox| b.rect.cx() > panel_left && b.rect.cx() < panel_right;
    let time = find(&|t| labels.time_remaining.iter().any(|l| l == t))
        .into_iter()
        .find(|b| b.rect.cy() > head.rect.cy() && in_panel(b));

    // Columns: the head's words right of "MATERIALS …"
    let line: Vec<&TextBox> = boxes
        .iter()
        .filter(|b| {
            (b.rect.cy() - head.rect.cy()).abs() < unit * 0.6
                && b.rect.x0 > head.rect.x1 - 1.0
                && b.rect.cx() < panel_right
        })
        .collect();
    let wanted: Vec<&Vec<String>> = if status == Status::Processing {
        vec![&labels.quality, &labels.yield_, &labels.todo, &labels.done]
    } else {
        vec![&labels.quality, &labels.yield_]
    };
    let columns = find_columns(&head_words(&line), &wanted)?;

    // Rows: the names below the head, left of the first column, above the summary lines
    let end = boxes
        .iter()
        .filter(|b| b.rect.y1 > head.rect.y1 && in_panel(b))
        .filter(|b| {
            let t = norm(&b.text);
            // A section label ("// PROCESSING", "// RESULTS") too: a message over the panel may hide the summary
            // lines
            labels.time_remaining.contains(&t)
                || (labels.yield_.contains(&t) && b.rect.x0 < columns[0].cx - unit)
                || b.text.trim_start().starts_with("//")
        })
        .filter(|b| b.rect.cy() > head.rect.y1)
        .map(|b| b.rect.y0)
        .fold(time.map_or(f32::MAX, |t| t.rect.y0), f32::min);
    // Names start near the table's left edge, after the material's icon (~50 px at 1600 px height: a share of the
    // head's width, its height is too small a unit), and end before the first column's center
    let mut rows: Vec<&TextBox> = boxes
        .iter()
        .filter(|b| b.rect.y0 > head.rect.y1 - unit * 0.2 && b.rect.y1 < end)
        .filter(|b| {
            b.rect.x0 > head.rect.x0 - unit * 2.0
                && b.rect.x0 < head.rect.x0 + NAME_INDENT * head.rect.w()
                && b.rect.x1 < columns[0].cx
        })
        .filter(|b| {
            let t = norm(&b.text);
            t.len() >= 3 && t.chars().any(|c| c.is_alphabetic())
        })
        .collect();
    rows.sort_by(|a, b| a.rect.y0.total_cmp(&b.rect.y0));
    let time = time.and_then(|t| {
        boxes
            .iter()
            .filter(|b| {
                b.rect.x0 > t.rect.x1 && b.rect.x0 < panel_right && (b.rect.cy() - t.rect.cy()).abs() < t.rect.h()
            })
            .find(|b| duration(&b.text).is_some())
            .cloned()
    });

    // The station: the biggest text wholly above the panel header (not another panel's header) that starts left of
    // it (the title may reach over the first panel); not a "// REFINERY SYSTEM C47.02" label (the slanted terminal
    // makes its box as tall as the title's)
    let station = boxes
        .iter()
        .filter(|b| {
            b.rect.y1 < header.rect.y0
                && b.rect.x0 < header.rect.x0
                && norm(&b.text).len() >= 3
                && !b.text.trim_start().starts_with(['/', '|'])
        })
        .max_by(|a, b| a.rect.h().total_cmp(&b.rect.h()))
        .map(|b| b.text.trim().to_string());

    Some(Layout {
        status,
        station,
        time,
        names: rows.iter().map(|b| b.text.trim().to_string()).collect(),
        rows: rows.iter().map(|b| b.rect).collect(),
        columns,
    })
}

/// The cell of a row in a column: between the column's neighbours, the row's height.
pub(crate) fn cell(row: Rect, columns: &[Column], i: usize) -> Rect {
    let cx = columns[i].cx;
    let left = if i > 0 { (columns[i - 1].cx + cx) / 2.0 } else { cx - (columns[1].cx - cx) / 2.0 };
    let right =
        if i + 1 < columns.len() { (columns[i + 1].cx + cx) / 2.0 } else { cx + (cx - columns[i - 1].cx) / 2.0 };
    let pad = row.h() * 0.2;
    Rect { x0: left, y0: row.y0 - pad, x1: right, y1: row.y1 + pad }
}
