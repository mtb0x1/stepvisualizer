use std::fmt;

/// Crate-wide error type for fallible operations (GPU init, rendering, parsing).
///
/// It wraps a human-readable message so callers can surface errors to the UI
/// without depending on `Box<dyn std::error::Error>`. Conversion from `String`
/// or `&str` lets existing `return Err(msg.into())` sites move over unchanged.
#[derive(Debug)]
pub struct StepVizError(pub String);

impl fmt::Display for StepVizError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for StepVizError {}

impl From<String> for StepVizError {
    fn from(msg: String) -> Self {
        StepVizError(msg)
    }
}

impl From<&str> for StepVizError {
    fn from(msg: &str) -> Self {
        StepVizError(msg.to_string())
    }
}
