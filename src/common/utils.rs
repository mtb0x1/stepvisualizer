//! General utilities: pure text processing, math, formatting, color mapping, and Web/DOM helpers.
use std::borrow::Cow;
use std::fmt::Write;

use glam::{DVec3, Vec2, Vec4};

use crate::common::constants::{DEFAULT_TOLERANCE, MAX_TOLERANCE, MIN_TOLERANCE, NA};
use crate::common::render::{RenderablePart, visible_bounds};
use crate::common::types::BoundingBox;
use ruststep::ast::Parameter;

/// Case-insensitive ASCII substring search without heap allocations.
pub const fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    if n.len() > h.len() {
        return false;
    }
    let max_start = h.len() - n.len();
    let mut i = 0;
    while i <= max_start {
        let mut j = 0;
        let mut matches = true;
        while j < n.len() {
            if !h[i + j].eq_ignore_ascii_case(&n[j]) {
                matches = false;
                break;
            }
            j += 1;
        }
        if matches {
            return true;
        }
        i += 1;
    }
    false
}

/// Formats a string value, returning `NA` ("N/A") if empty.
#[inline]
pub const fn format_or_na(val: &str) -> &str {
    if val.is_empty() { NA } else { val }
}

/// Formats a list of strings joined by `", "`, returning `NA` ("N/A") if empty or all strings are empty.
pub fn format_list_or_na(list: &[String]) -> Cow<'_, str> {
    if list.is_empty() || list.iter().all(|s| s.is_empty()) {
        Cow::Borrowed(NA)
    } else {
        Cow::Owned(list.join(", "))
    }
}

/// Trims surrounding whitespace and single/double quotes from a unit name string.
#[inline]
pub fn clean_unit_name(name: &str) -> &str {
    name.trim().trim_matches('\'').trim_matches('"')
}

/// Signed tetrahedron volume for 3D vertices using double-precision glam vectors.
#[inline(always)]
pub fn triangle_signed_volume(p0: DVec3, p1: DVec3, p2: DVec3) -> f64 {
    p0.dot(p1.cross(p2))
}

/// Area of a single 3D triangle using double-precision glam vectors.
#[inline(always)]
pub fn triangle_area(p0: DVec3, p1: DVec3, p2: DVec3) -> f64 {
    0.5 * (p1 - p0).cross(p2 - p0).length()
}

/// Converts spherical coordinates (azimuth, elevation, distance) around a `target` center into Cartesian 3D coordinates.
#[inline(always)]
pub fn spherical_to_cartesian(azimuth: f64, elevation: f64, distance: f64, target: DVec3) -> DVec3 {
    let (sin_az, cos_az) = azimuth.sin_cos();
    let (sin_el, cos_el) = elevation.sin_cos();
    target + DVec3::new(cos_az * cos_el, sin_el, sin_az * cos_el) * distance
}

/// Compute adaptive scale-aware tessellation tolerance based on model bounding box extent.
pub fn compute_adaptive_tolerance(bbox: Option<&BoundingBox>) -> f64 {
    if let Some(bbox) = bbox {
        let extent = bbox.max_extent();
        if extent > 0.0 {
            return (extent * 0.001).clamp(MIN_TOLERANCE, MAX_TOLERANCE);
        }
    }
    DEFAULT_TOLERANCE
}

/// Bounding-box center across all parts; `DVec3::ZERO` when there is no geometry.
pub fn compute_parts_center(parts: &[RenderablePart]) -> DVec3 {
    visible_bounds(parts, &[])
        .map(|b| b.center())
        .unwrap_or(DVec3::ZERO)
}

/// Computes a normalized geometric face normal from three points, falling back to DVec3::Y if degenerate.
#[inline(always)]
pub fn geometric_normal(p0: DVec3, p1: DVec3, p2: DVec3) -> DVec3 {
    (p1 - p0).cross(p2 - p0).normalize_or(DVec3::Y)
}

/// Converts raw bytes to megabytes (MiB: 1024 * 1024).
#[inline(always)]
pub const fn bytes_to_mb(bytes: f64) -> f64 {
    bytes / (1024.0 * 1024.0)
}

