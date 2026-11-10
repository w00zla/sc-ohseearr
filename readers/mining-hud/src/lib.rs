//! The mining HUD's reader: the signature a ship's scan shows for an ore or harvestable spot, a small plate (a
//! pin icon and a number such as "42,000") above the heading tape.
//!
//! The plate moves with the HUD (it sways, and tilts with the head), so it has no fixed place: a window around
//! the HUD's centre is cut out, upscaled and read, and the signature is the line written as the game writes it, with
//! thousands separators and at least 1,000. Nothing else anchors it (no label, hence no locale): any such number
//! in the window is taken for the signature, so the app reads only while a ship's scan HUD is up.
//!
//! Resolution independent: the window is measured in shares of the frame's height (the game's UI scales with it).

use sc_ohseearr_core::{FilterType, Ocr, Rect, TextBox, crop, resize};
use sc_ohseearr_sc::reader::{Diagnostics, Field, Frame, ReadError, Reader, Reading};
use serde::Serialize;

/// The window the plate is looked for in: x from the frame's centre, y from its top, in shares of the frame's
/// height (x0, y0, x1, y1). The plate was seen at x −0.06…+0.04 and y 0.30…0.36 (3840×1600 and 1920×1080).
const WINDOW: (f32, f32, f32, f32) = (-0.25, 0.22, 0.25, 0.45);
/// The window is upscaled to what it measures in a frame this high (by a whole factor): the digits are ~10 px high
/// at 1080p and ~20 px at 1600, and read reliably from ~40 px
const READ_HEIGHT: f32 = 3200.0;
/// The height of the number's box as the detector gives it, a share of the frame's height (24.9 of 1600, 16.7 of
/// 1080 over the lines read without the icon), and a character's width in it, a share of that height (0.38–0.41)
const NUMBER_HEIGHT: f32 = 0.0155;
const CHAR_WIDTH: f32 = 0.40;
/// What a signature is written with: a line that looks like a misread signature is read again as these only
const SIGNATURE_CHARS: &str = "0123456789,";

// ---------------------------------------------------------------------------------------------------------------
// Output

/// What the mining HUD shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MiningHud {
    /// The scan's signature (1,000 and up). Nothing read (`Field::none`): no plate in view; a text without a
    /// value: a plate whose number could not be read.
    pub signature: Field<u32>,
    /// The number's box (frame pixels) for a plate found, without the pin icon, for an app that draws next to it: as
    /// the detector gave it, or, where the icon was read into the line, a box of a number's size at the line's right
    /// edge and middle height.
    pub rect: Option<Rect>,
}

// ---------------------------------------------------------------------------------------------------------------
// Diagnostics

#[derive(Debug, Clone, Default, Serialize)]
pub struct MiningHudDiagnostics {
    /// The text boxes read in the window (frame pixels)
    pub boxes: Vec<TextBox>,
    /// The window (frame pixels)
    pub window: Option<Rect>,
}

