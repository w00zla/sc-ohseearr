//! The engine's errors.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A model package is invalid: its manifest, a missing file, the wrong family for the caller
    Package(String),
    /// ONNX Runtime failed: creating a session (DirectML not available included), running it, reading its output
    Runtime(String),
    /// A model's output does not fit its package (a recognizer's classes and its dictionary)
    Mismatch(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn runtime(e: impl fmt::Display) -> Self {
        Self::Runtime(e.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package(m) => write!(f, "model package: {m}"),
            Self::Runtime(m) => write!(f, "ONNX Runtime: {m}"),
            Self::Mismatch(m) => write!(f, "model mismatch: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// For callers that report errors as text.
impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.to_string()
    }
}
