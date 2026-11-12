//! Prints one JSON line per frame (the probe, the output and the diagnostics) over folders of screenshots or
//! recordings, to compare a change against the lines before it:
//!
//! `cargo run --release -p sc-ohseearr-mining-hud --example regress-mining-hud -- [--data <root>] [--threads n]
//!  [--gpu] [<folder>...]`
//!
//! Without folders those of `regression.toml`, relative to the data root (`--data`, else as README.md says). Folder
//! arguments are used as given (relative ones against the working directory); a missing folder is skipped.
//! `--threads`: ONNX Runtime's intra-op threads, default 3. Each folder's frames: `*.jpg`, then `*.png`, sorted by
//! name. The lines go to stdout (`{"folder", "file", "probe", "output", "diagnostics"}`, or `{"folder", "file",
//! "error"}`), progress and times to stderr.
use std::time::Instant;

use sc_ohseearr_mining_hud::MiningHudReader;
use sc_ohseearr_sc::{Frame, Reader};
use sc_ohseearr_test_support::{Args, frames};

fn main() {
    let args = Args::parse();
    let folders = args.folders(env!("CARGO_MANIFEST_DIR"));
    let mut ocr = args.ocr();
    let mut reader = MiningHudReader::new();
    let mut id = 0;
    for (_, folder) in folders {
        if !folder.is_dir() {
            eprintln!("{}: missing, skipped", folder.display());
            continue;
        }
        for file in frames(&folder) {
            let Ok(image) = image::open(&file) else {
                eprintln!("{}: not an image, skipped", file.display());
                continue;
            };
            let image = image.to_rgb8();
            id += 1;
            let frame = Frame { id, image: &image, cursor: None, locale: None };
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
