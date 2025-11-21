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

pub struct AppTracer;

pub(crate) const STEP_TRACER: &str = "[STEP_TRACER]";

pub trait AppTracerTrait {
    fn init();
    fn info(msg: &str);
    fn error(msg: &str);
    fn warn(msg: &str);
    fn debug(msg: &str);
    fn trace(msg: &str);
    fn tracing_enabled_from_url() -> bool;
    fn tracing_level_from_url() -> Option<Level>;
}

impl AppTracerTrait for AppTracer {
    fn init() {
        if let Some(level) = Self::tracing_level_from_url() {
            let fmt_layer: tracing_subscriber::fmt::Layer<
                Registry,
                tracing_subscriber::fmt::format::DefaultFields,
                tracing_subscriber::fmt::format::Format<
                    tracing_subscriber::fmt::format::Full,
                    UtcTime<time::format_description::well_known::Rfc3339>,
                >,
                MakeConsoleWriter,
            > = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_timer(UtcTime::rfc_3339())
                .with_writer(MakeConsoleWriter)
                .with_span_events(FmtSpan::ACTIVE);

            let perf_layer: tracing_web::PerformanceEventsLayer<
                tracing_subscriber::layer::Layered<
                    tracing_subscriber::fmt::Layer<
                        Registry,
                        tracing_subscriber::fmt::format::DefaultFields,
                        tracing_subscriber::fmt::format::Format<
                            tracing_subscriber::fmt::format::Full,
                            UtcTime<time::format_description::well_known::Rfc3339>,
                        >,
                        MakeConsoleWriter,
                    >,
                    Registry,
                >,
                tracing_web::FormatSpanFromFields<Pretty>,
            > = performance_layer().with_details_from_fields(Pretty::default());

            let subscriber = Registry::default()
                .with(fmt_layer)
                .with(perf_layer)
                .with(tracing_subscriber::filter::LevelFilter::from_level(level));
            tracing::subscriber::set_global_default(subscriber)
                .expect("Failed to set global default subscriber");
            tracing::info!(
                "{} StepViz tracing initialized with level: {}",
                STEP_TRACER,
                level.as_str()
            );
        }
    }

    fn tracing_enabled_from_url() -> bool {
        if let Some(window) = web_sys::window() {
            if let Ok(search) = window.location().search() {
                let query = search.trim_start_matches('?');
                for pair in query.split('&') {
                    if pair.is_empty() {
                        continue;
                    }
                    let mut parts = pair.splitn(2, '=');
                    let key = parts.next().unwrap_or("");
                    if key.eq_ignore_ascii_case("tracing") {
                        let value = parts.next().unwrap_or("").to_ascii_lowercase();
                        return value == "on" || value == "true" || value == "1";
                    }
                }
            }
        }
        false
    }

    fn tracing_level_from_url() -> Option<Level> {
        if !Self::tracing_enabled_from_url() {
            return None;
        }

        if let Some(window) = web_sys::window() {
            if let Ok(search) = window.location().search() {
                let query = search.trim_start_matches('?');
                for pair in query.split('&') {
                    if pair.is_empty() {
                        continue;
                    }
                    let mut parts = pair.splitn(2, '=');
                    let key = parts.next().unwrap_or("");
                    if key.eq_ignore_ascii_case("level") {
                        let value = parts.next().unwrap_or("").to_ascii_lowercase();
                        return match value.as_str() {
                            "trace" => Some(Level::TRACE),
                            "debug" => Some(Level::DEBUG),
                            "info" => Some(Level::INFO),
                            "warn" => Some(Level::WARN),
                            "error" => Some(Level::ERROR),
                            _ => Some(Level::INFO), // Default level
                        };
                    }
                }
            }
        }
        Some(Level::TRACE) // Default if 'level' param is not present
    }

    fn info(message: &str) {
        tracing::info!("{} {}", STEP_TRACER, message);
    }

    fn debug(message: &str) {
        tracing::debug!("{} {}", STEP_TRACER, message);
    }

    fn trace(message: &str) {
        tracing::trace!("{} {}", STEP_TRACER, message);
    }

    fn error(message: &str) {
        tracing::error!("{} {}", STEP_TRACER, message);
    }

    fn warn(message: &str) {
        tracing::warn!("{} {}", STEP_TRACER, message);
    }
}

#[macro_export]
macro_rules! trace_span {
    ($name:expr) => {
        //-FIXME : for debug we log only on specific functions
        //if $name == "step_extract_wsgl_reqs" || $name == "render_wgpu_on_canvas" {
        //tracing::error!("tracing enabled for _NAME_ {}", $name);
        let sp = tracing::span!(
            tracing::Level::INFO,
            crate::apptracing::STEP_TRACER,
            "{}",
            $name
        );
        let _ = sp.entered();
        //}
    };
}
