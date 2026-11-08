//! The regression run: one JSON line per frame (neutral output + diagnostics) over folders of debug recordings
//! (`read-N.jpg` with the cursor in `read-N.json`) or screenshots (no cursor).
//!
//! `cargo run --release -p sc-ohseearr-warehouse --example regress-warehouse -- [<folder>...] [--data <root>]
//!  [--threads n] [--gpu]`
//!
//! Without folders those of `regression.toml`, relative to the data root (`--data`, else as README.md says).
//! Folder arguments are used as given (relative ones against the working directory); a missing folder is skipped.
//! `--threads`: ONNX Runtime's intra-op threads, default 3. Each folder's frames (`*.jpg`, then `*.png`, sorted by
//! name) go through one `WarehouseReader` in order, as the companion reads them: the prior of a strong fit carries
//! over to the next frames, `reset` at each folder. Diagnostics are on (every tile's badge read), as in the
//! companion's `inventory_dump` and its debug window. The lines go to stdout (the timing to stderr):
//! `{"frame", "cursor", "output", "diagnostics"}`; `frame` is `<folder>/<file stem>`.
use sc_ohseearr_core::NanoDet;
use sc_ohseearr_sc::{Frame, Reader};
use sc_ohseearr_test_support::{Args, cursor_of, frames};
use sc_ohseearr_warehouse::WarehouseReader;

fn main() {
    let args = Args::parse();
    let folders = args.folders(env!("CARGO_MANIFEST_DIR"));
    let mut ocr = args.ocr();
    // The layout detector on the same threads and device as the OCR
    let layout =
        NanoDet::new(&sc_ohseearr_model_sc_warehouse_layout::package(), args.threads, args.gpu).expect("layout model");
    let mut reader = WarehouseReader::new(layout).expect("layout model");
    reader.set_diagnostics(true);
    let (mut read, mut total_ms) = (0u128, 0u128);
    let mut id = 0u64;
    for (folder, dir) in &folders {
        if !dir.is_dir() {
            eprintln!("{}: missing, skipped", dir.display());
            continue;
        }
        reader.reset();
        for path in frames(dir) {
            let name = path.file_stem().expect("file name").to_string_lossy().to_string();
            let image = image::open(&path).expect("frame").to_rgb8();
            let cursor = cursor_of(&path);
            id += 1;
            let frame = Frame { id, image: &image, cursor, locale: None };
            let started = std::time::Instant::now();
            let reading = reader.read(&mut ocr, &frame).expect("read");
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
