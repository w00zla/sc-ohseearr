//! Opens the capture of an executable's window, grabs a frame and writes it as PNG, then waits for a frame
//! with the mouse cursor at rest.
//!
//! `cargo run --release -p sc-ohseearr-capture-linux --example grab-linux -- [exe] [out.png] [token file] [delay_s]`
//! (defaults: `StarCitizen.exe`, `grab.png`, no token file, 0). The executable must run. The portal shows its
//! dialog: pick the executable's window. With a token file (`-` for none) the restore token is kept there, and
//! the next run opens the same window without the dialog. The cursor is only known while it is in the window:
//! give `delay_s` to move it there after starting.

#[cfg(target_os = "linux")]
fn main() {
    use std::time::{Duration, Instant};

    use sc_ohseearr_capture_linux::WindowCapture;

    let mut args = std::env::args().skip(1);
    let exe = args.next().unwrap_or_else(|| "StarCitizen.exe".into());
    let out = args.next().unwrap_or_else(|| "grab.png".into());
    let token_file = args.next().filter(|file| file != "-");
    let delay: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let mut capture = WindowCapture::new(&exe);
    let token = token_file.as_ref().and_then(|file| std::fs::read_to_string(file).ok());
    let token = token.as_deref().map(str::trim).filter(|token| !token.is_empty());
    let asked = Instant::now();
    let new_token = match capture.open(token) {
        Ok(new_token) => new_token,
        Err(e) => {
            eprintln!("cannot open the capture of {exe}: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "open after {:.1} s ({}), restore token: {}",
        asked.elapsed().as_secs_f32(),
        if token.is_some() { "a restore token was sent" } else { "no restore token" },
        if new_token.is_some() { "returned" } else { "none" }
    );
    if let (Some(file), Some(new_token)) = (&token_file, &new_token)
        && let Err(e) = std::fs::write(file, new_token)
    {
        eprintln!("cannot write {file}: {e}");
    }

    std::thread::sleep(Duration::from_secs(delay));

    match capture.grab(Duration::from_secs(2)) {
        Ok(Some(c)) => {
            let (w, h) = c.pixels.size();
            println!("{w}×{h}, cursor {:?}, {:?}", c.cursor, c.info);
            match c.pixels.into_rgb().save(&out) {
                Ok(()) => println!("written: {out}"),
                Err(e) => eprintln!("cannot write {out}: {e}"),
            }
        }
        Ok(None) => eprintln!("no image within 2 s"),
        Err(e) => {
            eprintln!("capture failed: {e}");
            std::process::exit(1);
        }
    }

    println!("waiting for the cursor to rest (10 s at most) …");
    let waiting = Instant::now();
    while waiting.elapsed() < Duration::from_secs(10) {
        match capture.grab_rested(Duration::from_secs(1)) {
            Ok(Some(c)) => {
                println!("at rest for {} ms: cursor {:?}, {:?}", c.rested.as_millis(), c.cursor, c.info);
                break;
            }
            Ok(None) => println!("no image at rest within 1 s"),
            Err(e) => {
                eprintln!("capture failed: {e}");
                std::process::exit(1);
            }
        }
    }
    capture.close();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("this window capture is Linux only");
}
