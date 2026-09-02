//! Tracing setup: console formatter + performance-entry layers, opt-in via
//! URL query parameters (`?tracing=on&level=trace` or shorthand `?tracing=debug`),
//! plus leveled logging helpers used across the crate.
use tracing::Level;
use tracing_subscriber::{
    Registry,
    fmt::{
        format::{FmtSpan, Pretty},
        time::UtcTime,
    },
    prelude::*,
};
use tracing_web::{MakeConsoleWriter, performance_layer};
use web_sys::console;

use super::constants::STEP_TRACER;
use super::utils::url_query_param;

/// Install the global subscriber, but only when tracing is enabled via URL.
pub fn init() {
    if let Some(level) = tracing_level_from_url() {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_timer(UtcTime::<time::format_description::well_known::Rfc3339>::rfc_3339())
            .with_writer(MakeConsoleWriter)
            .with_span_events(FmtSpan::ACTIVE);

        let perf_layer = performance_layer().with_details_from_fields(Pretty::default());

        let subscriber = Registry::default()
            .with(fmt_layer)
            .with(perf_layer)
            .with(tracing_subscriber::filter::LevelFilter::from_level(level));

        if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
            console::error_1(&format!("Failed to set global default subscriber: {err}").into());
        }
        tracing::info!(
            "{} StepViz tracing initialized with level: {}",
            STEP_TRACER,
            level.as_str()
        );
    }
}

/// Whether `?tracing=...` is present and enabled in the URL.
pub fn tracing_enabled_from_url() -> bool {
    tracing_level_from_url().is_some()
}

/// The level selected by `?level=...` or `?tracing=<level>`; `None` when tracing is disabled.
/// Defaults: TRACE when `level` is absent, INFO for unknown values.
pub fn tracing_level_from_url() -> Option<Level> {
    let tracing_param = url_query_param("tracing")?;

    let level_str = match tracing_param.as_str() {
        "on" | "true" | "1" => url_query_param("level").unwrap_or_else(|| "trace".to_string()),
        other => other.to_string(),
    };

    Some(match level_str.as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    })
}

pub fn debug(message: &str) {
    tracing::debug!("{} {}", STEP_TRACER, message);
}

pub fn warn(message: &str) {
    tracing::warn!("{} {}", STEP_TRACER, message);
}

pub fn error(message: &str) {
    tracing::error!("{} {}", STEP_TRACER, message);
}

/// Open an INFO span named after the call site, entered for the remainder of
/// the enclosing scope. Every span carries the `STEP_TRACER` prefix so it is
/// recognizable in the console and in the performance panel.
///
/// Use for coarse milestones (file loaded, frame rendered) — not for hot
/// inner loops, where span overhead would dominate.
#[macro_export]
macro_rules! trace_span {
    ($name:expr) => {
        let sp = tracing::info_span!($name, tracer = $crate::common::constants::STEP_TRACER);
        // don't use let _ =, it will drop the guard immediately
        let _entered = sp.entered();
    };
}
