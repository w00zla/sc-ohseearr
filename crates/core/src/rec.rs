//! Text line recognition with CTC decoding (PaddleOCR's rec models; pre-processing after RapidOCR): each line
//! is scaled to the model's line height, padded with zeros on the right, normalized as its manifest says;
//! lines of similar shape are read together in batches; the output is decoded greedily (blank first, the
//! dictionary, optionally a space).

use std::time::Instant;

use image::RgbImage;
use ort::value::Tensor;

use crate::engine::{Engine, Timings};
use crate::geom::Rect;
use crate::imaging::{FilterType, channels, crop, resize};
use crate::manifest::{Family, Input, ModelPackage};

/// Lines per recognizer run (RapidOCR's rec_batch_num; default of `CtcRecognizer::batch`)
pub const BATCH: usize = 6;
/// The input widths on the GPU (default of `CtcRecognizer::gpu_widths`): lines are padded to the next, wider
/// ones get their own
pub const GPU_WIDTHS: [usize; 7] = [320, 480, 640, 960, 1280, 1920, 2560];

pub struct CtcRecognizer {
    engine: Engine,
    input: Input,
    height: u32,
    width: u32,
    // Index 0 is the CTC blank
    chars: Vec<String>,
    /// Lines per model run
    pub batch: usize,
    /// On the GPU every batch is padded to `batch` lines of one of these widths (few input shapes)
    pub gpu_widths: Vec<usize>,
    timings: Timings,
}

impl CtcRecognizer {
    /// `threads`: ONNX Runtime's intra-op threads on the CPU; `gpu`: DirectML (an error when it does not work).
    pub fn new(package: &ModelPackage, threads: usize, gpu: bool) -> Result<Self, String> {
        package.expect(Family::CtcRec)?;
        let m = &package.manifest;
        let ctc = m.ctc.as_ref().expect("checked by Manifest::parse");
        let mut chars = vec![String::new()];
        chars.extend(package.dict.as_deref().expect("checked by ModelPackage").lines().map(|l| l.trim_end_matches('\r').to_string()));
        if ctc.space_char {
            chars.push(" ".into());
        }
        Ok(Self {
            engine: Engine::new("rec", package.model.clone(), threads, gpu)?,
            input: m.input.clone(),
            height: m.input.height.expect("checked by Manifest::parse"),
            width: m.input.width.expect("checked by Manifest::parse"),
            chars,
            batch: BATCH,
            gpu_widths: GPU_WIDTHS.to_vec(),
            timings: Timings::default(),
        })
    }

    /// Whether it runs on the GPU.
    pub fn gpu(&self) -> bool {
        self.engine.gpu()
    }

    /// The timings since the last call (and starts over).
    pub fn take_timings(&mut self) -> Timings {
        std::mem::take(&mut self.timings)
    }

    /// The characters it can read (without the blank).
    pub fn charset(&self) -> &[String] {
        &self.chars[1..]
    }

    /// The text in `rect` of `img` as one line, with its mean character confidence.
    pub fn recognize(&mut self, img: &RgbImage, rect: Rect) -> Result<(String, f32), String> {
        Ok(self.recognize_all(img, &[rect])?.pop().expect("one line"))
    }

    /// The text of every rect (in their order).
    pub fn recognize_all(&mut self, img: &RgbImage, rects: &[Rect]) -> Result<Vec<(String, f32)>, String> {
        let started = Instant::now();
        let crops: Vec<RgbImage> = rects.iter().map(|r| crop(img, *r)).collect();
        self.timings.rec_prep += started.elapsed();
        self.recognize_images(&crops)
    }

    /// The text of every image (one text line each, in their order).
    pub fn recognize_images(&mut self, crops: &[RgbImage]) -> Result<Vec<(String, f32)>, String> {
        self.recognize_images_in(crops, None)
    }

    /// The same, picking only characters of `allowed` (a count: "Xx× 0123456789"): the recognizer otherwise
    /// takes a look-alike now and then. Each step's probability is taken among the allowed characters.
    pub fn recognize_images_only(&mut self, crops: &[RgbImage], allowed: &str) -> Result<Vec<(String, f32)>, String> {
        let mask: Vec<bool> = self.chars.iter().map(|c| c.chars().count() == 1 && c.chars().all(|ch| allowed.contains(ch))).collect();
        self.recognize_images_in(crops, Some(&mask))
    }

