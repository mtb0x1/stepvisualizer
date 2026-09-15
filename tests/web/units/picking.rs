use glam::{DVec3, Mat4, Vec3};
use stepvisualizer::common::color::Color;
use stepvisualizer::common::constants::{
    DEFAULT_TOLERANCE, MAX_TOLERANCE, MIN_TOLERANCE, compute_adaptive_tolerance,
};
use stepvisualizer::common::render::{GpuVertex, RenderablePart};
use stepvisualizer::common::types::{BoundingBox, ViewportSize};
use stepvisualizer::common::utils::{
    geometric_normal, ray_triangle_intersect, raycast_parts, screen_point_to_ray,
};
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn test_ray_triangle_intersect() {
    let v0 = DVec3::new(0.0, 0.0, 0.0);
    let v1 = DVec3::new(2.0, 0.0, 0.0);
    let v2 = DVec3::new(0.0, 2.0, 0.0);

    // Ray pointing straight at triangle from +Z
    let origin = DVec3::new(0.5, 0.5, 10.0);
    let dir = DVec3::new(0.0, 0.0, -1.0);
    let hit = ray_triangle_intersect(origin, dir, v0, v1, v2);
    assert!(hit.is_some());
    approx::assert_relative_eq!(hit.unwrap(), 10.0, epsilon = 1e-6);

    // Ray pointing away
    let dir_away = DVec3::new(0.0, 0.0, 1.0);
    assert!(ray_triangle_intersect(origin, dir_away, v0, v1, v2).is_none());

    // Ray missing
    let origin_miss = DVec3::new(5.0, 5.0, 10.0);
    assert!(ray_triangle_intersect(origin_miss, dir, v0, v1, v2).is_none());
}

#[wasm_bindgen_test]
fn test_screen_point_to_ray() {
    use glam::dcamera::rh::proj::directx::perspective;
    use glam::dcamera::rh::view::look_at_mat4;

    let eye = DVec3::new(0.0, 0.0, 5.0);
    let target = DVec3::ZERO;
    let view = look_at_mat4(eye, target, DVec3::Y);
    let proj = perspective(std::f64::consts::FRAC_PI_3, 1.0, 0.1, 100.0);
    let viewport = ViewportSize::new(800, 800);

    // Screen center (400, 400) produces ray directly down -Z
    let (_origin, dir) = screen_point_to_ray(400.0, 400.0, viewport, view, proj);
    approx::assert_relative_eq!(dir.x, 0.0, epsilon = 1e-5);
    approx::assert_relative_eq!(dir.y, 0.0, epsilon = 1e-5);
    approx::assert_relative_eq!(dir.z, -1.0, epsilon = 1e-5);
}

#[wasm_bindgen_test]
fn test_raycast_parts() {
    let part = RenderablePart {
        vertices: vec![
            GpuVertex::new(Vec3::new(0.0, 0.0, 0.0), Vec3::Z),
            GpuVertex::new(Vec3::new(2.0, 0.0, 0.0), Vec3::Z),
            GpuVertex::new(Vec3::new(0.0, 2.0, 0.0), Vec3::Z),
        ],
        indices: vec![0, 1, 2],
        model_matrix: Mat4::IDENTITY,
        color: Color::WHITE,
        name: None,
    };

    let parts = vec![part];
    let origin = DVec3::new(0.5, 0.5, 5.0);
    let dir = DVec3::new(0.0, 0.0, -1.0);

    // Visible hit
    let hit = raycast_parts(origin, dir, &parts, &[true]);
    assert!(hit.is_some());
    let (hit_pos, normal) = hit.unwrap();
    approx::assert_relative_eq!(hit_pos.x, 0.5, epsilon = 1e-5);
    approx::assert_relative_eq!(hit_pos.y, 0.5, epsilon = 1e-5);
    approx::assert_relative_eq!(hit_pos.z, 0.0, epsilon = 1e-5);
    approx::assert_relative_eq!(normal.z, 1.0, epsilon = 1e-5);

    // Hidden part returns None
    assert!(raycast_parts(origin, dir, &parts, &[false]).is_none());
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
fn test_compute_adaptive_tolerance() {
    assert_eq!(compute_adaptive_tolerance(None), DEFAULT_TOLERANCE);

    let small_bbox = BoundingBox::new(DVec3::ZERO, DVec3::new(0.01, 0.01, 0.01));
    assert_eq!(compute_adaptive_tolerance(Some(&small_bbox)), MIN_TOLERANCE);

    let huge_bbox = BoundingBox::new(DVec3::ZERO, DVec3::new(1000.0, 1000.0, 1000.0));
    assert_eq!(compute_adaptive_tolerance(Some(&huge_bbox)), MAX_TOLERANCE);

    let mid_bbox = BoundingBox::new(DVec3::ZERO, DVec3::new(10.0, 10.0, 10.0));
    approx::assert_relative_eq!(
        compute_adaptive_tolerance(Some(&mid_bbox)),
        0.01,
        epsilon = 1e-6
    );
}
