//! The read (ported unchanged from the Quartermaster companion's `inventory.rs`, 2026-11-06; screenshots and live
//! frames of 2026-10-18 on).
//!
//! The terminal is a screen in the world: the grid's size and spacing change with where the player stands
//! (column pitch 260–290 px at 1600 px frame height, rows a little tighter), so nothing is fixed on the screen.
//! The grid is fitted per frame: the detected badge boxes and quality texts are anchors, each implying its tile's
//! corner for a given pitch; the pitch and phase with the most agreeing anchors win (per axis), and the four
//! consecutive columns with the most anchors are the warehouse's. The anchors and the tooltip's lines come from
//! the warehouse's layout detector (badges and quality texts as boxes of their kind, nothing recognized for the
//! fit). It sees windows of the frame scaled to LAYOUT_FRAME_H: without a prior the whole pane right of the
//! screen centre in overlapping windows (the elevator pane on the left is ignored); with a prior and a cursor,
//! one window around the cursor (the hovered tile's and its neighbours' anchors, the tooltip's window), with the
//! prior's pitch. Then only the tooltip's lines, the hovered tile's badge (upscaled) and quality band are
//! recognized. The hovered tile is the one under the cursor, in screenshots the one under the tooltip's cursor
//! point (a little up and left of the title).
//!
//! Anything unexpected yields fewer tiles or empty fields, never a panic.

use image::RgbImage;
use image::imageops;
use sc_ohseearr_core::{Detection, Error, FilterType, NanoDet, Ocr, Rect, TextBox, nms, resize};
use sc_ohseearr_sc::{Field, Frame, Name, norm, parse};

use crate::{Cell, CellKind, Layout, Tile, Tooltip, Warehouse, WarehouseDiagnostics};

/// The warehouse pane lies right of this share of the frame's width
const PANE_LEFT: f32 = 0.45;
/// The grid lies within these shares of the frame's height (the filter row and "LOCATION" above, the button bar
/// below)
const GRID_TOP: f32 = 0.3;
const GRID_BOTTOM: f32 = 0.84;
/// What the detector reads (shares of the height): the grid with some room, below it the tooltip of a bottom row
/// tile
const READ_TOP: f32 = 0.25;
const READ_BOTTOM: f32 = 0.95;
/// The layout detector's windows: its input size of the frame scaled to this height (as it was trained; at 1080p
/// a window is 720×480 frame pixels), overlapping by WINDOW_OVERLAP (detector pixels) where they cover the pane
const LAYOUT_FRAME_H: f32 = 720.0;
const WINDOW_OVERLAP: f32 = 40.0;
/// The tooltip's lines and the quality text are recognized scaled up as if the frame were this high (×1.5 at
/// 1600 px)
const LINE_HEIGHT: f32 = 2400.0;
/// A detected quality text is read with this share of its height around it
const QUALITY_PAD: f32 = 0.25;
/// Badges and quality texts anchor the fit from this detector score: the character inventory's "0/0" labels came
/// as both up to 0.5 and fitted a grid there (screenshots of 2026-10-18), the warehouse's anchors score above it
/// 97 % of the time
const ANCHOR_SCORE: f32 = 0.55;
/// With a cursor and a known layout one window is read, its top left this many pitches left and up of the cursor
/// (the hovered tile's anchors and its neighbours'); the rest of it holds the tooltip's window right and below
/// (as in training)
const AROUND: f32 = 1.6;
/// Height of a text line (shares of the frame height: a badge 18 px high at 1600 px)
const TEXT_MIN_H: f32 = 0.011;
const TEXT_MAX_H: f32 = 0.032;
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
/// The quality band at the tile's bottom left and the badge area at its top right, as shares of the tile
/// (reaching into the gaps around it: the fitted edges are a few pixels off)
const QUALITY_BAND: (f32, f32, f32, f32) = (-0.08, 0.76, 0.55, 1.02);
const BADGE_AREA: (f32, f32, f32, f32) = (0.66, -0.02, 1.06, 0.19);
/// Where a detected badge is looked for: the badge area with room for the fit's error (right of the tile's
/// middle: the quality text of the tile above sits at the left)
const BADGE_SEARCH: (f32, f32, f32, f32) = (0.55, -0.15, 1.12, 0.32);
/// The badge crops are upscaled by this before recognition (small text in a dark box)
const BADGE_SCALE: u32 = 3;
/// Padding around a detected badge box when it is read on its own, on its right a share of its height
const BADGE_PAD: f32 = 6.0;
const BADGE_RIGHT_PAD: f32 = 0.6;
/// A badge's light box stands out from the tile by at least this much luma (0–255), and is at least this share
/// of the searched area's height
const BADGE_CONTRAST: u32 = 60;
const BADGE_MIN_SHARE: f32 = 0.25;
/// What a count badge can hold ("X 14", "×1"): the recognizer picks among these only
const BADGE_CHARS: &str = "Xx× 0123456789";
/// Minimum recognizer confidence of a cell's text
const MIN_SCORE: f32 = 0.5;
/// The tooltip opens right below the cursor: the cursor is about this far up and left of its title
const CURSOR_OFFSET: f32 = 0.01;
/// A quality band covered by the tooltip beyond this share is not read
const TOOLTIP_COVER: f32 = 0.2;
/// The same for a badge, far stricter: read as digits only, the tooltip's text passes for a count
const BADGE_COVER: f32 = 0.05;

