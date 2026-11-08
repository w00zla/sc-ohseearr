//! The refinery terminal's reader: the station's name from the terminal's title and every work order shown
//! side by side (each a panel with its own table, the newest not necessarily first), running ("Processing":
//! refined materials with quality and yield in cSCU, time remaining) or finished ("Completed": quality and
//! yield). With more work orders than fit, the terminal scrolls them sideways: a frame shows only some, a panel
//! cut by the edge is left out.
//!
//! Resolution independent: frames are scaled by their height (the game's UI scales with it, on 16:9 as on
//! ultrawide). A panel is found by its texts (table head, header, time label), the columns by the table head's
//! words, the rows by the material names below it. The texts come from the frame's `Locale` by loc key (`KEYS`;
//! the language pack's and the English table's, whichever it has); a cell the full-frame pass missed (single
//! digits are small) is read again on its own. Names stay as the game shows them, with the loc keys whose text
//! reads like them.
//!
//! Layout as of SC 4.x; anything unexpected yields no panel or empty fields, never a panic.

mod labels;
mod layout;

use sc_ohseearr_core::{FilterType, Ocr, Rect, TextBox, crop, resize};
use sc_ohseearr_sc::parse::{duration, number};
use sc_ohseearr_sc::reader::{Diagnostics, Field, Frame, Name, ReadError, Reader, Reading};
use sc_ohseearr_sc::{LocaleError, norm};
use serde::Serialize;

use labels::LabelCache;
pub use labels::{KEYS, Labels};
use layout::{Layout, cell, layouts};

/// Height the frame is scaled to for the full pass (3840×1600: half)
const READ_HEIGHT: u32 = 800;
/// Height for the cheap check whether a refinery terminal is in view: only its big titles are read
const CHECK_HEIGHT: u32 = 320;
/// Width / height of a box that may hold the table head "MATERIALS YIELDED (CSCU)" (about 12 as one box, about 7
/// when split before the parenthesis)
const HEAD_RATIO: f32 = 5.0;
/// A value cell the full pass missed is upscaled by this before it is read on its own
const CELL_SCALE: u32 = 3;
/// The "Work Order Accepted" message box around its text, in shares of the text's box (x0, y0, x1, y1): the box
/// measured ~1.65× the text's width and ~7.5× its height (3840×1600 and 1920×1080), with a margin
const ACCEPTED_BOX: (f32, f32, f32, f32) = (-0.45, -4.5, 1.45, 5.5);

// ---------------------------------------------------------------------------------------------------------------
// Output

/// What a full read of a frame found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Screen {
    /// The work order overview
    Overview,
    /// A refinery terminal, but no work order panel (its start page, a quote, a message over it)
    Terminal,
    /// No refinery terminal
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Processing,
    Completed,
}

/// The work orders of one terminal as one frame shows them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Refinery {
    pub screen: Screen,
    /// The terminal's title, e.g. "CRU-L1 AMBITIOUS DREAM STATION" (the same for every panel: the leftmost's)
    pub station: Option<Name>,
    /// Left to right
    pub orders: Vec<WorkOrder>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkOrder {
    pub status: Status,
    /// Processing: the time remaining, in seconds
    pub time_remaining: Field<u32>,
    pub rows: Vec<Row>,
    /// The "Work Order Accepted" message lies over the panel (for a few seconds after an order was accepted):
    /// rows may be missing or misread
    pub covered: bool,
}

/// One table row; `None` values where nothing could be read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// The refined material as the game shows it
    pub material: Name,
    /// 0–1000
    pub quality: Field<u32>,
    /// In cSCU
    pub yield_cscu: Field<u32>,
}

// ---------------------------------------------------------------------------------------------------------------
// Diagnostics

