//! The warehouse layout model as an example of a layout detector plus targeted reads: finds the count badges,
//! quality texts and tooltip lines in a window of a freight manager screenshot and reads each box.
//! `cargo run --release --example layout_dump -- <image> [x y w h] [--layout <package dir>]`
//!
//! The window (frame pixels) defaults to the whole image; the model sees it scaled to 480×320, so give it a
//! window of that aspect ratio around the grid (at 720 px frame height a 480×320 window holds about two by two
//! tiles). Prints per box: class, score, box, text.
use std::path::Path;

use image::imageops;
use sc_ohseearr_core::{ModelPackage, NanoDet, Ocr, Rect, crop};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let layout = match args.iter().position(|a| a == "--layout") {
        Some(i) => {
            args.remove(i);
            ModelPackage::load(Path::new(&args.remove(i))).expect("layout package")
        }
        None => sc_ohseearr_model_sc_warehouse_layout::package(),
    };
    let frame = image::open(&args[0]).expect("image").to_rgb8();
    let n: Vec<u32> = args[1..].iter().map(|s| s.parse().expect("x y w h")).collect();
    let (x, y, w, h) = if n.len() == 4 { (n[0], n[1], n[2], n[3]) } else { (0, 0, frame.width(), frame.height()) };
    let window = imageops::crop_imm(&frame, x, y, w, h).to_image();

    let mut detector = NanoDet::new(&layout, 3, false).expect("layout model");
    let mut ocr =
        Ocr::new(&sc_ohseearr_model_paddle_det_tiny::package(), &sc_ohseearr_model_sc_rec_latin::package(), 3, false)
            .expect("ocr models");
    let found = detector.detect(&window).expect("detect");
    let crops: Vec<_> = found.iter().map(|d| crop(&window, d.rect)).collect();
    let texts = ocr.recognize_images(&crops).expect("read");
    for (d, (text, score)) in found.iter().zip(texts) {
        let r = Rect {
            x0: d.rect.x0 + x as f32,
            y0: d.rect.y0 + y as f32,
            x1: d.rect.x1 + x as f32,
            y1: d.rect.y1 + y as f32,
        };
        println!(
            "{:8} {:.2}  {:5.0} {:5.0} {:4.0} {:4.0}  {:.2}  {}",
            detector.classes()[d.class],
            d.score,
            r.x0,
            r.y0,
            r.w(),
            r.h(),
            score,
            text
        );
    }
}
