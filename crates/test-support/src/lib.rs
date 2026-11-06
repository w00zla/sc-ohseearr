//! What the use cases' tests and `regress` examples share: the data root, the arguments (`--data`, `--threads`,
//! `--gpu`), the folders of `regression.toml`, the frames of a folder, the cursor of a debug recording's frame and
//! the OCR they run on. Dev-only (`publish = false`).

use std::path::{Path, PathBuf};

use image::RgbImage;
use sc_ohseearr_core::Ocr;

/// The default of `--threads`: ONNX Runtime's intra-op threads, as in the companion's regression dumps.
pub const THREADS: usize = 3;
/// The threads the tests' `Ocr` runs on.
pub const TEST_THREADS: usize = 4;

/// The data root of tests and regress runs: `$SC_OHSEEARR_DATA`, else `<repo>/local`.
pub fn data_root() -> PathBuf {
    std::env::var_os("SC_OHSEEARR_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local"))
}

/// The detector and recognizer packages compiled in, on `threads` CPU threads or the GPU.
pub fn ocr(threads: usize, gpu: bool) -> Ocr {
    Ocr::new(&sc_ohseearr_model_paddle_det_tiny::package(), &sc_ohseearr_model_sc_rec_latin::package(), threads, gpu)
        .expect("ocr models")
}

/// The tests' `Ocr`: `TEST_THREADS` on the CPU.
pub fn test_ocr() -> Ocr {
    ocr(TEST_THREADS, false)
}

/// The screenshot folder `<data root>/screenshots/<folder>`; None (and a note on stderr) when it is missing, so the
/// test is skipped.
pub fn screenshots(folder: &str) -> Option<PathBuf> {
    let dir = data_root().join("screenshots").join(folder);
    if !dir.is_dir() {
        eprintln!("{} missing, skipped", dir.display());
        return None;
    }
    Some(dir)
}

/// The `*.jpg` of `dir` whose name ends in `suffix`.
pub fn shot(dir: &Path, suffix: &str) -> RgbImage {
    let file = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with(&format!("{suffix}.jpg")))
        .unwrap_or_else(|| panic!("{}: no {suffix}.jpg", dir.display()));
    image::open(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display())).to_rgb8()
}

/// The frames of `dir`:`*.jpg`, then `*.png` (case-insensitive), each sorted by name; none for a missing folder.
pub fn frames(dir: &Path) -> Vec<PathBuf> {
    let of = |ext: &str| {
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
    };
    let mut files = of("jpg");
    files.extend(of("png"));
    files
}

/// The cursor a debug recording stored with the frame (`read-N.json` next to `read-N.jpg`), if any.
pub fn cursor_of(frame: &Path) -> Option<(f32, f32)> {
    let text = std::fs::read_to_string(frame.with_extension("json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let c = json.get("cursor")?.as_array()?;
    Some((c.first()?.as_f64()? as f32, c.get(1)?.as_f64()? as f32))
}

/// The folders of `regression.toml` in `manifest_dir`, relative to `root`: (as named, path).
pub fn regression_folders(manifest_dir: &Path, root: &Path) -> Vec<(String, PathBuf)> {
    let path = manifest_dir.join("regression.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let config: toml::Table = toml::from_str(&text).expect("regression.toml");
    config["folders"]
        .as_array()
        .expect("regression.toml: folders")
        .iter()
        .map(|f| f.as_str().expect("regression.toml: a folder path"))
        .map(|f| (f.to_string(), root.join(f)))
        .collect()
}

/// A regress example's command line: the shared options taken out at once, the example's own with
/// [`value`](Self::value) and [`flag`](Self::flag), the rest are folders ([`folders`](Self::folders)).
///
/// - `--data <root>`: the data root (default [`data_root`]);
/// - `--threads <n>`: ONNX Runtime's intra-op threads on the CPU (default [`THREADS`]);
/// - `--gpu`: DirectML (feature `directml`).
pub struct Args {
    pub data: PathBuf,
    pub threads: usize,
    pub gpu: bool,
    rest: Vec<String>,
}

impl Args {
    /// The process's arguments.
    pub fn parse() -> Self {
        Self::from(std::env::args().skip(1).collect())
    }

    /// These arguments.
    pub fn from(rest: Vec<String>) -> Self {
        let mut args = Self { data: PathBuf::new(), threads: THREADS, gpu: false, rest };
        args.data = args.value("--data").map_or_else(data_root, PathBuf::from);
        args.threads = args.value("--threads").map_or(THREADS, |n| n.parse().expect("--threads n"));
        args.gpu = args.flag("--gpu");
        args
    }

    /// Takes out `flag` and the value after it.
    pub fn value(&mut self, flag: &str) -> Option<String> {
        let i = self.rest.iter().position(|a| a == flag)?;
        self.rest.remove(i);
        assert!(i < self.rest.len(), "{flag} needs a value");
        Some(self.rest.remove(i))
    }

    /// Takes out `flag`; whether it was given.
    pub fn flag(&mut self, flag: &str) -> bool {
        self.rest.iter().position(|a| a == flag).map(|i| self.rest.remove(i)).is_some()
    }

    /// The remaining arguments as folders (used as given, relative ones against the working directory), else
    /// those of `regression.toml` in `manifest_dir` relative to the data root: (as named, path).
    pub fn folders(&self, manifest_dir: &str) -> Vec<(String, PathBuf)> {
        if self.rest.is_empty() {
            regression_folders(Path::new(manifest_dir), &self.data)
        } else {
            self.rest.iter().map(|a| (a.clone(), PathBuf::from(a))).collect()
        }
    }

    /// The `Ocr` on these terms.
    pub fn ocr(&self) -> Ocr {
        ocr(self.threads, self.gpu)
    }

    /// "GPU" or "n threads", for the timing line.
    pub fn mode(&self) -> String {
        if self.gpu { "GPU".to_string() } else { format!("{} threads", self.threads) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_options_come_out_the_rest_stays() {
        let to = |s: &[&str]| s.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let mut a =
            Args::from(to(&["a", "--threads", "5", "--english", "en.ini", "--gpu", "--data", "root", "--stateful"]));
        assert_eq!((a.threads, a.gpu, a.data.as_path()), (5, true, Path::new("root")));
        assert_eq!(a.value("--english").as_deref(), Some("en.ini"));
        assert!(a.flag("--stateful"));
        assert!(!a.flag("--stateful"));
        assert_eq!(a.folders("unused"), vec![("a".to_string(), PathBuf::from("a"))]);
        let a = Args::from(Vec::new());
        assert_eq!((a.threads, a.gpu), (THREADS, false));
    }
}