/// Where the reader found a work order panel's parts.
#[derive(Debug, Clone, Serialize)]
pub struct PanelDiagnostics {
    /// The value columns in table order (processing: quality, yield, to do, done; completed: quality, yield):
    /// the head's words (normalized) and their centers
    pub columns: Vec<(String, f32)>,
    /// The name boxes of the rows, top to bottom (those without a quality too)
    pub row_boxes: Vec<Rect>,
    /// Names of rows left out: no quality read (a stray text, a message over the panel)
    pub dropped_rows: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RefineryDiagnostics {
    /// The text boxes read, top to bottom: only the wide ones when no table head was among them
    pub boxes: Vec<TextBox>,
    /// One per work order, as `Refinery::orders` (filled only after `set_diagnostics(true)`)
    pub panels: Vec<PanelDiagnostics>,
}

impl Diagnostics for RefineryDiagnostics {
    fn boxes(&self) -> &[TextBox] {
        &self.boxes
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The reader

/// The refinery terminal's reader. Keeps the labels it built from the locale, nothing about earlier frames.
#[derive(Default)]
pub struct RefineryReader {
    labels: LabelCache,
    diagnostics: bool,
}

impl RefineryReader {
    pub fn new() -> Self {
        Self::default()
    }

    fn labels(&mut self, frame: &Frame) -> Result<&Labels, ReadError> {
        let locale = frame.locale.ok_or(LocaleError::NoLanguageTable)?;
        Ok(self.labels.get(locale))
    }
}

/// A read that found no work order: the screen and the boxes read.
fn bare(screen: Screen, boxes: Vec<TextBox>) -> Reading<Refinery, RefineryDiagnostics> {
    Reading {
        output: Refinery { screen, station: None, orders: Vec::new() },
        diagnostics: RefineryDiagnostics { boxes, panels: Vec::new() },
    }
}

impl Reader for RefineryReader {
    type Output = Refinery;
    type Diagnostics = RefineryDiagnostics;

    /// Whether a refinery terminal is likely in view: its big titles ("Refinement Center", a work order's
    /// "Processing"/"Completed") read at a low resolution. Cheap (~30–110 ms at two threads on 3840×1600); a
    /// false hit only costs a full read.
    fn probe(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<bool, ReadError> {
        let labels = self.labels(frame)?;
        let boxes = ocr.read(frame.image, CHECK_HEIGHT)?;
        Ok(boxes.iter().any(|b| is_title(&b.text, labels)))
    }

    /// Reads the work order overview in a frame. Only the wide boxes are read first: without a table head among
    /// them (most frames) the rest is not read at all.
    fn read(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<Reading<Refinery, RefineryDiagnostics>, ReadError> {
        let with_panels = self.diagnostics;
        let labels = self.labels(frame)?;
        let image = frame.image;
        let rects = ocr.detect(image, READ_HEIGHT)?;
        let (wide, rest): (Vec<Rect>, Vec<Rect>) = rects.into_iter().partition(|r| r.w() >= r.h() * HEAD_RATIO);
        let mut boxes = TextBox::from_reads(wide.clone(), ocr.recognize_all(image, &wide)?);
        let terminal = boxes.iter().any(|b| is_title(&b.text, labels) || is_head(&b.text, labels));
        if !boxes.iter().any(|b| is_head(&b.text, labels)) {
            return Ok(bare(if terminal { Screen::Terminal } else { Screen::Nothing }, boxes));
        }
        boxes.extend(TextBox::from_reads(rest.clone(), ocr.recognize_all(image, &rest)?));
        boxes.sort_by(|a, b| a.rect.y0.total_cmp(&b.rect.y0).then(a.rect.x0.total_cmp(&b.rect.x0)));
        let found = layouts(&boxes, labels);
        if found.is_empty() {
            return Ok(bare(Screen::Terminal, boxes));
        }
        // The title is the same for every panel: the leftmost's
        let station = found[0].station.as_deref().map(|s| Name::new(s, frame.locale));
        let accepted = accepted_boxes(&boxes, labels);
        let mut orders = Vec::new();
        let mut panels = Vec::new();
        for l in found {
            let covered = accepted.iter().any(|a| overlaps(*a, l.area));
            let (mut order, panel) = read_order(ocr, frame, &boxes, l)?;
            order.covered = covered;
            orders.push(order);
            if with_panels {
                panels.push(panel);
            }
        }
        Ok(Reading {
            output: Refinery { screen: Screen::Overview, station, orders },
            diagnostics: RefineryDiagnostics { boxes, panels },
        })
    }

    fn reset(&mut self) {
        self.labels.clear();
    }

    fn set_diagnostics(&mut self, on: bool) {
        self.diagnostics = on;
    }
}

fn is_title(text: &str, labels: &Labels) -> bool {
    let t = norm(text);
    labels.center.iter().any(|c| t.contains(c.as_str()))
        || labels.processing.contains(&t)
        || labels.completed.contains(&t)
}

fn is_head(text: &str, labels: &Labels) -> bool {
    let t = norm(text);
    labels.head.iter().any(|h| t.starts_with(h.as_str()))
}

/// Where a "Work Order Accepted" message lies: the message box around each of its texts.
fn accepted_boxes(boxes: &[TextBox], labels: &Labels) -> Vec<Rect> {
    boxes.iter().filter(|b| labels.accepted.contains(&norm(&b.text))).map(|b| b.rect.part(ACCEPTED_BOX)).collect()
}

/// Whether two boxes overlap (`b` may reach to `f32::MAX`).
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}

fn read_order(
    ocr: &mut Ocr,
    frame: &Frame,
    boxes: &[TextBox],
    l: Layout,
) -> Result<(WorkOrder, PanelDiagnostics), ReadError> {
    let mut rows = Vec::new();
    let mut dropped_rows = Vec::new();
    // Quality and yield are the first two columns of both tables
    for (rect, name) in l.rows.iter().zip(&l.names) {
        let mut values = Vec::new();
        for i in 0..2 {
            let c = cell(*rect, &l.columns, i);
            // The full pass's number in this cell, else the cell read on its own: upscaled and as digits only (a
            // lone thin "1" went missing or read as a letter)
            let found = boxes
                .iter()
                .find(|b| b.rect.cx() > c.x0 && b.rect.cx() < c.x1 && b.rect.cy() > c.y0 && b.rect.cy() < c.y1);
            let from_box =
                found.and_then(|b| number(&b.text).map(|v| Field::new(Some(v), b.text.clone(), b.score, false)));
            let field = match from_box {
                Some(f) => f,
                None => {
                    let piece = crop(frame.image, c);
                    let big =
                        resize(&piece, piece.width() * CELL_SCALE, piece.height() * CELL_SCALE, FilterType::CatmullRom);
                    match ocr.recognize_images_only(&[big], "0123456789")?.pop() {
                        Some((text, score)) => Field::new(number(&text), text, score, false),
                        None => Field::none(),
                    }
                }
            };
            values.push(field);
        }
        // A row always shows its quality: without one it is a stray text (a message over the panel, "YIELD" half
        // hidden)
        if values[0].value.is_none() {
            dropped_rows.push(name.clone());
            continue;
        }
        let yield_cscu = values.pop().expect("two values");
        let quality = values.pop().expect("two values");
        rows.push(Row { material: Name::new(name.as_str(), frame.locale), quality, yield_cscu });
    }
    let time_remaining = match &l.time {
        Some(b) => Field::new(duration(&b.text), b.text.clone(), b.score, false),
        None => Field::none(),
    };
    let panel = PanelDiagnostics {
        columns: l.columns.iter().map(|c| (c.key.clone(), c.cx)).collect(),
        row_boxes: l.rows.clone(),
        dropped_rows,
    };
    Ok((WorkOrder { status: l.status, time_remaining, rows, covered: false }, panel))
}

// ---------------------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Column, find_columns, head_words};
    use image::RgbImage;
    use sc_ohseearr_sc::{LocTable, Locale};

    /// The 10 keys with the English texts (test data standing in for the English table).
    const ENGLISH_INI: &str = "\
refinery_ui_jobcard_table_materialsyielded=Materials Yielded (cSCU)
refinery_ui_workordercard_title_processing=Processing
refinery_ui_workordercard_title_completed=Completed
refinery_ui_remainingtime=Time Remaining
ui_quality=Quality
refinery_ui_jobcard_table_yield=Yield
refinery_ui_jobcard_table_todo=To Do
refinery_ui_jobcard_table_done=Done
refinery_ui_refinementcenter=Refinement Center
refinery_ui_Notification_JobCreated=Work Order Accepted
";

    /// A language pack with some of the keys translated.
    const GERMAN_INI: &str = "\
refinery_ui_jobcard_table_materialsyielded=Gewonnene Materialien (cSCU)
refinery_ui_jobcard_table_yield=Ertrag
refinery_ui_jobcard_table_todo=To-Do
refinery_ui_jobcard_table_done=Fertig
refinery_ui_workordercard_title_completed=Completed
";

    fn english() -> Locale {
        Locale::new(None, Some(LocTable::parse(ENGLISH_INI))).unwrap()
    }

    fn german() -> Locale {
        Locale::new(Some(LocTable::parse(GERMAN_INI)), Some(LocTable::parse(ENGLISH_INI))).unwrap()
    }

    #[test]
    fn merged_head_words_get_their_share() {
        let b =
            TextBox { rect: Rect { x0: 0.0, y0: 0.0, x1: 130.0, y1: 10.0 }, text: "QUALITY YIELD".into(), score: 1.0 };
        let w = head_words(&[&b]);
        assert_eq!(w.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), ["QUALITY", "YIELD"]);
        assert!((w[0].cx - 35.0).abs() < 1.0 && (w[1].cx - 105.0).abs() < 1.0);
    }

    #[test]
    fn labels_from_the_locale_pack_first_without_duplicates() {
        let l = Labels::from_locale(&german());
        assert_eq!(l.head, ["GEWONNENEMATERIALIEN", "MATERIALSYIELDED"]);
        assert_eq!(l.todo, ["TODO"]);
        assert_eq!(l.completed, ["COMPLETED"]);
        assert_eq!(l.time_remaining, ["TIMEREMAINING"]);
        assert_eq!(l.yield_, ["ERTRAG", "YIELD"]);
        let l = Labels::from_locale(&english());
        assert_eq!(l.head, ["MATERIALSYIELDED"]);
        assert_eq!(l.center, ["REFINEMENTCENTER"]);
        // Only a pack: only its texts
        let only_pack = Locale::new(Some(LocTable::parse(GERMAN_INI)), None).unwrap();
        assert!(Labels::from_locale(&only_pack).quality.is_empty());
    }

    #[test]
    fn the_accepted_message_covers_the_panels_under_it() {
        let l = Labels::from_locale(&english());
        assert_eq!(l.accepted, ["WORKORDERACCEPTED"]);
        let text = |x0: f32, text: &str| TextBox {
            rect: Rect { x0, y0: 500.0, x1: x0 + 400.0, y1: 520.0 },
            text: text.into(),
            score: 1.0,
        };
        let found = accepted_boxes(&[text(1000.0, "WORK ORDER ACCEPTED"), text(0.0, "WORK ORDER COMPLETE")], &l);
        // The message box: 820–1580 wide, 410–610 high
        assert_eq!(found.len(), 1);
        let panel = |x0: f32, x1: f32| Rect { x0, y0: 300.0, x1, y1: f32::MAX };
        assert!(overlaps(found[0], panel(900.0, 1100.0)));
        assert!(overlaps(found[0], panel(1300.0, 1500.0)));
        assert!(overlaps(found[0], panel(500.0, 830.0)));
        assert!(!overlaps(found[0], panel(1600.0, 1900.0)));
        assert!(!overlaps(found[0], Rect { x0: 900.0, y0: 0.0, x1: 1100.0, y1: 400.0 }));
    }

    #[test]
    fn label_cache_follows_the_locale() {
        let mut cache = LabelCache::default();
        let (en, de) = (english(), german());
        assert_eq!(cache.get(&en).yield_, ["YIELD"]);
        assert_eq!(cache.get(&en).yield_, ["YIELD"]);
        assert_eq!(cache.get(&de).yield_, ["ERTRAG", "YIELD"]);
    }

    #[test]
    fn no_locale_is_an_error() {
        let image = RgbImage::new(4, 4);
        let frame = Frame { id: 0, image: &image, cursor: None, locale: None };
        let mut reader = RefineryReader::new();
        assert_eq!(reader.labels(&frame).unwrap_err(), ReadError::Locale(LocaleError::NoLanguageTable));
        let en = english();
        let frame = Frame { locale: Some(&en), ..frame };
        assert!(reader.labels(&frame).is_ok());
    }

    #[test]
    fn columns_of_one_or_more_words_in_any_language() {
        let words = |keys: &[(&str, f32)]| {
            keys.iter().map(|(k, cx)| Column { key: k.to_string(), cx: *cx }).collect::<Vec<_>>()
        };
        let l = Labels::from_locale(&english());
        let found = find_columns(
            &words(&[("QUALITY", 10.0), ("YIELD", 20.0), ("TO", 29.0), ("DO", 31.0), ("DONE", 40.0)]),
            &[&l.quality, &l.yield_, &l.todo, &l.done],
        )
        .unwrap();
        assert_eq!(
            found.iter().map(|c| (c.key.as_str(), c.cx)).collect::<Vec<_>>(),
            [("QUALITY", 10.0), ("YIELD", 20.0), ("TODO", 30.0), ("DONE", 40.0)]
        );
        let l = Labels::from_locale(&german());
        let found = find_columns(
            &words(&[("QUALITY", 10.0), ("ERTRAG", 20.0), ("TODO", 30.0), ("FERTIG", 40.0)]),
            &[&l.quality, &l.yield_, &l.todo, &l.done],
        );
        assert_eq!(found.map(|f| f.len()), Some(4));
        // A column missing: no table
        assert!(
            find_columns(&words(&[("QUALITY", 10.0), ("YIELD", 20.0)]), &[&l.quality, &l.yield_, &l.todo]).is_none()
        );
    }

    // Frames of debug recordings (the submodule fixtures/, ADR 0021) against what the terminal shows, checked by hand.
    // Slow in debug builds: cargo test --release.
    mod frames {
        use super::*;
        use sc_ohseearr_test_support::{fixture, test_ocr};

        const STATION: &str = "CRU-L1 AMBITIOUS DREAM STATION";

        fn read(ocr: &mut Ocr, name: &str) -> Refinery {
            let en = english();
            let (image, _) = fixture("refinery", name);
            let f = Frame { id: 0, image: &image, cursor: None, locale: Some(&en) };
            RefineryReader::new().read(ocr, &f).unwrap().output
        }

        fn overview(ocr: &mut Ocr, name: &str) -> Refinery {
            let r = read(ocr, name);
            assert_eq!(r.screen, Screen::Overview, "{name}");
            assert_eq!(r.station.as_ref().map(|s| s.text.as_str()), Some(STATION), "{name}");
            r
        }

        fn probe(ocr: &mut Ocr, name: &str) -> bool {
            let en = english();
            let (image, _) = fixture("refinery", name);
            let f = Frame { id: 0, image: &image, cursor: None, locale: Some(&en) };
            RefineryReader::new().probe(ocr, &f).unwrap()
        }

        type Rows<'a> = Vec<(&'a str, Option<u32>, Option<u32>)>;

        fn rows(o: &WorkOrder) -> Rows<'_> {
            o.rows.iter().map(|r| (r.material.text.as_str(), r.quality.value, r.yield_cscu.value)).collect()
        }