    fn recognize_images_in(&mut self, crops: &[RgbImage], allowed: Option<&[bool]>) -> Result<Vec<(String, f32)>, String> {
        let (height, width) = (self.height, self.width);
        let ratio = |c: &RgbImage| c.width() as f32 / c.height() as f32;
        // Similar widths in one batch: the narrow lines are padded less
        let mut order: Vec<usize> = (0..crops.len()).collect();
        order.sort_by(|&a, &b| ratio(&crops[a]).total_cmp(&ratio(&crops[b])));
        let mut out = vec![(String::new(), 0f32); crops.len()];
        // `v / (255 * std) - mean / std`: with PaddleOCR's 0.5/0.5 exactly its `v / 127.5 - 1`
        let div: [f32; 3] = std::array::from_fn(|c| 255.0 * self.input.std[c]);
        let off: [f32; 3] = std::array::from_fn(|c| self.input.mean[c] / self.input.std[c]);
        for batch in order.chunks(self.batch) {
            let started = Instant::now();
            let max_ratio = batch.iter().map(|&i| ratio(&crops[i])).fold(width as f32 / height as f32, f32::max);
            let mut padded = (height as f32 * max_ratio) as u32;
            // On the GPU one of a few shapes (`Engine`): a full batch, the next of its widths
            let rows = if self.engine.gpu() {
                padded = self.gpu_widths.iter().copied().find(|&w| w >= padded as usize).map_or(padded, |w| w as u32);
                self.batch
            } else {
                batch.len()
            };
            let plane = (height * padded) as usize;
            // Zero padding on the right, as RapidOCR does
            let mut data = vec![0f32; rows * 3 * plane];
            for (n, &i) in batch.iter().enumerate() {
                let c = &crops[i];
                let resized_w = ((height as f32 * ratio(c)).ceil() as u32).clamp(1, padded);
                let resized = resize(c, resized_w, height, FilterType::Bilinear);
                let base = n * 3 * plane;
                for (x, y, p) in resized.enumerate_pixels() {
                    let j = (y * padded + x) as usize;
                    for (ch, v) in channels(self.input.order, p).into_iter().enumerate() {
                        data[base + ch * plane + j] = v as f32 / div[ch] - off[ch];
                    }
                }
            }
            let shape = [rows, 3, height as usize, padded as usize];
            let input = Tensor::from_array((shape, data)).map_err(|e| e.to_string())?;
            let prepared = Instant::now();
            let outputs = self.engine.session("rec", &shape)?.run(ort::inputs![input]).map_err(|e| e.to_string())?;
            let ran = Instant::now();
            self.timings.rec_prep += prepared - started;
            self.timings.rec_run += ran - prepared;
            self.timings.rec_runs += 1;
            self.timings.rec_lines += batch.len() as u32;
            let (shape, probs) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let (steps, classes) = (shape[1] as usize, shape[2] as usize);
            if classes != self.chars.len() {
                return Err(format!("rec model has {classes} classes, the dictionary {}: wrong dictionary", self.chars.len()));
            }
            for (n, &i) in batch.iter().enumerate() {
                out[i] = ctc_decode(&probs[n * steps * classes..(n + 1) * steps * classes], steps, classes, &self.chars, allowed);
            }
            self.timings.rec_post += ran.elapsed();
        }
        Ok(out)
    }
}

/// Greedy CTC decoding of `steps`×`classes` probabilities: the most likely class per step, repeats and blanks
/// dropped; the score is the mean probability of the kept characters.
/// `allowed`: only these classes (and the blank) can be picked, each step's probability taken among them (a
/// "6" the model rates just below a "日" is still a sure "6"); None = every class.
pub(crate) fn ctc_decode(probs: &[f32], steps: usize, classes: usize, chars: &[String], allowed: Option<&[bool]>) -> (String, f32) {
    let mut text = String::new();
    let mut scores = Vec::new();
    let mut last = 0usize;
    let ok = |i: usize| i == 0 || allowed.is_none_or(|a| a.get(i).copied().unwrap_or(false));
    for t in 0..steps {
        let row = &probs[t * classes..(t + 1) * classes];
        let (best, p) = row.iter().enumerate().filter(|&(i, _)| ok(i)).fold((0, f32::MIN), |acc, (i, &v)| if v > acc.1 { (i, v) } else { acc });
        let p = match allowed {
            Some(_) => {
                let mass: f32 = row.iter().enumerate().filter(|&(i, _)| ok(i)).map(|(_, &v)| v).sum();
                if mass > 0.0 { p / mass } else { 0.0 }
            }
            None => p,
        };
        if best != 0
            && best != last
            && let Some(c) = chars.get(best)
        {
            text.push_str(c);
            scores.push(p);
        }
        last = best;
    }
    let score = if scores.is_empty() { 0.0 } else { scores.iter().sum::<f32>() / scores.len() as f32 };
    (text, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctc_drops_blanks_and_repeats() {
        let chars: Vec<String> = ["", "a", "b"].iter().map(|s| s.to_string()).collect();
        // a a _ a b b _
        let seq = [1, 1, 0, 1, 2, 2, 0];
        let mut probs = vec![0f32; seq.len() * 3];
        for (t, &c) in seq.iter().enumerate() {
            probs[t * 3 + c] = 0.9;
        }
        assert_eq!(ctc_decode(&probs, seq.len(), 3, &chars, None).0, "aab");
    }

    #[test]
    fn ctc_picks_only_allowed_characters() {
        let chars: Vec<String> = ["", "X", "6", "日"].iter().map(|s| s.to_string()).collect();
        // X, then a step the model rates 日 0.6, 6 0.3, blank 0.1
        let probs = [0.0, 0.9, 0.05, 0.05, 0.1, 0.0, 0.3, 0.6];
        assert_eq!(ctc_decode(&probs, 2, 4, &chars, None).0, "X日");
        let allowed = [true, true, true, false];
        let (text, score) = ctc_decode(&probs, 2, 4, &chars, Some(&allowed));
        assert_eq!(text, "X6");
        // 6 among the allowed: 0.3 / (0.1 + 0.3)
        assert!(score > 0.8, "{score}");
    }
}
