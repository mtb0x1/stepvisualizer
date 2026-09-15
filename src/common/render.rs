//! Tessellation of STEP geometry into GPU-ready triangle meshes, plus the
//! per-part mesh type the renderer and metric calculations operate on.
use super::logger;
use crate::trace_span;
use bytemuck::{Pod, Zeroable};

use serde::{Deserialize, Serialize};
use truck_geometry::prelude::*;
use truck_meshalgo::prelude::*;

use crate::common::color::{Color, StepColorMap, part_color};
use crate::common::step_names::StepNameMap;
use crate::common::time::now_ms;
use crate::common::types::BoundingBox;
use crate::common::utils::{
    compute_parts_center, geometric_normal, triangle_area, triangle_signed_volume,
};
use glam::{DVec3, Mat4, Vec3};

/// Interleaved GPU vertex: 3D position and surface normal.
/// 24 bytes, 4-byte aligned. Matches WebGPU vertex buffer layout.
#[repr(C)]
#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Pod,
    Zeroable,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct GpuVertex {
    pub position: Vec3,
    pub normal: Vec3,
}

impl GpuVertex {
    #[inline(always)]
    pub const fn new(position: Vec3, normal: Vec3) -> Self {
        Self { position, normal }
    }
}

/// One tessellated part (typically one shell): vertex/index buffers plus the
/// per-part model matrix, color, and optional name. Serializable, so whole models round-trip
/// through localStorage without re-tessellating.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RenderablePart {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    pub model_matrix: Mat4,
    pub color: Color,
    #[serde(default)]
    pub name: Option<String>,
}

impl Default for RenderablePart {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            model_matrix: Mat4::IDENTITY,
            color: Color::DEFAULT_PART,
            name: None,
        }
    }
}

impl RenderablePart {
    /// Returns the number of triangles in this part.
    #[inline]
    pub const fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Returns the number of vertices in this part.
    #[inline]
    pub const fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Translates the part's model matrix by `offset`.
    #[inline]
    pub fn translate(&mut self, offset: DVec3) {
        self.model_matrix.w_axis += offset.as_vec3().extend(0.0);
    }

    /// Iterate over the triangles referenced by the index buffer as double-precision coordinates.
    ///
    /// Malformed entries are skipped: an incomplete trailing triple (the index
    /// count is not a multiple of 3) and triples pointing outside the vertex
    /// buffer. Tessellated parts are always well-formed, so this only guards
    /// against corrupted deserialized models.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    fn triangles(&self) -> impl Iterator<Item = (DVec3, DVec3, DVec3)> + '_ {
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
                self.vertices[idx0].position.as_dvec3(),
                self.vertices[idx1].position.as_dvec3(),
                self.vertices[idx2].position.as_dvec3(),
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

/// Tessellate `step_table` into renderable parts — or return the cached
/// Result of [`extract_render_parts`], containing the extracted meshes, the
/// number of skipped shells, and any descriptive warnings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TessellationOutput {
    pub parts: Vec<RenderablePart>,
    pub skipped_shells: usize,
    pub warnings: Vec<String>,
}

/// Tessellates geometry from STEP entity tables into GPU-ready [`RenderablePart`]s.
/// Returns a [`TessellationOutput`] containing parts, count of skipped shells, and warnings.
///
/// `tolerance` is the triangulation tolerance (smaller = finer, slower).
/// The whole-model centering translation is baked into each part's model
/// matrix so geometry stays immutable across frames.
pub fn extract_render_parts(
    step_tables: &[truck_stepio::r#in::Table],
    colors: Option<&StepColorMap>,
    names: Option<&StepNameMap>,
    tolerance: f64,
) -> TessellationOutput {
    trace_span!("extract_render_parts");

    let total_start = now_ms();
    let mut parts_to_render = Vec::new();
    let mut total_skipped: usize = 0;
    let mut warnings = Vec::new();

    for (i, table) in step_tables.iter().enumerate() {
        let section_start = now_ms();
        let skipped = tessellate_table(
            table,
            colors,
            names,
            tolerance,
            &mut parts_to_render,
            &mut warnings,
        );
        total_skipped += skipped;
        let tessellate_ms = now_ms() - section_start;
        let msg = format!(
            "extract_render_parts => section {i}: tessellated {} parts in {:.2} ms (shells: {}, skipped: {})",
            parts_to_render.len(),
            tessellate_ms,
            table.shell.len(),
            skipped
        );
        logger::debug(&msg);
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
    logger::debug(&summary);

    // Center the whole model at the origin once, by baking the centering
    // translation into each part's model matrix. This keeps the geometry
    // immutable across frames so the renderer no longer needs to mutate it.
    let center = compute_parts_center(&parts_to_render);
    let offset = -center;
    for part in &mut parts_to_render {
        part.translate(offset);
    }

    TessellationOutput {
        parts: parts_to_render,
        skipped_shells: total_skipped,
        warnings,
    }
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
            let world_pos = part
                .model_matrix
                .transform_point3(vertex.position)
                .as_dvec3();
            bbox.expand_point(world_pos);
        }
    }

    if visible_count > 0 && bbox.is_valid() {
        Some(bbox)
    } else {
        None
    }
}

