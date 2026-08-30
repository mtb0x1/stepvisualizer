//! Tracing setup: console formatter + performance-entry layers, opt-in via
//! URL query parameters (`?tracing=on&level=trace`), plus the leveled
//! logging helpers used across the crate.
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

/// Stateless façade over the tracing setup and the leveled helpers.
pub struct AppTracer;

use crate::common::constants::STEP_TRACER;

/// Logging contract for the crate.
///
/// Implementations are stateless; the trait exists so call sites depend on
/// the abstraction rather than a concrete logger, and so the helpers can be
/// resolved statically.
pub trait AppTracerTrait {
    /// Install the global subscriber, but only when tracing is enabled via
    /// URL (see [`AppTracerTrait::tracing_enabled_from_url`]).
    fn init();
    fn error(msg: &str);
    fn warn(msg: &str);
    fn debug(msg: &str);
    /// Whether `?tracing=on|true|1` is present in the URL.
    fn tracing_enabled_from_url() -> bool;
    /// The level selected by `?level=...`; `None` when tracing is disabled.
    /// Defaults: TRACE when `level` is absent, INFO for unknown values.
    fn tracing_level_from_url() -> Option<Level>;
}

impl AppTracerTrait for AppTracer {
    fn init() {
        if let Some(level) = Self::tracing_level_from_url() {
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

    fn tracing_enabled_from_url() -> bool {
        url_query_param("tracing")
            .is_some_and(|value| value == "on" || value == "true" || value == "1")
    }

    fn tracing_level_from_url() -> Option<Level> {
        if !Self::tracing_enabled_from_url() {
            return None;
        }

        // Defaults: TRACE when the `level` param is absent, INFO when its
        // value is not a recognized level name.
        Some(match url_query_param("level").as_deref() {
            Some("trace") => Level::TRACE,
            Some("debug") => Level::DEBUG,
            Some("info") => Level::INFO,
            Some("warn") => Level::WARN,
            Some("error") => Level::ERROR,
            Some(_) => Level::INFO,
            None => Level::TRACE,
        })
    }

    fn debug(message: &str) {
        tracing::debug!("{} {}", STEP_TRACER, message);
    }

    fn error(message: &str) {
        tracing::error!("{} {}", STEP_TRACER, message);
    }

    fn warn(message: &str) {
        tracing::warn!("{} {}", STEP_TRACER, message);
    }
}

/// Reads a query parameter from the current URL (e.g. `?tracing=on&level=debug`).
///
/// Keys are matched case-insensitively; the value is returned lowercased.
/// Returns `None` when the key is absent or the URL cannot be inspected.
fn url_query_param(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let query = search.trim_start_matches('?');

    query.split('&').find_map(|pair| {
        // A bare key without '=' counts as present with an empty value,
        // matching the original `splitn(2, '=')` parser.
        let (pair_key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        pair_key
            .eq_ignore_ascii_case(key)
            .then(|| value.to_ascii_lowercase())
    })
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
        let sp = tracing::info_span!(
            $name,
            tracer = $crate::common::constants::STEP_TRACER
        );
        // don't use let _ =, it will drop the guard immediately
        let _entered = sp.entered();
    };
}
