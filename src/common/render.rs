//! Tessellation of STEP geometry into GPU-ready triangle meshes, plus the
//! per-part mesh type the renderer and metric calculations operate on.
use crate::{
    apptracing::{AppTracer, AppTracerTrait},
    trace_span,
};
use bytemuck::{Pod, Zeroable};

use serde::{Deserialize, Serialize};
use truck_geometry::prelude::*;
use truck_meshalgo::prelude::*;

use crate::common::constants::part_color;
use crate::common::time::now_ms;
use crate::common::types::BoundingBox;
use crate::common::utils::{
    compute_parts_center, geometric_normal, triangle_area, triangle_signed_volume,
};
use glam::{Mat4, Vec3, Vec4};

/// Vertex layout shared with the render pipeline: position + normal, both
/// `Float32x3`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq, Serialize, Deserialize)]
pub struct GpuVertex {
    pub position: Vec3,
    pub normal: Vec3,
}

impl GpuVertex {
    /// Construct a new GPU vertex with 3D position and normal.
    #[inline(always)]
    pub const fn new(position: Vec3, normal: Vec3) -> Self {
        Self { position, normal }
    }
}

/// One tessellated part (typically one shell): vertex/index buffers plus the
/// per-part model matrix and color. Serializable, so whole models round-trip
/// through localStorage without re-tessellating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderablePart {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    pub model_matrix: Mat4,
    pub color: Vec4,
}

impl Default for RenderablePart {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            model_matrix: Mat4::IDENTITY,
            color: Vec4::new(0.8, 0.8, 0.8, 1.0),
        }
    }
}

impl RenderablePart {
    /// Returns the number of triangles in this part.
    pub const fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Returns the number of vertices in this part.
    pub const fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Translates the part's model matrix by `offset`.
    pub fn translate(&mut self, offset: Vec3) {
        self.model_matrix.w_axis += offset.extend(0.0);
    }

    /// Iterate over the triangles referenced by the index buffer.
    ///
    /// Malformed entries are skipped: an incomplete trailing triple (the index
    /// count is not a multiple of 3) and triples pointing outside the vertex
    /// buffer. Tessellated parts are always well-formed, so this only guards
    /// against corrupted deserialized models.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    fn triangles(&self) -> impl Iterator<Item = (Vec3, Vec3, Vec3)> + '_ {
        self.indices.chunks_exact(3).filter_map(|tri| {
            let idx0 = tri[0] as usize;
            let idx1 = tri[1] as usize;
            let idx2 = tri[2] as usize;
            if idx0 >= self.vertices.len()
                || idx1 >= self.vertices.len()
                || idx2 >= self.vertices.len()
            {
                return None;
            }
            Some((
                self.vertices[idx0].position,
                self.vertices[idx1].position,
                self.vertices[idx2].position,
            ))
        })
    }

    /// Signed-volume decomposition: each triangle contributes v0·(v1×v2)/6
    /// relative to the origin. The absolute sum is the enclosed volume for a
    /// watertight mesh; open meshes give an approximation.
    pub fn calculate_volume(&self) -> f64 {
        let volume: f64 = self
            .triangles()
            .map(|(v0, v1, v2)| triangle_signed_volume(v0.as_dvec3(), v1.as_dvec3(), v2.as_dvec3()))
            .sum();
        (volume / 6.0).abs()
    }

    /// Sum of triangle areas, each ½|(v1−v0)×(v2−v0)|.
    pub fn calculate_surface_area(&self) -> f64 {
        self.triangles()
            .map(|(v0, v1, v2)| triangle_area(v0.as_dvec3(), v1.as_dvec3(), v2.as_dvec3()))
            .sum()
    }
}

