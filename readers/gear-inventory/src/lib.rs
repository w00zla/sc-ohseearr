//! The gear-inventory reader: the character inventory ("Storage Access", SC 4.x), the hovered item's
//! tooltip, the storage pane's page and the tile under the cursor.
//!
//! Pattern: calibrated geometry. The screen is a panel around the frame's centre; positions are measured shares
//! of the frame's height, scaled by a factor `k` the page number's place tells. See `read.rs` for the algorithm
//! and `README.md` for the reader card.
//!
//! - `GearInventoryReader` implements `Reader`: `probe` reads the page number (which also tells this screen
//!   from the freight manager's terminal), `read` reads the rest. The reader keeps `k` across frames.
//! - `read_components` reads one frame without state (the regression's reference path).

mod read;

use image::RgbImage;
use sc_ohseearr_core::{Line, Ocr, Rect, TextBox};
use sc_ohseearr_sc::reader::{Diagnostics, Field, Frame, Name, ReadError, Reader, Reading};
use sc_ohseearr_sc::{CenterUi, Locale};
use serde::Serialize;

pub use read::parse_page;
use read::{PageRead, RawTooltip, hovered_tile, read_count, read_page, read_tooltip};

/// The storage pane's page ("2/3").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Page {
    pub index: u32,
    pub total: u32,
}

/// The hovered tile of the storage grid: its row in the page's content (0 = the first, also when scrolled out of
/// view), its first unit (0–3) and how many it spans, its area in frame pixels, and the stack's count where the
/// tile shows one ("2x"; no value: a single piece, or the count was not read).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cell {
    pub row: u32,
    pub col: u32,
    pub span: u32,
    pub rect: Rect,
    pub count: Field<u32>,
}

/// The hovered item's tooltip.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tooltip {
    /// The first line, with the loc keys whose text reads like it (when the frame had a locale)
    pub title: Name,
    /// Every line below it, top to bottom
    pub lines: Vec<Line>,
    /// The area its texts cover (frame pixels)
    pub rect: Rect,
}

/// What one frame of the inventory screen shows. Nothing of it on a frame without the screen.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GearInventory {
    pub page: Option<Page>,
    /// The tile under the mouse cursor (None: no cursor, outside the grid, or no lit tile there)
    pub cell: Option<Cell>,
    pub tooltip: Option<Tooltip>,
}

/// What a read needed to know and saw besides its output.
#[derive(Debug, Clone, Serialize)]
pub struct GearDiagnostics {
    /// Every text box the read areas gave, frame pixels: the page number's area, the tooltip's, the count's
    pub boxes: Vec<TextBox>,
    /// The UI scale `k` the read used: the page number's place, else the one before, else 1.0 (no frame read: 1.0)
    pub scale: f32,
}

impl Default for GearDiagnostics {
    fn default() -> Self {
        Self { boxes: Vec::new(), scale: 1.0 }
    }
}

impl Diagnostics for GearDiagnostics {
    fn boxes(&self) -> &[TextBox] {
        &self.boxes
    }
}

/// The probe's page read, kept for `read` of the same frame.
struct Probed {
    id: u64,
    page: PageRead,
    boxes: Vec<TextBox>,
}

/// The reader of the character inventory. Keeps the UI scale of the last frame that showed the page number.
#[derive(Default)]
pub struct GearInventoryReader {
    k: Option<f32>,
    probed: Option<Probed>,
}

impl GearInventoryReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// The UI scale kept from the last frame that showed the page number.
    pub fn scale(&self) -> Option<f32> {
        self.k
    }

    /// `probe`, with the choice of the scale hint: `hinted` reads the page number only around where the kept scale
    /// puts it (cheap, ~30 ms; without a kept scale it searches), else always over every place it can have (the app
    /// does this while it does not know the screen is shown). The result is kept for `read` of the same frame id.
    pub fn probe_with(&mut self, ocr: &mut Ocr, frame: &Frame, hinted: bool) -> Result<bool, ReadError> {
        let (page, boxes) = read_page(ocr, frame.image, if hinted { self.k } else { None })?;
        let found = page.is_some();
        self.probed = Some(Probed { id: frame.id, page, boxes });
        Ok(found)
    }
}

impl Reader for GearInventoryReader {
    type Output = GearInventory;
    type Diagnostics = GearDiagnostics;

