//! Geometry, math, and spatial queries.
use glam::{DMat4, DVec3, DVec4};

use crate::common::constants::{DEFAULT_TOLERANCE, MAX_TOLERANCE, MIN_TOLERANCE};
use crate::common::render::{RenderablePart, visible_bounds};
use crate::common::types::{BoundingBox, ViewportSize};

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
#[inline]
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
#[inline]
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
#[inline]
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
#[inline]
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
#[inline]
pub fn ray_aabb_intersect(origin: DVec3, dir: DVec3, bbox: &BoundingBox) -> bool {
    let mut tmin = f64::NEG_INFINITY;
    let mut tmax = f64::INFINITY;

    macro_rules! check_axis {
        ($o:expr, $d:expr, $min_v:expr, $max_v:expr) => {
            if $d.abs() < 1e-9 {
                if $o < $min_v || $o > $max_v {
                    return false;
                }
            } else {
                let inv_d = 1.0 / $d;
                let mut t1 = ($min_v - $o) * inv_d;
                let mut t2 = ($max_v - $o) * inv_d;
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                tmin = tmin.max(t1);
                tmax = tmax.min(t2);
                if tmin > tmax || tmax < 0.0 {
                    return false;
                }
            }
        };
    }

    check_axis!(origin.x, dir.x, bbox.min.x, bbox.max.x);
    check_axis!(origin.y, dir.y, bbox.min.y, bbox.max.y);
    check_axis!(origin.z, dir.z, bbox.min.z, bbox.max.z);

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
