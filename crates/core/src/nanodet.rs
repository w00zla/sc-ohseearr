//! Layout detection with a NanoDet-Plus model: boxes of given classes (a screen's parts: count badges,
//! tooltip lines, …) in a fixed-size input. Much cheaper than the text detector over the same area (the
//! warehouse: ~4 ms on the CPU where the text detector took ~75 ms), and the boxes come with their meaning.

use std::time::Instant;

use image::RgbImage;
use ort::value::Tensor;

use crate::engine::{Engine, Timings};
use crate::geom::{Detection, Rect, nms};
use crate::imaging::{FilterType, resize, to_chw};
use crate::manifest::{Family, Input, ModelPackage, Nanodet as Params};

pub struct NanoDet {
    engine: Engine,
    input: Input,
    width: u32,
    height: u32,
    params: Params,
    timings: Timings,
}

impl NanoDet {
    /// `threads`: ONNX Runtime's intra-op threads on the CPU; `gpu`: DirectML (an error when it does not work).
    pub fn new(package: &ModelPackage, threads: usize, gpu: bool) -> Result<Self, String> {
        package.expect(Family::Nanodet)?;
        let m = &package.manifest;
        Ok(Self {
            engine: Engine::new("layout", package.model.clone(), threads, gpu)?,
            input: m.input.clone(),
            width: m.input.width.expect("checked by Manifest::parse"),
            height: m.input.height.expect("checked by Manifest::parse"),
            params: m.nanodet.clone().expect("checked by Manifest::parse"),
            timings: Timings::default(),
        })
    }

    /// The fixed input size (width, height): callers crop to this aspect ratio.
    pub fn input_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The classes in the model's order (`Detection::class` indexes them).
    pub fn classes(&self) -> &[String] {
        &self.params.classes
    }

    /// The IoU above which boxes of one class count as one (for merging the detections of several crops).
    pub fn nms_limit(&self) -> f32 {
        self.params.nms
    }

    /// Whether it runs on the GPU.
    pub fn gpu(&self) -> bool {
        self.engine.gpu()
    }

    /// The timings since the last call (and starts over), counted as a detector's.
    pub fn take_timings(&mut self) -> Timings {
        std::mem::take(&mut self.timings)
    }

    /// The boxes in `img` (scaled to the input size), in `img`'s pixels, best first.
    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Detection>, String> {
        let started = Instant::now();
        let (w, h) = img.dimensions();
        let resized = resize(img, self.width, self.height, FilterType::Bilinear);
        let shape = [1usize, 3, self.height as usize, self.width as usize];
        let input = Tensor::from_array((shape, to_chw(&resized, self.width, &self.input))).map_err(|e| e.to_string())?;
        let prepared = Instant::now();
        let outputs = self.engine.session("layout", &shape)?.run(ort::inputs![input]).map_err(|e| e.to_string())?;
        let ran = Instant::now();
        self.timings.det_prep += prepared - started;
        self.timings.det_run += ran - prepared;
        self.timings.det_runs += 1;
        let (_, out) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let mut found = decode(out, self.width, self.height, &self.params);
        let (sx, sy) = (w as f32 / self.width as f32, h as f32 / self.height as f32);
        for d in &mut found {
            d.rect = d.rect.scaled_into(sx, sy, w, h);
        }
        self.timings.det_post += ran.elapsed();
        Ok(found)
    }
}

/// The boxes from a NanoDet-Plus output (prior points of every stride, row by row, each with its classes'
/// scores and per side a distribution over the bins), above the score, one box per overlapping group of a
/// class (the best scored), in input pixels.
pub(crate) fn decode(out: &[f32], width: u32, height: u32, p: &Params) -> Vec<Detection> {
    let classes = p.classes.len();
    let bins = p.reg_max + 1;
    let per_point = classes + 4 * bins;
    let mut found: Vec<Detection> = Vec::new();
    let mut i = 0;
    for &s in &p.strides {
        let (fw, fh) = (width.div_ceil(s), height.div_ceil(s));
        for y in 0..fh {
            for x in 0..fw {
                let q = &out[i * per_point..(i + 1) * per_point];
                i += 1;
                let (class, &score) = q[..classes].iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).expect("classes");
                if score < p.score {
                    continue;
                }
                // Each side's distance: the expectation of its bins (softmax), in strides
                let side = |n: usize| {
                    let d = &q[classes + n * bins..classes + (n + 1) * bins];
                    let max = d.iter().copied().fold(f32::MIN, f32::max);
                    let e: Vec<f32> = d.iter().map(|v| (v - max).exp()).collect();
                    let sum: f32 = e.iter().sum();
                    e.iter().enumerate().map(|(b, v)| b as f32 * v).sum::<f32>() / sum * s as f32
                };
                let (cx, cy) = ((x * s) as f32, (y * s) as f32);
                let rect = Rect { x0: cx - side(0), y0: cy - side(1), x1: cx + side(2), y1: cy + side(3) };
                found.push(Detection { class, score, rect });
            }
        }
    }
    nms(found, p.nms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_from_prior_points() {
        let p = Params { strides: vec![8, 16, 32, 64], reg_max: 7, classes: ["a", "b", "c", "d"].map(String::from).to_vec(), score: 0.35, nms: 0.5 };
        let (w, h) = (480u32, 320u32);
        let per = p.classes.len() + 4 * (p.reg_max + 1);
        let points: u32 = p.strides.iter().map(|s| w.div_ceil(*s) * h.div_ceil(*s)).sum();
        let mut out = vec![0f32; points as usize * per];
        // A box of class 1 at the stride-8 point (10, 5), every side 2 bins (16 px) away, and a weaker
        // neighbour on almost the same box; a class-0 box somewhere else below the score
        let mut set = |point: usize, class: usize, score: f32, bins: [usize; 4]| {
            let q = &mut out[point * per..(point + 1) * per];
            q[class] = score;
            for (side, b) in bins.into_iter().enumerate() {
                q[p.classes.len() + side * (p.reg_max + 1) + b] = 20.0;
            }
        };
        let fw = w.div_ceil(8) as usize;
        set(5 * fw + 10, 1, 0.9, [2, 2, 2, 2]);
        set(5 * fw + 11, 1, 0.6, [3, 2, 1, 2]);
        set(9 * fw + 30, 0, 0.2, [1, 1, 1, 1]);
        let found = decode(&out, w, h, &p);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].class, 1);
        let r = found[0].rect;
        assert!((r.x0 - 64.0).abs() < 0.1 && (r.y0 - 24.0).abs() < 0.1 && (r.x1 - 96.0).abs() < 0.1 && (r.y1 - 56.0).abs() < 0.1, "{r:?}");
    }
}
