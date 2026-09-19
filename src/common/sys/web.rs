//! DOM, Web API, and storage bindings.

/// Returns the current high-resolution time in milliseconds.
/// Falls back to 0.0 if the browser window or performance API is unavailable.
#[inline(always)]
pub fn now_ms() -> f64 {
    web_sys::window()
        .expect("window exists: WASM main thread")
        .performance()
        .expect("performance exists: WASM main thread")
        .now()
}

/// Reads a query parameter from the current URL (e.g. `?tracing=on&level=debug`).
/// Keys are matched case-insensitively; the value is returned lowercased.
/// Returns `None` when the key is absent or the URL cannot be inspected.
#[cold]
#[inline(never)]
pub fn url_query_param(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let query = search.trim_start_matches('?');

    query.split('&').find_map(|pair| {
        let (pair_key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        pair_key.eq_ignore_ascii_case(key).then(|| value.to_ascii_lowercase())
    })
}

/// Whether the browser exposes the `navigator.gpu` entry point.
#[cold]
#[inline(never)]
pub fn browser_has_webgpu() -> bool {
    web_sys::window()
        .map(|window| {
            // We use js_sys::Reflect here because the current `web_sys` bindings
            // might not expose `.gpu()` on `Navigator` reliably across all targets yet.
            js_sys::Reflect::has(&window.navigator(), &wasm_bindgen::JsValue::from_str("gpu"))
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Detect the deployment environment from `window.location.pathname` and return
/// a namespacing prefix for storage keys:
/// - `/stepvisualizer/testing/…`   → `"testing:"`
/// - `/stepvisualizer/production/…` → `"production:"`
/// - local dev / unknown            → `""` (no prefix, fully backward-compatible)
#[cold]
#[inline(never)]
pub fn detect_env_prefix() -> &'static str {
    let path = web_sys::window().and_then(|w| w.location().pathname().ok()).unwrap_or_default();
    if path.contains("/testing") {
        "testing:"
    } else if path.contains("/production") {
        "production:"
    } else {
        ""
    }
}

/// Reads `window.location.host` (hostname + port, e.g. `"localhost:8080"` or
/// `"myapp.example.com"`). The result is leaked once to a `&'static str` so it
/// can be stored in a `thread_local! OnceCell` without lifetime gymnastics.
#[cold]
#[inline(never)]
pub fn detect_host() -> &'static str {
    let host = web_sys::window()
        .and_then(|w| w.location().host().ok())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "localhost".to_string());
    // Leak once — this is called at most once per thread (WASM is single-threaded).
    Box::leak(host.into_boxed_str())
}

/// Returns the combined, per-origin storage prefix: `"{host}:{env_prefix}"`.
///
/// Examples:
/// - local dev (no env path)  → `"localhost:8080:"`
/// - testing branch           → `"localhost:8080:testing:"`
/// - production deploy        → `"myapp.example.com:production:"`
///
/// Cached after first call via a `thread_local! OnceCell`.
pub fn storage_prefix() -> &'static str {
    std::thread_local! {
        static CACHE: std::cell::OnceCell<&'static str> = const { std::cell::OnceCell::new() };
    }
    CACHE.with(|c| {
        *c.get_or_init(|| {
            let host = detect_host();
            let env = detect_env_prefix();
            // Format: "host:env_prefix"  e.g. "localhost:8080:testing:"
            // The env_prefix already carries its trailing ":" (or is empty).
            let combined = format!("{host}:{env}");
            Box::leak(combined.into_boxed_str())
        })
    })
}

/// Sanitise a host string for use inside an IndexedDB database name (which must
/// be a plain identifier-like string). Replaces `.` and `:` with `_`.
///
/// Examples: `"localhost:8080"` → `"localhost_8080"`,
///           `"myapp.example.com"` → `"myapp_example_com"`.
pub fn sanitize_host_for_db_name(host: &str) -> String {
    host.replace(['.', ':'], "_")
}

/// Extracts the first selected file from an `<input type="file">` change event.
pub fn input_file(event: &web_sys::Event) -> Option<web_sys::File> {
    use wasm_bindgen::JsCast;
    let input: web_sys::HtmlInputElement =
        event.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())?;
    input.files()?.get(0)
}