/// What the layout detector finds (its classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A tile's count badge, its light box ("X 14")
    Badge,
    /// A tile's quality text ("526Q")
    Quality,
    /// The tooltip's title (its first line)
    Title,
    /// Another line of the tooltip ("Capacity: 1.00mSCU", the chip's name and quality, "0.001 SCU")
    TipLine,
}

impl Kind {
    /// The kind of a model class.
    pub(crate) fn of_class(name: &str) -> Option<Kind> {
        match name {
            "badge" => Some(Kind::Badge),
            "quality" => Some(Kind::Quality),
            "title" => Some(Kind::Title),
            "tipline" => Some(Kind::TipLine),
            _ => None,
        }
    }
}

fn part(tile: Rect, (fx0, fy0, fx1, fy1): (f32, f32, f32, f32)) -> Rect {
    Rect {
        x0: tile.x0 + fx0 * tile.w(),
        y0: tile.y0 + fy0 * tile.h(),
        x1: tile.x0 + fx1 * tile.w(),
        y1: tile.y0 + fy1 * tile.h(),
    }
}

fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1
}

/// The share of `b` that `a` covers.
fn covered(a: Rect, b: Rect) -> f32 {
    let w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    w * h / (b.w() * b.h()).max(1.0)
}

// --- texts -------------------------------------------------------------------------------------------------------

/// A count badge: "X1", "X 14", "×1", "XI" (the 1 read as I or l), "X1D" (the 0 read as D or O), "X" (digits
/// lost) -> (count, uncertain). Anything else is no badge.
pub fn parse_badge(text: &str) -> Option<(u32, bool)> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let rest = t.strip_prefix(['X', 'x', '×'])?;
    if rest.is_empty() {
        return Some((1, true));
    }
    let digits: String = rest
        .chars()
        .map(|c| match c {
            'I' | 'l' | '|' => '1',
            'D' | 'O' | 'o' => '0',
            _ => c,
        })
        .collect();
    if !digits.chars().all(|c| c.is_ascii_digit()) || digits.len() > 4 {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    (n >= 1).then_some((n, false))
}

