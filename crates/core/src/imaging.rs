//! Image helpers shared by the models: SIMD resizing, crops, input tensors.

pub use fast_image_resize::FilterType;
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::RgbImage;
use image::imageops;

use crate::geom::Rect;
use crate::manifest::{ChannelOrder, Input};

/// `img` scaled to `width`×`height` (at least one pixel each) with `filter`: SIMD (`fast_image_resize`), the
/// same kernels as `image`'s resize (its Triangle is `Bilinear` here). The text detector needs that smoothing:
/// fed a crop directly at a size of multiples of 32, it lost small texts.
pub fn resize(img: &RgbImage, width: u32, height: u32, filter: FilterType) -> RgbImage {
    let src = ImageRef::new(img.width(), img.height(), img.as_raw(), PixelType::U8x3).expect("an RgbImage's buffer");
    let mut dst = Image::new(width, height, PixelType::U8x3);
    Resizer::new()
        .resize(&src, &mut dst, &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter)))
        .expect("one pixel type, no crop");
    RgbImage::from_raw(width, height, dst.into_vec()).expect("width × height × 3 bytes")
}

/// The part of `img` in `rect` (at least one pixel).
pub fn crop(img: &RgbImage, rect: Rect) -> RgbImage {
    let (w, h) = img.dimensions();
    let x0 = (rect.x0.floor().max(0.0) as u32).min(w.saturating_sub(1));
    let y0 = (rect.y0.floor().max(0.0) as u32).min(h.saturating_sub(1));
    let cw = ((rect.x1.ceil() as u32).min(w)).saturating_sub(x0).max(1);
    let ch = ((rect.y1.ceil() as u32).min(h)).saturating_sub(y0).max(1);
    imageops::crop_imm(img, x0, y0, cw, ch).to_image()
}

/// A pixel's channels in the model's order.
pub(crate) fn channels(order: ChannelOrder, p: &image::Rgb<u8>) -> [u8; 3] {
    match order {
        ChannelOrder::Bgr => [p[2], p[1], p[0]],
        ChannelOrder::Rgb => [p[0], p[1], p[2]],
    }
}

/// CHW planes in the input's channel order, normalized as `(x/255 - mean) / std` per channel (mean and std
/// given in that order).
pub(crate) fn to_chw(img: &RgbImage, width: u32, input: &Input) -> Vec<f32> {
    let plane = (img.height() * width) as usize;
    let mut data = vec![0f32; 3 * plane];
    let scale: [f32; 3] = std::array::from_fn(|c| 1.0 / (255.0 * input.std[c]));
    let offset: [f32; 3] = std::array::from_fn(|c| input.mean[c] / input.std[c]);
    for (x, y, p) in img.enumerate_pixels() {
        let i = (y * width + x) as usize;
        for (c, v) in channels(input.order, p).into_iter().enumerate() {
            data[c * plane + i] = v as f32 * scale[c] - offset[c];
        }
    }
    data
}
