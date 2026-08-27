use crate::{AppTracer, AppTracerTrait, trace_span};
use bytemuck::{Pod, Zeroable};
use js_sys::Date;

use serde::{Deserialize, Serialize};
use truck_geometry::prelude::*;
use truck_meshalgo::prelude::*;

use crate::common::constants::COLORS;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq, Serialize, Deserialize)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderablePart {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    pub model_matrix: [f32; 16],
    pub color: [f32; 4],
}

impl Default for RenderablePart {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            model_matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            color: [0.8, 0.8, 0.8, 1.0],
        }
    }
}

impl RenderablePart {
    /// Iterate over the triangles referenced by the index buffer.
    ///
    /// Malformed entries are skipped: an incomplete trailing triple (the index
    /// count is not a multiple of 3) and triples pointing outside the vertex
    /// buffer. Tessellated parts are always well-formed, so this only guards
    /// against corrupted deserialized models.
    fn triangles(&self) -> impl Iterator<Item = ([f32; 3], [f32; 3], [f32; 3])> + '_ {
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
            .map(|(v0, v1, v2)| {
                let cross_x = v1[1] * v2[2] - v1[2] * v2[1];
                let cross_y = v1[2] * v2[0] - v1[0] * v2[2];
                let cross_z = v1[0] * v2[1] - v1[1] * v2[0];
                (v0[0] * cross_x + v0[1] * cross_y + v0[2] * cross_z) as f64
            })
            .sum();
        (volume / 6.0).abs()
    }

    /// Sum of triangle areas, each ½|(v1−v0)×(v2−v0)|.
    pub fn calculate_surface_area(&self) -> f64 {
        let area: f64 = self
            .triangles()
            .map(|(v0, v1, v2)| {
                let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
                let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
                let cross_x = edge1[1] * edge2[2] - edge1[2] * edge2[1];
                let cross_y = edge1[2] * edge2[0] - edge1[0] * edge2[2];
                let cross_z = edge1[0] * edge2[1] - edge1[1] * edge2[0];
                (cross_x * cross_x + cross_y * cross_y + cross_z * cross_z).sqrt() as f64
            })
            .sum();
        area * 0.5
    }
}

pub fn step_extract_wsgl_reqs(
    file_id: &str,
    step_table: &truck_stepio::r#in::Table,
    tolerance: f64,
) -> Vec<RenderablePart> {
    trace_span!("step_extract_wsgl_reqs");

        if let Some(cached) = crate::common::cache::get_cached_parts(file_id) {
        return cached;
    }

    let total_start = now_ms();
    let mut parts_to_render = Vec::new();

    let section_start = now_ms();
    let table = step_table;

    let msg = format!(
        "step_extract_wsgl_reqs => built table for section {} in {:.2} ms (shells: {})",
        0,
        now_ms() - section_start,
        table.shell.len()
    );
    AppTracer::debug(&msg);
    let section_start = now_ms();
    tessellate_table(&table, tolerance, &mut parts_to_render);
    let tessellate_ms = now_ms() - section_start;
    let msg = format!(
        "step_extract_wsgl_reqs => tessellated {} parts in {:.2} ms",
        parts_to_render.len(),
        tessellate_ms
    );
    AppTracer::debug(&msg);

    let total_ms = now_ms() - total_start;
    let vertices: usize = parts_to_render.iter().map(|p| p.vertices.len()).sum();
    let triangles: usize = parts_to_render.iter().map(|p| p.indices.len() / 3).sum();

    let summary = format!(
        "step_extract_wsgl_reqs => tessellation summary: {:.2} ms, parts={}, vertices={}, triangles={}",
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
    for part in &mut parts_to_render {
        part.model_matrix[12] -= center[0];
        part.model_matrix[13] -= center[1];
        part.model_matrix[14] -= center[2];
    }

    crate::common::cache::cache_parts(file_id, &parts_to_render);
    parts_to_render
}

fn compute_parts_center(parts: &[RenderablePart]) -> [f32; 3] {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for part in parts {
        for vertex in &part.vertices {
            for i in 0..3 {
                min[i] = min[i].min(vertex.position[i]);
                max[i] = max[i].max(vertex.position[i]);
            }
        }
    }
    if parts.is_empty() || min[0] == f32::INFINITY {
        return [0.0, 0.0, 0.0];
    }
    [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ]
}

fn tessellate_table(
    table: &truck_stepio::r#in::Table,
    tolerance: f64,
    parts_to_render: &mut Vec<RenderablePart>,
) {
    for (shell_index, shell) in table.shell.values().enumerate() {
        let model_matrix: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        let compress_start = now_ms();
        let cshell = match table.to_compressed_shell(shell) {
            Ok(cshell) => cshell,
            Err(err) => {
                let msg = format!(
                    "step_extract_wsgl_reqs => failed to compress shell {}: {}",
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
            if let Some(mut mesh) = face.surface {
                let needs_invert = !face.orientation;
                if needs_invert {
                    mesh.invert();
                }

                let face_positions = mesh.positions();
                let face_normals = mesh.normals();

                let base_index = vertices.len() as u32;

                vertices.extend(
                    face_positions
                        .iter()
                        .zip(face_normals.iter())
                        .map(|(p, n)| GpuVertex {
                            position: [p.x as f32, p.y as f32, p.z as f32],
                            normal: [n.x as f32, n.y as f32, n.z as f32],
                        }),
                );

                let faces = mesh.faces();

                let tri_faces = faces.tri_faces();
                let quad_faces = faces.quad_faces();

                for tri in tri_faces {
                    if needs_invert {
                        indices.push(base_index + tri[0].pos as u32);
                        indices.push(base_index + tri[2].pos as u32);
                        indices.push(base_index + tri[1].pos as u32);
                    } else {
                        indices.push(base_index + tri[0].pos as u32);
                        indices.push(base_index + tri[1].pos as u32);
                        indices.push(base_index + tri[2].pos as u32);
                    }
                }

                for quad in quad_faces {
                    if needs_invert {
                        indices.push(base_index + quad[0].pos as u32);
                        indices.push(base_index + quad[2].pos as u32);
                        indices.push(base_index + quad[1].pos as u32);

                        indices.push(base_index + quad[0].pos as u32);
                        indices.push(base_index + quad[3].pos as u32);
                        indices.push(base_index + quad[2].pos as u32);
                    } else {
                        indices.push(base_index + quad[0].pos as u32);
                        indices.push(base_index + quad[1].pos as u32);
                        indices.push(base_index + quad[2].pos as u32);

                        indices.push(base_index + quad[0].pos as u32);
                        indices.push(base_index + quad[2].pos as u32);
                        indices.push(base_index + quad[3].pos as u32);
                    }
                }
            }
        }

        if !vertices.is_empty() && !indices.is_empty() {
            let color_index = parts_to_render.len() % COLORS.len();
            let color3 = COLORS[color_index];
            let color = [color3[0], color3[1], color3[2], 1.0];

            parts_to_render.push(RenderablePart {
                vertices,
                indices,
                model_matrix,
                color,
            });
        }

        let shell_msg = format!(
            "step_extract_wsgl_reqs => shell {} processed (compress {:.2} ms, triangulation {:.2} ms, parts={})",
            shell_index,
            compress_ms,
            triangulation_ms,
            parts_to_render.len()
        );
        AppTracer::debug(&shell_msg);
    }
}

fn now_ms() -> f64 {
    Date::now()
}