impl Diagnostics for MiningHudDiagnostics {
    fn boxes(&self) -> &[TextBox] {
        &self.boxes
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The reader

/// The mining HUD's reader. Keeps the last frame's reading (`probe` reads, `read` of the same frame reuses it),
/// nothing else about earlier frames.
#[derive(Default)]
pub struct MiningHudReader {
    last: Option<(u64, Reading<MiningHud, MiningHudDiagnostics>)>,
}

impl MiningHudReader {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Reader for MiningHudReader {
    type Output = MiningHud;
    type Diagnostics = MiningHudDiagnostics;

    /// Whether a signature is in view: the full read, kept for `read` of the same frame.
    fn probe(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<bool, ReadError> {
        let reading = scan(ocr, frame)?;
        let found = reading.output.signature.value.is_some();
        self.last = Some((frame.id, reading));
        Ok(found)
    }

    fn read(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<Reading<MiningHud, MiningHudDiagnostics>, ReadError> {
        match self.last.take() {
            Some((id, reading)) if id == frame.id => Ok(reading),
            _ => scan(ocr, frame),
        }
    }

    fn reset(&mut self) {
        self.last = None;
    }

    /// No diagnostics cost extra work.
    fn set_diagnostics(&mut self, _on: bool) {}
}

/// The window in frame pixels, on whole pixels and within the frame.
fn window(width: u32, height: u32) -> Rect {
    let (cx, s) = (width as f32 / 2.0, height as f32);
    Rect {
        x0: (cx + WINDOW.0 * s).floor().max(0.0),
        y0: (WINDOW.1 * s).floor(),
        x1: (cx + WINDOW.2 * s).ceil().min(width as f32),
        y1: (WINDOW.3 * s).ceil(),
    }
}

fn scan(ocr: &mut Ocr, frame: &Frame) -> Result<Reading<MiningHud, MiningHudDiagnostics>, ReadError> {
    let (width, height) = frame.image.dimensions();
    let window = window(width, height);
    let piece = crop(frame.image, window);
    let scale = (READ_HEIGHT / height as f32).round().max(1.0) as u32;
    let big = resize(&piece, piece.width() * scale, piece.height() * scale, FilterType::CatmullRom);
    let lines = ocr.read(&big, big.height())?;

    // The best scored line that reads as a signature; else a line that looks like one misread ("54,00D"), read again
    // as digits and commas only
    let best = |a: &&TextBox, b: &&TextBox| a.score.total_cmp(&b.score);
    let (signature, found) = match lines.iter().filter(|l| signature(&l.text).is_some()).max_by(best) {
        Some(l) => {
            // Anything before the number is the icon read into the line: the box is wider and higher than the number
            let number = l.text.split_whitespace().last().unwrap_or_default();
            let with_icon = (number != l.text).then(|| number.chars().count());
            (Field::new(signature(&l.text), l.text.clone(), l.score, false), Some((l.rect, with_icon)))
        }
        None => match lines.iter().filter(|l| signature_like(&l.text)).max_by(best) {
            Some(l) => {
                let again = ocr.recognize_images_only(&[crop(&big, l.rect)], SIGNATURE_CHARS)?.pop();
                let field = match again.and_then(|(text, s)| signature(&text).map(|v| (v, text, s))) {
                    Some((v, text, s)) => Field::new(Some(v), text, s, false),
                    None => Field::new(None, l.text.clone(), l.score, false),
                };
                (field, Some((l.rect, None)))
            }
            None => (Field::none(), None),
        },
    };

    // From the upscaled window back to frame pixels
    let k = scale as f32;
    let in_frame =
        |r: Rect| Rect { x0: r.x0 / k, y0: r.y0 / k, x1: r.x1 / k, y1: r.y1 / k }.offset(window.x0, window.y0);
    let rect = found.map(|(r, with_icon)| match with_icon {
        Some(chars) => number_rect(in_frame(r), chars, height),
        None => in_frame(r),
    });
    let boxes = lines.into_iter().map(|l| TextBox { rect: in_frame(l.rect), ..l }).collect();
    Ok(Reading {
        output: MiningHud { signature, rect },
        diagnostics: MiningHudDiagnostics { boxes, window: Some(window) },
    })
}

/// The number's box in a line's box that also holds the icon: the icon is left of the number and higher, so the
/// line's right edge and middle height are the number's, and its size is what a number's box measures.
fn number_rect(line: Rect, chars: usize, frame_height: u32) -> Rect {
    let h = NUMBER_HEIGHT * frame_height as f32;
    let w = CHAR_WIDTH * h * chars as f32;
    Rect { x0: line.x1 - w, y0: line.cy() - h / 2.0, x1: line.x1, y1: line.cy() + h / 2.0 }
}

/// A signature as the game writes it: "42,000", "3,170", "1,250,000" -> the number. Always with thousands
/// separators (so 1,000 and up), no leading zero. The pin icon before it is often read as a character of its own
/// ("9 20,000", ", 42,000"): only the text after the last blank counts.
fn signature(text: &str) -> Option<u32> {
    let digits = |g: &str| g.chars().all(|c| c.is_ascii_digit());
    let mut groups = text.split_whitespace().last()?.split(',');
    let first = groups.next()?;
    if first.is_empty() || first.len() > 3 || first.starts_with('0') || !digits(first) {
        return None;
    }
    let mut number = first.to_string();
    for g in groups {
        if g.len() != 3 || !digits(g) {
            return None;
        }
        number.push_str(g);
    }
    // No separator: not a signature (a heading, a distance)
    if number.len() == first.len() {
        return None;
    }
    number.parse().ok()
}

/// Whether a text has a signature's shape with some digits misread ("54,00D", "14,0D0"): one word, groups of three
/// after each comma, more than half of it digits. A text with a blank is left alone: the icon read into it would
/// become a digit when read as digits only.
fn signature_like(text: &str) -> bool {
    let t = text.trim();
    let mut groups = t.split(',');
    let first = groups.next().unwrap_or_default();
    let rest: Vec<&str> = groups.collect();
    let digits = t.chars().filter(char::is_ascii_digit).count();
    !t.contains(char::is_whitespace)
        && (1..=3).contains(&first.chars().count())
        && !rest.is_empty()
        && rest.iter().all(|g| g.chars().count() == 3)
        && digits * 2 > t.chars().count()
}

// ---------------------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    #[test]
    fn signatures_as_the_game_writes_them() {
        assert_eq!(signature("42,000"), Some(42000));
        assert_eq!(signature("3,170"), Some(3170));
        assert_eq!(signature("1,250,000"), Some(1250000));
        // The icon read as a character of its own
        assert_eq!(signature("9 20,000"), Some(20000));
        assert_eq!(signature(", 42,000"), Some(42000));
        assert_eq!(signature("Ļ 29,740 "), Some(29740));
        // No separator, a wrong group, a leading zero, other texts
        assert_eq!(signature("2104"), None);
        assert_eq!(signature("8,70"), None);
        assert_eq!(signature("0,500"), None);
        assert_eq!(signature("1234,000"), None);
        assert_eq!(signature("54,00D"), None);
        assert_eq!(signature("3,0 G"), None);
        assert_eq!(signature("5.0km"), None);
        assert_eq!(signature(""), None);
    }

    #[test]
    fn misread_signatures_keep_their_shape() {
        assert!(signature_like("54,00D"));
        assert!(signature_like("14,0D0"));
        assert!(!signature_like("9 54,00D"), "the icon would be read as a digit");
        assert!(!signature_like("8,70"));
        assert!(!signature_like("UNK,NOW"));
        assert!(!signature_like("5.0km"));
        assert!(!signature_like("140"));
    }

    #[test]
    fn the_window_scales_with_the_height() {
        assert_eq!(window(3840, 1600), Rect { x0: 1520.0, y0: 352.0, x1: 2320.0, y1: 720.0 });
        assert_eq!(window(1920, 1080), Rect { x0: 690.0, y0: 237.0, x1: 1230.0, y1: 486.0 });
        // A frame narrower than the window: the frame's width
        assert_eq!(window(400, 1000), Rect { x0: 0.0, y0: 220.0, x1: 400.0, y1: 450.0 });
    }

    #[test]
    fn the_number_without_the_icon_read_into_its_line() {
        // "▼ 10,000" at 3840×1600: the line's box 85 × 33, a number's about 60 × 25
        let line = Rect { x0: 1886.0, y0: 522.0, x1: 1972.0, y1: 555.0 };
        let r = number_rect(line, 6, 1600);
        assert_eq!((r.x1, r.cy()), (1972.0, 538.5));
        assert!((r.w() - 59.5).abs() < 0.1 && (r.h() - 24.8).abs() < 0.1, "{r:?}");
    }

    #[test]
    fn a_frame_without_the_hud_reads_nothing() {
        let image = RgbImage::new(1920, 1080);
        let frame = Frame { id: 1, image: &image, cursor: None, locale: None };
        let mut ocr = sc_ohseearr_test_support::test_ocr();
        let mut reader = MiningHudReader::new();
        assert!(!reader.probe(&mut ocr, &frame).unwrap());
        let reading = reader.read(&mut ocr, &frame).unwrap();
        assert_eq!(reading.output, MiningHud { signature: Field::none(), rect: None });
        assert_eq!(reading.diagnostics.window, Some(window(1920, 1080)));
    }
}
