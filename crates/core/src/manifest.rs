//! Model packages: an ONNX model with its manifest (`model.toml`) and, for recognizers, its dictionary. The
//! manifest is the contract between a model and the code running it: which family decodes it, how its input
//! is normalized, its thresholds and classes. A package is compiled in (a model crate's `package()`) or
//! loaded from a folder (`ModelPackage::load`).

use std::borrow::Cow;
use std::path::Path;

use serde::Deserialize;

/// The bytes of a model: borrowed when compiled in, owned when loaded.
pub type Bytes = Cow<'static, [u8]>;

/// How the model is run and decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    /// A text detector with DB post-processing (PaddleOCR's det models): a probability map of text pixels
    DbDet,
    /// A text line recognizer with CTC decoding (PaddleOCR's rec models): per time step the classes'
    /// probabilities, class 0 the blank
    CtcRec,
    /// An object detector of the NanoDet-Plus kind: per prior point the classes' scores and per side a
    /// distribution over bins of the stride
    Nanodet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelOrder {
    Bgr,
    Rgb,
}

/// The model's input: channel order and normalization `(x/255 - mean) / std` per channel, mean and std in
/// the model's channel order; fixed or nominal size where the family needs one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub order: ChannelOrder,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    /// NanoDet: the fixed input width; CTC: the nominal line width (wider lines widen the input)
    pub width: Option<u32>,
    /// NanoDet: the fixed input height; CTC: the line height
    pub height: Option<u32>,
}

/// DB post-processing: the probability threshold, the minimum mean probability of a region's own pixels,
/// the unclip ratio (each box grows by `area * unclip / perimeter`), the minimum box side in map pixels.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Db {
    pub thresh: f32,
    pub box_thresh: f32,
    pub unclip: f32,
    #[serde(default = "default_min_size")]
    pub min_size: f32,
}

fn default_min_size() -> f32 {
    3.0
}

/// CTC decoding: the dictionary file (one character per line, after the blank) and whether a space follows
/// it as the last class.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ctc {
    pub dict: String,
    #[serde(default)]
    pub space_char: bool,
}

/// NanoDet-Plus decoding: the strides of its heads, the bins per side minus one, the classes in the model's
/// order, the score a box counts from and the IoU above which boxes of one class are merged.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nanodet {
    pub strides: Vec<u32>,
    pub reg_max: usize,
    pub classes: Vec<String>,
    pub score: f32,
    pub nms: f32,
}

/// A package's `model.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub family: Family,
    /// SPDX identifier of the model's license
    pub license: String,
    #[serde(default)]
    pub description: String,
    /// Where it comes from: upstream URL and revision, or the recipe and data that trained it
    #[serde(default)]
    pub source: String,
    pub input: Input,
    pub db: Option<Db>,
    pub ctc: Option<Ctc>,
    pub nanodet: Option<Nanodet>,
}

impl Manifest {
    /// Parses and checks a manifest: the family's own section is there and the input has what it needs.
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest = toml::from_str(text).map_err(|e| format!("model.toml: {e}"))?;
        let need = |ok: bool, what: &str| if ok { Ok(()) } else { Err(format!("{}: {:?} needs {what}", m.id, m.family)) };
        match m.family {
            Family::DbDet => need(m.db.is_some(), "a [db] section")?,
            Family::CtcRec => {
                need(m.ctc.is_some(), "a [ctc] section")?;
                need(m.input.width.is_some() && m.input.height.is_some(), "input width and height")?;
            }
            Family::Nanodet => {
                need(m.nanodet.is_some(), "a [nanodet] section")?;
                need(m.input.width.is_some() && m.input.height.is_some(), "input width and height")?;
            }
        }
        Ok(m)
    }
}

/// A model with its manifest and, for a CTC recognizer, its dictionary.
#[derive(Debug, Clone)]
pub struct ModelPackage {
    pub manifest: Manifest,
    pub model: Bytes,
    pub dict: Option<Cow<'static, str>>,
}

impl ModelPackage {
    /// A compiled-in package (what a model crate's `package()` returns).
    pub fn embedded(manifest: &str, model: &'static [u8], dict: Option<&'static str>) -> Result<Self, String> {
        let manifest = Manifest::parse(manifest)?;
        let package = Self { manifest, model: Cow::Borrowed(model), dict: dict.map(Cow::Borrowed) };
        package.check()?;
        Ok(package)
    }

    /// A package from a folder: `model.toml`, `model.onnx` and the dictionary the manifest names.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
        let manifest = Manifest::parse(&String::from_utf8(read("model.toml")?).map_err(|e| format!("model.toml: {e}"))?)?;
        let dict = match &manifest.ctc {
            Some(ctc) => Some(Cow::Owned(String::from_utf8(read(&ctc.dict)?).map_err(|e| format!("{}: {e}", ctc.dict))?)),
            None => None,
        };
        let package = Self { model: Cow::Owned(read("model.onnx")?), manifest, dict };
        package.check()?;
        Ok(package)
    }

    fn check(&self) -> Result<(), String> {
        if self.manifest.family == Family::CtcRec && self.dict.is_none() {
            return Err(format!("{}: a CTC recognizer needs its dictionary", self.manifest.id));
        }
        Ok(())
    }

    /// Whether this package is of `family` (an error naming both otherwise).
    pub(crate) fn expect(&self, family: Family) -> Result<(), String> {
        if self.manifest.family == family {
            Ok(())
        } else {
            Err(format!("{}: a {:?} model, not {family:?}", self.manifest.id, self.manifest.family))
        }
    }
}
