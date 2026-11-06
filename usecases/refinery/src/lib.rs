//! The refinery terminal's screen reader: the station's name from the terminal's title and every work order shown
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
use sc_ohseearr_sc::reader::{Diagnostics, Field, Frame, Name, ReadError, Reading, ScreenReader};
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

impl ScreenReader for RefineryReader {
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
        let mut orders = Vec::new();
        let mut panels = Vec::new();
        for l in found {
            let (order, panel) = read_order(ocr, frame, &boxes, l)?;
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
    Ok((WorkOrder { status: l.status, time_remaining, rows }, panel))
}

// ---------------------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Column, find_columns, head_words};
    use image::RgbImage;
    use sc_ohseearr_sc::{LocTable, Locale};

    /// The 9 keys with the English texts (test data standing in for the English table).
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

    // The user's screenshots (the data folder, not in git) against what the terminal shows; skipped without them.
    // Slow in debug builds: cargo test --release.
    mod screenshots {
        use super::*;
        use sc_ohseearr_test_support::{screenshots, shot as frame, test_ocr};
        use std::path::{Path, PathBuf};

        const STATION: &str = "CRU-L1 AMBITIOUS DREAM STATION";

        fn setup(folder: &str) -> Option<(Ocr, PathBuf)> {
            let dir = screenshots(folder)?;
            Some((test_ocr(), dir))
        }

        fn read(ocr: &mut Ocr, dir: &Path, suffix: &str) -> Option<Refinery> {
            let en = english();
            let image = frame(dir, suffix);
            let f = Frame { id: 0, image: &image, cursor: None, locale: Some(&en) };
            let r = RefineryReader::new().read(ocr, &f).unwrap().output;
            (r.screen == Screen::Overview).then_some(r)
        }

        fn probe(ocr: &mut Ocr, dir: &Path, suffix: &str) -> bool {
            let en = english();
            let image = frame(dir, suffix);
            let f = Frame { id: 0, image: &image, cursor: None, locale: Some(&en) };
            RefineryReader::new().probe(ocr, &f).unwrap()
        }

        type Rows<'a> = Vec<(&'a str, Option<u32>, Option<u32>)>;

