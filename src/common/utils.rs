//! General utilities: pure text processing, math, formatting, color mapping, and Web/DOM helpers.
use std::borrow::Cow;
use std::fmt::Write;

use glam::{DMat4, DVec3, DVec4, Vec2};

use crate::common::constants::{DEFAULT_TOLERANCE, MAX_TOLERANCE, MIN_TOLERANCE, NA};
use crate::common::render::{RenderablePart, visible_bounds};
use crate::common::types::{BoundingBox, ViewportSize};
use crate::ruststep::ast::Parameter;

/// Case-insensitive ASCII substring offset search without heap allocations.
pub const fn find_ignore_ascii_case(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return Some(0);
    }
    if n.len() > h.len() {
        return None;
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
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Case-insensitive ASCII substring search without heap allocations.
pub const fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    find_ignore_ascii_case(haystack, needle).is_some()
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

/// Unprojects a 2D screen coordinate (pixels from canvas top-left) into a 3D world-space ray `(origin, direction)`.
///
/// Assumes WebGPU NDC clip-space conventions: X in `[-1, 1]`, Y in `[-1, 1]` (upwards), Z in `[0, 1]`.
pub fn screen_point_to_ray(
    screen_x: f64,
    screen_y: f64,
    viewport_size: ViewportSize,
    view_matrix: DMat4,
    projection_matrix: DMat4,
) -> (DVec3, DVec3) {
    let width = (viewport_size.width as f64).max(1.0);
    let height = (viewport_size.height as f64).max(1.0);

    let ndc_x = (2.0 * screen_x / width) - 1.0;
    let ndc_y = 1.0 - (2.0 * screen_y / height);

    let inv_vp = (projection_matrix * view_matrix).inverse();

    // Near plane in WebGPU NDC is z = 0.0, far plane is z = 1.0
    let near_clip = inv_vp * DVec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let far_clip = inv_vp * DVec4::new(ndc_x, ndc_y, 1.0, 1.0);

    let near_pt = near_clip.truncate() / near_clip.w;
    let far_pt = far_clip.truncate() / far_clip.w;

    let dir = (far_pt - near_pt).normalize_or(DVec3::NEG_Z);
    (near_pt, dir)
}

/// Möller–Trumbore ray-triangle intersection.
///
/// Returns the distance `t` along the ray `origin + dir * t` if the ray intersects
/// the triangle `(v0, v1, v2)`, or `None` if it misses or is parallel.
pub fn ray_triangle_intersect(
    origin: DVec3,
    dir: DVec3,
    v0: DVec3,
    v1: DVec3,
    v2: DVec3,
) -> Option<f64> {
    const EPSILON: f64 = 1e-9;
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = dir.cross(edge2);
    let a = edge1.dot(h);
    if a.abs() < EPSILON {
        return None;
    }
    let f = 1.0 / a;
    let s = origin - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = f * dir.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * edge2.dot(q);
    if t > EPSILON { Some(t) } else { None }
}

/// Ray-AABB slab intersection test.
pub fn ray_aabb_intersect(origin: DVec3, dir: DVec3, bbox: &BoundingBox) -> bool {
    let mut tmin = f64::NEG_INFINITY;
    let mut tmax = f64::INFINITY;

    for i in 0..3 {
        let (o, d, min_v, max_v) = match i {
            0 => (origin.x, dir.x, bbox.min.x, bbox.max.x),
            1 => (origin.y, dir.y, bbox.min.y, bbox.max.y),
            _ => (origin.z, dir.z, bbox.min.z, bbox.max.z),
        };

        if d.abs() < 1e-9 {
            if o < min_v || o > max_v {
                return false;
            }
        } else {
            let inv_d = 1.0 / d;
            let mut t1 = (min_v - o) * inv_d;
            let mut t2 = (max_v - o) * inv_d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax || tmax < 0.0 {
                return false;
            }
        }
    }
    true
}

/// Raycasts against all visible geometry parts and returns the closest hit `(hit_point, normal)`.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn raycast_parts(
    origin: DVec3,
    dir: DVec3,
    parts: &[RenderablePart],
    visibility: &[bool],
) -> Option<(DVec3, DVec3)> {
    let mut closest_t = f64::INFINITY;
    let mut closest_hit = None;

    for (part_idx, part) in parts.iter().enumerate() {
        if !visibility.get(part_idx).copied().unwrap_or(true) || part.indices.is_empty() {
            continue;
        }

        // Fast rejection with part bounding box in world space
        let mut part_bbox = BoundingBox::EMPTY;
        for vertex in &part.vertices {
            let world_pos = part
                .model_matrix
                .transform_point3(vertex.position)
                .as_dvec3();
            part_bbox.expand_point(world_pos);
        }

        if part_bbox.is_valid() && !ray_aabb_intersect(origin, dir, &part_bbox) {
            continue;
        }

        for tri in part.indices.chunks_exact(3) {
            let idx0 = tri[0] as usize;
            let idx1 = tri[1] as usize;
            let idx2 = tri[2] as usize;
            if idx0 >= part.vertices.len()
                || idx1 >= part.vertices.len()
                || idx2 >= part.vertices.len()
            {
                continue;
            }

            let v0 = part
                .model_matrix
                .transform_point3(part.vertices[idx0].position)
                .as_dvec3();
            let v1 = part
                .model_matrix
                .transform_point3(part.vertices[idx1].position)
                .as_dvec3();
            let v2 = part
                .model_matrix
                .transform_point3(part.vertices[idx2].position)
                .as_dvec3();

            if let Some(t) = ray_triangle_intersect(origin, dir, v0, v1, v2)
                && t < closest_t
            {
                closest_t = t;
                let hit_point = origin + dir * t;
                let normal = (v1 - v0).cross(v2 - v0).normalize_or(DVec3::Y);
                closest_hit = Some((hit_point, normal));
            }
        }
    }

    closest_hit
}

/// Bytes in one mebibyte (1024 * 1024).
pub const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;

/// Converts raw bytes to megabytes (MiB: 1024 * 1024).
#[inline(always)]
pub const fn bytes_to_mb(bytes: f64) -> f64 {
    bytes / BYTES_PER_MIB
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

pub use crate::common::color::{PART_COLORS, PART_COLORS_COUNT, part_color};

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

/// Extracts the numeric entity ID if the parameter is a `Parameter::Ref(Name::Entity(id))`.
#[inline]
pub const fn param_as_ref(param: &Parameter) -> Option<u64> {
    match param {
        Parameter::Ref(crate::ruststep::ast::Name::Entity(id)) => Some(*id),
        _ => None,
    }
}

/// Extracts a float value if the parameter is `Parameter::Real` or `Parameter::Integer`.
#[inline]
pub const fn param_as_real(param: &Parameter) -> Option<f64> {
    match param {
        Parameter::Real(v) => Some(*v),
        Parameter::Integer(v) => Some(*v as f64),
        _ => None,
    }
}

/// Recursively extracts all numeric entity IDs referenced within a `Parameter` (handling nested lists).
pub fn extract_entity_refs(param: &Parameter) -> Vec<u64> {
    let mut refs = Vec::new();
    collect_refs_recursive(param, &mut refs);
    refs
}

/// Helper for recursive collection of entity references within a `Parameter`.
pub fn collect_refs_recursive(param: &Parameter, out: &mut Vec<u64>) {
    match param {
        Parameter::Ref(crate::ruststep::ast::Name::Entity(id)) => out.push(*id),
        Parameter::List(list) => {
            for item in list {
                collect_refs_recursive(item, out);
            }
        }
        _ => {}
    }
}
