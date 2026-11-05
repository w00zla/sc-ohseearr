//! OCR engine for Star Citizen screens on ONNX Runtime, on the CPU or the GPU (DirectML, feature `directml`).
//!
//! Three model families, each run from a model package (an ONNX model with its `model.toml` manifest):
//! - `DbDetector`: where text is (PaddleOCR's det models, DB post-processing);
//! - `CtcRecognizer`: what one text line says (PaddleOCR's rec models, CTC decoding);
//! - `NanoDet`: where a screen's parts are, by class (a layout detector trained per screen).
//!
//! `Ocr` pairs a detector and a recognizer for the usual "find the lines, read them".

mod det;
mod engine;
mod geom;
mod imaging;
mod manifest;
mod nanodet;
mod ocr;
mod rec;

pub use det::{CPU_MAX_HEIGHT, DbDetector};
pub use engine::Timings;
pub use geom::{Detection, Rect, TextBox, iou, nms};
pub use imaging::{FilterType, crop, resize};
pub use manifest::{Bytes, ChannelOrder, Ctc, Db, Family, Input, Manifest, ModelPackage, Nanodet};
pub use nanodet::NanoDet;
pub use ocr::Ocr;
pub use rec::{BATCH, CtcRecognizer, GPU_WIDTHS};