/// Tessellate `step_table` into renderable parts — or return the cached
/// result for `file_id` when it was computed earlier this session.
///
/// `tolerance` is the triangulation tolerance (smaller = finer, slower).
/// The whole-model centering translation is baked into each part's model
/// matrix so geometry stays immutable across frames.
pub fn extract_render_parts(
    step_tables: &[truck_stepio::r#in::Table],
    tolerance: f64,
) -> (Vec<RenderablePart>, usize) {
    trace_span!("extract_render_parts");

    let total_start = now_ms();
    let mut parts_to_render = Vec::new();
    let mut total_skipped: usize = 0;

    for (i, table) in step_tables.iter().enumerate() {
        let section_start = now_ms();
        let skipped = tessellate_table(table, tolerance, &mut parts_to_render);
        total_skipped += skipped;
        let tessellate_ms = now_ms() - section_start;
        let msg = format!(
            "extract_render_parts => section {i}: tessellated {} parts in {:.2} ms (shells: {}, skipped: {})",
            parts_to_render.len(),
            tessellate_ms,
            table.shell.len(),
            skipped
        );
        AppTracer::debug(&msg);
    }

    let total_ms = now_ms() - total_start;
    let vertices: usize = parts_to_render.iter().map(|p| p.vertex_count()).sum();
    let triangles: usize = parts_to_render.iter().map(|p| p.triangle_count()).sum();

    let summary = format!(
        "extract_render_parts => tessellation summary: {:.2} ms, sections={}, parts={}, vertices={}, triangles={}, skipped={}",
        total_ms,
        step_tables.len(),
        parts_to_render.len(),
        vertices,
        triangles,
        total_skipped
    );
    AppTracer::debug(&summary);

    // Center the whole model at the origin once, by baking the centering
    // translation into each part's model matrix. This keeps the geometry
    // immutable across frames so the renderer no longer needs to mutate it.
    let center = compute_parts_center(&parts_to_render);
    let offset = -center;
    for part in &mut parts_to_render {
        part.translate(offset);
    }

    (parts_to_render, total_skipped)
}

/// Bounding box over a subset of parts, taking `visibility` into account.
/// Returns `None` if no geometry is visible.
pub fn visible_bounds(parts: &[RenderablePart], visibility: &[bool]) -> Option<BoundingBox> {
    let mut bbox = BoundingBox::EMPTY;
    let mut visible_count = 0;

    for (index, part) in parts.iter().enumerate() {
        if !visibility.get(index).copied().unwrap_or(true) || part.vertices.is_empty() {
            continue;
        }
        visible_count += 1;
        for vertex in &part.vertices {
            bbox.expand_point_vec3(vertex.position);
        }
    }

    if visible_count > 0 && bbox.is_valid() {
        Some(bbox)
    } else {
        None
    }
}

/// Append one tessellated face's mesh to the part's vertex/index buffers.
///
/// `orientation` is the shell face's orientation flag. Reversed faces get
/// their mesh inverted (`mesh.invert()`), which inverts normals and reverses
/// face vertex order to match the render pipeline's front-face CCW convention.
fn append_face_geometry(
    mut mesh: truck_polymesh::PolygonMesh,
    orientation: bool,
    vertices: &mut Vec<GpuVertex>,
    indices: &mut Vec<u32>,
    vertex_map: &mut std::collections::HashMap<(usize, Option<usize>), u32>,
) {
    if !orientation {
        mesh.invert();
    }
    mesh.triangulate();

    let positions = mesh.positions();
    let normals = mesh.normals();

    vertex_map.clear();

    for face in mesh.face_iter() {
        if face.len() < 3 {
            continue;
        }

        // Geometric normal fallback from the first three distinct points of the face
        let p0 = match positions.get(face[0].pos) {
            Some(p) => p,
            None => continue,
        };
        let p1 = match positions.get(face[1].pos) {
            Some(p) => p,
            None => continue,
        };
        let p2 = match positions.get(face[2].pos) {
            Some(p) => p,
            None => continue,
        };

        let fallback_normal = geometric_normal(
            Vec3::new(p0.x as f32, p0.y as f32, p0.z as f32),
            Vec3::new(p1.x as f32, p1.y as f32, p1.z as f32),
            Vec3::new(p2.x as f32, p2.y as f32, p2.z as f32),
        );

        // Triangulate polygon (triangle fan: 0, i, i+1)
        for i in 1..(face.len() - 1) {
            let tri = [face[0], face[i], face[i + 1]];
            for v in tri {
                let pos = match positions.get(v.pos) {
                    Some(p) => p,
                    None => continue,
                };
                let key = (v.pos, v.nor);
                let idx = *vertex_map.entry(key).or_insert_with(|| {
                    let normal = match v.nor.and_then(|idx| normals.get(idx)) {
                        Some(n) => Vec3::new(n.x as f32, n.y as f32, n.z as f32),
                        None => fallback_normal,
                    };
                    let new_idx = vertices.len() as u32;
                    vertices.push(GpuVertex {
                        position: Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32),
                        normal,
                    });
                    new_idx
                });
                indices.push(idx);
            }
        }
    }
}

