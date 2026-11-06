//! The warehouse screen reader: the freight manager's warehouse tab (freight elevator terminal, SC 4.x).
//!
//! A grid of 4 columns of tiles, scrolled without pages. Every tile has a count badge top right ("X1", "X14");
//! a material tile shows its quality bottom left ("572Q", the Q dim). The icons look alike, so the name comes from
//! the tooltip of the hovered tile: its title, for materials also the chip's quality and the stack's SCU.
//!
//! The terminal is a screen in the world, so nothing is fixed on the screen: the grid is fitted per frame from
//! the boxes the warehouse's layout detector finds (`sc-warehouse-layout`), then only the tooltip's lines, the
//! hovered tile's quality text and badge are recognized. Once a frame gave a strong fit, the reader keeps that
//! layout as the prior: the next frames with a cursor read one window around it, and a weaker fit keeps the
//! prior's pitch. `reset` drops the prior (the screen was left: the next one may be seen from elsewhere).
//!
//! How it reads, step by step: `read.rs`.

mod fit;
mod read;

use sc_ohseearr_core::{Error, NanoDet, Ocr, Rect, TextBox, Timings};
use sc_ohseearr_sc::{Diagnostics, Field, Frame, Name, ReadError, Reading, ScreenReader};
use serde::Serialize;

pub use read::parse_badge;

/// The fit's weight (on its weaker axis) from which a layout becomes the prior of the next reads.
pub const STRONG_FIT_WEIGHT: u32 = fit::STRONG_FIT_WEIGHT;

/// What the warehouse shows in a frame. No tiles: no warehouse grid in the frame.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Warehouse {
    /// Every tile of the fitted grid, row by row, left to right (unread fields empty)
    pub tiles: Vec<Tile>,
    /// The hovered tile's tooltip
    pub tooltip: Option<Tooltip>,
}

/// One tile as the frame shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tile {
    /// The whole tile in frame pixels
    pub rect: Rect,
    /// Grid column (0 = leftmost) and row (0 = topmost in this frame; the scroll offset is unknown)
    pub col: u32,
    pub row: u32,
    /// The tile the tooltip belongs to (under the cursor; without a cursor, the tooltip's)
    pub hovered: bool,
    /// The quality text (materials). Read for the hovered tile only, for every tile without a cursor; no value
    /// for an item, a text hidden by the tooltip, cut off or read below the confidence limit. `ambiguous`: the
    /// text may also be another quality ("1000": 100Q or 1000)
    pub quality: Field<u32>,
    /// The count badge. Read for the hovered tile, for every tile with diagnostics on or without a cursor.
    /// `raw`: the read the count came from, else every read of the badge joined by " | ". `ambiguous`: the
    /// badge read "X" without digits, taken as 1
    pub count: Field<u32>,
    /// The tooltip covers the badge, so it was not read
    pub badge_covered: bool,
}

/// The tooltip of the hovered tile.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tooltip {
    /// The title: the item's or material's name (its loc keys when the frame has a locale, sorted)
    pub name: Name,
    /// The title's box in frame pixels
    pub name_rect: Rect,
    /// The chip's quality (materials)
    pub quality: Field<u32>,
    /// The stack's amount in SCU as an exact decimal ("0.263"; "0.001" for one piece of an item material)
    pub scu: Field<String>,
    /// The area its texts cover
    pub rect: Rect,
}

/// What a read did beyond its output.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WarehouseDiagnostics {
    /// The layout detector's anchors as "[badge]" and "[quality]" (with the detector's score), then the tooltip's
    /// titles and lines as read
    pub boxes: Vec<TextBox>,
    /// Every tile part recognized, tile by tile (its quality text, then its badge reads in order)
    pub cells: Vec<Cell>,
    /// The fitted grid; None: no grid in the frame
    pub layout: Option<Layout>,
}

