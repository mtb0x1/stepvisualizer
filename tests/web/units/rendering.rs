use glam::{DVec3, DVec4, Mat4, Vec3};
use stepvisualizer::common::color::Color;
use stepvisualizer::common::render::{GpuVertex, RenderablePart, visible_bounds};
use stepvisualizer::common::types::{BoundingBox, ViewportSize};
use stepvisualizer::rendering::camera::{CameraState, spherical_to_cartesian};
use wasm_bindgen_test::*;

fn create_cube_part(size: f32) -> RenderablePart {
    let vertices = vec![
        GpuVertex::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(-1.0, -1.0, -1.0)),
        GpuVertex::new(Vec3::new(size, 0.0, 0.0), Vec3::new(1.0, -1.0, -1.0)),
        GpuVertex::new(Vec3::new(size, size, 0.0), Vec3::new(1.0, 1.0, -1.0)),
        GpuVertex::new(Vec3::new(0.0, size, 0.0), Vec3::new(-1.0, 1.0, -1.0)),
        GpuVertex::new(Vec3::new(0.0, 0.0, size), Vec3::new(-1.0, -1.0, 1.0)),
        GpuVertex::new(Vec3::new(size, 0.0, size), Vec3::new(1.0, -1.0, 1.0)),
        GpuVertex::new(Vec3::new(size, size, size), Vec3::new(1.0, 1.0, 1.0)),
        GpuVertex::new(Vec3::new(0.0, size, size), Vec3::new(-1.0, 1.0, 1.0)),
    ];

    // Front (+Z)
    // Back (-Z)
    // Right (+X)
    // Left (-X)
    // Top (+Y)
    // Bottom (-Y)
    let indices = vec![
        4, 5, 6, 4, 6, 7, 0, 3, 2, 0, 2, 1, 1, 2, 6, 1, 6, 5, 0, 4, 7, 0, 7, 3, 3, 7, 6, 3, 6, 2,
        0, 1, 5, 0, 5, 4,
    ];

    RenderablePart {
        vertices,
        indices,
        model_matrix: Mat4::IDENTITY,
        color: Color::WHITE,
        name: None,
    }
}

fn create_box_part(min_x: f32, max_x: f32) -> RenderablePart {
    RenderablePart {
        vertices: vec![
            GpuVertex::new(Vec3::new(min_x, 0.0, 0.0), Vec3::Y),
            GpuVertex::new(Vec3::new(max_x, 1.0, 1.0), Vec3::Y),
        ],
        indices: vec![0, 1, 0],
        model_matrix: Mat4::IDENTITY,
        color: Color::WHITE,
        name: None,
    }
}

#[wasm_bindgen_test]
fn test_spherical_to_cartesian() {
    let pos = spherical_to_cartesian(0.0, 0.0, 5.0, DVec3::ZERO);
    approx::assert_relative_eq!(pos.x, 5.0, epsilon = 1e-6);
    approx::assert_relative_eq!(pos.y, 0.0, epsilon = 1e-6);
    approx::assert_relative_eq!(pos.z, 0.0, epsilon = 1e-6);

    let offset_pos = spherical_to_cartesian(0.0, 0.0, 5.0, DVec3::new(1.0, 2.0, 3.0));
    approx::assert_relative_eq!(offset_pos.x, 6.0, epsilon = 1e-6);
    approx::assert_relative_eq!(offset_pos.y, 2.0, epsilon = 1e-6);
    approx::assert_relative_eq!(offset_pos.z, 3.0, epsilon = 1e-6);
}

