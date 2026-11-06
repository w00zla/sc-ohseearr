//! Text detection with DB post-processing (PaddleOCR's det models; pre- and post-processing follow
//! RapidOCR 3.x): the detector sees the image normalized as its manifest says, its probability map is
//! thresholded, dilated, split into connected regions (kept by the mean probability of their own pixels) and
//! each region's box is grown by `area * unclip / perimeter`. Boxes are axis-aligned (no rotated text).

use crate::error::{Error, Result};
use std::time::Instant;

use image::RgbImage;
use ort::value::Tensor;

use crate::engine::{Engine, Timings};
use crate::geom::Rect;
use crate::imaging::{FilterType, resize, to_chw};
use crate::manifest::{Bytes, Db, Family, Input, ModelPackage};

/// Detector inputs up to this height run on the CPU in GPU mode too (default of `DbDetector::cpu_max_height`):
/// ~1 ms each there, while each of their shapes held a DirectML session of its own (~13 MB VRAM each, 2026-10-31)
pub const CPU_MAX_HEIGHT: u32 = 192;

pub struct DbDetector {
    engine: Engine,
    // The model on the CPU in GPU mode (`detect_on_cpu`, small inputs), made on first use
    cpu: Option<Engine>,
    model: Bytes,
    threads: usize,
    input: Input,
    db: Db,
    /// In GPU mode inputs up to this height run on the CPU
    pub cpu_max_height: u32,
    timings: Timings,
}

impl DbDetector {
    /// `threads`: ONNX Runtime's intra-op threads on the CPU; `gpu`: DirectML (an error when it does not work).
    pub fn new(package: &ModelPackage, threads: usize, gpu: bool) -> Result<Self> {
        package.expect(Family::DbDet)?;
        let m = &package.manifest;
        Ok(Self {
            engine: Engine::new("det", package.model.clone(), threads, gpu)?,
            cpu: None,
            model: package.model.clone(),
            threads,
            input: m.input.clone(),
            db: m.db.clone().expect("checked by Manifest::parse"),
            cpu_max_height: CPU_MAX_HEIGHT,
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

    /// Text boxes in `img`, detected on a copy scaled down to at most `max_height` pixels high (never up), in
    /// `img`'s pixels, top to bottom. In GPU mode small inputs run on the CPU (`cpu_max_height`).
    pub fn detect(&mut self, img: &RgbImage, max_height: u32) -> Result<Vec<Rect>> {
        self.detect_in(img, max_height, false)
    }

    /// The same on the CPU in GPU mode too: for a large input read rarely, whose DirectML session would hold
    /// its VRAM for as long as the detector lives.
    pub fn detect_on_cpu(&mut self, img: &RgbImage, max_height: u32) -> Result<Vec<Rect>> {
        self.detect_in(img, max_height, true)
    }

    fn detect_in(&mut self, img: &RgbImage, max_height: u32, cpu: bool) -> Result<Vec<Rect>> {
        let started = Instant::now();
        let (w, h) = img.dimensions();
        let ratio = (max_height as f32 / h as f32).min(1.0);
        let round32 = |v: f32| (((v / 32.0).round() as u32) * 32).max(32);
        let (rw, rh) = (round32(w as f32 * ratio), round32(h as f32 * ratio));
        let resized = resize(img, rw, rh, FilterType::Bilinear);
        let input = Tensor::from_array(([1usize, 3, rh as usize, rw as usize], to_chw(&resized, rw, &self.input)))
            .map_err(Error::runtime)?;
        let prepared = Instant::now();
        let det = if self.engine.gpu() && (cpu || rh <= self.cpu_max_height) {
            if self.cpu.is_none() {
                self.cpu = Some(Engine::new("det", self.model.clone(), self.threads, false)?);
            }
            self.cpu.as_mut().expect("made above")
        } else {
            &mut self.engine
        };
        let outputs =
            det.session("det", &[1, 3, rh as usize, rw as usize])?.run(ort::inputs![input]).map_err(Error::runtime)?;
        let ran = Instant::now();
        self.timings.det_prep += prepared - started;
        self.timings.det_run += ran - prepared;
        self.timings.det_runs += 1;
        let (_, prob) = outputs[0].try_extract_tensor::<f32>().map_err(Error::runtime)?;
        let mut boxes = db_boxes(prob, rw as usize, rh as usize, &self.db);
        let (sx, sy) = (w as f32 / rw as f32, h as f32 / rh as f32);
        for b in &mut boxes {
            *b = b.scaled_into(sx, sy, w, h);
        }
        boxes.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        self.timings.det_post += ran.elapsed();
        Ok(boxes)
    }
}

/// Boxes of the regions above the threshold in a `w`×`h` probability map (DB post-processing, axis-aligned),
/// in map pixels.
pub(crate) fn db_boxes(prob: &[f32], w: usize, h: usize, db: &Db) -> Vec<Rect> {
    // Threshold, then dilate with a 2×2 kernel (as RapidOCR's use_dilation)
    let raw: Vec<bool> = prob.iter().map(|&p| p > db.thresh).collect();
    let mut mask = raw.clone();
    for y in 0..h {
        for x in 0..w {
            if raw[y * w + x] {
                continue;
            }
            let left = x > 0 && raw[y * w + x - 1];
            let up = y > 0 && raw[(y - 1) * w + x];
            let diag = x > 0 && y > 0 && raw[(y - 1) * w + x - 1];
            mask[y * w + x] = left || up || diag;
        }
    }
    let mut seen = vec![false; w * h];
    let mut stack = Vec::new();
    let mut boxes = Vec::new();
    for start in 0..w * h {
        if !mask[start] || seen[start] {
            continue;
        }
        // One 8-connected region: its bounding box and the mean probability of its own pixels (not of the
        // box: a tilted line, as on a terminal in the world, fills its axis-aligned box only partly, and the
        // box's mean fell below the threshold for whole tooltips)
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut sum, mut count) = (0f32, 0usize);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            sum += prob[i];
            count += 1;
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if mask[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
        if bw.min(bh) < db.min_size {
            continue;
        }
        if sum / (count as f32) < db.box_thresh {
            continue;
        }
        let d = bw * bh * db.unclip / (2.0 * (bw + bh));
        let r = Rect { x0: x0 as f32 - d, y0: y0 as f32 - d, x1: x1 as f32 + d + 1.0, y1: y1 as f32 + d + 1.0 };
        if r.w().min(r.h()) < db.min_size + 2.0 {
            continue;
        }
        boxes.push(r);
    }
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_finds_separate_regions() {
        let (w, h) = (40, 20);
        let mut prob = vec![0f32; w * h];
        for y in 5..12 {
            for x in 2..15 {
                prob[y * w + x] = 0.9;
            }
            for x in 25..38 {
                prob[y * w + x] = 0.9;
            }
        }
        let db = Db { thresh: 0.2, box_thresh: 0.4, unclip: 1.4, min_size: 3.0 };
        let boxes = db_boxes(&prob, w, h, &db);
        assert_eq!(boxes.len(), 2);
        assert!(boxes.iter().all(|b| b.y0 < 5.0 && b.y1 > 12.0));
    }
}
