//! Smoke test of the compiled-in model packages: each loads and runs on a small synthetic image (a line of block
//! letters, no game data), and the outputs have the expected shape. `cargo test` catches a broken model or manifest
//! without local data.

use image::{Rgb, RgbImage};
use sc_ohseearr_core::{NanoDet, Ocr, crop};

/// 5×7 bitmaps of the letters drawn (rows top to bottom, bit 4 = left column).
fn glyph(c: char) -> [u8; 7] {
    match c {
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        _ => [0; 7],
    }
}

/// `text` in light block letters (`scale` px per dot) on a dark ground, with a margin around it.
fn line(text: &str, scale: u32) -> RgbImage {
    let (cw, margin) = (6 * scale, 4 * scale);
    let mut img =
        RgbImage::from_pixel(2 * margin + cw * text.chars().count() as u32, 2 * margin + 7 * scale, Rgb([20, 30, 30]));
    for (i, c) in text.chars().enumerate() {
        for (row, bits) in glyph(c).iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = margin + i as u32 * cw + col * scale + dx;
                        let y = margin + row as u32 * scale + dy;
                        img.put_pixel(x, y, Rgb([230, 240, 235]));
                    }
                }
            }
        }
    }
    img
}

#[test]
fn detector_and_recognizer_run() {
    let mut ocr =
        Ocr::new(&sc_ohseearr_model_paddle_det_tiny::package(), &sc_ohseearr_model_sc_rec_latin::package(), 2, false)
            .expect("det and rec packages");
    let img = line("SCU 42", 4);
    let (w, h) = img.dimensions();
    let rects = ocr.detect(&img, h).expect("det runs");
    eprintln!("det: {rects:?}");
    // The line is found (a model fed wrongly finds nothing in it)
    assert!(rects.iter().any(|r| r.contains(w as f32 / 2.0, h as f32 / 2.0)), "{rects:?}");
    for r in &rects {
        assert!(
            r.x0 >= 0.0 && r.y0 >= 0.0 && r.x1 <= w as f32 && r.y1 <= h as f32 && r.w() > 0.0 && r.h() > 0.0,
            "{r:?}"
        );
    }
    // The recognizer on the whole line and on a crop: one result per image, a probability as score
    let crops =
        vec![img.clone(), crop(&img, sc_ohseearr_core::Rect { x0: 0.0, y0: 0.0, x1: w as f32 / 2.0, y1: h as f32 })];
    let texts = ocr.recognize_images(&crops).expect("rec runs, its dictionary fits");
    eprintln!("rec: {texts:?}");
    assert_eq!(texts.len(), crops.len());
    assert!(texts.iter().all(|(_, s)| (0.0..=1.0).contains(s)), "{texts:?}");
    // Block letters are not the game's font; the digits read anyway
    assert!(texts[0].0.ends_with("42"), "{texts:?}");
    let digits = ocr.recognize_images_only(&crops[..1], "0123456789").expect("rec with a charset");
    assert_eq!(digits.len(), 1);
    assert!(digits[0].0.chars().all(|c| c.is_ascii_digit()), "{digits:?}");
}

#[test]
fn layout_detector_runs() {
    let package = sc_ohseearr_model_sc_warehouse_layout::package();
    let classes = package.manifest.nanodet.as_ref().expect("[nanodet]").classes.clone();
    let mut nanodet = NanoDet::new(&package, 2, false).expect("layout package");
    assert_eq!(nanodet.classes(), classes.as_slice());
    let (iw, ih) = nanodet.input_size();
    // A window of the model's input size and a smaller one (padded by the family)
    for (w, h) in [(iw, ih), (iw / 2, ih / 3)] {
        let mut img = RgbImage::from_pixel(w, h, Rgb([20, 30, 30]));
        image::imageops::overlay(&mut img, &line("X42", 3), 10, 10);
        let found = nanodet.detect(&img).expect("nanodet runs");
        eprintln!("{w}×{h}: {found:?}");
        assert!(found.iter().all(|d| d.class < classes.len() && (0.0..=1.0).contains(&d.score)), "{found:?}");
    }
}

/// The raw output shape of a package's model for an all-zero input of `shape` (straight through ONNX Runtime).
fn output_shape(package: &sc_ohseearr_core::ModelPackage, shape: [usize; 4]) -> Vec<i64> {
    let mut session = ort::session::Session::builder().unwrap().commit_from_memory(&package.model).expect("model");
    let input = ort::value::Tensor::from_array((shape, vec![0f32; shape.iter().product()])).unwrap();
    let outputs = session.run(ort::inputs![input]).expect("run");
    let (out, _) = outputs[0].try_extract_tensor::<f32>().expect("f32 output");
    out.to_vec()
}

#[test]
fn raw_outputs_fit_the_manifests() {
    // det: a probability map of the input's size
    let det = sc_ohseearr_model_paddle_det_tiny::package();
    assert_eq!(output_shape(&det, [1, 3, 64, 96]), [1, 1, 64, 96]);
    // rec: per time step one probability per class (blank, the dictionary, the space)
    let rec = sc_ohseearr_model_sc_rec_latin::package();
    let height = rec.manifest.input.height.expect("rec input height") as usize;
    let classes = 1 + rec.dict.as_deref().expect("dict").lines().count() + 1;
    let shape = output_shape(&rec, [2, 3, height, 320]);
    assert_eq!((shape.len(), shape[0], shape[2]), (3, 2, classes as i64), "{shape:?}");
    // nanodet: per prior point of every stride the classes' scores and 4 × (reg_max + 1) bins
    let layout = sc_ohseearr_model_sc_warehouse_layout::package();
    let n = layout.manifest.nanodet.as_ref().expect("[nanodet]");
    let (w, h) = (layout.manifest.input.width.expect("width"), layout.manifest.input.height.expect("height"));
    let priors: u32 = n.strides.iter().map(|s| w.div_ceil(*s) * h.div_ceil(*s)).sum();
    let shape = output_shape(&layout, [1, 3, h as usize, w as usize]);
    assert_eq!(shape, [1, priors as i64, (n.classes.len() + 4 * (n.reg_max + 1)) as i64]);
}