        fn rows(o: &WorkOrder) -> Rows<'_> {
            o.rows.iter().map(|r| (r.material.text.as_str(), r.quality.value, r.yield_cscu.value)).collect()
        }

        #[test]
        fn the_running_job_and_no_overview_elsewhere() {
            let Some((mut ocr, dir)) = setup("refinery flow") else { return };
            let v = read(&mut ocr, &dir, "E87").expect("overview");
            assert_eq!(v.station.as_ref().map(|s| s.text.as_str()), Some(STATION));
            assert_eq!(v.orders.len(), 1);
            assert_eq!(v.orders[0].status, Status::Processing);
            assert_eq!(v.orders[0].time_remaining.value, Some(57 * 60 + 45));
            assert_eq!(
                rows(&v.orders[0]),
                vec![
                    ("TITANIUM", Some(516), Some(9)),
                    ("ASLARITE", Some(575), Some(8)),
                    ("AGRICIUM", Some(346), Some(175)),
                    ("AGRICIUM", Some(588), Some(19)),
                    ("AGRICIUM", Some(667), Some(20)),
                ]
            );
            // The welcome screen, the terminal from the side and the quotes
            for suffix in ["20D", "130", "05E", "0F4", "55D"] {
                assert!(read(&mut ocr, &dir, suffix).is_none(), "{suffix}");
            }
        }

        #[test]
        fn the_cheap_check_finds_the_terminal() {
            for (folder, suffixes) in [
                ("refinery flow", &["20D", "E87", "0F4"][..]),
                ("refinery flow 2", &["DC5", "2E4"]),
                ("refinery flow 3", &["BB4", "575"]),
            ] {
                let Some((mut ocr, dir)) = setup(folder) else { return };
                for suffix in suffixes {
                    assert!(probe(&mut ocr, &dir, suffix), "{suffix}");
                }
            }
            let Some((mut ocr, dir)) = setup("refinery flow") else { return };
            // The terminal from the side: nothing readable
            assert!(!probe(&mut ocr, &dir, "130"));
        }

        // Work order 2 started next to the finished work order 1 (right of it)
        #[test]
        fn running_work_order_next_to_a_finished_one() {
            let Some((mut ocr, dir)) = setup("refinery flow 2") else { return };
            let finished = vec![
                ("TITANIUM", Some(516), Some(9)),
                ("ASLARITE", Some(575), Some(8)),
                ("AGRICIUM", Some(346), Some(175)),
                ("AGRICIUM", Some(588), Some(19)),
                ("AGRICIUM", Some(667), Some(20)),
            ];
            // Only the finished work order
            for suffix in ["DC5", "DF8"] {
                let v = read(&mut ocr, &dir, suffix).unwrap_or_else(|| panic!("{suffix}: no overview"));
                assert_eq!(v.orders.len(), 1, "{suffix}");
                assert_eq!(v.orders[0].status, Status::Completed, "{suffix}");
                assert_eq!(rows(&v.orders[0]), finished, "{suffix}");
            }
            // Just accepted (a message over the panel), then the overview
            for suffix in ["2E4", "7A5", "578"] {
                let v = read(&mut ocr, &dir, suffix).unwrap_or_else(|| panic!("{suffix}: no overview"));
                assert_eq!(v.station.as_ref().map(|s| s.text.as_str()), Some(STATION), "{suffix}");
                let running: Vec<_> = v.orders.iter().filter(|o| o.status == Status::Processing).collect();
                assert_eq!(running.len(), 1, "{suffix}");
                assert_eq!(running[0].time_remaining.value, Some(4 * 3600 + 17 * 60), "{suffix}");
                assert_eq!(
                    rows(running[0]),
                    vec![
                        ("HEPHAESTANITE", Some(572), Some(91)),
                        ("SILICON", Some(310), Some(540)),
                        ("SILICON", Some(510), Some(105)),
                    ],
                    "{suffix}"
                );
            }
        }

        // Work order 3 started next to the still running work order 2
        #[test]
        fn two_running_work_orders() {
            let Some((mut ocr, dir)) = setup("refinery flow 3") else { return };
            // Just accepted (a message over the panel), then the overview
            for suffix in ["84C", "F76", "575"] {
                let v = read(&mut ocr, &dir, suffix).unwrap_or_else(|| panic!("{suffix}: no overview"));
                let running: Vec<_> = v.orders.iter().filter(|o| o.status == Status::Processing).collect();
                assert_eq!(
                    running.iter().map(|o| rows(o)).collect::<Vec<_>>(),
                    vec![
                        vec![
                            ("HEPHAESTANITE", Some(572), Some(91)),
                            ("SILICON", Some(310), Some(540)),
                            ("SILICON", Some(510), Some(105))
                        ],
                        vec![
                            ("TITANIUM", Some(516), Some(3)),
                            ("ASLARITE", Some(575), Some(3)),
                            ("AGRICIUM", Some(346), Some(28)),
                            ("AGRICIUM", Some(588), Some(16))
                        ],
                    ],
                    "{suffix}"
                );
                assert!(running.iter().all(|o| o.time_remaining.value.is_some()), "{suffix}");
            }
            // The whole overview: the finished work order 1 left of both
            let v = read(&mut ocr, &dir, "575").unwrap();
            assert_eq!(
                v.orders.iter().map(|o| o.status).collect::<Vec<_>>(),
                [Status::Completed, Status::Processing, Status::Processing]
            );
            assert_eq!(v.orders[1].time_remaining.value, Some(2 * 3600 + 54 * 60));
            assert_eq!(v.orders[2].time_remaining.value, Some(13 * 60 + 55));
        }
    }
}
