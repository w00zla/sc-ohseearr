//! Captured images and the grey thumbnail the change gate compares.

use image::{GrayImage, RgbImage};

/// The thumbnail's size
pub(crate) const THUMB: (u32, u32) = (96, 40);
// Samples per thumbnail pixel and axis (a box average of the frame's pixels; fewer let small details
// flicker)
const THUMB_SAMPLES: u32 = 16;

/// A captured image: the capture's BGRA rows (converted only when read) or an RGB image (test images).
pub enum Pixels {
    Rgb(RgbImage),
    /// Tightly packed rows, 4 bytes per pixel
    Bgra {
        data: Vec<u8>,
        width: u32,
        height: u32,
    },
}

impl Pixels {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Pixels::Rgb(img) => img.dimensions(),
            Pixels::Bgra { width, height, .. } => (*width, *height),
        }
    }

    /// Red, green, blue at (x, y).
    fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        match self {
            Pixels::Rgb(img) => img.get_pixel(x, y).0,
            Pixels::Bgra { data, width, .. } => {
                let i = ((y * width + x) * 4) as usize;
                [data[i + 2], data[i + 1], data[i]]
            }
        }
    }

    /// The grey thumbnail for the gate: each pixel the mean of a grid of samples in its cell (cheap even
    /// on 4K frames, unlike a full resize).
    pub fn thumb(&self) -> GrayImage {
        let (w, h) = self.size();
        GrayImage::from_fn(THUMB.0, THUMB.1, |tx, ty| {
            let mut sum = 0u32;
            for sy in 0..THUMB_SAMPLES {
                for sx in 0..THUMB_SAMPLES {
                    let x = ((tx * THUMB_SAMPLES + sx) * w / (THUMB.0 * THUMB_SAMPLES)).min(w.saturating_sub(1));
                    let y = ((ty * THUMB_SAMPLES + sy) * h / (THUMB.1 * THUMB_SAMPLES)).min(h.saturating_sub(1));
                    let [r, g, b] = self.rgb(x, y);
                    sum += (r as u32 * 77 + g as u32 * 150 + b as u32 * 29) >> 8;
                }
            }
            image::Luma([(sum / (THUMB_SAMPLES * THUMB_SAMPLES)) as u8])
        })
    }

    pub fn into_rgb(self) -> RgbImage {
        match self {
            Pixels::Rgb(img) => img,
            Pixels::Bgra { data, width, height } => {
                let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
                for px in data.as_chunks::<4>().0 {
                    rgb.extend_from_slice(&[px[2], px[1], px[0]]);
                }
                RgbImage::from_raw(width, height, rgb).expect("width × height × 3 bytes")
            }
        }
    }
}

/// BGRA rows of `pitch` bytes (padded) packed tightly: the `data` of [`Pixels::Bgra`]. `data` must hold `height`
/// rows of at least `width` pixels.
pub fn unpad(data: &[u8], pitch: usize, width: u32, height: u32) -> Vec<u8> {
    let row = width as usize * 4;
    let mut out = Vec::with_capacity(row * height as usize);
    for line in data.chunks(pitch).take(height as usize) {
        out.extend_from_slice(&line[..row]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_rows_with_padding() {
        // 2×2 pixels, rows padded to 12 bytes
        let data = [
            1, 2, 3, 255, 4, 5, 6, 255, 0, 0, 0, 0, //
            7, 8, 9, 255, 10, 11, 12, 255, 0, 0, 0, 0,
        ];
        assert_eq!(unpad(&data, 12, 2, 2), vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]);
    }

    #[test]
    fn bgra_and_rgb_give_the_same_picture() {
        // 2×1: a red and a blue pixel
        let bgra = Pixels::Bgra { data: vec![0, 0, 255, 255, 255, 0, 0, 255], width: 2, height: 1 };
        let rgb = Pixels::Rgb(RgbImage::from_raw(2, 1, vec![255, 0, 0, 0, 0, 255]).unwrap());
        assert_eq!(bgra.thumb(), rgb.thumb());
        assert_eq!(bgra.into_rgb(), rgb.into_rgb());
    }

    #[test]
    fn thumb_of_a_flat_image_is_its_grey() {
        let t = Pixels::Rgb(RgbImage::from_pixel(192, 80, image::Rgb([100, 100, 100]))).thumb();
        assert_eq!(t.dimensions(), THUMB);
        // The weights sum to 256, so a flat grey stays within one level of itself
        assert!(t.pixels().all(|p| p[0].abs_diff(100) <= 1));
    }
}
