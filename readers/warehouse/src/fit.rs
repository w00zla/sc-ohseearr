//! The grid fit: the detected badges and quality texts are anchors, each implying its tile's corner for a given
//! pitch; the pitch and phase with the most agreeing anchors win (per axis), and the four consecutive columns with
//! the most anchors are the warehouse's. A weak fit keeps the pitch of the last strong one.

use sc_ohseearr_core::Rect;

use crate::Layout;

/// The warehouse pane lies right of this share of the frame's width
pub(crate) const PANE_LEFT: f32 = 0.45;
/// The grid lies within these shares of the frame's height (the filter row and "LOCATION" above, the button bar
/// below)
const GRID_TOP: f32 = 0.3;
const GRID_BOTTOM: f32 = 0.84;
/// Pitch candidates (shares of the frame height) and the step between them (1080p 16:9 with the terminal at
/// arm's length: 0.13)
const PITCH_MIN: f32 = 0.10;
const PITCH_MAX: f32 = 0.20;
const PITCH_STEP: f32 = 0.0015;
/// Anchors within this share of the pitch agree on a phase
const PHASE_TOL: f32 = 0.05;
/// Agreeing anchor weight a fit needs (badge 1, quality text 2), and the weight from which a fit is strong enough
/// to set the pitch on its own; a weaker fit keeps the pitch of the last strong one (`hint`: the terminal does not
/// move while the player stands) and fits only the phase
const MIN_FIT_WEIGHT: u32 = 3;
pub(crate) const STRONG_FIT_WEIGHT: u32 = 6;
/// Rows are a little tighter than columns (the terminal is seen from above): the row pitch as a share of the
/// column pitch
const ROW_RATIO: (f32, f32) = (0.85, 1.05);
/// Columns of the warehouse grid
const COLUMNS: usize = 4;
/// Tile geometry as shares of the pitch: the tile's size, and where its texts sit (the badge's right edge and
/// top, the quality text's left edge and bottom) relative to the tile's top left
pub(crate) const TILE_SIZE: f32 = 0.93;
const BADGE_RIGHT: f32 = 0.89;
const BADGE_TOP: f32 = 0.03;
const QUALITY_LEFT: f32 = 0.02;
const QUALITY_BOTTOM: f32 = 0.92;

/// An anchor's implied tile corner for a pitch: `base - factor * pitch`.
#[derive(Clone, Copy)]
struct Anchor {
    base: f32,
    factor: f32,
    weight: u32,
}

/// The pitch and phase (tile corner = phase + k · pitch) most anchors agree on, with the agreeing anchors' weight,
/// over the pitches in `range` (frame pixels); among equal weights the pitch nearest `hint`. None without anchors.
fn fit_axis(anchors: &[Anchor], range: (f32, f32), hint: Option<f32>) -> Option<(f32, f32, u32)> {
    if anchors.is_empty() {
        return None;
    }
    let better = |weight: u32, pitch: f32, best: &Option<(f32, f32, u32)>| match best {
        None => true,
        Some((bp, _, bw)) => {
            weight > *bw || (weight == *bw && hint.is_some_and(|h| (pitch - h).abs() < (bp - h).abs()))
        }
    };
    let mut best: Option<(f32, f32, u32)> = None;
    let step = PITCH_STEP * (range.1 / PITCH_MAX);
    let steps = ((range.1 - range.0) / step).round().max(0.0) as usize;
    for i in 0..=steps {
        let pitch = range.0 + i as f32 * step;
        let tol = PHASE_TOL * pitch;
        // Phases on the circle [0, pitch); the window of width tol with the most weight
        let mut phases: Vec<(f32, u32)> =
            anchors.iter().map(|a| ((a.base - a.factor * pitch).rem_euclid(pitch), a.weight)).collect();
        phases.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n = phases.len();
        for start in 0..n {
            let (p0, _) = phases[start];
            let mut weight = 0;
            let mut sum = 0.0;
            for k in 0..n {
                let (p, w) = phases[(start + k) % n];
                // Unwrap past the circle's end
                let p = if k > 0 && p < p0 { p + pitch } else { p };
                if p - p0 > tol {
                    break;
                }
                weight += w;
                sum += p * w as f32;
            }
            let phase = (sum / weight as f32).rem_euclid(pitch);
            if better(weight, pitch, &best) {
                best = Some((pitch, phase, weight));
            }
        }
    }
    best
}

