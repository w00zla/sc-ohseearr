//! Prints what the OCR reads in an image, one line per text box (x, y, width, height, score, text):
//! `cargo run --release --example ocr_dump -- <image> [max_height] [threads] [--det <package dir>] [--rec <package dir>] [--gpu]`
//!
//! Without `--det`/`--rec` the compiled-in packages (`paddle-det-tiny`, `sc-rec-latin`); with them a package
//! folder loaded at runtime (`model.toml` + `model.onnx` [+ dictionary]). `--gpu` needs the feature `directml`
//! and DirectML.dll 1.15 next to the exe.
use std::path::Path;

use sc_ohseearr_core::{ModelPackage, Ocr};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |flag: &str| args.iter().position(|a| a == flag).map(|i| {
        args.remove(i);
        args.remove(i)
    });
    let det = take("--det").map_or_else(|| Ok(sc_ohseearr_model_paddle_det_tiny::package()), |d| ModelPackage::load(Path::new(&d))).expect("detector package");
    let rec = take("--rec").map_or_else(|| Ok(sc_ohseearr_model_sc_rec_latin::package()), |d| ModelPackage::load(Path::new(&d))).expect("recognizer package");
    let gpu = args.iter().position(|a| a == "--gpu").map(|i| args.remove(i)).is_some();
    let img = image::open(&args[0]).expect("image").to_rgb8();
    let max_height: u32 = args.get(1).map_or(800, |s| s.parse().expect("max_height"));
    let threads: usize = args.get(2).map_or(4, |s| s.parse().expect("threads"));
    let mut ocr = Ocr::new(&det, &rec, threads, gpu).expect("models");
    let t = std::time::Instant::now();
    let boxes = ocr.read(&img, max_height).expect("read");
    for b in &boxes {
        println!("{:5.0} {:5.0} {:5.0} {:5.0}  {:.2}  {}", b.rect.x0, b.rect.y0, b.rect.w(), b.rect.h(), b.score, b.text);
    }
    eprintln!("{} boxes in {:?} ({})", boxes.len(), t.elapsed(), ocr.take_timings());
}
