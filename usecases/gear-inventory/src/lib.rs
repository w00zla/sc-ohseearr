//! The gear-inventory screen reader: the character inventory ("Storage Access", SC 4.x), the hovered item's
//! tooltip, the storage pane's page and the tile under the cursor.
//!
//! Pattern: calibrated geometry. The screen is a panel around the frame's centre; positions are measured shares
//! of the frame's height, scaled by a factor `k` the page number's place tells. See `read.rs` for the algorithm
//! and `README.md` for the use case card.
//!
//! - `GearInventoryReader` implements `ScreenReader`: `probe` reads the page number (which also tells this screen
//!   from the freight manager's terminal), `read` reads the rest. The reader keeps `k` across frames.
//! - `read_components` reads one frame without state (the regression's reference path).

mod read;

use image::RgbImage;
use sc_ohseearr_core::{Line, Ocr, Rect, TextBox};
use sc_ohseearr_sc::reader::{Diagnostics, Field, Frame, Name, ReadError, Reading, ScreenReader};
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
    diagnostics: bool,
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

impl ScreenReader for GearInventoryReader {
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

    fn set_diagnostics(&mut self, on: bool) {
        self.diagnostics = on;
    }
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
    use std::path::{Path, PathBuf};

    /// The data root: `SC_OHSEEARR_DATA`, else `<repo>/local`.
    fn data_root() -> PathBuf {
        std::env::var_os("SC_OHSEEARR_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local"))
    }

    fn setup(dir: &str) -> Option<(Ocr, PathBuf)> {
        let shots = data_root().join("screenshots").join(dir);
        if !shots.is_dir() {
            eprintln!("{} missing, skipped", shots.display());
            return None;
        }
        let ocr = Ocr::new(
            &sc_ohseearr_model_paddle_det_tiny::package(),
            &sc_ohseearr_model_sc_rec_latin::package(),
            4,
            false,
        )
        .unwrap();
        Some((ocr, shots))
    }

    fn frame(dir: &Path, suffix: &str) -> RgbImage {
        let file = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.to_string_lossy().ends_with(&format!("{suffix}.jpg")))
            .unwrap();
        image::open(file).unwrap().to_rgb8()
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

    // The user's screenshots (local data, not in git) against what the tooltips show; skipped without
    // them. Slow in debug builds: cargo test --release.
    #[test]
    fn tooltips_of_the_inventory_screen() {
        let Some((mut ocr, dir)) = setup("inventory 3") else { return };
        #[allow(clippy::type_complexity)]
        let cases: &[(&str, &str, &[(&str, &str)], u32)] = &[
            ("19-58-50-5AE", "Ardor-3 Salvaged Repeater", &[], 2),
            ("19-58-53-039", "M3A Cannon Defense Division", &[], 2),
            (
                "19-59-02-415",
                "Civ/1/A Atlas",
                &[("Integrity:", "200.00"), ("Quantum Speed:", "259.05 Mm/s"), ("Quantum Fuel Burn:", "7.55 per Km")],
                2,
            ),
            ("19-59-09-0F9", "Mil/3/A Blizzard", &[("Coolant Rating:", "60.00"), ("Integrity:", "1800.00")], 2),
            ("19-59-22-420", "Sth/1/C HeatSafe", &[("Coolant Rating:", "22.00"), ("Integrity:", "110.00")], 2),
            ("19-59-28-865", "Mil/2/B CoverAll", &[("Integrity:", "660.00"), ("Max. Shield Strength:", "10560.00")], 2),
            ("19-59-35-5AD", "Mil/1/B Sovereign IP-10", &[], 2),
            ("19-59-38-F64", "Mil/1/B BriskAir IC-10", &[], 3),
        ];
        for (suffix, title, stats, page) in cases {
            let started = std::time::Instant::now();
            let read = read_components(&mut ocr, &frame(&dir, suffix), None, None).unwrap().output;
            eprintln!(
                "{suffix}: {:?} in {:?}, page {:?}",
                read.tooltip.as_ref().map(|t| &t.title.text),
                started.elapsed(),
                read.page
            );
            let tip = read.tooltip.unwrap_or_else(|| panic!("{suffix}: no tooltip"));
            assert_eq!(norm(&tip.title.text), norm(title), "{suffix}: {}", tip.title.text);
            for (label, value) in *stats {
                assert!(
                    has_stat(&tip, label, value),
                    "{suffix}: {label} {value} not in {:?}",
                    tip.lines.iter().map(|l| &l.text).collect::<Vec<_>>()
                );
            }
            // No stat line reads as one where the tooltip has none
            if stats.is_empty() {
                assert!(!tip.lines.iter().any(|l| norm(&l.text).starts_with("INTEGRITY")), "{suffix}: {:?}", tip.lines);
            }
            assert_eq!(read.page.map(|p| (p.index, p.total)), Some((*page, 3)), "{suffix}");
        }
    }

    #[test]
    fn fps_gear_tooltips_tiles_and_stacks() {
        let Some((mut ocr, dir)) = setup("inventory 4") else { return };
        // Shot, title, stat lines, the last line before a second tooltip, a cursor on the hovered
        // tile (the shots have none: the tile's centre) with its span and stack count
        type Case<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)], &'a str, (f32, f32), u32, Option<u32>);
        let cases: &[Case] = &[
            (
                "15-53-26-8DE",
                "Artimex Arms Canuto",
                &[("Damage Mitigation:", "0.30"), ("Min Temp:", "-61.00"), ("Max Temp:", "+91.00")],
                "Registered To You",
                (2634.0, 467.0),
                1,
                None,
            ),
            (
                "15-54-15-7E3",
                "Gallant Rifle",
                &[("Fire Rate:", "900.00 RPM"), ("Impact Force:", "+0.00 %"), ("Recoil Handling:", "-0.00 %")],
                "Recoil Handling: -0.00 %",
                (2867.0, 612.0),
                2,
                None,
            ),
            (
                "15-55-06-F11",
                "CQ7 Rifle",
                &[
                    ("Fire Rate:", "713.49 RPM"),
                    ("Impact Force:", "+6.11 %"),
                    ("Recoil Kick:", "-28.44 %"),
                    ("Recoil Smoothness:", "+28.44 %"),
                ],
                "Recoil Handling: +28.44 %",
                (0.0, 0.0),
                0,
                None,
            ),
            ("15-54-40-B2A", "Emod Stabilizer3", &[], "", (2788.0, 1219.0), 1, Some(2)),
            ("15-54-54-211", "Veil Flash Hider2", &[], "", (0.0, 0.0), 0, None),
        ];
        for (suffix, title, stats, last, cursor, span, count) in cases {
            let frame = frame(&dir, suffix);
            let read = read_components(&mut ocr, &frame, (*span > 0).then_some(*cursor), None).unwrap().output;
            let tip = read.tooltip.unwrap_or_else(|| panic!("{suffix}: no tooltip"));
            let texts: Vec<&str> = tip.lines.iter().map(|l| l.text.as_str()).collect();
            assert_eq!(norm(&tip.title.text), norm(title), "{suffix}: {}", tip.title.text);
            for (label, value) in *stats {
                assert!(has_stat(&tip, label, value), "{suffix}: {label} {value} not in {texts:?}");
            }
            if !last.is_empty() {
                assert_eq!(texts.last().map(|t| norm(t)), Some(norm(last)), "{suffix}: {texts:?}");
            }
            // Nothing of the tooltip below (the equipped piece, the weapon list)
            assert!(
                !texts.iter().any(|t| t.starts_with("1:") || t.contains("Dust Devil") || t.contains("Sion")),
                "{suffix}: {texts:?}"
            );
            if *span > 0 {
                let cell = read.cell.unwrap_or_else(|| panic!("{suffix}: no tile at {cursor:?}"));
                assert_eq!((cell.span, cell.count.value), (*span, *count), "{suffix}: {cell:?}");
            }
        }
    }

    #[test]
    fn the_reader_keeps_the_scale_and_probe_is_reused() {
        let Some((mut ocr, dir)) = setup("inventory 3") else { return };
        let image = frame(&dir, "19-59-09-0F9");
        let mut reader = GearInventoryReader::new();
        assert_eq!(reader.scale(), None);
        let f = Frame { id: 7, image: &image, cursor: None, locale: None };
        assert!(reader.probe(&mut ocr, &f).unwrap());
        let read = reader.read(&mut ocr, &f).unwrap();
        assert_eq!(read.output.page, Some(Page { index: 2, total: 3 }));
        assert_eq!(read.output.tooltip.as_ref().map(|t| norm(&t.title.text)), Some(norm("Mil/3/A Blizzard")));
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
