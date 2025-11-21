use crate::{AppTracer, AppTracerTrait, trace_span};
use bytemuck::{Pod, Zeroable};
use js_sys::Date;

use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use truck_geometry::prelude::*;
use truck_meshalgo::prelude::*;

thread_local! {
    static RENDER_PART_CACHE: RefCell<HashMap<String, Rc<Vec<RenderablePart>>>> =
        RefCell::new(HashMap::new());
}

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
    pub color: [f32; 3],
    pub visible: bool,
}

impl Default for RenderablePart {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            model_matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            color: [0.8, 0.8, 0.8],
            visible: true,
        }
    }
}

pub fn step_extract_wsgl_reqs(
    file_id: &str,
    step_table: &truck_stepio::r#in::Table,
) -> Vec<RenderablePart> {
    trace_span!("step_extract_wsgl_reqs");

    if let Some(cached) = try_get_cached_parts(file_id) {
        // let msg = format!(
        //     "step_extract_wsgl_reqs => cache hit for {} ({} parts)",
        //     file_id,
        //     cached.len()
        // );
        // AppTracer::debug(&msg);
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
    tessellate_table(&table, &mut parts_to_render);
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

    cache_parts(file_id, &parts_to_render);
    parts_to_render
}

fn tessellate_table(table: &truck_stepio::r#in::Table, parts_to_render: &mut Vec<RenderablePart>) {
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

        //this has to be smaller than the radius of the sphere
        //FIXME: this is a hack
        //allow user to set tolerance (trigger 3D scene re-render)
        use crate::common::constants::DEFAULT_TOLERANCE;
        let tolerance = DEFAULT_TOLERANCE; // smaller => higher quality, but slower
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
            let color = COLORS[color_index];

            parts_to_render.push(RenderablePart {
                vertices,
                indices,
                model_matrix,
                color,
                visible: true,
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

fn try_get_cached_parts(file_id: &str) -> Option<Vec<RenderablePart>> {
    RENDER_PART_CACHE.with(|cache| cache.borrow().get(file_id).map(|parts| (**parts).clone()))
}

fn cache_parts(file_id: &str, parts: &[RenderablePart]) {
    let rc = Rc::new(parts.to_vec());
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().insert(file_id.to_string(), rc);
    });
}

pub fn drop_cached_parts(file_id: &str) {
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().remove(file_id);
    });
}

pub fn clear_cached_parts() {
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}
