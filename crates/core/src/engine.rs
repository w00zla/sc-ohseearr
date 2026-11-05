//! ONNX Runtime sessions of one model, on the CPU or on the GPU (DirectML).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::ValueType;

use crate::manifest::Bytes;

/// A model's ONNX Runtime sessions. On the CPU one for every input shape. On the GPU (DirectML, its
/// redistributable DirectML.dll next to the exe) one per input shape with its dimensions fixed: DirectML
/// compiles a session's graph for the first shape it runs, every other shape cost ~20 ms of CPU per run
/// (2026-10-27, RTX 5080). Callers keep the shapes few (fixed screen areas, padded recognizer batches); a read
/// then takes a few ms of CPU instead of hundreds. Each session keeps its largest run's buffers, so the VRAM is
/// the sum over the shapes.
pub(crate) enum Engine {
    Cpu(Session),
    Gpu { model: Bytes, symbols: Vec<String>, sessions: HashMap<Vec<usize>, Session> },
}

impl Engine {
    /// `threads`: ONNX Runtime's intra-op threads on the CPU. `gpu`: DirectML (an error when it does not work).
    pub(crate) fn new(name: &str, model: Bytes, threads: usize, gpu: bool) -> Result<Self, String> {
        if !gpu {
            return Ok(Self::Cpu(session(name, &model, threads, None)?));
        }
        // A first session tells whether DirectML works at all and the input's dimension names
        let probe = session(name, &model, 1, Some(&[]))?;
        let symbols = match probe.inputs()[0].dtype() {
            ValueType::Tensor { dimension_symbols, .. } => dimension_symbols.to_vec(),
            other => return Err(format!("{name}: input {other:?}")),
        };
        Ok(Self::Gpu { model, symbols, sessions: HashMap::new() })
    }

    /// The session for an input of `shape`.
    pub(crate) fn session(&mut self, name: &str, shape: &[usize]) -> Result<&mut Session, String> {
        match self {
            Self::Cpu(s) => Ok(s),
            Self::Gpu { model, symbols, sessions } => {
                if !sessions.contains_key(shape) {
                    let started = Instant::now();
                    let dims: Vec<(String, i64)> = symbols.iter().zip(shape).filter(|(s, _)| !s.is_empty()).map(|(s, &v)| (s.clone(), v as i64)).collect();
                    sessions.insert(shape.to_vec(), session(name, model, 1, Some(&dims))?);
                    log::debug!("OCR {name} on the GPU for {shape:?} in {:?} ({} sessions)", started.elapsed(), sessions.len());
                }
                Ok(sessions.get_mut(shape).expect("inserted above"))
            }
        }
    }

    pub(crate) fn gpu(&self) -> bool {
        matches!(self, Self::Gpu { .. })
    }
}

/// One session on the CPU (`gpu` None) or on DirectML with these dimensions fixed.
fn session(name: &str, model: &[u8], threads: usize, gpu: Option<&[(String, i64)]>) -> Result<Session, String> {
    let e = |x: &dyn std::fmt::Display| format!("{name}: {x}");
    let mut b = Session::builder().map_err(|x| e(&x))?;
    b = b.with_optimization_level(GraphOptimizationLevel::Level3).map_err(|x| e(&x))?.with_intra_threads(threads).map_err(|x| e(&x))?;
    if let Some(dims) = gpu {
        #[cfg(not(all(windows, feature = "directml")))]
        return Err(format!("{name}: no GPU in this build ({dims:?}; DirectML: Windows with the feature `directml`)"));
        // DirectML needs sequential runs without memory patterns (ONNX Runtime's documentation)
        #[cfg(all(windows, feature = "directml"))]
        {
            b = b.with_memory_pattern(false).map_err(|x| e(&x))?.with_parallel_execution(false).map_err(|x| e(&x))?;
            b = b.with_execution_providers([ort::ep::directml::DirectML::default().build().error_on_failure()]).map_err(|x| e(&x))?;
            for (symbol, size) in dims {
                b = b.with_dimension_override(symbol, *size).map_err(|x| e(&x))?;
            }
        }
    }
    b.commit_from_memory(model).map_err(|x| e(&x))
}

/// Where the time went since the last `take_timings`: per model kind the preparation (resize, crops, input
/// tensor), the model run and the post-processing (boxes, decoding). Layout detectors count as detectors.
#[derive(Debug, Default, Clone, Copy)]
pub struct Timings {
    pub det_prep: Duration,
    pub det_run: Duration,
    pub det_post: Duration,
    pub det_runs: u32,
    pub rec_prep: Duration,
    pub rec_run: Duration,
    pub rec_post: Duration,
    pub rec_runs: u32,
    pub rec_lines: u32,
}

impl Timings {
    pub fn total(&self) -> Duration {
        self.det_prep + self.det_run + self.det_post + self.rec_prep + self.rec_run + self.rec_post
    }
}

impl std::ops::AddAssign for Timings {
    fn add_assign(&mut self, o: Self) {
        self.det_prep += o.det_prep;
        self.det_run += o.det_run;
        self.det_post += o.det_post;
        self.det_runs += o.det_runs;
        self.rec_prep += o.rec_prep;
        self.rec_run += o.rec_run;
        self.rec_post += o.rec_post;
        self.rec_runs += o.rec_runs;
        self.rec_lines += o.rec_lines;
    }
}

impl std::fmt::Display for Timings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = |d: Duration| d.as_millis();
        write!(
            f,
            "det {}+{}+{} ms ({} runs), rec {}+{}+{} ms ({} lines in {} runs)",
            ms(self.det_prep),
            ms(self.det_run),
            ms(self.det_post),
            self.det_runs,
            ms(self.rec_prep),
            ms(self.rec_run),
            ms(self.rec_post),
            self.rec_lines,
            self.rec_runs
        )
    }
}