#[wasm_bindgen_test]
fn test_camera_orbit_zoom_pan() {
    let camera = CameraState::default();
    let orbited = camera.orbit(10.0, 5.0);
    assert_eq!(orbited.azimuth, 0.5 + 0.1);
    assert_eq!(orbited.elevation, 0.5 + 0.05);

    let zoomed = camera.zoom(2.0);
    assert_eq!(zoomed.distance, 6.0);

    let canvas_size = ViewportSize::new(800, 600);
    let eye_before = camera.eye_position();
    let target_before = camera.target;
    let panned = camera.pan(10.0, 20.0, canvas_size);
    let eye_after = panned.eye_position();
    let target_after = panned.target;

    approx::assert_relative_eq!(
        (eye_before - target_before).x,
        (eye_after - target_after).x,
        epsilon = 1e-6
    );
    approx::assert_relative_eq!(
        (eye_before - target_before).y,
        (eye_after - target_after).y,
        epsilon = 1e-6
    );
    approx::assert_relative_eq!(
        (eye_before - target_before).z,
        (eye_after - target_after).z,
        epsilon = 1e-6
    );
}

#[wasm_bindgen_test]
fn test_camera_set_target() {
    let camera = CameraState::default();
    let eye_before = camera.eye_position();
    let new_target = DVec3::new(1.0, 2.0, -1.0);
    let retargeted = camera.set_target(new_target);
    let eye_after = retargeted.eye_position();

    approx::assert_relative_eq!(eye_before.x, eye_after.x, epsilon = 1e-5);
    approx::assert_relative_eq!(eye_before.y, eye_after.y, epsilon = 1e-5);
    approx::assert_relative_eq!(eye_before.z, eye_after.z, epsilon = 1e-5);
    assert_eq!(retargeted.target, new_target);
}

#[wasm_bindgen_test]
fn test_projection_matrix_ndc() {
    let eye = DVec3::new(0.0, 0.0, 10.0);
    let target = DVec3::ZERO;
    let view = stepvisualizer::common::look_at_mat4(eye, target, DVec3::Y);
    let proj = stepvisualizer::common::perspective(std::f64::consts::FRAC_PI_3, 1.0, 0.1, 100.0);
    let vp = proj * view;

    let clip = vp * DVec4::new(0.0, 0.0, 0.0, 1.0);
    let ndc = clip.truncate() / clip.w;
    assert!(ndc.z >= 0.0 && ndc.z <= 1.0, "ndc.z was {}", ndc.z);

    let eye_300 = spherical_to_cartesian(0.8, 0.9, 302.25, DVec3::ZERO);
    let view_300 = stepvisualizer::common::look_at_mat4(eye_300, DVec3::ZERO, DVec3::Y);
    let proj_300 = stepvisualizer::common::perspective(
        std::f64::consts::FRAC_PI_3,
        1152.0 / 834.0,
        0.1,
        10075.0,
    );
    let vp_300 = proj_300 * view_300;
    let v_clip = vp_300 * DVec4::new(0.88, -6.35, 40.22, 1.0);
    let v_ndc = v_clip.truncate() / v_clip.w;
    assert!(v_ndc.x >= -1.0 && v_ndc.x <= 1.0, "v_ndc.x = {}", v_ndc.x);
    assert!(v_ndc.y >= -1.0 && v_ndc.y <= 1.0, "v_ndc.y = {}", v_ndc.y);
    assert!(v_ndc.z >= 0.0 && v_ndc.z <= 1.0, "v_ndc.z = {}", v_ndc.z);
}

#[wasm_bindgen_test]
fn metric_cube_volume_and_surface_area() {
    let cube = create_cube_part(1.0);
    approx::assert_relative_eq!(cube.calculate_surface_area(), 6.0, epsilon = 1e-6);
    approx::assert_relative_eq!(cube.calculate_volume(), 1.0, epsilon = 1e-6);

    let scaled = create_cube_part(3.0);
    approx::assert_relative_eq!(scaled.calculate_surface_area(), 54.0, epsilon = 1e-6);
    approx::assert_relative_eq!(scaled.calculate_volume(), 27.0, epsilon = 1e-6);
}

