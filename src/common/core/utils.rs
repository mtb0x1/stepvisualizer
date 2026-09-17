//! General utilities: pure text processing, formatting, and color mapping.

use std::borrow::Cow;
use std::fmt::Write;

use glam::Vec2;
use smol_str::{SmolStr, format_smolstr};

use crate::common::constants::NA;

/// Case-insensitive ASCII substring offset search without heap allocations.
#[inline]
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
#[inline]
pub const fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    find_ignore_ascii_case(haystack, needle).is_some()
}

/// Formats a string value, returning `NA` ("N/A") if empty.
#[inline]
pub const fn format_or_na(val: &str) -> &str {
    if val.is_empty() { NA } else { val }
}

/// Formats a list of strings joined by `", "`, returning `NA` ("N/A") if empty or all strings are empty.
#[inline]
pub fn format_list_or_na(list: &[SmolStr]) -> Cow<'_, str> {
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

/// Bytes in one mebibyte (1024 * 1024).
pub const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;

/// Converts raw bytes to megabytes (MiB: 1024 * 1024).
#[inline(always)]
pub const fn bytes_to_mb(bytes: f64) -> f64 {
    bytes / BYTES_PER_MIB
}

/// Formats a byte size into a human-readable megabyte string with one decimal place.
#[inline]
pub fn format_bytes_mb(bytes: f64) -> SmolStr {
    format_smolstr!("{:.1} MB", bytes_to_mb(bytes))
}

/// Formats a metric value with an optional unit symbol and power exponent (e.g. `12.3456 mm³`, `45.6789 mm²`, `10.50 mm`).
#[inline]
pub fn format_metric_with_unit(value: f64, unit_symbol: Option<&str>, power: u32) -> SmolStr {
    match unit_symbol.filter(|u| !u.is_empty()) {
        Some(u) => match power {
            3 => format_smolstr!("{value:.4} {u}³"),
            2 => format_smolstr!("{value:.4} {u}²"),
            _ => format_smolstr!("{value:.2} {u}"),
        },
        None => match power {
            3 | 2 => format_smolstr!("{value:.4}"),
            _ => format_smolstr!("{value:.2}"),
        },
    }
}

/// Formats 3D bounding box coordinates from `glam::DVec3` into formatted min/max display strings.
#[inline]
pub fn format_bbox_coordinates(
    min: glam::DVec3,
    max: glam::DVec3,
    unit_symbol: Option<&str>,
) -> (SmolStr, SmolStr) {
    let u = unit_symbol.filter(|u| !u.is_empty()).unwrap_or("");
    let space = if u.is_empty() { "" } else { " " };
    let format_pt = |p: glam::DVec3, prefix: &str| {
        format_smolstr!("{prefix}: {:.3}, {:.3}, {:.3}{space}{u}", p.x, p.y, p.z)
    };
    (format_pt(min, "min"), format_pt(max, "max"))
}

/// Maps numeric samples to an SVG polyline points string `"x,y x,y ..."` scaled to width, height, and max value.
#[inline]
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
