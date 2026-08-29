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
use crate::common::math::Mat4;
use crate::common::time::now_ms;
use crate::common::types::BoundingBox;

/// Vertex layout shared with the render pipeline: position + normal, both
/// `Float32x3`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq, Serialize, Deserialize)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// One tessellated part (typically one shell): vertex/index buffers plus the
/// per-part model matrix and color. Serializable, so whole models round-trip
/// through localStorage without re-tessellating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderablePart {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    pub model_matrix: Mat4,
    pub color: [f32; 4],
}

impl Default for RenderablePart {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            model_matrix: Mat4::IDENTITY,
            color: [0.8, 0.8, 0.8, 1.0],
        }
    }
}

impl RenderablePart {
    /// Returns the number of triangles in this part.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Returns the number of vertices in this part.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Translates the part's model matrix by `offset`.
    pub fn translate(&mut self, offset: [f32; 3]) {
        self.model_matrix.0[12] += offset[0];
        self.model_matrix.0[13] += offset[1];
        self.model_matrix.0[14] += offset[2];
    }

    /// Iterate over the triangles referenced by the index buffer.
    ///
    /// Malformed entries are skipped: an incomplete trailing triple (the index
    /// count is not a multiple of 3) and triples pointing outside the vertex
    /// buffer. Tessellated parts are always well-formed, so this only guards
    /// against corrupted deserialized models.
    fn triangles(&self) -> impl Iterator<Item = ([f32; 3], [f32; 3], [f32; 3])> + '_ {
        self.indices.as_chunks::<3>().0.iter().filter_map(|tri| {
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
            .map(|(v0, v1, v2)| triangle_signed_volume(v0, v1, v2))
            .sum();
        (volume / 6.0).abs()
    }

    /// Sum of triangle areas, each ½|(v1−v0)×(v2−v0)|.
    pub fn calculate_surface_area(&self) -> f64 {
        self.triangles()
            .map(|(v0, v1, v2)| triangle_area(v0, v1, v2))
            .sum()
    }
}

/// Signed tetrahedron volume for a single triangle v0, v1, v2 relative to the origin.
#[inline(always)]
fn triangle_signed_volume(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> f64 {
    let p0 = [v0[0] as f64, v0[1] as f64, v0[2] as f64];
    let p1 = [v1[0] as f64, v1[1] as f64, v1[2] as f64];
    let p2 = [v2[0] as f64, v2[1] as f64, v2[2] as f64];
    let cross_x = p1[1] * p2[2] - p1[2] * p2[1];
    let cross_y = p1[2] * p2[0] - p1[0] * p2[2];
    let cross_z = p1[0] * p2[1] - p1[1] * p2[0];
    p0[0] * cross_x + p0[1] * cross_y + p0[2] * cross_z
}

/// Area of a single triangle with vertices v0, v1, v2.
#[inline(always)]
fn triangle_area(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> f64 {
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

/// Tessellate `step_table` into renderable parts — or return the cached
/// result for `file_id` when it was computed earlier this session.
///
/// `tolerance` is the triangulation tolerance (smaller = finer, slower).
/// The whole-model centering translation is baked into each part's model
/// matrix so geometry stays immutable across frames.
pub fn extract_render_parts(
    file_id: &str,
    step_table: &truck_stepio::r#in::Table,
    tolerance: f64,
) -> Vec<RenderablePart> {
    trace_span!("extract_render_parts");

    if let Some(cached) = crate::common::cache::get_cached_parts(file_id) {
        return cached;
    }

    let total_start = now_ms();
    let mut parts_to_render = Vec::new();

    let section_start = now_ms();
    let table = step_table;

    let msg = format!(
        "extract_render_parts => built table for section {} in {:.2} ms (shells: {})",
        0,
        now_ms() - section_start,
        table.shell.len()
    );
    AppTracer::debug(&msg);
    let section_start = now_ms();
    tessellate_table(table, tolerance, &mut parts_to_render);
    let tessellate_ms = now_ms() - section_start;
    let msg = format!(
        "extract_render_parts => tessellated {} parts in {:.2} ms",
        parts_to_render.len(),
        tessellate_ms
    );
    AppTracer::debug(&msg);

    let total_ms = now_ms() - total_start;
    let vertices: usize = parts_to_render.iter().map(|p| p.vertex_count()).sum();
    let triangles: usize = parts_to_render.iter().map(|p| p.triangle_count()).sum();

    let summary = format!(
        "extract_render_parts => tessellation summary: {:.2} ms, parts={}, vertices={}, triangles={}",
        total_ms,
        parts_to_render.len(),
        vertices,
        triangles
    );
    AppTracer::debug(&summary);

    // Center the whole model at the origin once, by baking the centering
    // translation into each part's model matrix. This keeps the geometry
    // immutable across frames so the renderer no longer needs to mutate it.
    let center = compute_parts_center(&parts_to_render);
    let offset = [-center[0], -center[1], -center[2]];
    for part in &mut parts_to_render {
        part.translate(offset);
    }

    crate::common::cache::cache_parts(file_id, &parts_to_render);
    parts_to_render
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
            bbox.expand_point([
                vertex.position[0] as f64,
                vertex.position[1] as f64,
                vertex.position[2] as f64,
            ]);
        }
    }

    if visible_count > 0 && bbox.is_valid() {
        Some(bbox)
    } else {
        None
    }
}

/// Bounding-box center across all parts; `[0,0,0]` when there is no geometry.
fn compute_parts_center(parts: &[RenderablePart]) -> [f32; 3] {
    visible_bounds(parts, &[])
        .map(|b| b.center_f32())
        .unwrap_or([0.0, 0.0, 0.0])
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
) {
    if !orientation {
        mesh.invert();
    }

    let positions = mesh.positions();
    let normals = mesh.normals();

    let mut vertex_map = std::collections::HashMap::<(usize, Option<usize>), u32>::new();

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

        let d1 = [
            (p1.x - p0.x) as f64,
            (p1.y - p0.y) as f64,
            (p1.z - p0.z) as f64,
        ];
        let d2 = [
            (p2.x - p0.x) as f64,
            (p2.y - p0.y) as f64,
            (p2.z - p0.z) as f64,
        ];
        let cross = [
            d1[1] * d2[2] - d1[2] * d2[1],
            d1[2] * d2[0] - d1[0] * d2[2],
            d1[0] * d2[1] - d1[1] * d2[0],
        ];
        let len = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let fallback_normal = if len > 1e-12 {
            [
                (cross[0] / len) as f32,
                (cross[1] / len) as f32,
                (cross[2] / len) as f32,
            ]
        } else {
            [0.0, 1.0, 0.0]
        };

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
                        Some(n) => [n.x as f32, n.y as f32, n.z as f32],
                        None => fallback_normal,
                    };
                    let new_idx = vertices.len() as u32;
                    vertices.push(GpuVertex {
                        position: [pos.x as f32, pos.y as f32, pos.z as f32],
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
) {
    for (shell_index, shell) in table.shell.values().enumerate() {
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
                append_face_geometry(mesh, face.orientation, &mut vertices, &mut indices);
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
}
