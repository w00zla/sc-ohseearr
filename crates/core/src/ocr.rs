//! The usual pair: a text detector and a recognizer, sharing threads and device.

use crate::error::Result;
use image::RgbImage;

use crate::det::DbDetector;
use crate::engine::Timings;
use crate::geom::{Rect, TextBox};
use crate::manifest::ModelPackage;
use crate::rec::CtcRecognizer;

pub struct Ocr {
    pub det: DbDetector,
    pub rec: CtcRecognizer,
}

impl Ocr {
    /// A detector and a recognizer on the CPU (`threads` each) or the GPU (an error when DirectML does not
    /// work). ONNX Runtime's memory arena keeps the largest run's memory: drop the `Ocr` when reading stops.
    pub fn new(det: &ModelPackage, rec: &ModelPackage, threads: usize, gpu: bool) -> Result<Self> {
        Ok(Self { det: DbDetector::new(det, threads, gpu)?, rec: CtcRecognizer::new(rec, threads, gpu)? })
    }

    /// Whether it runs on the GPU.
    pub fn gpu(&self) -> bool {
        self.det.gpu()
    }

    /// The timings of both since the last call (and starts over).
    pub fn take_timings(&mut self) -> Timings {
        let mut t = self.det.take_timings();
        t += self.rec.take_timings();
        t
    }

    /// See `DbDetector::detect`.
    pub fn detect(&mut self, img: &RgbImage, max_height: u32) -> Result<Vec<Rect>> {
        self.det.detect(img, max_height)
    }

    /// See `DbDetector::detect_on_cpu`.
    pub fn detect_on_cpu(&mut self, img: &RgbImage, max_height: u32) -> Result<Vec<Rect>> {
        self.det.detect_on_cpu(img, max_height)
    }

    /// See `CtcRecognizer::recognize`.
    pub fn recognize(&mut self, img: &RgbImage, rect: Rect) -> Result<(String, f32)> {
        self.rec.recognize(img, rect)
    }

    /// See `CtcRecognizer::recognize_all`.
    pub fn recognize_all(&mut self, img: &RgbImage, rects: &[Rect]) -> Result<Vec<(String, f32)>> {
        self.rec.recognize_all(img, rects)
    }

    /// See `CtcRecognizer::recognize_images`.
    pub fn recognize_images(&mut self, crops: &[RgbImage]) -> Result<Vec<(String, f32)>> {
        self.rec.recognize_images(crops)
    }

    /// See `CtcRecognizer::recognize_images_only`.
    pub fn recognize_images_only(&mut self, crops: &[RgbImage], allowed: &str) -> Result<Vec<(String, f32)>> {
        self.rec.recognize_images_only(crops, allowed)
    }

    /// Detects and recognizes every text line (see `detect`); empty lines are dropped.
    pub fn read(&mut self, img: &RgbImage, max_height: u32) -> Result<Vec<TextBox>> {
        let rects = self.detect(img, max_height)?;
        let texts = self.recognize_all(img, &rects)?;
        Ok(rects
            .into_iter()
            .zip(texts)
            .filter(|(_, (t, _))| !t.trim().is_empty())
            .map(|(rect, (text, score))| TextBox { rect, text, score })
            .collect())
    }
}
