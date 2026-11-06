//! Grabs one frame of an executable's window and writes it as PNG.
//!
//! `cargo run --release -p sc-ohseearr-capture-win --example grab -- [exe] [out.png] [delay_s]`
//! (defaults: `StarCitizen.exe`, `grab.png`, 0). The window must be in front: give `delay_s` to switch to
//! it after starting.

#[cfg(windows)]
fn main() {
    use std::time::Duration;

    use sc_ohseearr_capture_win::{Grab, WindowCapture, set_per_monitor_dpi_aware};

    let mut args = std::env::args().skip(1);
    let exe = args.next().unwrap_or_else(|| "StarCitizen.exe".into());
    let out = args.next().unwrap_or_else(|| "grab.png".into());
    let delay: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    // Already set is fine
    let _ = set_per_monitor_dpi_aware();
    let mut capture = WindowCapture::new(&exe);
    std::thread::sleep(Duration::from_secs(delay));

    let Some(found) = capture.window() else {
        let why = if capture.is_running() { "running, but not in front or minimized" } else { "not running" };
        eprintln!("window not found: {exe} ({why})");
        std::process::exit(1);
    };
    if !found.in_front {
        eprintln!("window not found in front: {exe}");
        std::process::exit(1);
    }
    // The first grabs may bring no new image (timeout, pointer only): try a few times
    for _ in 0..10 {
        match capture.grab_window(found.window) {
            Ok(Grab::Frame(c)) => {
                let (w, h) = c.pixels.size();
                println!("{w}×{h}, cursor {:?}, {:?}", c.cursor, c.info);
                if let Err(e) = c.pixels.into_rgb().save(&out) {
                    eprintln!("cannot write {out}: {e}");
                    std::process::exit(1);
                }
                println!("written: {out}");
                return;
            }
            Ok(Grab::Skipped(skip)) => eprintln!("no new image ({skip:?}), trying again"),
            Err(e) => {
                eprintln!("capture failed: {e}");
                std::process::exit(1);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("no frame within 10 tries");
    std::process::exit(1);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("window capture is Windows only");
}
