//! The regression run: one JSON line per frame (neutral output + diagnostics) over folders of debug recordings
//! (`read-N.jpg` with the cursor in `read-N.json`) or screenshots (no cursor).
//!
//! `cargo run --release --example regress -- [<folder>...] [--data <root>] [--threads n] [--stateful] [--gpu]`
//!
//! Without folders those of `regression.toml` (relative to the data root: `--data`, else `SC_OHSEEARR_DATA`, else
//! `<repo>/local`). Folder arguments are used as given (relative ones against the working directory). By default
//! every frame goes through `read_components` (stateless, the reference of the companion's dump);
//! `--stateful` runs a `GearInventoryReader` over each folder's frames in order instead (probe, read, the kept
//! scale). The lines go to stdout (the timing to stderr): `{"frame", "cursor", "output", "diagnostics"}`.
use std::path::{Path, PathBuf};

use sc_ohseearr_core::Ocr;
use sc_ohseearr_gear_inventory::{GearInventoryReader, read_components};
use sc_ohseearr_sc::reader::{Frame, ScreenReader};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let threads: usize = take_value(&mut args, "--threads").map_or(4, |n| n.parse().expect("--threads n"));
    let data = take_value(&mut args, "--data")
        .or_else(|| std::env::var("SC_OHSEEARR_DATA").ok())
        .map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local"), PathBuf::from);
    let stateful = take_flag(&mut args, "--stateful");
    let gpu = take_flag(&mut args, "--gpu");
    let folders: Vec<PathBuf> = if args.is_empty() {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("regression.toml"))
            .expect("regression.toml");
        let config: toml::Table = toml::from_str(&text).expect("regression.toml");
        config["folders"].as_array().expect("folders").iter().map(|f| data.join(f.as_str().expect("path"))).collect()
    } else {
        args.iter().map(PathBuf::from).collect()
    };
    let mut ocr = Ocr::new(
        &sc_ohseearr_model_paddle_det_tiny::package(),
        &sc_ohseearr_model_sc_rec_latin::package(),
        threads,
        gpu,
    )
    .expect("ocr models");
    let (mut frames, mut total_ms) = (0u128, 0u128);
    let mut id = 0u64;
    for dir in &folders {
        let mut reader = GearInventoryReader::new();
        let folder = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        for path in sorted(dir, "jpg").into_iter().chain(sorted(dir, "png")) {
            let name = path.file_stem().expect("file name").to_string_lossy().to_string();
            let image = image::open(&path).expect("frame").to_rgb8();
            let cursor = cursor_of(&path);
            let started = std::time::Instant::now();
            let reading = if stateful {
                id += 1;
                let frame = Frame { id, image: &image, cursor, locale: None };
                reader.probe(&mut ocr, &frame).expect("probe");
                reader.read(&mut ocr, &frame).expect("read")
            } else {
                read_components(&mut ocr, &image, cursor, None).expect("read")
            };
            total_ms += started.elapsed().as_millis();
            frames += 1;
            let line = serde_json::json!({
                "frame": format!("{folder}/{name}"),
                "cursor": cursor,
                "output": reading.output,
                "diagnostics": reading.diagnostics,
            });
            println!("{line}");
        }
    }
    if let Some(avg) = total_ms.checked_div(frames) {
        eprintln!("timing: {frames} frames, {avg} ms per read");
    }
}

fn take_value(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.remove(i);
    Some(args.remove(i))
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    args.iter().position(|a| a == flag).map(|i| args.remove(i)).is_some()
}

/// The cursor a debug recording stored with the frame (`read-N.json` next to `read-N.jpg`), if any.
fn cursor_of(frame: &Path) -> Option<(f32, f32)> {
    let text = std::fs::read_to_string(frame.with_extension("json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let c = json.get("cursor")?.as_array()?;
    Some((c.first()?.as_f64()? as f32, c.get(1)?.as_f64()? as f32))
}

/// The files of `dir` with the extension `ext`, sorted by name (as the companion's dump does).
fn sorted(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}