/// An axis fitted freely over the pitch range when that fit is strong (or there is no `hint`), else locked to the
/// hint pitch with the phase from whatever anchors there are (a read around the cursor sees few: the hovered
/// tile's and its neighbours').
fn fit_or_lock(anchors: &[Anchor], range: (f32, f32), hint: Option<f32>) -> Option<(f32, f32, u32)> {
    let free = fit_axis(anchors, range, hint);
    match (free, hint) {
        (Some(f), _) if f.2 >= STRONG_FIT_WEIGHT => Some(f),
        (Some(f), None) if f.2 >= MIN_FIT_WEIGHT => Some(f),
        (_, Some(h)) => fit_axis(anchors, (h, h), None),
        _ => None,
    }
}

/// The COLUMNS consecutive candidate columns (`phase + k · pitch` across the pane) with the most anchor weight;
/// ties go to the window nearest the last layout's first column, else the rightmost.
fn columns(phase: f32, pitch: f32, xs: &[Anchor], frame_w: f32, last: Option<&Layout>) -> Option<Vec<f32>> {
    let first_k = ((PANE_LEFT * frame_w - phase) / pitch).ceil() as i32;
    let last_k = ((frame_w - TILE_SIZE * pitch - phase) / pitch).floor() as i32;
    let candidates: Vec<f32> = (first_k..=last_k).map(|k| phase + k as f32 * pitch).collect();
    if candidates.is_empty() {
        return None;
    }
    let weight_at = |x0: f32| -> u32 {
        xs.iter().filter(|a| ((a.base - a.factor * pitch) - x0).abs() <= PHASE_TOL * pitch).map(|a| a.weight).sum()
    };
    let weights: Vec<u32> = candidates.iter().map(|&x| weight_at(x)).collect();
    let n = candidates.len().min(COLUMNS);
    let near = |i: usize| match last.and_then(|l| l.cols.first()) {
        Some(&c) => -(candidates[i] - c).abs(),
        None => i as f32,
    };
    let (start, total) = (0..=candidates.len() - n)
        .map(|i| (i, weights[i..i + n].iter().sum::<u32>()))
        .max_by(|a, b| a.1.cmp(&b.1).then(near(a.0).total_cmp(&near(b.0))))?;
    if total == 0 {
        return None;
    }
    Some(candidates[start..start + n].to_vec())
}