/// Formats a byte size into a human-readable megabyte string with one decimal place.
pub fn format_bytes_mb(bytes: f64) -> String {
    format!("{:.1} MB", bytes_to_mb(bytes))
}

/// Formats a metric value with an optional unit symbol and power exponent (e.g. `12.3456 mm³`, `45.6789 mm²`, `10.50 mm`).
pub fn format_metric_with_unit(value: f64, unit_symbol: Option<&str>, power: u32) -> String {
    let suffix = match unit_symbol {
        Some(u) if !u.is_empty() => match power {
            3 => format!(" {u}³"),
            2 => format!(" {u}²"),
            1 => format!(" {u}"),
            _ => format!(" {u}"),
        },
        _ => String::new(),
    };
    match power {
        3 | 2 => format!("{value:.4}{suffix}"),
        _ => format!("{value:.2}{suffix}"),
    }
}

/// Formats 3D bounding box coordinates from `DVec3` into formatted min/max display strings.
pub fn format_bbox_coordinates(
    min: DVec3,
    max: DVec3,
    unit_symbol: Option<&str>,
) -> (String, String) {
    let unit_suffix = match unit_symbol {
        Some(u) if !u.is_empty() => format!(" {u}"),
        _ => String::new(),
    };
    let min_str = format!("min: {:.3}, {:.3}, {:.3}{unit_suffix}", min.x, min.y, min.z);
    let max_str = format!("max: {:.3}, {:.3}, {:.3}{unit_suffix}", max.x, max.y, max.z);
    (min_str, max_str)
}

/// Maps numeric samples to an SVG polyline points string `"x,y x,y ..."` scaled to width, height, and max value.
#[inline(never)]
pub fn build_svg_polyline_points(samples: &[f32], width: f32, height: f32, max_val: f32) -> String {
    if samples.is_empty() {
        return String::new();
    }
    let n = samples.len();
    let mut out = String::with_capacity(n * 12);
    for (i, &v) in samples.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let x = if n == 1 {
            0.0
        } else {
            (i as f32 / (n - 1) as f32) * width
        };
        let y = height - (v.min(max_val) / max_val) * height;
        let pt = Vec2::new(x, y);
        let _ = write!(out, "{:.1},{:.1}", pt.x, pt.y);
    }
    out
}

/// Per-part palette, cycled by part index.
pub const PART_COLORS_COUNT: usize = 10;
pub const PART_COLORS: [Vec4; PART_COLORS_COUNT] = [
    Vec4::new(0.8, 0.2, 0.2, 1.0),
    Vec4::new(0.2, 0.8, 0.2, 1.0),
    Vec4::new(0.2, 0.2, 0.8, 1.0),
    Vec4::new(0.8, 0.8, 0.2, 1.0),
    Vec4::new(0.8, 0.2, 0.8, 1.0),
    Vec4::new(0.2, 0.8, 0.8, 1.0),
    Vec4::new(0.6, 0.4, 0.2, 1.0),
    Vec4::new(0.4, 0.6, 0.8, 1.0),
    Vec4::new(0.8, 0.6, 0.4, 1.0),
    Vec4::new(0.6, 0.8, 0.4, 1.0),
];

/// Returns the RGBA color as `Vec4` for part at `index`, cycling through the palette.
#[inline(always)]
pub const fn part_color(index: usize) -> Vec4 {
    PART_COLORS[index % PART_COLORS_COUNT]
}

/// Returns a status color string for FPS visualization (green >= 50, yellow >= 30, red < 30).
#[inline]
pub const fn fps_color(fps: f32) -> &'static str {
    if fps >= 50.0 {
        "#4ade80"
    } else if fps >= 30.0 {
        "#facc15"
    } else {
        "#f87171"
    }
}

/// Returns the current high-resolution time in milliseconds.
/// Falls back to 0.0 if the browser window or performance API is unavailable.
pub fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
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
        pair_key
            .eq_ignore_ascii_case(key)
            .then(|| value.to_ascii_lowercase())
    })
}