#[wasm_bindgen_test]
fn metric_corrupted_index_skipping() {
    let part = RenderablePart {
        vertices: vec![
            GpuVertex::new(Vec3::new(0.0, 0.0, 0.0), Vec3::Z),
            GpuVertex::new(Vec3::new(1.0, 0.0, 0.0), Vec3::Z),
            GpuVertex::new(Vec3::new(0.0, 1.0, 0.0), Vec3::Z),
        ],
        // First triangle valid (area 0.5), second index out of bounds, trailing incomplete triple
        indices: vec![0, 1, 2, 0, 1, 999, 0, 1],
        model_matrix: Mat4::IDENTITY,
        color: Color::WHITE,
        name: None,
    };

    approx::assert_relative_eq!(part.calculate_surface_area(), 0.5, epsilon = 1e-6);
}

#[wasm_bindgen_test]
fn part_translate_accumulation() {
    let mut part = RenderablePart::default();
    part.translate(DVec3::new(1.0, 2.0, 3.0));
    part.translate(DVec3::new(4.0, 5.0, 6.0));

    assert_eq!(
        part.model_matrix.w_axis.truncate(),
        Vec3::new(5.0, 7.0, 9.0)
    );
}

#[wasm_bindgen_test]
fn renderable_part_serde_roundtrip() {
    let mut part = RenderablePart::default();
    part.translate(DVec3::new(10.0, 20.0, 30.0));
    let json = serde_json::to_string(&part).expect("serialize part");
    let deserialized: RenderablePart = serde_json::from_str(&json).expect("deserialize part");
    assert_eq!(part, deserialized);
}

#[wasm_bindgen_test]
fn visible_bounds_filtering_and_transforms() {
    let part_a = create_box_part(0.0, 1.0);
    let part_b = create_box_part(2.0, 4.0);
    let parts = vec![part_a, part_b];

    // Both visible
    let bounds = visible_bounds(&parts, &[true, true]).expect("valid bounds");
    assert_eq!(bounds.min.x, 0.0);
    assert_eq!(bounds.max.x, 4.0);

    // Single part hidden
    let bounds_hidden = visible_bounds(&parts, &[false, true]).expect("valid bounds");
    assert_eq!(bounds_hidden.min.x, 2.0);
    assert_eq!(bounds_hidden.max.x, 4.0);

    // All hidden returns None
    assert!(visible_bounds(&parts, &[false, false]).is_none());

    // Model matrix translation
    let mut translated_part = create_box_part(0.0, 2.0);
    translated_part.translate(DVec3::new(10.0, 20.0, 30.0));
    let trans_bounds = visible_bounds(&[translated_part], &[true]).expect("valid bounds");
    approx::assert_relative_eq!(trans_bounds.min.x, 10.0, epsilon = 1e-5);
    approx::assert_relative_eq!(trans_bounds.max.x, 12.0, epsilon = 1e-5);
}

#[wasm_bindgen_test]
fn bbox_operations() {
    let mut bbox = BoundingBox::EMPTY;
    assert!(!bbox.is_valid());

    bbox.expand_point(DVec3::new(1.0, 2.0, 3.0));
    assert!(bbox.is_valid());
    assert_eq!(bbox.min, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(bbox.max, DVec3::new(1.0, 2.0, 3.0));

    bbox.expand_point(DVec3::new(5.0, 10.0, 7.0));
    assert_eq!(bbox.min, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(bbox.max, DVec3::new(5.0, 10.0, 7.0));
    assert_eq!(bbox.size_x(), 4.0);
    assert_eq!(bbox.size_y(), 8.0);
    assert_eq!(bbox.size_z(), 4.0);
    assert_eq!(bbox.max_extent(), 8.0);

    let centered = BoundingBox::new(
        DVec3::new(-10.0, -20.0, -30.0),
        DVec3::new(10.0, 20.0, 30.0),
    );
    assert_eq!(centered.center(), DVec3::ZERO);

    // NaN / Infinity check
    let nan_bbox = BoundingBox::new(DVec3::new(f64::NAN, 0.0, 0.0), DVec3::new(1.0, 1.0, 1.0));
    assert!(!nan_bbox.is_valid());

    // Inverted bounds
    let inverted = BoundingBox::new(DVec3::new(10.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0));
    assert!(!inverted.is_valid());
}