// --- the grid fit ------------------------------------------------------------------------------------------------

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
fn layout_from_anchors(
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

/// The tooltip as read: its name, the chip's quality, the SCU, the title's box and the area its texts cover.
#[derive(Debug)]
struct Tip {
    name: String,
    quality: Field<u32>,
    scu: Field<String>,
    title: Rect,
    rect: Rect,
}

/// The tooltip: its title is the first of the detector's `titles` (read; at least three letters) whose cursor
/// point (a little up and left of it) lies in a tile or in the gap next to one; the `lines` below it within the
/// tooltip's reach may hold the material's "Capacity" line, the chip "<name>" "<quality>" and the amount. Returns
/// the tooltip and the index of the hovered tile.
fn tooltip_from_boxes(titles: &[TextBox], lines: &[TextBox], tiles: &[Rect], frame_h: f32) -> Option<(Tip, usize)> {
    let s = frame_h;
    let near = CURSOR_OFFSET * s;
    let (title, hovered) =
        titles.iter().filter(|b| b.text.chars().filter(|c| c.is_alphabetic()).count() >= 3).find_map(|b| {
            let (cx, cy) = (b.rect.x0 - near, b.rect.y0 - near);
            let dist = |t: &Rect| (t.x0 - cx).max(cx - t.x1).max(0.0).hypot((t.y0 - cy).max(cy - t.y1).max(0.0));
            let nearest = (0..tiles.len()).min_by(|&i, &j| dist(&tiles[i]).total_cmp(&dist(&tiles[j])));
            tiles
                .iter()
                .position(|t| contains(*t, cx, cy))
                .or(nearest.filter(|&i| dist(&tiles[i]) <= near))
                .map(|i| (b, i))
        })?;
    // A slanted title's box is taller than its line
    let line = title.rect.h().clamp(TEXT_MIN_H * s, TEXT_MAX_H * s);
    let group: Vec<&TextBox> = std::iter::once(title)
        .chain(lines.iter().filter(|b| {
            b.rect.x0 > title.rect.x0 - 2.0 * line
                && b.rect.x0 < title.rect.x1 + 12.0 * line
                && b.rect.y0 >= title.rect.y0
                && b.rect.y0 < title.rect.y1 + 6.0 * line
        }))
        .collect();
    let digits = |t: &str| {
        (!t.is_empty() && t.chars().all(|c| c.is_ascii_digit()))
            .then(|| t.parse::<u32>().ok())
            .flatten()
            .filter(|q| (1..=1000).contains(q))
    };
    // Each value with the box it was read from
    let mut quality: Option<(u32, &TextBox)> = None;
    let mut scu: Option<(String, &TextBox)> = None;
    let mut rect = title.rect;
    // The tooltip is see-through: a tile's quality behind it shows in the title's row ("Janalite 360" over a 360Q
    // tile), at times in the title's own box. The quality is the chip's: right of the name's repeat in the chip
    // row; and the name is the title without such a trailing number when the chip row repeats it that way
    let below_title = |b: &TextBox| b.rect.cy() > title.rect.y1;
    let repeat =
        |key: &str| group.iter().find(|b| !std::ptr::eq(**b, title) && below_title(b) && norm(&b.text) == key).copied();
    let full = title.text.trim();
    let head = match full.rsplit_once(char::is_whitespace) {
        Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => Some(head.trim_end()),
        _ => None,
    };
    let (name, key, chip_name) = match head.map(|h| (h, norm(h))) {
        Some((h, k)) if repeat(&k).is_some() => (h.to_string(), k.clone(), repeat(&k)),
        _ => {
            let k = norm(full);
            (full.to_string(), k.clone(), repeat(&k))
        }
    };
    let mut loose = None;
    for &b in &group {
        rect = rect.union(b.rect);
        if std::ptr::eq(b, title) {
            continue;
        }
        let t = b.text.trim();
        if let Some(v) = parse::scu(t) {
            scu = scu.or(Some((v, b)));
        } else if let Some(q) = digits(t) {
            if chip_name.is_some_and(|c| (b.rect.cy() - c.rect.cy()).abs() < 0.5 * line && b.rect.x0 > c.rect.x0) {
                quality = quality.or(Some((q, b)));
            } else if below_title(b) {
                loose = loose.or(Some((q, b)));
            }
        } else if let Some(rest) = norm(t).strip_prefix(key.as_str()) {
            // The chip's name with its number in one box ("Aphorite 523")
            if let Some(q) = digits(rest) {
                quality = quality.or(Some((q, b)));
            }
        }
    }
    // Without the name's repeat read: a number below the title row, never one in it
    let quality =
        quality.or(loose).map_or(Field::none(), |(q, b)| Field::new(Some(q), b.text.as_str(), b.score, false));
    let scu = scu.map_or(Field::none(), |(v, b)| Field::new(Some(v), b.text.as_str(), b.score, false));
    (!name.is_empty()).then_some((Tip { name, quality, scu, title: title.rect, rect }, hovered))
}

/// The pixels of `rect` in `frame` (its edges truncated, unlike `sc_ohseearr_core::crop`: the read was tuned on
/// these).
fn crop(frame: &RgbImage, rect: Rect) -> RgbImage {
    let (w, h) = frame.dimensions();
    let x0 = (rect.x0.max(0.0) as u32).min(w.saturating_sub(1));
    let y0 = (rect.y0.max(0.0) as u32).min(h.saturating_sub(1));
    let cw = ((rect.x1 as u32).min(w)).saturating_sub(x0).max(1);
    let ch = ((rect.y1 as u32).min(h)).saturating_sub(y0).max(1);
    imageops::crop_imm(frame, x0, y0, cw, ch).to_image()
}

/// A badge's crop upscaled for the recognizer (small text in a box).
fn upscaled_badge(frame: &RgbImage, rect: Rect) -> RgbImage {
    let c = crop(frame, rect);
    resize(&c, c.width() * BADGE_SCALE, c.height() * BADGE_SCALE, FilterType::CatmullRom)
}

/// The count badge's light box in `rect` of `frame`: the largest 4-connected region of pixels brighter than
/// halfway between the area's darkest and brightest (the badge is dark text on a light box; the tile's dark ground
/// and frame lines around it made PP-OCRv6 tiny read an icon, "图", for "X 1", and small lose the thin 1). None
/// without one of a badge's shape inside (an empty area, a box cut by the area's edge).
fn badge_box(frame: &RgbImage, rect: Rect) -> Option<Rect> {
    let x0 = (rect.x0.max(0.0) as u32).min(frame.width().saturating_sub(1));
    let y0 = (rect.y0.max(0.0) as u32).min(frame.height().saturating_sub(1));
    let (w, h) = (
        ((rect.x1.max(0.0) as u32).min(frame.width()).saturating_sub(x0)) as usize,
        ((rect.y1.max(0.0) as u32).min(frame.height()).saturating_sub(y0)) as usize,
    );
    if w < 4 || h < 4 {
        return None;
    }
    let luma: Vec<u32> = (0..w * h)
        .map(|i| {
            let p = frame.get_pixel(x0 + (i % w) as u32, y0 + (i / w) as u32);
            (299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32) / 1000
        })
        .collect();
    let (lo, hi) = (*luma.iter().min().expect("pixels"), *luma.iter().max().expect("pixels"));
    if hi - lo < BADGE_CONTRAST {
        return None;
    }
    let light: Vec<bool> = luma.iter().map(|&l| l > (lo + hi) / 2).collect();
    let mut seen = vec![false; w * h];
    let mut best: Option<(usize, (usize, usize, usize, usize))> = None;
    let mut stack = Vec::new();
    for start in 0..w * h {
        if !light[start] || seen[start] {
            continue;
        }
        let (mut count, mut bx) = (0, (usize::MAX, usize::MAX, 0, 0));
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            count += 1;
            bx = (bx.0.min(x), bx.1.min(y), bx.2.max(x), bx.3.max(y));
            let neighbours = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for j in neighbours.into_iter().flatten() {
                if light[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if best.is_none_or(|(c, _)| count > c) {
            best = Some((count, bx));
        }
    }
    let (_, (bx0, by0, bx1, by1)) = best?;
    let (bw, bh) = ((bx1 - bx0 + 1) as f32, (by1 - by0 + 1) as f32);
    // A badge is wider than high, no sliver, and wholly inside
    if bw < bh || bh < BADGE_MIN_SHARE * h as f32 || bx0 == 0 || by0 == 0 || bx1 + 1 == w || by1 + 1 == h {
        return None;
    }
    Some(Rect {
        x0: (x0 + bx0 as u32) as f32,
        y0: (y0 + by0 as u32) as f32,
        x1: (x0 + bx1 as u32 + 1) as f32,
        y1: (y0 + by1 as u32 + 1) as f32,
    })
}

/// The text lines in `rects` of `frame`, each scaled up as if the frame were LINE_HEIGHT high (tiny misspells them
/// at native size: "Dalivine").
fn read_lines(ocr: &mut Ocr, frame: &RgbImage, rects: &[Rect]) -> Result<Vec<(String, f32)>, Error> {
    let up = (LINE_HEIGHT / frame.height() as f32).max(1.0);
    let crops: Vec<RgbImage> = rects
        .iter()
        .map(|&r| {
            let c = crop(frame, r);
            resize(&c, (c.width() as f32 * up) as u32, (c.height() as f32 * up) as u32, FilterType::CatmullRom)
        })
        .collect();
    ocr.recognize_images(&crops)
}

/// The layout detector's windows over `frame` (frame pixels, of the detector's aspect `input`): one around the
/// `cursor` when the `last` layout is known, else overlapping ones over the whole `pane`.
fn windows(
    pane: Rect,
    frame_h: f32,
    input: (u32, u32),
    cursor: Option<(f32, f32)>,
    last: Option<&Layout>,
) -> Vec<Rect> {
    let k = frame_h / LAYOUT_FRAME_H;
    let (ww, wh) = (input.0 as f32 * k, input.1 as f32 * k);
    // Shifted to stay inside the pane (a pane narrower than a window: at its left)
    let at = |x0: f32, y0: f32| {
        let x0 = x0.min(pane.x1 - ww).max(pane.x0);
        let y0 = y0.min(pane.y1 - wh).max(pane.y0);
        Rect { x0, y0, x1: x0 + ww, y1: y0 + wh }
    };
    if let (Some((cx, cy)), Some(l)) = (cursor, last) {
        return vec![at(cx - AROUND * l.pitch_x, cy - AROUND * l.pitch_y)];
    }
    // As few windows per axis as cover it with the overlap, spread evenly
    let starts = |from: f32, len: f32, win: f32| -> Vec<f32> {
        let n = ((len - win) / (win - WINDOW_OVERLAP * k)).ceil().max(0.0) as usize + 1;
        (0..n).map(|i| if n == 1 { from } else { from + (len - win).max(0.0) * i as f32 / (n - 1) as f32 }).collect()
    };
    let ys = starts(pane.y0, pane.h(), wh);
    starts(pane.x0, pane.w(), ww).into_iter().flat_map(|x| ys.iter().map(move |&y| at(x, y))).collect()
}

/// Reads the warehouse grid in the frame: the layout `detector` (its classes as `kinds`) over the pane right of the
/// screen centre, or, with the mouse cursor (frame pixels) and the `last` layout, around the cursor only (see
/// `windows`); the layout from its badges and quality texts (a weak fit keeps the last layout's pitch), then the
/// tooltip's lines, the hovered tile's quality band and badge (every tile's with `all_badges`, or without a
/// cursor; both upscaled) recognized from the frame. Every cell is a tile (unread fields empty). The hovered tile
/// is the one under the cursor when the capture knows it; else (screenshots) the one with the tooltip chip's
/// quality or whose quality the tooltip hides, else the one under the tooltip's cursor point. No tiles when no
/// grid is in view. The boxes: the anchors as "[badge]" and "[quality]", the tooltip's lines read.
pub(crate) fn read_grid(
    ocr: &mut Ocr,
    detector: &mut NanoDet,
    kinds: &[Kind],
    frame: &Frame,
    last: Option<&Layout>,
    all_badges: bool,
) -> Result<(Warehouse, WarehouseDiagnostics), Error> {
    let (image, cursor) = (frame.image, frame.cursor);
    let (w, h) = image.dimensions();
    let s = h as f32;
    // Only the grid's height and the tooltips' room below it (the search, filter and capacity rows above hold
    // nothing of the grid)
    let pane = Rect { x0: PANE_LEFT * w as f32, y0: READ_TOP * s, x1: w as f32, y1: (READ_BOTTOM * s).min(s) };
    let mut found: Vec<Detection> = Vec::new();
    for win in windows(pane, s, detector.input_size(), cursor, last) {
        let (ox, oy) = (win.x0.max(0.0).floor(), win.y0.max(0.0).floor());
        for mut d in detector.detect(&crop(image, win))? {
            d.rect = Rect { x0: ox + d.rect.x0, y0: oy + d.rect.y0, x1: ox + d.rect.x1, y1: oy + d.rect.y1 };
            found.push(d);
        }
    }
    // One box where windows overlap
    let found = nms(found, detector.nms_limit());
    let of = |kind: Kind| -> Vec<Detection> { found.iter().filter(|d| kinds[d.class] == kind).copied().collect() };
    let anchors = |kind: Kind| -> Vec<Detection> { of(kind).into_iter().filter(|d| d.score >= ANCHOR_SCORE).collect() };
    let (badges, qualities) = (anchors(Kind::Badge), anchors(Kind::Quality));
    let mut boxes: Vec<TextBox> = badges
        .iter()
        .map(|d| TextBox { rect: d.rect, text: "[badge]".into(), score: d.score })
        .chain(qualities.iter().map(|d| TextBox { rect: d.rect, text: "[quality]".into(), score: d.score }))
        .collect();
    let rects = |ds: &[Detection]| -> Vec<Rect> { ds.iter().map(|d| d.rect).collect() };
    let Some(layout) = layout_from_anchors(&rects(&badges), &rects(&qualities), w as f32, s, last) else {
        return Ok((Warehouse::default(), WarehouseDiagnostics { boxes, ..Default::default() }));
    };
    let positions: Vec<(usize, usize)> =
        (0..layout.rows.len()).flat_map(|r| (0..layout.cols.len()).map(move |c| (r, c))).collect();
    let tile_rects: Vec<Rect> = positions.iter().map(|&(r, c)| layout.tile_rect(r, c)).collect();
    // The tooltip's lines, its titles first (best scored first), each scaled up as if the frame were LINE_HEIGHT
    // high: tiny misspells them at native size ("Dalivine")
    let tip: Vec<Detection> = of(Kind::Title).into_iter().chain(of(Kind::TipLine)).collect();
    let texts = read_lines(ocr, image, &rects(&tip))?;
    let (mut titles, mut lines) = (Vec::new(), Vec::new());
    for (d, (text, score)) in tip.into_iter().zip(texts).filter(|(_, (t, _))| !t.trim().is_empty()) {
        let b = TextBox { rect: d.rect, text, score };
        if kinds[d.class] == Kind::Title { titles.push(b) } else { lines.push(b) }
    }
    // Only a title the detector found names the tooltip (a body line like "Registered To You" took the title's
    // place when it went unread)
    let tooltip = tooltip_from_boxes(&titles, &lines, &tile_rects, s);
    boxes.extend(titles);
    boxes.extend(lines);
    // The hovered tile's quality (every tile's without a cursor: the hovered rule needs them)
    let under_cursor = cursor.and_then(|(cx, cy)| tile_rects.iter().position(|t| contains(*t, cx, cy)));
    let read: Vec<usize> =
        if cursor.is_some() { under_cursor.into_iter().collect() } else { (0..positions.len()).collect() };
    // The quality text the detector found in the tile's band (a little grown), else the whole band (it reached
    // into a tooltip's edge next to it: "37104")
    let quality_rects: Vec<Rect> = tile_rects
        .iter()
        .map(|&t| {
            let band = part(t, QUALITY_BAND);
            let found = qualities
                .iter()
                .filter(|d| contains(band, d.rect.cx(), d.rect.cy()))
                .max_by(|a, b| a.score.total_cmp(&b.score));
            found.map_or(band, |d| {
                let m = QUALITY_PAD * d.rect.h();
                Rect { x0: d.rect.x0 - m, y0: d.rect.y0 - m, x1: d.rect.x1 + m, y1: d.rect.y1 + m }
            })
        })
        .collect();
    let read_texts = read_lines(ocr, image, &read.iter().map(|&i| quality_rects[i]).collect::<Vec<_>>())?;
    let mut quality_texts: Vec<Option<(String, f32)>> = vec![None; positions.len()];
    for (&i, t) in read.iter().zip(read_texts) {
        quality_texts[i] = Some(t);
    }
    let hidden: Vec<bool> = quality_rects
        .iter()
        .map(|&r| tooltip.as_ref().is_some_and(|(t, _)| covered(t.rect, r) > TOOLTIP_COVER))
        .collect();
    let qualities: Vec<Option<(u32, bool)>> = quality_texts
        .iter()
        .zip(&hidden)
        .map(|(read, hidden)| {
            read.as_ref()
                .filter(|(_, score)| *score >= MIN_SCORE && !hidden)
                .and_then(|(text, _)| text.split_whitespace().next().and_then(parse::quality))
        })
        .collect();
    // The hovered tile
    let hovered: Option<usize> = match cursor {
        Some(_) => under_cursor,
        None => tooltip.as_ref().map(|(tip, under_point)| {
            let candidates: Vec<usize> = (0..positions.len())
                .filter(|&i| {
                    tip.quality.value.is_some()
                        && (qualities[i].map(|q| q.0) == tip.quality.value || (qualities[i].is_none() && hidden[i]))
                })
                .collect();
            match candidates.as_slice() {
                [one] => *one,
                _ => *under_point,
            }
        }),
    };
    // The badges: the area at the tile's top right and, when the detector found the badge there, that box on its
    // own (the count's digits are thin: whichever read has them wins), both upscaled, read as a badge's characters
    // only. One the tooltip covers (the cursor near the tile's top: the tooltip opens there) is not read: its text
    // would pass for a count
    let badge_hidden = |i: usize| {
        tooltip.as_ref().is_some_and(|(t, _)| covered(t.rect, part(tile_rects[i], BADGE_AREA)) > BADGE_COVER)
    };
    let wanted: Vec<usize> =
        (0..positions.len()).filter(|&i| (all_badges || hovered == Some(i)) && !badge_hidden(i)).collect();
    let upscaled = |r: Rect| upscaled_badge(image, badge_box(image, r).unwrap_or(r));
    let mut badge_rects: Vec<(usize, Rect)> = Vec::new();
    for &i in &wanted {
        let area = part(tile_rects[i], BADGE_AREA);
        // The fitted grid can sit ~30 px off the tiles: a detected badge around the corner counts, the nearest to
        // where the badge should be. It is read first: with digits in both reads, the first wins, and the box
        // sits on the badge
        let search = part(tile_rects[i], BADGE_SEARCH);
        let dist = |d: &&Detection| (d.rect.cx() - area.cx()).abs() + (d.rect.cy() - area.cy()).abs();
        let found = badges
            .iter()
            .filter(|d| contains(search, d.rect.cx(), d.rect.cy()))
            .min_by(|a, b| dist(a).total_cmp(&dist(b)));
        // More room on the right (a share of its height): a digit at the crop's edge is read as "8" or lost
        // ("X 6" -> "X8", "X")
        if let Some(d) = found {
            let r = d.rect;
            let right = BADGE_PAD.max(BADGE_RIGHT_PAD * r.h());
            badge_rects
                .push((i, Rect { x0: r.x0 - BADGE_PAD, y0: r.y0 - BADGE_PAD, x1: r.x1 + right, y1: r.y1 + BADGE_PAD }));
        }
        badge_rects.push((i, area));
    }
    let badge_texts =
        ocr.recognize_images_only(&badge_rects.iter().map(|&(_, r)| upscaled(r)).collect::<Vec<_>>(), BADGE_CHARS)?;
    let mut cells = Vec::new();
    let mut tiles = Vec::new();
    for (i, (&(row, col), &rect)) in positions.iter().zip(&tile_rects).enumerate() {
        let (row, col) = (row as u32, col as u32);
        if let Some((text, score)) = &quality_texts[i] {
            cells.push(Cell {
                row,
                col,
                kind: CellKind::Quality,
                rect: quality_rects[i],
                text: text.clone(),
                score: *score,
            });
        }
        let reads: Vec<(&Rect, &(String, f32))> =
            badge_rects.iter().zip(&badge_texts).filter(|((j, _), _)| *j == i).map(|((_, r), t)| (r, t)).collect();
        for (r, (text, score)) in &reads {
            cells.push(Cell { row, col, kind: CellKind::Badge, rect: **r, text: text.clone(), score: *score });
        }
        // The badge read with digits, else the uncertain one
        let parsed: Vec<((u32, bool), &String, f32)> = reads
            .iter()
            .filter(|(_, (_, sc))| *sc >= MIN_SCORE)
            .filter_map(|(_, (t, sc))| parse_badge(t).map(|p| (p, t, *sc)))
            .collect();
        let count = match parsed.iter().find(|(p, _, _)| !p.1).or(parsed.first()) {
            Some((p, t, score)) => Field::new(Some(p.0), t.as_str(), *score, p.1),
            None => Field::new(
                None,
                reads.iter().map(|(_, (t, _))| t.as_str()).collect::<Vec<_>>().join(" | "),
                reads.iter().map(|(_, (_, sc))| *sc).fold(0.0, f32::max),
                false,
            ),
        };
        let quality = match &quality_texts[i] {
            Some((text, score)) => {
                Field::new(qualities[i].map(|q| q.0), text.as_str(), *score, qualities[i].is_some_and(|q| q.1))
            }
            None => Field::none(),
        };
        tiles.push(Tile {
            rect,
            col,
            row,
            hovered: hovered == Some(i),
            quality,
            count,
            badge_covered: badge_hidden(i),
        });
    }
    let tooltip = tooltip.map(|(t, _)| {
        let mut name = Name::new(t.name, frame.locale);
        name.loc_keys.sort();
        Tooltip { name, name_rect: t.title, quality: t.quality, scu: t.scu, rect: t.rect }
    });
    Ok((Warehouse { tiles, tooltip }, WarehouseDiagnostics { boxes, cells, layout: Some(layout) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_badges_light_box() {
        // A dark tile with a frame line on top and a light 30×20 box at (20, 10) holding dark text
        let mut frame = RgbImage::from_pixel(80, 50, image::Rgb([30, 40, 40]));
        for x in 0..80 {
            frame.put_pixel(x, 2, image::Rgb([120, 170, 160]));
        }
        for (x, y) in (20..50).flat_map(|x| (10..30).map(move |y| (x, y))) {
            let text = (26..44).contains(&x) && (14..26).contains(&y) && x % 3 == 0;
            frame.put_pixel(x, y, if text { image::Rgb([30, 40, 40]) } else { image::Rgb([140, 200, 190]) });
        }
        let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect { x0, y0, x1, y1 };
        assert_eq!(badge_box(&frame, r(0.0, 0.0, 80.0, 50.0)), Some(r(20.0, 10.0, 50.0, 30.0)));
        // Cut by the area's edge, or no contrast: none
        assert_eq!(badge_box(&frame, r(30.0, 0.0, 80.0, 50.0)), None);
        assert_eq!(badge_box(&frame, r(55.0, 10.0, 80.0, 50.0)), None);
    }

    #[test]
    fn badges() {
        assert_eq!(parse_badge("X1"), Some((1, false)));
        assert_eq!(parse_badge("X 14"), Some((14, false)));
        assert_eq!(parse_badge("×1"), Some((1, false)));
        assert_eq!(parse_badge("XI"), Some((1, false)));
        assert_eq!(parse_badge("X51"), Some((51, false)));
        assert_eq!(parse_badge("X1D"), Some((10, false)));
        assert_eq!(parse_badge("X1O"), Some((10, false)));
        assert_eq!(parse_badge("X"), Some((1, true)));
        assert_eq!(parse_badge("Xa"), None);
        assert_eq!(parse_badge("EXPAND"), None);
        assert_eq!(parse_badge("4x"), None);
    }

    fn tb(x0: f32, y0: f32, w: f32, h: f32, text: &str) -> TextBox {
        TextBox { rect: Rect { x0, y0, x1: x0 + w, y1: y0 + h }, text: text.into(), score: 1.0 }
    }

    /// The fit over text boxes as the layout detector would have sorted them: badges and the tiles' quality texts
    /// (with their Q; a bare number is the tooltip chip's) as anchors.
    fn layout_from_boxes(boxes: &[TextBox], frame_w: f32, frame_h: f32, last: Option<&Layout>) -> Option<Layout> {
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
    fn live_boxes() -> Vec<TextBox> {
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

    #[test]
    fn tooltip_under_the_cursor_names_the_hovered_tile() {
        let boxes = live_boxes();
        let l = layout_from_boxes(&boxes, 3840.0, 1600.0, None).unwrap();
        let tiles: Vec<Rect> =
            (0..l.rows.len()).flat_map(|r| (0..4).map(move |c| (r, c))).map(|(r, c)| l.tile_rect(r, c)).collect();
        // The detector's title "Hephaestanite" and the tooltip's other lines
        let (t, hovered) = tooltip_from_boxes(&boxes[14..15], &boxes[15..], &tiles, 1600.0).expect("tooltip");
        assert_eq!(
            (t.name.as_str(), t.quality.value, t.scu.value.as_deref()),
            ("Hephaestanite", Some(572), Some("0.263"))
        );
        assert_eq!((t.quality.raw.as_str(), t.scu.raw.as_str()), ("572", "0.263 SCU"));
        assert_eq!((t.title.x0, t.title.y0), (2786.0, 930.0));
        // The title at (2786, 930): the cursor at (2770, 914) is in the row starting near 753, column 2
        let (row, col) = (hovered / 4, hovered % 4);
        assert_eq!(col, 2);
        assert!((l.rows[row] - 753.0).abs() < 15.0, "{}", l.rows[row]);
        assert!(t.rect.x1 >= 3153.0 && t.rect.y1 >= 1020.0);
        // An item's tooltip: the title and a body line
        let title = [tb(2300.0, 820.0, 240.0, 30.0, "RediMake Item Fabricator")];
        let (t, hovered) =
            tooltip_from_boxes(&title, &[tb(2300.0, 860.0, 160.0, 24.0, "Registered To You")], &tiles, 1600.0)
                .expect("tooltip");
        assert_eq!(t.name, "RediMake Item Fabricator");
        assert_eq!((t.quality, t.scu), (Field::none(), Field::none()));
        assert_eq!(hovered % 4, 1);
        // An item's name with an amount in it ("StorAll 2 SCU Self-"), its cursor point in the gap a little above
        // a tile's top (1080p recording of 2026-10-30, read 216: 2 px): still the title, of the tile below
        let title = [tb(l.cols[2] + 40.0, l.rows[row] - 3.0 + 0.01 * 1600.0, 300.0, 30.0, "StorAll 2 SCU Self-")];
        let (t, hovered) = tooltip_from_boxes(&title, &[], &tiles, 1600.0).expect("tooltip");
        assert_eq!((t.name.as_str(), hovered / 4, hovered % 4), ("StorAll 2 SCU Self-", row, 2));
        // No title read: no tooltip, however title-like a line is
        assert!(tooltip_from_boxes(&[], &title, &tiles, 1600.0).is_none());
    }

    #[test]
    fn the_quality_is_the_chips_not_a_tile_behind_the_tooltip() {
        // The recording of 2026-10-19 (read 47): Janalite 269Q hovered, the see-through tooltip over the
        // neighbour's "360Q", which shows in the title's row
        let boxes = [
            tb(1972.0, 917.0, 80.0, 37.0, "269Q"),
            tb(2097.0, 924.0, 108.0, 32.0, "Janalite"),
            tb(2250.0, 926.0, 55.0, 27.0, "360"),
            tb(2520.0, 928.0, 81.0, 37.0, "348Q"),
            tb(2096.0, 961.0, 175.0, 28.0, "Capacity: 1.00mSCU"),
            tb(2109.0, 986.0, 75.0, 25.0, "Janalite"),
            tb(2216.0, 987.0, 43.0, 27.0, "269"),
            tb(2345.0, 990.0, 96.0, 25.0, "0.001 SCU"),
        ];
        let tiles = [Rect { x0: 1960.0, y0: 640.0, x1: 2210.0, y1: 960.0 }];
        // The title and the lines read (the tiles' qualities "269Q", "348Q" are no tooltip lines; the neighbour's
        // "360" shows through the see-through tooltip and is one)
        let lines = [&boxes[2..3], &boxes[4..]].concat();
        let (t, _) = tooltip_from_boxes(&boxes[1..2], &lines, &tiles, 1600.0).expect("tooltip");
        assert_eq!((t.name.as_str(), t.quality.value, t.scu.value.as_deref()), ("Janalite", Some(269), Some("0.001")));
        // The neighbour's quality in the title's own box (tiny, 1080p recording of 2026-10-30: "Glacosite 258")
        let mut title = boxes[1].clone();
        title.text = "Janalite 360".into();
        title.rect.x1 = 2305.0;
        let (t, _) = tooltip_from_boxes(&[title], &boxes[4..], &tiles, 1600.0).expect("tooltip");
        assert_eq!((t.name.as_str(), t.quality.value), ("Janalite", Some(269)));
        // An item's name may end in a number: without the chip's repeat it stays
        let (t, _) = tooltip_from_boxes(
            &[tb(2097.0, 924.0, 108.0, 32.0, "Polaris 2955")],
            &[tb(2096.0, 961.0, 175.0, 28.0, "Registered To You")],
            &tiles,
            1600.0,
        )
        .expect("tooltip");
        assert_eq!((t.name.as_str(), t.quality.value), ("Polaris 2955", None));
    }
}
