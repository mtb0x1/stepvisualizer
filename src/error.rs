//! Crate-wide domain error types.
use std::fmt;

/// Crate-wide domain error type for all fallible operations (parsing, geometry, GPU init, rendering, storage).
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum StepVizError {
    /// Failure while reading file contents from browser FileReader.
    FileRead(String),
    /// File size exceeds the maximum allowed upload limit.
    FileTooLarge { size_bytes: f64, max_bytes: f64 },
    /// Failure while parsing the STEP text or token stream.
    Parse(String),
    /// STEP file contains no data section or no usable entities.
    EmptyDataSection,
    /// Header records could not be mapped to a valid STEP header.
    InvalidHeader(String),
    /// Browser does not support WebGPU.
    GpuUnavailable(String),
    /// Failed to request adapter or device during WebGPU initialization.
    GpuInitFailed(String),
    /// Runtime failure during frame rendering or surface acquisition.
    RenderError(String),
    /// Local persistence or cache error.
    Storage(String),
    /// General or unclassified error message.
    Generic(String),
}

impl fmt::Display for StepVizError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileRead(msg) => write!(f, "Failed to read file: {msg}"),
            Self::FileTooLarge {
                size_bytes,
                max_bytes,
            } => {
                let size_mb = size_bytes / (1024.0 * 1024.0);
                let max_mb = max_bytes / (1024.0 * 1024.0);
                write!(
                    f,
                    "File too large ({size_mb:.1} MB). Maximum allowed is {max_mb:.0} MB."
                )
            }
            Self::Parse(msg) => write!(f, "Failed to parse STEP: {msg}"),
            Self::EmptyDataSection => {
                write!(
                    f,
                    "STEP file has no usable data sections (empty meta/entities)."
                )
            }
            Self::InvalidHeader(msg) => write!(f, "Failed to parse header: {msg}"),
            Self::GpuUnavailable(msg) => write!(f, "WebGPU is unavailable: {msg}"),
            Self::GpuInitFailed(msg) => write!(f, "Failed to initialize WebGPU: {msg}"),
            Self::RenderError(msg) => write!(f, "Render error: {msg}"),
            Self::Storage(msg) => write!(f, "Storage error: {msg}"),
            Self::Generic(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for StepVizError {}

impl From<String> for StepVizError {
    fn from(msg: String) -> Self {
        StepVizError::Generic(msg)
    }
}

impl From<&str> for StepVizError {
    fn from(msg: &str) -> Self {
        StepVizError::Generic(msg.to_string())
    }
}

