//! Time utilities.

/// Returns the current high-resolution time in milliseconds.
/// Falls back to 0.0 if the browser window or performance API is unavailable.
pub fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}