        /// (status, time remaining in seconds, rows) per work order
        fn orders(r: &Refinery) -> Vec<(Status, Option<u32>, Rows<'_>)> {
            r.orders.iter().map(|o| (o.status, o.time_remaining.value, rows(o))).collect()
        }

        const DONE_1: [(&str, Option<u32>, Option<u32>); 4] = [
            ("QUARTZ", Some(330), Some(264)),
            ("QUARTZ", Some(641), Some(35)),
            ("HEPHAESTANITE", Some(572), Some(24)),
            ("SILICON", Some(510), Some(16)),
        ];
        const DONE_2: [(&str, Option<u32>, Option<u32>); 5] = [
            ("QUARTZ", Some(330), Some(134)),
            ("QUARTZ", Some(522), Some(19)),
            ("HEPHAESTANITE", Some(572), Some(19)),
            ("SILICON", Some(510), Some(1)),
            ("SILICON", Some(782), Some(5)),
        ];
        const RUNNING_A: [(&str, Option<u32>, Option<u32>); 6] = [
            ("TITANIUM", Some(516), Some(24)),
            ("ASLARITE", Some(287), Some(3)),
            ("ASLARITE", Some(575), Some(8)),
            ("AGRICIUM", Some(346), Some(253)),
            ("AGRICIUM", Some(667), Some(31)),
            ("AGRICIUM", Some(852), Some(62)),
        ];
        const RUNNING_B: [(&str, Option<u32>, Option<u32>); 4] = [
            ("QUARTZ", Some(330), Some(221)),
            ("QUARTZ", Some(522), Some(90)),
            ("HEPHAESTANITE", Some(572), Some(18)),
            ("SILICON", Some(510), Some(25)),
        ];
        const RUNNING_C: [(&str, Option<u32>, Option<u32>); 5] = [
            ("TITANIUM", Some(516), Some(14)),
            ("ASLARITE", Some(575), Some(8)),
            ("AGRICIUM", Some(346), Some(161)),
            ("AGRICIUM", Some(588), Some(6)),
            ("AGRICIUM", Some(667), Some(35)),
        ];