    /// Whether the storage pane's page number is in the frame.
    fn probe(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<bool, ReadError> {
        self.probe_with(ocr, frame, true)
    }

    /// The page, the tile under the cursor, its tooltip and its stack count. With a cursor that is on no lit tile
    /// the tooltip is not read (it cannot belong to a tile). The kept scale is updated when the page was read.
    fn read(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<Reading<GearInventory, GearDiagnostics>, ReadError> {
        let (page, boxes) = match self.probed.take().filter(|p| p.id == frame.id) {
            Some(p) => (p.page, p.boxes),
            None => read_page(ocr, frame.image, self.k)?,
        };
        let reading = rest(ocr, frame.image, frame.cursor, page, self.k, true, frame.locale, boxes)?;
        if page.is_some() {
            self.k = Some(reading.diagnostics.scale);
        }
        Ok(reading)
    }

    fn reset(&mut self) {
        self.k = None;
        self.probed = None;
    }

    /// A no-op: the diagnostics (the read boxes, the scale) cost nothing extra and are always complete.
    fn set_diagnostics(&mut self, _on: bool) {}
}

/// Reads one frame of the inventory screen without state, as the companion's `read_components` did: the page
/// number over every place it can have, the scale it tells (else 1.0), the tooltip always (also without a cursor
/// or a tile), the tile under `cursor` and with a tooltip its count.
pub fn read_components(
    ocr: &mut Ocr,
    image: &RgbImage,
    cursor: Option<(f32, f32)>,
    locale: Option<&Locale>,
) -> Result<Reading<GearInventory, GearDiagnostics>, ReadError> {
    let (page, boxes) = read_page(ocr, image, None)?;
    rest(ocr, image, cursor, page, None, false, locale, boxes)
}

/// The rest of a read once the page number was read: the scale from the page's place (else `last`, else the
/// measured place), the tile under the cursor, the tooltip (`need_tile`: not without a tile when there is a
/// cursor) and with a tooltip the tile's stack count. `boxes` = the page's.
#[allow(clippy::too_many_arguments)]
fn rest(
    ocr: &mut Ocr,
    image: &RgbImage,
    cursor: Option<(f32, f32)>,
    page: PageRead,
    last: Option<f32>,
    need_tile: bool,
    locale: Option<&Locale>,
    mut boxes: Vec<TextBox>,
) -> Result<Reading<GearInventory, GearDiagnostics>, ReadError> {
    let k = page.map(|p| p.1).or(last).unwrap_or(1.0);
    let ui = CenterUi::new(image, k);
    let mut cell = cursor.and_then(|c| hovered_tile(image, ui, c));
    let tooltip: Option<RawTooltip> = if need_tile && cell.is_none() && cursor.is_some() {
        None
    } else {
        let (tooltip, tip_boxes) = read_tooltip(ocr, image, ui)?;
        boxes.extend(tip_boxes);
        tooltip
    };
    if let (Some(c), Some(_)) = (cell.as_mut(), &tooltip) {
        let (count, count_boxes) = read_count(ocr, image, c.rect)?;
        c.count = count;
        boxes.extend(count_boxes);
    }
    let tooltip = tooltip.map(|t| Tooltip { title: Name::new(t.title, locale), lines: t.lines, rect: t.rect });
    Ok(Reading {
        output: GearInventory { page: page.map(|p| p.0), cell, tooltip },
        diagnostics: GearDiagnostics { boxes, scale: k },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc_ohseearr_sc::norm;
    use sc_ohseearr_test_support::{fixture, test_ocr};

    /// A frame of `fixtures/gear-inventory` and its recorded cursor
    fn frame(name: &str) -> (RgbImage, Option<(f32, f32)>) {
        fixture("gear-inventory", name)
    }

    /// Whether a line reads as `label` followed by `value` (letters and digits compared)
    fn has_stat(tip: &Tooltip, label: &str, value: &str) -> bool {
        let want = norm(&format!("{label}{value}"));
        tip.lines.iter().any(|l| norm(&l.text) == want || norm(&l.text).starts_with(&want))
    }

    #[test]
    fn no_screen_no_output() {
        let out = GearInventory::default();
        assert_eq!(out, GearInventory { page: None, cell: None, tooltip: None });
        assert_eq!(GearDiagnostics::default().scale, 1.0);
        assert!(GearDiagnostics::default().boxes().is_empty());
    }

    // Frames of debug recordings (the submodule fixtures/, ADR 0021) against what the tooltips show, checked by hand.
    // Slow in debug builds: cargo test --release.
    #[test]
    fn tooltips_pages_and_hovered_tiles() {
        let mut ocr = test_ocr();
        // Frame, title, stat lines, the last line (empty: not checked), page, the hovered tile's span
        type Case<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)], &'a str, (u32, u32), u32);
        let cases: &[Case] = &[
            // 3840×1600
            ("1793393861-read-5", "P8-AR \"CitizenCon 2954\" Rifle", &[], "Registered To You", (1, 5), 2),
            (
                "1793393861-read-7",
                "Karna \"Alpine Sunset\" Rifle",
                &[
                    ("Fire Rate:", "600.00 RPM"),
                    ("Impact Force:", "+0.00 %"),
                    ("Recoil Kick:", "+0.00 %"),
                    ("Recoil Smoothness:", "-0.00 %"),
                    ("Recoil Handling:", "-0.00 %"),
                ],
                "",
                (1, 5),
                2,
            ),
            // 1920×1080
            (
                "1793454173-read-164",
                "Mil/3/A TS-2",
                &[
                    ("Integrity:", "2300.00"),
                    ("Quantum Speed:", "509.72 Mm/s"),
                    ("Quantum Fuel Burn:", "141.68 per Km"),
                ],
                "",
                (1, 5),
                1,
            ),
            ("1793454173-read-273", "M3A Cannon Defense Division", &[], "", (3, 5), 2),
            (
                "1793454173-read-299",
                "Sth/1/C HeatSafe",
                &[("Coolant Rating:", "22.00"), ("Integrity:", "110.00")],
                "",
                (4, 5),
                1,
            ),
            (
                "1793454173-read-340",
                "Brandt Module",
                &[("Shatter Damage:", "-30%"), ("Mining Laser Power:", "135%"), ("Resistance:", "+15%")],
                "",
                (4, 5),
                1,
            ),
        ];
        for (name, title, stats, last, page, span) in cases {
            let (image, cursor) = frame(name);
            let read = read_components(&mut ocr, &image, cursor, None).unwrap().output;
            let tip = read.tooltip.as_ref().unwrap_or_else(|| panic!("{name}: no tooltip"));
            let texts: Vec<&str> = tip.lines.iter().map(|l| l.text.as_str()).collect();
            assert_eq!(norm(&tip.title.text), norm(title), "{name}: {}", tip.title.text);
            for (label, value) in *stats {
                assert!(has_stat(tip, label, value), "{name}: {label} {value} not in {texts:?}");
            }
            // No stat line reads as one where the tooltip has none
            if stats.is_empty() {
                assert!(!texts.iter().any(|t| norm(t).starts_with("INTEGRITY")), "{name}: {texts:?}");
            }
            if !last.is_empty() {
                assert_eq!(texts.last().map(|t| norm(t)), Some(norm(last)), "{name}: {texts:?}");
            }
            assert_eq!(read.page.map(|p| (p.index, p.total)), Some(*page), "{name}");
            assert_eq!(read.cell.map(|c| c.span), Some(*span), "{name}: no tile at {cursor:?}");
        }
    }

    #[test]
    fn the_reader_keeps_the_scale_and_probe_is_reused() {
        let mut ocr = test_ocr();
        let (image, _) = frame("1793454173-read-299");
        let mut reader = GearInventoryReader::new();
        assert_eq!(reader.scale(), None);
        let f = Frame { id: 7, image: &image, cursor: None, locale: None };
        assert!(reader.probe(&mut ocr, &f).unwrap());
        let read = reader.read(&mut ocr, &f).unwrap();
        assert_eq!(read.output.page, Some(Page { index: 4, total: 5 }));
        assert_eq!(read.output.tooltip.as_ref().map(|t| norm(&t.title.text)), Some(norm("Sth/1/C HeatSafe")));
        let k = reader.scale().expect("scale kept");
        assert_eq!(read.diagnostics.scale, k);
        // Same result as the stateless read of the same frame
        let plain = read_components(&mut ocr, &image, None, None).unwrap();
        assert_eq!(plain.output, read.output);
        assert_eq!(plain.diagnostics.boxes.len(), read.diagnostics.boxes.len());
        reader.reset();
        assert_eq!(reader.scale(), None);
    }
}