/// Whether the browser exposes the `navigator.gpu` entry point.
#[cold]
#[inline(never)]
pub fn browser_has_webgpu() -> bool {
    web_sys::window()
        .map(|window| {
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
    let path = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default();
    if path.contains("/testing") {
        "testing:"
    } else if path.contains("/production") {
        "production:"
    } else {
        ""
    }
}

/// Extracts the first selected file from an `<input type="file">` change event.
pub fn input_file(event: &web_sys::Event) -> Option<web_sys::File> {
    use wasm_bindgen::JsCast;
    let input: web_sys::HtmlInputElement = event
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())?;
    input.files()?.get(0)
}

/// Extracts a slice of `Parameter`s if the parameter is a `Parameter::List`.
#[inline]
pub const fn param_as_list(param: &Parameter) -> Option<&[Parameter]> {
    match param {
        Parameter::List(list) => Some(list.as_slice()),
        _ => None,
    }
}

/// Extracts the string slice if the parameter is a `Parameter::Enumeration`.
#[inline]
pub const fn param_as_enum(param: &Parameter) -> Option<&str> {
    match param {
        Parameter::Enumeration(value) => Some(value.as_str()),
        _ => None,
    }
}

/// Extracts a string slice if the parameter is either `Parameter::Enumeration` or `Parameter::String`.
#[inline]
pub const fn param_as_str(param: &Parameter) -> Option<&str> {
    match param {
        Parameter::Enumeration(value) => Some(value.as_str()),
        Parameter::String(value) => Some(value.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_contains_ignore_ascii_case() {
        assert!(contains_ignore_ascii_case(
            "AUTOMOTIVE_DESIGN",
            "automotive"
        ));
        assert!(contains_ignore_ascii_case(
            "CONFIG_CONTROL_DESIGN",
            "DESIGN"
        ));
        assert!(contains_ignore_ascii_case("AP203", "ap203"));
        assert!(contains_ignore_ascii_case("anything", ""));
        assert!(!contains_ignore_ascii_case("short", "longer_needle"));
        assert!(!contains_ignore_ascii_case("hello world", "xyz"));
    }

    #[wasm_bindgen_test]
    fn test_format_or_na() {
        assert_eq!(format_or_na(""), "N/A");
        assert_eq!(format_or_na("hello"), "hello");
    }

    #[wasm_bindgen_test]
    fn test_format_list_or_na() {
        assert_eq!(format_list_or_na(&[]), "N/A");
        assert_eq!(format_list_or_na(&[String::new(), String::new()]), "N/A");
        assert_eq!(
            format_list_or_na(&["Alice".to_string(), "Bob".to_string()]),
            "Alice, Bob"
        );
    }

    #[wasm_bindgen_test]
    fn test_clean_unit_name() {
        assert_eq!(clean_unit_name("  'MM'  "), "MM");
        assert_eq!(clean_unit_name("\"INCH\""), "INCH");
        assert_eq!(clean_unit_name("  MILLIMETRE  "), "MILLIMETRE");
    }

    #[wasm_bindgen_test]
    fn test_triangle_signed_volume() {
        let v0 = DVec3::new(1.0, 0.0, 0.0);
        let v1 = DVec3::new(0.0, 1.0, 0.0);
        let v2 = DVec3::new(0.0, 0.0, 1.0);
        let vol = triangle_signed_volume(v0, v1, v2);
        approx::assert_relative_eq!(vol, 1.0, epsilon = 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_triangle_area() {
        let v0 = DVec3::new(0.0, 0.0, 0.0);
        let v1 = DVec3::new(1.0, 0.0, 0.0);
        let v2 = DVec3::new(0.0, 1.0, 0.0);
        let area = triangle_area(v0, v1, v2);
        approx::assert_relative_eq!(area, 0.5, epsilon = 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_spherical_to_cartesian() {
        let pos = spherical_to_cartesian(0.0, 0.0, 5.0, DVec3::ZERO);
        approx::assert_relative_eq!(pos.x, 5.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos.y, 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos.z, 0.0, epsilon = 1e-6);

        let target = DVec3::new(1.0, 2.0, 3.0);
        let pos_offset = spherical_to_cartesian(0.0, 0.0, 5.0, target);
        approx::assert_relative_eq!(pos_offset.x, 6.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos_offset.y, 2.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos_offset.z, 3.0, epsilon = 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_compute_adaptive_tolerance() {
        assert_eq!(compute_adaptive_tolerance(None), DEFAULT_TOLERANCE);

        let small_bbox = BoundingBox::new(
            DVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            DVec3 {
                x: 0.01,
                y: 0.01,
                z: 0.01,
            },
        );
        assert_eq!(compute_adaptive_tolerance(Some(&small_bbox)), MIN_TOLERANCE);

        let huge_bbox = BoundingBox::new(
            DVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            DVec3 {
                x: 1000.0,
                y: 1000.0,
                z: 1000.0,
            },
        );
        assert_eq!(compute_adaptive_tolerance(Some(&huge_bbox)), MAX_TOLERANCE);

        let mid_bbox = BoundingBox::new(
            DVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            DVec3 {
                x: 10.0,
                y: 10.0,
                z: 10.0,
            },
        );
        approx::assert_relative_eq!(
            compute_adaptive_tolerance(Some(&mid_bbox)),
            0.01,
            epsilon = 1e-6
        );
    }

    #[wasm_bindgen_test]
    fn test_geometric_normal() {
        let p0 = DVec3::new(0.0, 0.0, 0.0);
        let p1 = DVec3::new(1.0, 0.0, 0.0);
        let p2 = DVec3::new(0.0, 1.0, 0.0);
        let normal = geometric_normal(p0, p1, p2);
        assert_eq!(normal, DVec3::Z);

        let degenerate_normal = geometric_normal(p0, p1, DVec3::new(2.0, 0.0, 0.0));
        assert_eq!(degenerate_normal, DVec3::Y);
    }

    #[wasm_bindgen_test]
    fn test_bytes_and_mb_formatting() {
        let bytes = 50.0 * 1024.0 * 1024.0;
        assert_eq!(bytes_to_mb(bytes), 50.0);
        assert_eq!(format_bytes_mb(bytes), "50.0 MB");
        assert_eq!(format_bytes_mb(1024.0 * 512.0), "0.5 MB");
    }

    #[wasm_bindgen_test]
    fn test_format_metric_with_unit() {
        assert_eq!(
            format_metric_with_unit(12.34567, Some("mm"), 3),
            "12.3457 mm³"
        );
        assert_eq!(
            format_metric_with_unit(45.67891, Some("mm"), 2),
            "45.6789 mm²"
        );
        assert_eq!(format_metric_with_unit(10.5, Some("mm"), 1), "10.50 mm");
        assert_eq!(format_metric_with_unit(10.5, None, 1), "10.50");
    }

    #[wasm_bindgen_test]
    fn test_format_bbox_coordinates() {
        let min = DVec3::new(1.1234, 2.5678, 3.9);
        let max = DVec3::new(10.0, 20.0, 30.0);
        let (min_s, max_s) = format_bbox_coordinates(min, max, Some("mm"));
        assert_eq!(min_s, "min: 1.123, 2.568, 3.900 mm");
        assert_eq!(max_s, "max: 10.000, 20.000, 30.000 mm");

        let (min_no_unit, max_no_unit) = format_bbox_coordinates(min, max, None);
        assert_eq!(min_no_unit, "min: 1.123, 2.568, 3.900");
        assert_eq!(max_no_unit, "max: 10.000, 20.000, 30.000");
    }

    #[wasm_bindgen_test]
    fn test_param_helpers() {
        let list_param = Parameter::List(vec![
            Parameter::Enumeration("MILLI".to_string()),
            Parameter::Enumeration("METRE".to_string()),
        ]);
        let enum_param = Parameter::Enumeration("INCH".to_string());
        let str_param = Parameter::String("foot".to_string());
        let int_param = Parameter::Integer(42);

        assert_eq!(param_as_list(&list_param).map(|l| l.len()), Some(2));
        assert_eq!(param_as_list(&enum_param), None);

        assert_eq!(param_as_enum(&enum_param), Some("INCH"));
        assert_eq!(param_as_enum(&str_param), None);

        assert_eq!(param_as_str(&enum_param), Some("INCH"));
        assert_eq!(param_as_str(&str_param), Some("foot"));
        assert_eq!(param_as_str(&int_param), None);
    }
}
