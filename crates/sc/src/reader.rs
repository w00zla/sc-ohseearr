//! The interface of a screen reader (ADR 0010): one game screen, its own state, neutral output.
//!
//! Which screen is visible is the app's business (it may probe several readers in its own order); a reader answers
//! `probe` ("is my screen in this frame?") and `read` ("what does it show?"). Work done by `probe` may be reused by
//! `read` of the same frame (same `Frame::id`). Output is what the game shows, typed; what a value is uncertain
//! about is in its `Field`; everything the app needs only for troubleshooting is in the diagnostics.

use image::RgbImage;
use sc_ohseearr_core::{Error, Ocr, TextBox};
use std::fmt;

use crate::locale::{Locale, LocaleError};

/// One frame to read.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    /// Identifies the frame: `probe` and `read` with the same id may share work (a capture time works)
    pub id: u64,
    pub image: &'a RgbImage,
    /// The mouse cursor in frame pixels, if known (hover-driven screens need it)
    pub cursor: Option<(f32, f32)>,
    /// The game's texts; readers that anchor on labels need it, others use it to resolve names to loc keys
    pub locale: Option<&'a Locale>,
}

/// A value read from a text, with what it was read from.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Field<T> {
    /// None: the text could not be read as this value
    pub value: Option<T>,
    /// The text as the OCR read it
    pub raw: String,
    /// The OCR's confidence in the text (0–1; 0 when nothing was read)
    pub score: f32,
    /// The text allows another value too (a quality "1000" may be 100Q); or the value is a default for missing
    /// digits (a badge "X" taken as 1)
    pub ambiguous: bool,
}

impl<T> Field<T> {
    pub fn new(value: Option<T>, raw: impl Into<String>, score: f32, ambiguous: bool) -> Self {
        Self { value, raw: raw.into(), score, ambiguous }
    }

    /// Nothing read.
    pub fn none() -> Self {
        Self { value: None, raw: String::new(), score: 0.0, ambiguous: false }
    }
}

/// A name the game shows (an item, a material, a station) and the loc keys whose text reads like it (`Locale`;
/// empty without a locale or without a match).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Name {
    pub text: String,
    pub loc_keys: Vec<String>,
}

impl Name {
    pub fn new(text: impl Into<String>, locale: Option<&Locale>) -> Self {
        let text = text.into();
        let loc_keys = locale.map(|l| l.keys_for(&text).into_iter().map(str::to_string).collect()).unwrap_or_default();
        Self { text, loc_keys }
    }
}

/// A read's result and its diagnostics.
#[derive(Debug, Clone)]
pub struct Reading<O, D> {
    pub output: O,
    pub diagnostics: D,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The OCR failed (a model, ONNX Runtime, the GPU)
    Ocr(Error),
    /// The reader needs the game's texts and has none
    Locale(LocaleError),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ocr(e) => e.fmt(f),
            Self::Locale(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<Error> for ReadError {
    fn from(e: Error) -> Self {
        Self::Ocr(e)
    }
}

impl From<LocaleError> for ReadError {
    fn from(e: LocaleError) -> Self {
        Self::Locale(e)
    }
}

impl From<ReadError> for String {
    fn from(e: ReadError) -> Self {
        e.to_string()
    }
}

/// Every reader's diagnostics include the text boxes it read (frame pixels).
pub trait Diagnostics: Default {
    fn boxes(&self) -> &[TextBox];
}

/// A reader of one game screen.
pub trait ScreenReader {
    type Output;
    type Diagnostics: Diagnostics;

    /// Whether this reader's screen is in the frame.
    fn probe(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<bool, ReadError>;

    /// What the screen shows. On a frame without the screen the output says so (empty, None fields), no error.
    fn read(&mut self, ocr: &mut Ocr, frame: &Frame) -> Result<Reading<Self::Output, Self::Diagnostics>, ReadError>;

    /// Forgets what it learned from earlier frames (the screen was left).
    fn reset(&mut self);

    /// Diagnostics that cost extra work (reading more than the output needs) only while on.
    fn set_diagnostics(&mut self, on: bool);
}