impl Diagnostics for WarehouseDiagnostics {
    fn boxes(&self) -> &[TextBox] {
        &self.boxes
    }
}

/// A tile part recognized.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cell {
    pub row: u32,
    pub col: u32,
    pub kind: CellKind,
    /// The area recognized, in frame pixels
    pub rect: Rect,
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CellKind {
    Badge,
    Quality,
}

/// Where the grid's tiles are in a frame.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layout {
    /// The tiles' left edges, left to right
    pub cols: Vec<f32>,
    /// The tiles' tops, top to bottom
    pub rows: Vec<f32>,
    pub pitch_x: f32,
    pub pitch_y: f32,
    /// The weaker axis' agreeing anchor weight (≥ `STRONG_FIT_WEIGHT`: the pitch is trusted)
    pub weight: u32,
}

impl Layout {
    pub fn tile_rect(&self, row: usize, col: usize) -> Rect {
        let (x0, y0) = (self.cols[col], self.rows[row]);
        Rect { x0, y0, x1: x0 + fit::TILE_SIZE * self.pitch_x, y1: y0 + fit::TILE_SIZE * self.pitch_y }
    }
}

/// The layout detector with its classes as kinds, and the terms it was made on.
struct LayoutModel {
    detector: NanoDet,
    kinds: Vec<read::Kind>,
    threads: usize,
    gpu: bool,
}

impl LayoutModel {
    fn new(threads: usize, gpu: bool) -> Result<Self, Error> {
        let detector = NanoDet::new(&sc_ohseearr_model_sc_warehouse_layout::package(), threads, gpu)?;
        let kinds = detector
            .classes()
            .iter()
            .map(|c| read::Kind::of_class(c).ok_or_else(|| Error::Mismatch(format!("layout model: unknown class {c}"))))
            .collect::<Result<_, _>>()?;
        Ok(Self { detector, kinds, threads, gpu })
    }
}

/// The warehouse reader. It keeps the last strong fit's layout across frames (`reset` drops it) and makes its
/// layout detector on first use, on the terms of the `Ocr` it is given (threads, GPU; made again when they
/// change). Drop the reader to free the detector's memory.
#[derive(Default)]
pub struct WarehouseReader {
    model: Option<LayoutModel>,
    prior: Option<Layout>,
    diagnostics: bool,
    cache: Option<(u64, Reading<Warehouse, WarehouseDiagnostics>)>,
}

impl WarehouseReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// The layout the next read starts from (the last fit with weight ≥ `STRONG_FIT_WEIGHT`).
    pub fn prior(&self) -> Option<&Layout> {
        self.prior.as_ref()
    }

    /// The layout detector's timings since the last call (and starts over); the `Ocr` keeps its own.
    pub fn take_timings(&mut self) -> Timings {
        self.model.as_mut().map(|m| m.detector.take_timings()).unwrap_or_default()
    }

    /// The frame's reading, read once per frame id.
    fn reading(
        &mut self,
        ocr: &mut Ocr,
        frame: &Frame,
    ) -> Result<&Reading<Warehouse, WarehouseDiagnostics>, ReadError> {
        if !self.cache.as_ref().is_some_and(|(id, _)| *id == frame.id) {
            let (threads, gpu) = (ocr.threads(), ocr.gpu());
            if !self.model.as_ref().is_some_and(|m| m.threads == threads && m.gpu == gpu) {
                self.model = Some(LayoutModel::new(threads, gpu)?);
            }
            let model = self.model.as_mut().expect("made above");
            let (output, diagnostics) =
                read::read_grid(ocr, &mut model.detector, &model.kinds, frame, self.prior.as_ref(), self.diagnostics)?;
            // Only a strong fit sets the layout the weaker reads keep
            if let Some(l) = &diagnostics.layout
                && l.weight >= STRONG_FIT_WEIGHT
            {
                self.prior = Some(l.clone());
            }
            self.cache = Some((frame.id, Reading { output, diagnostics }));
        }
        Ok(&self.cache.as_ref().expect("set above").1)
    }
}