// Fast Vertex Deduplication Helpers
//
// When turning STEP shapes into 3D meshes for WebGPU, each triangle corner has
// two pieces of info:
//   1. `pos`: where the point is in 3D space.
//   2. `nor`: which direction that surface is pointing (surface normal).
//
// Why "pack" them into a single u64? (pack_vertex_key)
//
// Imagine you have two numbers: your House Number (`pos`) and your Apartment Number (`nor`).
// Previously, Rust kept them as a tuple `(usize, Option<usize>)`. In WebAssembly (WASM),
// an `Option<usize>` needs an extra tag byte plus memory alignment padding, making the whole
// pair take up 16 bytes of memory! When looking up a point in our map, the CPU had to inspect
// two separate memory slots and check if the apartment exists.
//
// But in 32-bit WASM, both numbers easily fit inside 32 bits (up to 4 billion, far more points
// than any single face will ever have!).
//
// So instead of carrying two bulky boxes, we glue them into one single 64-bit number (`u64`):
//   - Top 32 bits: House Number (`pos`) shifted left by 32 bits.
//   - Bottom 32 bits: Apartment Number (`nor`), or `u32::MAX` if there is no normal.
//
// Now, checking if we already drew this exact point is just comparing a single number,
// which takes a single CPU instruction!
#[inline(always)]
fn pack_vertex_key(pos: usize, nor: Option<usize>) -> u64 {
    let nor_u32 = match nor {
        Some(n) => n as u32,
        None => u32::MAX,
    };
    ((pos as u64) << 32) | (nor_u32 as u64)
}

// Vertex welding hash map using SplitMix64 FastU64Hasher (see fast_hash.rs)
use crate::common::fast_hash::FastU64Map;
type VertexMap = FastU64Map<u32>;

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
    vertex_map: &mut VertexMap,
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

        // Geometric normal fallback from the first three distinct points of the face (computed in f64)
        let p0 = match positions.get(face[0].pos) {
            Some(p) => DVec3::new(p.x, p.y, p.z),
            None => continue,
        };
        let p1 = match positions.get(face[1].pos) {
            Some(p) => DVec3::new(p.x, p.y, p.z),
            None => continue,
        };
        let p2 = match positions.get(face[2].pos) {
            Some(p) => DVec3::new(p.x, p.y, p.z),
            None => continue,
        };

        let fallback_normal = geometric_normal(p0, p1, p2);

        // Triangulate polygon (triangle fan: 0, i, i+1)
        for i in 1..(face.len() - 1) {
            let tri = [face[0], face[i], face[i + 1]];
            for v in tri {
                let pos = match positions.get(v.pos) {
                    Some(p) => p,
                    None => continue,
                };
                let key = pack_vertex_key(v.pos, v.nor);
                let idx = *vertex_map.entry(key).or_insert_with(|| {
                    let normal = match v.nor.and_then(|idx| normals.get(idx)) {
                        Some(n) => Vec3::new(n.x as f32, n.y as f32, n.z as f32),
                        None => fallback_normal.as_vec3(),
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
/// to compress or has missing edges is skipped with a warning instead of failing the whole file.
fn tessellate_table(
    table: &truck_stepio::r#in::Table,
    colors: Option<&StepColorMap>,
    names: Option<&StepNameMap>,
    tolerance: f64,
    parts_to_render: &mut Vec<RenderablePart>,
    warnings: &mut Vec<String>,
) -> usize {
    let mut shells = Vec::with_capacity(table.shell.len());
    shells.extend(table.shell.iter());
    shells.sort_by_key(|(k, _)| *k);
    let mut skipped: usize = 0;
    let mut vertex_map = VertexMap::default();
    for (shell_index, (shell_key, shell)) in shells.into_iter().enumerate() {
        let model_matrix = Mat4::IDENTITY;

        let compress_start = now_ms();
        let cshell = match table.to_compressed_shell(shell) {
            Ok(cshell) => cshell,
            Err(err) => {
                let warn = format!("shell {shell_index} failed to compress: {err}");
                logger::warn(&format!("extract_render_parts => {warn}"));
                warnings.push(warn);
                skipped += 1;
                continue;
            }
        };
        let compress_ms = now_ms() - compress_start;

        // Defensive guard: if a compressed shell has faces but zero valid edges
        // (e.g. unhandled curve geometry), truck-meshalgo will panic at line 244 on
        // empty boundary point vectors. Gracefully log and skip instead of aborting.
        if !cshell.faces.is_empty() && cshell.edges.is_empty() {
            let warn = format!("shell {shell_index} has faces but no valid boundary edges");
            logger::warn(&format!(
                "extract_render_parts => {warn}; skipped to avoid panic"
            ));
            warnings.push(warn);
            skipped += 1;
            continue;
        }

        let tri_start = now_ms();

        // tolerance: smaller => higher quality, but slower
        let poly_shell = cshell.triangulation(tolerance);
        let triangulation_ms = now_ms() - tri_start;

        let estimated_faces = poly_shell.faces.len();
        let mut vertices = Vec::with_capacity(estimated_faces * 3);
        let mut indices = Vec::with_capacity(estimated_faces * 3);
        vertex_map.clear();

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
            let color = colors
                .and_then(|c| c.get(*shell_key))
                .unwrap_or_else(|| part_color(parts_to_render.len()));
            let name = names.and_then(|n| n.get(*shell_key)).map(String::from);

            parts_to_render.push(RenderablePart {
                vertices,
                indices,
                model_matrix,
                color,
                name,
            });
        }

        let shell_msg = format!(
            "extract_render_parts => shell {} processed (compress {:.2} ms, triangulation {:.2} ms, parts={})",
            shell_index,
            compress_ms,
            triangulation_ms,
            parts_to_render.len()
        );
        logger::debug(&shell_msg);
    }
    skipped
}