        #[test]
        fn no_terminal_and_the_terminal_without_overview() {
            let mut ocr = test_ocr();
            assert!(!probe(&mut ocr, "1794142372-read-0"));
            assert_eq!(read(&mut ocr, "1794142372-read-0").screen, Screen::Nothing);
            assert!(probe(&mut ocr, "1794147615-read-2"));
            assert_eq!(read(&mut ocr, "1794147615-read-2").screen, Screen::Terminal);
        }

        #[test]
        fn work_orders_at_3840x1600() {
            let mut ocr = test_ocr();
            let done = || vec![(Status::Completed, None, DONE_1.to_vec()), (Status::Completed, None, DONE_2.to_vec())];
            assert!(probe(&mut ocr, "1794142372-read-10"));
            assert_eq!(orders(&overview(&mut ocr, "1794142372-read-10")), done());
            let mut with_running = done();
            with_running.push((Status::Processing, Some(3600 + 38 * 60), RUNNING_A.to_vec()));
            assert_eq!(orders(&overview(&mut ocr, "1794142372-read-15")), with_running);
        }

        #[test]
        fn work_orders_at_1080p() {
            let mut ocr = test_ocr();
            assert_eq!(
                orders(&overview(&mut ocr, "1794147615-read-4")),
                vec![
                    (Status::Completed, None, DONE_2.to_vec()),
                    (Status::Processing, Some(11 * 60 + 42), RUNNING_A.to_vec()),
                    (Status::Processing, Some(32 * 60 + 56), RUNNING_B.to_vec()),
                ]
            );
            assert_eq!(
                orders(&overview(&mut ocr, "1794147615-read-27")),
                vec![
                    (Status::Processing, Some(10 * 60 + 52), RUNNING_A.to_vec()),
                    (Status::Processing, Some(32 * 60 + 6), RUNNING_B.to_vec()),
                    (Status::Processing, Some(3600 + 19 * 60), RUNNING_C.to_vec()),
                ]
            );
        }

        // "SILICON 782" at 1080p, a borderline glyph (ADR 0018): read right, or scored below the companion's 0.95
        #[test]
        fn a_misread_digit_scores_low() {
            let mut ocr = test_ocr();
            let v = overview(&mut ocr, "1794147615-read-12");
            let order = v.orders.iter().find(|o| o.rows.len() == 5).expect("work order 2");
            let row = &order.rows[4];
            assert_eq!(row.material.text, "SILICON");
            assert!(row.quality.value == Some(782) || row.quality.score < 0.95, "{:?}", row.quality);
        }
    }
}
