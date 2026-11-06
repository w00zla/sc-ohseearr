//! The regression run: one JSON line per frame (neutral output + diagnostics) over folders of debug recordings
//! (`read-N.jpg` with the cursor in `read-N.json`) or screenshots (no cursor).
//!
//! `cargo run --release -p sc-ohseearr-gear-inventory --example regress-gear-inventory -- [<folder>...]
//!  [--data <root>] [--threads n] [--stateful] [--gpu]`
//!
//! Without folders those of `regression.toml`, relative to the data root (`--data`, else as README.md says).
//! Folder arguments are used as given (relative ones against the working directory); a missing folder is skipped.
//! `--threads`: ONNX Runtime's intra-op threads, default 3. Each folder's frames: `*.jpg`, then `*.png`, sorted by
//! name. By default every frame goes through `read_components` (stateless, the reference of the companion's dump);
//! `--stateful` runs a `GearInventoryReader` over each folder's frames in order instead (probe, read, the kept
//! scale). The lines go to stdout (the timing to stderr): `{"frame", "cursor", "output", "diagnostics"}`; `frame`
//! is `<folder's name>/<file stem>`.
use sc_ohseearr_gear_inventory::{GearInventoryReader, read_components};
use sc_ohseearr_sc::reader::{Frame, ScreenReader};
use sc_ohseearr_test_support::{Args, cursor_of, frames};

fn main() {
    let mut args = Args::parse();
    let stateful = args.flag("--stateful");
    let folders = args.folders(env!("CARGO_MANIFEST_DIR"));
    let mut ocr = args.ocr();
    let (mut read, mut total_ms) = (0u128, 0u128);
    let mut id = 0u64;
    for (_, dir) in &folders {
        if !dir.is_dir() {
            eprintln!("{}: missing, skipped", dir.display());
            continue;
        }
        let mut reader = GearInventoryReader::new();
        let folder = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        for path in frames(dir) {
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
            read += 1;
            let line = serde_json::json!({
                "frame": format!("{folder}/{name}"),
                "cursor": cursor,
                "output": reading.output,
                "diagnostics": reading.diagnostics,
            });
            println!("{line}");
        }
    }
    if let Some(avg) = total_ms.checked_div(read) {
        eprintln!("timing: {read} frames, {}, {avg} ms per read", args.mode());
    }
}