impl ScreenReader for WarehouseReader {
    type Output = Warehouse;
    type Diagnostics = WarehouseDiagnostics;

    /// Whether a warehouse grid is fitted in the frame. That takes the whole read: `read` of the same frame id
    /// returns it.
    fn probe(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<bool, ReadError> {
        Ok(self.reading(ocr, frame)?.diagnostics.layout.is_some())
    }

    fn read(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<Reading<Warehouse, WarehouseDiagnostics>, ReadError> {
        self.reading(ocr, frame).cloned()
    }

    fn reset(&mut self) {
        self.prior = None;
        self.cache = None;
    }

    /// On: every tile's badge is read (the costly part), not only the hovered tile's.
    fn set_diagnostics(&mut self, on: bool) {
        if on != self.diagnostics {
            self.diagnostics = on;
            self.cache = None;
        }
    }
}

// The local screenshots (not in git: $SC_OHSEEARR_DATA, else <repo>/local), read without a cursor (the whole pane,
// as the first read of a session); skipped without them. Slow in debug builds: cargo test --release.
#[cfg(test)]
mod screenshots {
    use super::*;
    use sc_ohseearr_sc::norm;
    use sc_ohseearr_test_support::{data_root, frames, test_ocr};
    use std::path::PathBuf;

    fn shots(dir: &str) -> Vec<PathBuf> {
        frames(&data_root().join("screenshots").join(dir))
    }

    #[test]
    fn grids_tooltips_and_hovered_stacks() {
        let (terminal, inventory) = (shots("fe terminal 1"), shots("inventory 2"));
        if terminal.is_empty() || inventory.is_empty() {
            eprintln!("screenshots missing, skipped");
            return;
        }
        let mut ocr = test_ocr();
        let mut reader = WarehouseReader::new();
        // Checked against the shots: the tooltip name's start, its quality chip and SCU, the hovered tile's count.
        // Ship components are not booked from the warehouse, and tiny misreads their names ("Mil/3/A Blizard",
        // "Mil/A VK-00"): only their start is checked
        // (file name end, title start, chip quality, SCU, hovered count)
        type Case<'a> = (&'a str, &'a str, Option<u32>, Option<&'a str>, Option<u32>);
        let cases: &[Case] = &[
            ("17-17-27-58F", "Beradom", Some(578), Some("0.001"), Some(3)),
            ("17-17-35-520", "Feynmaline", Some(561), Some("0.001"), Some(14)),
            ("13-04-05-C45", "Mil", None, None, Some(1)),
            // A ship component's tooltip has no chip (a stray "1" read as one before 2026-10-27)
            ("13-04-10-D57", "Mil", None, None, Some(1)),
        ];
        for (id, path) in terminal.iter().chain(&inventory).enumerate() {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let image = image::open(path).unwrap().to_rgb8();
            // Every shot on its own: no prior
            reader.reset();
            let frame = Frame { id: id as u64, image: &image, cursor: None, locale: None };
            assert!(reader.probe(&mut ocr, &frame).unwrap(), "{name}: no grid");
            let grid = reader.read(&mut ocr, &frame).unwrap().output;
            let Some((_, title, quality, scu, count)) = cases.iter().find(|c| name.ends_with(c.0)) else { continue };
            let tip = grid.tooltip.as_ref().unwrap_or_else(|| panic!("{name}: no tooltip"));
            assert!(norm(&tip.name.text).starts_with(&norm(title)), "{name}: {}", tip.name.text);
            assert_eq!((tip.quality.value, tip.scu.value.as_deref()), (*quality, *scu), "{name}");
            let hovered = grid.tiles.iter().find(|t| t.hovered).unwrap_or_else(|| panic!("{name}: no hovered tile"));
            assert_eq!(hovered.count.value, *count, "{name}");
        }
    }
}
