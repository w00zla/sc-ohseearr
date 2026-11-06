//! Prints one JSON line per frame (the probe, the output and the diagnostics) over folders of screenshots or
//! debug recordings, to compare a change against the lines before it:
//!
//! `cargo run --release -p sc-ohseearr-refinery --example regress -- [--english <global.ini>] [--pack <global.ini>]
//!  [--data <root>] [--threads n] [--diagnostics] [<folder>...]`
//!
//! The English table and/or a language pack are needed (the reader anchors on loc keys). Without folders it
//! reads `regression.toml`'s, relative to the data root (`--data`, else `$SC_OHSEEARR_DATA`, else `<repo>/local`).
//! A folder given on the command line is used as is (relative to the working directory). The lines go to stdout,
//! progress and times to stderr.
use std::path::{Path, PathBuf};
use std::time::Instant;

use sc_ohseearr_core::Ocr;
use sc_ohseearr_refinery::RefineryReader;
use sc_ohseearr_sc::{Frame, LocTable, Locale, ScreenReader};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |flag: &str| {
        args.iter().position(|a| a == flag).map(|i| {
            args.remove(i);
            args.remove(i)
        })
    };
    let english = take("--english");
    let pack = take("--pack");
    let data = take("--data");
    let threads: usize = take("--threads").map_or(4, |s| s.parse().expect("threads"));
    let diagnostics = args.iter().position(|a| a == "--diagnostics").map(|i| args.remove(i)).is_some();

    let load = |p: Option<String>| p.map(|p| LocTable::load(Path::new(&p)).unwrap_or_else(|e| panic!("{p}: {e}")));
    let locale = Locale::new(load(pack), load(english)).expect("--english and/or --pack");

    let root = data
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("SC_OHSEEARR_DATA").map(PathBuf::from))
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local"));
    let folders: Vec<PathBuf> = if args.is_empty() {
        let manifest = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("regression.toml")).unwrap();
        let manifest: toml::Table = manifest.parse().expect("regression.toml");
        manifest["folders"].as_array().expect("folders").iter().map(|f| root.join(f.as_str().unwrap())).collect()
    } else {
        args.iter().map(PathBuf::from).collect()
    };

    let mut ocr = Ocr::new(
        &sc_ohseearr_model_paddle_det_tiny::package(),
        &sc_ohseearr_model_sc_rec_latin::package(),
        threads,
        false,
    )
    .expect("models");
    let mut reader = RefineryReader::new();
    reader.set_diagnostics(diagnostics);
    let mut id = 0;
    for folder in folders {
        let Ok(dir) = std::fs::read_dir(&folder) else {
            eprintln!("{}: missing, skipped", folder.display());
            continue;
        };
        let mut files: Vec<PathBuf> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png"))
            })
            .collect();
        files.sort();
        for file in files {
            let Ok(image) = image::open(&file) else {
                eprintln!("{}: not an image, skipped", file.display());
                continue;
            };
            let image = image.to_rgb8();
            id += 1;
            let frame = Frame { id, image: &image, cursor: None, locale: Some(&locale) };
            let started = Instant::now();
            let probe = reader.probe(&mut ocr, &frame).map_err(|e| e.to_string());
            let probe_ms = started.elapsed().as_millis();
            let started = Instant::now();
            let read = reader.read(&mut ocr, &frame).map_err(|e| e.to_string());
            let read_ms = started.elapsed().as_millis();
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            let line = match (probe, read) {
                (Ok(probe), Ok(r)) => serde_json::json!({
                    "folder": folder.file_name().unwrap().to_string_lossy(),
                    "file": name,
                    "probe": probe,
                    "output": r.output,
                    "diagnostics": r.diagnostics,
                }),
                (probe, read) => serde_json::json!({
                    "folder": folder.file_name().unwrap().to_string_lossy(),
                    "file": name,
                    "error": probe.err().or(read.err()),
                }),
            };
            println!("{line}");
            eprintln!("{name}: probe {probe_ms} ms, read {read_ms} ms");
        }
    }
}