/// The grid's layout from the detected badges and quality texts right of the screen centre as anchors; `last` is
/// the last strong read's layout: a weak fit keeps its pitch, ties between column windows go to its columns.
/// `None` without any anchor, or without a column with one. The layout's `weight` tells how strong the weaker
/// axis' fit was.
pub(crate) fn layout_from_anchors(
    badges: &[Rect],
    qualities: &[Rect],
    frame_w: f32,
    frame_h: f32,
    last: Option<&Layout>,
) -> Option<Layout> {
    let s = frame_h;
    let in_pane = |r: &&Rect| r.x0 >= PANE_LEFT * frame_w && r.y0 >= GRID_TOP * s && r.y1 <= GRID_BOTTOM * s;
    let badges: Vec<&Rect> = badges.iter().filter(in_pane).collect();
    let qualities: Vec<&Rect> = qualities.iter().filter(in_pane).collect();
    let xs: Vec<Anchor> = badges
        .iter()
        .map(|r| Anchor { base: r.x1, factor: BADGE_RIGHT, weight: 1 })
        .chain(qualities.iter().map(|r| Anchor { base: r.x0, factor: QUALITY_LEFT, weight: 2 }))
        .collect();
    let ys: Vec<Anchor> = badges
        .iter()
        .map(|r| Anchor { base: r.y0, factor: BADGE_TOP, weight: 1 })
        .chain(qualities.iter().map(|r| Anchor { base: r.y1, factor: QUALITY_BOTTOM, weight: 2 }))
        .collect();
    // A single anchor fits anything without a prior; with one it sets the phase
    let (pitch_x, phase_x, weight_x) = fit_or_lock(&xs, (PITCH_MIN * s, PITCH_MAX * s), last.map(|l| l.pitch_x))?;
    let cols = columns(phase_x, pitch_x, &xs, frame_w, last)?;
    // Rows only a little tighter than the columns
    let (pitch_y, phase_y, weight_y) =
        fit_or_lock(&ys, (pitch_x * ROW_RATIO.0, pitch_x * ROW_RATIO.1), last.map(|l| l.pitch_y))?;
    let weight = weight_x.min(weight_y);
    // Rows: every tile fully inside the grid area
    let first_r = ((GRID_TOP * s - phase_y) / pitch_y).ceil() as i32;
    let last_r = ((GRID_BOTTOM * s - TILE_SIZE * pitch_y - phase_y) / pitch_y).floor() as i32;
    let rows: Vec<f32> = (first_r..=last_r).map(|k| phase_y + k as f32 * pitch_y).collect();
    if rows.is_empty() {
        return None;
    }
    Some(Layout { cols, rows, pitch_x, pitch_y, weight })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::read::parse_badge;
    use sc_ohseearr_core::TextBox;
    use sc_ohseearr_sc::parse;

    pub(crate) fn tb(x0: f32, y0: f32, w: f32, h: f32, text: &str) -> TextBox {
        TextBox { rect: Rect { x0, y0, x1: x0 + w, y1: y0 + h }, text: text.into(), score: 1.0 }
    }

    /// The fit over text boxes as the layout detector would have sorted them: badges and the tiles' quality texts
    /// (with their Q; a bare number is the tooltip chip's) as anchors.
    pub(crate) fn layout_from_boxes(
        boxes: &[TextBox],
        frame_w: f32,
        frame_h: f32,
        last: Option<&Layout>,
    ) -> Option<Layout> {
        let badges: Vec<Rect> = boxes.iter().filter(|b| parse_badge(&b.text).is_some()).map(|b| b.rect).collect();
        let qualities: Vec<Rect> = boxes
            .iter()
            .filter(|b| b.text.ends_with('Q') && parse::quality(&b.text).is_some())
            .map(|b| b.rect)
            .collect();
        layout_from_anchors(&badges, &qualities, frame_w, frame_h, last)
    }

    /// The live frame of 2026-10-18 (3840×1600): badges at a pitch of ~275 px, rows ~250 px apart, some badges
    /// missed, the panel's close button an "X" too.
    pub(crate) fn live_boxes() -> Vec<TextBox> {
        vec![
            tb(2185.0, 509.0, 40.0, 31.0, "XI"),
            tb(2457.0, 511.0, 40.0, 29.0, "X"),
            tb(3007.0, 515.0, 41.0, 31.0, "X"),
            tb(2187.0, 755.0, 45.0, 33.0, "X"),
            tb(2750.0, 759.0, 45.0, 33.0, "X"),
            tb(3032.0, 762.0, 43.0, 31.0, "X"),
            tb(2190.0, 1014.0, 47.0, 37.0, "X"),
            tb(1972.0, 954.0, 83.0, 39.0, "310Q"),
            tb(2259.0, 958.0, 83.0, 37.0, "310Q"),
            tb(2546.0, 960.0, 84.0, 37.0, "310Q"),
            tb(1970.0, 1227.0, 86.0, 41.0, "310Q"),
            // The panel's close button, the elevator pane's and the panel's texts: no anchors
            tb(2819.0, 208.0, 30.0, 30.0, "X"),
            tb(1426.0, 354.0, 258.0, 34.0, "SELECT ALL SHOWN"),
            tb(2089.0, 588.0, 57.0, 18.0, "STOR=ALL"),
            tb(2786.0, 930.0, 201.0, 33.0, "Hephaestanite"),
            tb(2789.0, 968.0, 169.0, 26.0, "Capacity: 1.00SCU"),
            tb(2972.0, 993.0, 45.0, 27.0, "572"),
            tb(2805.0, 994.0, 137.0, 26.0, "Hephaestanite"),
            tb(3055.0, 995.0, 98.0, 25.0, "0.263 SCU"),
        ]
    }

    #[test]
    fn layout_fitted_to_the_anchors_of_a_live_frame() {
        let l = layout_from_boxes(&live_boxes(), 3840.0, 1600.0, None).expect("layout");
        assert_eq!(l.cols.len(), 4);
        assert!((l.pitch_x - 280.0).abs() < 12.0, "{}", l.pitch_x);
        assert!((l.pitch_y - 250.0).abs() < 12.0, "{}", l.pitch_y);
        // Column 0 starts a little left of its quality texts (1970), the last column holds the badges at ~3050
        assert!((l.cols[0] - 1965.0).abs() < 12.0, "{:?}", l.cols);
        let last = l.tile_rect(0, 3);
        assert!(last.x1 > 3050.0 && last.x1 < 3100.0, "{last:?}");
        // Rows: the first anchors' row near y 500, one row per pitch down to the grid's bottom
        let row_of = |y: f32| l.rows.iter().position(|&r| (r - y).abs() < 15.0);
        assert!(row_of(503.0).is_some() && row_of(753.0).is_some() && row_of(1003.0).is_some(), "{:?}", l.rows);
        assert!(l.rows.iter().all(|&r| r >= 0.25 * 1600.0 && r + TILE_SIZE * l.pitch_y <= 0.92 * 1600.0));
        assert!(layout_from_boxes(&live_boxes()[11..], 3840.0, 1600.0, None).is_none(), "no anchors in the grid");
        // The last read's pitch decides among equally good fits
        let prior = Layout { pitch_x: l.pitch_x + 2.0, ..l.clone() };
        let hinted = layout_from_boxes(&live_boxes(), 3840.0, 1600.0, Some(&prior)).expect("layout");
        assert!((hinted.pitch_x - l.pitch_x).abs() <= 3.0);
        assert!(l.weight >= 3);
    }

    #[test]
    fn a_weak_fit_keeps_the_last_layouts_pitch_and_columns() {
        // Two rows of items: badges only, few of them (weight 4 per axis at most)
        let boxes = vec![
            tb(2185.0, 509.0, 40.0, 31.0, "X1"),
            tb(2457.0, 511.0, 40.0, 29.0, "X"),
            tb(2187.0, 755.0, 45.0, 33.0, "X"),
            tb(2750.0, 759.0, 45.0, 33.0, "X"),
        ];
        let last = Layout {
            cols: vec![1960.0, 2235.0, 2510.0, 2785.0],
            rows: vec![500.0, 750.0, 1000.0],
            pitch_x: 275.0,
            pitch_y: 250.0,
            weight: 8,
        };
        let locked = layout_from_boxes(&boxes, 3840.0, 1600.0, Some(&last)).expect("layout");
        assert!((locked.pitch_x - 275.0).abs() < 0.5 && (locked.pitch_y - 250.0).abs() < 0.5, "{locked:?}");
        assert!(locked.weight < STRONG_FIT_WEIGHT);
        // The rows follow the badges' tops
        assert!(locked.rows.iter().any(|&r| (r - 503.0).abs() < 15.0), "{:?}", locked.rows);
        // A read around the cursor with one badge: its column is the one the last layout had there, not the
        // rightmost window holding it
        let one = layout_from_boxes(&boxes[..1], 3840.0, 1600.0, Some(&last)).expect("layout");
        assert!((one.cols[0] - locked.cols[0]).abs() < 15.0, "{:?} vs {:?}", one.cols, locked.cols);
        assert!(layout_from_boxes(&boxes[..0], 3840.0, 1600.0, Some(&last)).is_none());
    }
}