/// Tessellate every shell in the table, producing one `RenderablePart` per
/// non-empty shell. Part colors cycle through [`COLORS`]; a shell that fails
/// to compress is skipped with a warning instead of failing the whole file.
fn tessellate_table(
    table: &truck_stepio::r#in::Table,
    tolerance: f64,
    parts_to_render: &mut Vec<RenderablePart>,
) -> usize {
    let mut shells = Vec::with_capacity(table.shell.len());
    shells.extend(table.shell.iter());
    shells.sort_by_key(|(k, _)| *k);
    let mut skipped: usize = 0;
    let mut vertex_map = std::collections::HashMap::<(usize, Option<usize>), u32>::new();
    for (shell_index, (_, shell)) in shells.into_iter().enumerate() {
        let model_matrix = Mat4::IDENTITY;

        let compress_start = now_ms();
        let cshell = match table.to_compressed_shell(shell) {
            Ok(cshell) => cshell,
            Err(err) => {
                let msg = format!(
                    "extract_render_parts => failed to compress shell {}: {}",
                    shell_index, err
                );
                AppTracer::warn(&msg);
                skipped += 1;
                continue;
            }
        };
        let compress_ms = now_ms() - compress_start;

        let tri_start = now_ms();

        // tolerance: smaller => higher quality, but slower
        let poly_shell = cshell.triangulation(tolerance);
        let triangulation_ms = now_ms() - tri_start;

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for face in poly_shell.faces {
            if let Some(mesh) = face.surface {
                append_face_geometry(
                    mesh,
                    face.orientation,
                    &mut vertices,
                    &mut indices,
                    &mut vertex_map,
                );
            }
        }

        if !vertices.is_empty() && !indices.is_empty() {
            let color = part_color(parts_to_render.len());

            parts_to_render.push(RenderablePart {
                vertices,
                indices,
                model_matrix,
                color,
            });
        }

        let shell_msg = format!(
            "extract_render_parts => shell {} processed (compress {:.2} ms, triangulation {:.2} ms, parts={})",
            shell_index,
            compress_ms,
            triangulation_ms,
            parts_to_render.len()
        );
        AppTracer::debug(&shell_msg);
    }
    skipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

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

        let indices = vec![
            // Front (+Z)
            4, 5, 6, 4, 6, 7, // Back (-Z)
            0, 3, 2, 0, 2, 1, // Right (+X)
            1, 2, 6, 1, 6, 5, // Left (-X)
            0, 4, 7, 0, 7, 3, // Top (+Y)
            3, 7, 6, 3, 6, 2, // Bottom (-Y)
            0, 1, 5, 0, 5, 4,
        ];

        RenderablePart {
            vertices,
            indices,
            model_matrix: Mat4::IDENTITY,
            color: Vec4::ONE,
        }
    }

    /// Verifies the surface area of a single flat right-angle triangle with side lengths 1.
    #[wasm_bindgen_test]
    fn metric_unit_triangle_area() {
        let part = RenderablePart {
            vertices: vec![
                GpuVertex::new(Vec3::new(0.0, 0.0, 0.0), Vec3::Z),
                GpuVertex::new(Vec3::new(1.0, 0.0, 0.0), Vec3::Z),
                GpuVertex::new(Vec3::new(0.0, 1.0, 0.0), Vec3::Z),
            ],
            indices: vec![0, 1, 2],
            model_matrix: Mat4::IDENTITY,
            color: Vec4::ONE,
        };

        approx::assert_relative_eq!(part.calculate_surface_area(), 0.5, epsilon = 1e-6);
    }

    /// Verifies that a closed 12-triangle unit cube evaluates to a surface area of exactly 6.0.
    #[wasm_bindgen_test]
    fn metric_unit_cube_surface_area() {
        let cube = create_cube_part(1.0);
        approx::assert_relative_eq!(cube.calculate_surface_area(), 6.0, epsilon = 1e-6);
    }

    /// Verifies that a closed 12-triangle unit cube evaluates to an enclosed volume of exactly 1.0.
    #[wasm_bindgen_test]
    fn metric_unit_cube_volume() {
        let cube = create_cube_part(1.0);
        approx::assert_relative_eq!(cube.calculate_volume(), 1.0, epsilon = 1e-6);
    }

    /// Verifies scaling invariance for surface area and volume of a closed cube of side length 3.0.
    #[wasm_bindgen_test]
    fn metric_scaled_cube_volume_and_area() {
        let cube = create_cube_part(3.0);
        approx::assert_relative_eq!(cube.calculate_surface_area(), 54.0, epsilon = 1e-6);
        approx::assert_relative_eq!(cube.calculate_volume(), 27.0, epsilon = 1e-6);
    }

    /// Verifies that an empty mesh yields zero volume and zero surface area.
    #[wasm_bindgen_test]
    fn metric_empty_mesh() {
        let empty = RenderablePart::default();
        assert_eq!(empty.calculate_surface_area(), 0.0);
        assert_eq!(empty.calculate_volume(), 0.0);
    }

    /// Verifies that corrupted index buffers containing out-of-bounds indices or incomplete trailing triples
    /// are gracefully skipped without panicking.
    #[wasm_bindgen_test]
    fn metric_corrupted_index_skipping() {
        let part = RenderablePart {
            vertices: vec![
                GpuVertex::new(Vec3::new(0.0, 0.0, 0.0), Vec3::Z),
                GpuVertex::new(Vec3::new(1.0, 0.0, 0.0), Vec3::Z),
                GpuVertex::new(Vec3::new(0.0, 1.0, 0.0), Vec3::Z),
            ],
            // First triangle valid (area 0.5), second out of bounds (index 999), trailing incomplete triple (0, 1)
            indices: vec![0, 1, 2, 0, 1, 999, 0, 1],
            model_matrix: Mat4::IDENTITY,
            color: Vec4::ONE,
        };

        approx::assert_relative_eq!(part.calculate_surface_area(), 0.5, epsilon = 1e-6);
    }

    /// Verifies that vertex_count and triangle_count reflect the buffers accurately.
    #[wasm_bindgen_test]
    fn metric_counts() {
        let part = RenderablePart {
            vertices: (0..24)
                .map(|_| GpuVertex::new(Vec3::ZERO, Vec3::Y))
                .collect(),
            indices: (0..36).collect(),
            model_matrix: Mat4::IDENTITY,
            color: Vec4::ONE,
        };

        assert_eq!(part.vertex_count(), 24);
        assert_eq!(part.triangle_count(), 12);
    }

    /// Verifies that translating a part updates its model matrix translation column.
    #[wasm_bindgen_test]
    fn part_translate_accumulation() {
        let mut part = RenderablePart::default();
        part.translate(Vec3::new(1.0, 2.0, 3.0));
        part.translate(Vec3::new(4.0, 5.0, 6.0));

        assert_eq!(
            part.model_matrix.w_axis.truncate(),
            Vec3::new(5.0, 7.0, 9.0)
        );
    }

    /// Verifies serde serialization and deserialization roundtrip for RenderablePart.
    #[wasm_bindgen_test]
    fn renderable_part_serde_roundtrip() {
        let mut part = RenderablePart::default();
        part.translate(Vec3::new(10.0, 20.0, 30.0));
        let json = serde_json::to_string(&part).expect("serialize part");
        let deserialized: RenderablePart = serde_json::from_str(&json).expect("deserialize part");
        assert_eq!(part, deserialized);
    }

    fn create_box_part(min_x: f32, max_x: f32) -> RenderablePart {
        RenderablePart {
            vertices: vec![
                GpuVertex::new(Vec3::new(min_x, 0.0, 0.0), Vec3::Y),
                GpuVertex::new(Vec3::new(max_x, 1.0, 1.0), Vec3::Y),
            ],
            indices: vec![0, 1, 0],
            model_matrix: Mat4::IDENTITY,
            color: Vec4::ONE,
        }
    }

    /// Verifies that visible_bounds encloses all parts when visibility flags are all true.
    #[wasm_bindgen_test]
    fn visible_bounds_all_visible() {
        let part_a = create_box_part(0.0, 1.0);
        let part_b = create_box_part(2.0, 4.0);
        let parts = vec![part_a, part_b];
        let visibility = vec![true, true];

        let bounds = visible_bounds(&parts, &visibility).expect("valid bounds");
        assert_eq!(bounds.min.x, 0.0);
        assert_eq!(bounds.max.x, 4.0);
    }

    /// Verifies that hidden parts (visibility = false) are excluded from the calculated bounding box.
    #[wasm_bindgen_test]
    fn visible_bounds_single_part_hidden() {
        let part_a = create_box_part(0.0, 1.0);
        let part_b = create_box_part(2.0, 4.0);
        let parts = vec![part_a, part_b];
        let visibility = vec![false, true];

        let bounds = visible_bounds(&parts, &visibility).expect("valid bounds");
        assert_eq!(bounds.min.x, 2.0);
        assert_eq!(bounds.max.x, 4.0);
    }

    /// Verifies that visible_bounds returns None when all parts are hidden.
    #[wasm_bindgen_test]
    fn visible_bounds_all_hidden() {
        let part_a = create_box_part(0.0, 1.0);
        let part_b = create_box_part(2.0, 4.0);
        let parts = vec![part_a, part_b];
        let visibility = vec![false, false];

        assert!(visible_bounds(&parts, &visibility).is_none());
    }

    /// Verifies that an empty visibility slice defaults to treating all parts as visible.
    #[wasm_bindgen_test]
    fn visible_bounds_missing_visibility_defaults_true() {
        let part_a = create_box_part(0.0, 1.0);
        let part_b = create_box_part(2.0, 4.0);
        let parts = vec![part_a, part_b];

        let bounds = visible_bounds(&parts, &[]).expect("valid bounds");
        assert_eq!(bounds.min.x, 0.0);
        assert_eq!(bounds.max.x, 4.0);
    }

    /// Verifies that parts with empty vertex buffers are ignored and do not affect the bounds calculation.
    #[wasm_bindgen_test]
    fn visible_bounds_empty_vertex_part_ignored() {
        let empty_part = RenderablePart::default();
        let valid_part = create_box_part(2.0, 5.0);
        let parts = vec![empty_part, valid_part];
        let visibility = vec![true, true];

        let bounds = visible_bounds(&parts, &visibility).expect("valid bounds");
        assert_eq!(bounds.min.x, 2.0);
        assert_eq!(bounds.max.x, 5.0);
    }
}
