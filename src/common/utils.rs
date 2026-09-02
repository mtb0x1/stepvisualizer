//! General utilities: pure text processing, math, formatting, color mapping, and Web/DOM helpers.
use std::borrow::Cow;
use std::fmt::Write;

use glam::Vec3;

use crate::common::constants::{DEFAULT_TOLERANCE, MAX_TOLERANCE, MIN_TOLERANCE, NA};
use crate::common::render::{RenderablePart, visible_bounds};
use crate::common::types::BoundingBox;

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
pub fn format_or_na(val: &str) -> &str {
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
pub fn clean_unit_name(name: &str) -> &str {
    name.trim().trim_matches('\'').trim_matches('"')
}

/// Signed tetrahedron volume for a single triangle v0, v1, v2 relative to the origin.
#[inline(always)]
pub fn triangle_signed_volume(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> f64 {
    let p0 = [v0[0] as f64, v0[1] as f64, v0[2] as f64];
    let p1 = [v1[0] as f64, v1[1] as f64, v1[2] as f64];
    let p2 = [v2[0] as f64, v2[1] as f64, v2[2] as f64];
    let cross_x = p1[1] * p2[2] - p1[2] * p2[1];
    let cross_y = p1[2] * p2[0] - p1[0] * p2[2];
    let cross_z = p1[0] * p2[1] - p1[1] * p2[0];
    p0[0] * cross_x + p0[1] * cross_y + p0[2] * cross_z
}

/// Area of a single 3D triangle with vertices v0, v1, v2.
#[inline(always)]
pub fn triangle_area(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> f64 {
    let edge1 = [
        (v1[0] - v0[0]) as f64,
        (v1[1] - v0[1]) as f64,
        (v1[2] - v0[2]) as f64,
    ];
    let edge2 = [
        (v2[0] - v0[0]) as f64,
        (v2[1] - v0[1]) as f64,
        (v2[2] - v0[2]) as f64,
    ];
    let cross_x = edge1[1] * edge2[2] - edge1[2] * edge2[1];
    let cross_y = edge1[2] * edge2[0] - edge1[0] * edge2[2];
    let cross_z = edge1[0] * edge2[1] - edge1[1] * edge2[0];
    0.5 * (cross_x * cross_x + cross_y * cross_y + cross_z * cross_z).sqrt()
}

/// Converts spherical coordinates (azimuth, elevation, distance) around a `target` center into Cartesian 3D coordinates.
#[inline(always)]
pub fn spherical_to_cartesian(azimuth: f32, elevation: f32, distance: f32, target: Vec3) -> Vec3 {
    target
        + Vec3::new(
            distance * azimuth.cos() * elevation.cos(),
            distance * elevation.sin(),
            distance * azimuth.sin() * elevation.cos(),
        )
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

/// Bounding-box center across all parts; `Vec3::ZERO` when there is no geometry.
pub fn compute_parts_center(parts: &[RenderablePart]) -> Vec3 {
    visible_bounds(parts, &[])
        .map(|b| b.center_f32())
        .unwrap_or(Vec3::ZERO)
}

/// Computes a normalized geometric face normal from three points, falling back to Vec3::Y if degenerate.
#[inline(always)]
pub fn geometric_normal(p0: Vec3, p1: Vec3, p2: Vec3) -> [f32; 3] {
    let d1 = p1 - p0;
    let d2 = p2 - p0;
    d1.cross(d2).normalize_or(Vec3::Y).to_array()
}

/// Converts raw bytes to megabytes (MiB: 1024 * 1024).
#[inline(always)]
pub fn bytes_to_mb(bytes: f64) -> f64 {
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

/// Formats 3D bounding box coordinates into formatted min/max display strings.
pub fn format_bbox_coordinates(
    min: [f64; 3],
    max: [f64; 3],
    unit_symbol: Option<&str>,
) -> (String, String) {
    let unit_suffix = match unit_symbol {
        Some(u) if !u.is_empty() => format!(" {u}"),
        _ => String::new(),
    };
    let min_str = format!("min: {:.3}, {:.3}, {:.3}{unit_suffix}", min[0], min[1], min[2]);
    let max_str = format!("max: {:.3}, {:.3}, {:.3}{unit_suffix}", max[0], max[1], max[2]);
    (min_str, max_str)
}

/// Maps numeric samples to an SVG polyline points string `"x,y x,y ..."` scaled to width, height, and max value.
pub fn build_svg_polyline_points(
    samples: &[f32],
    width: f32,
    height: f32,
    max_val: f32,
) -> String {
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
        let _ = write!(out, "{x:.1},{y:.1}");
    }
    out
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
        let v0 = [1.0, 0.0, 0.0];
        let v1 = [0.0, 1.0, 0.0];
        let v2 = [0.0, 0.0, 1.0];
        let vol = triangle_signed_volume(v0, v1, v2);
        approx::assert_relative_eq!(vol, 1.0, epsilon = 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_triangle_area() {
        let v0 = [0.0, 0.0, 0.0];
        let v1 = [1.0, 0.0, 0.0];
        let v2 = [0.0, 1.0, 0.0];
        let area = triangle_area(v0, v1, v2);
        approx::assert_relative_eq!(area, 0.5, epsilon = 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_spherical_to_cartesian() {
        let pos = spherical_to_cartesian(0.0, 0.0, 5.0, Vec3::ZERO);
        approx::assert_relative_eq!(pos.x, 5.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos.y, 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos.z, 0.0, epsilon = 1e-6);

        let target = Vec3::new(1.0, 2.0, 3.0);
        let pos_offset = spherical_to_cartesian(0.0, 0.0, 5.0, target);
        approx::assert_relative_eq!(pos_offset.x, 6.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos_offset.y, 2.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos_offset.z, 3.0, epsilon = 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_compute_adaptive_tolerance() {
        assert_eq!(compute_adaptive_tolerance(None), DEFAULT_TOLERANCE);

        let small_bbox = BoundingBox::new([0.0, 0.0, 0.0], [0.01, 0.01, 0.01]);
        assert_eq!(compute_adaptive_tolerance(Some(&small_bbox)), MIN_TOLERANCE);

        let huge_bbox = BoundingBox::new([0.0, 0.0, 0.0], [1000.0, 1000.0, 1000.0]);
        assert_eq!(compute_adaptive_tolerance(Some(&huge_bbox)), MAX_TOLERANCE);

        let mid_bbox = BoundingBox::new([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        approx::assert_relative_eq!(
            compute_adaptive_tolerance(Some(&mid_bbox)),
            0.01,
            epsilon = 1e-6
        );
    }

    #[wasm_bindgen_test]
    fn test_geometric_normal() {
        let p0 = Vec3::new(0.0, 0.0, 0.0);
        let p1 = Vec3::new(1.0, 0.0, 0.0);
        let p2 = Vec3::new(0.0, 1.0, 0.0);
        let normal = geometric_normal(p0, p1, p2);
        assert_eq!(normal, [0.0, 0.0, 1.0]);

        let degenerate_normal = geometric_normal(p0, p1, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(degenerate_normal, [0.0, 1.0, 0.0]);
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
        assert_eq!(format_metric_with_unit(12.34567, Some("mm"), 3), "12.3457 mm³");
        assert_eq!(format_metric_with_unit(45.67891, Some("mm"), 2), "45.6789 mm²");
        assert_eq!(format_metric_with_unit(10.5, Some("mm"), 1), "10.50 mm");
        assert_eq!(format_metric_with_unit(10.5, None, 1), "10.50");
    }

    #[wasm_bindgen_test]
    fn test_format_bbox_coordinates() {
        let min = [1.1234, 2.5678, 3.9];
        let max = [10.0, 20.0, 30.0];
        let (min_s, max_s) = format_bbox_coordinates(min, max, Some("mm"));
        assert_eq!(min_s, "min: 1.123, 2.568, 3.900 mm");
        assert_eq!(max_s, "max: 10.000, 20.000, 30.000 mm");

        let (min_no_unit, max_no_unit) = format_bbox_coordinates(min, max, None);
        assert_eq!(min_no_unit, "min: 1.123, 2.568, 3.900");
        assert_eq!(max_no_unit, "max: 10.000, 20.000, 30.000");
    }

    #[wasm_bindgen_test]
    fn test_build_svg_polyline_points() {
        assert_eq!(build_svg_polyline_points(&[], 100.0, 50.0, 100.0), "");
        let single = [50.0];
        assert_eq!(build_svg_polyline_points(&single, 100.0, 50.0, 100.0), "0.0,25.0");
        let samples = [0.0, 100.0];
        assert_eq!(build_svg_polyline_points(&samples, 100.0, 50.0, 100.0), "0.0,50.0 100.0,0.0");
    }
}
