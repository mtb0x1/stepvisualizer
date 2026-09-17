//! Domain types and pure logic shared across the app: STEP parsing,
//! tessellation, persistence, caching, and matrix math. No Yew code lives
//! here, so most of it is unit-testable on the host target as well.
pub mod ast_helpers;
pub mod color;
pub mod constants;
pub mod error;
pub mod exchange_index;
pub mod fast_hash;
pub mod fps_meter;
pub mod logger;
pub mod math;
pub mod parser;
pub mod render;
pub mod step_names;
pub mod time;
pub mod types;
pub mod utils;
pub mod web;

#[allow(unused_imports)]
pub use ast_helpers::{
    extract_entity_refs, param_as_enum, param_as_list, param_as_real, param_as_ref, param_as_str,
};
pub use color::{Color, PART_COLORS, PART_COLORS_COUNT, StepColorMap, part_color};
pub use exchange_index::ExchangeIndex;
pub use fast_hash::{FastBuildHasher, FastU64Hasher, FastU64Map, FastU64Set};
pub use glam::dcamera::rh::proj::directx::perspective;
pub use glam::dcamera::rh::view::look_at_mat4;
pub use glam::{DMat4, DVec3, Mat4, Vec3, Vec4};
#[allow(unused_imports)]
pub use math::{
    geometric_normal, ray_aabb_intersect, ray_triangle_intersect, raycast_parts,
    screen_point_to_ray, spherical_to_cartesian, triangle_area, triangle_signed_volume,
};
#[allow(unused_imports)]
pub use parser::{
    StepSchema, all_usable_sections, build_initial_metadata, compute_bounding_box, convert_header,
    extract_header_and_count, parse_units, probe_validate_step_buffer, validate_schema,
};
pub use render::{
    GpuVertex, RenderablePart, TessellationOutput, extract_render_parts, visible_bounds,
};
pub use step_names::StepNameMap;
#[allow(unused_imports)]
pub use types::{
    AuditMetadata, BoundingBox, FileId, FileIndexItem, LengthUnit, Metadata, StepModel,
    ViewportSize,
};
#[allow(unused_imports)]
pub use utils::{
    build_svg_polyline_points, clean_unit_name, contains_ignore_ascii_case, find_ignore_ascii_case,
    format_bbox_coordinates, format_list_or_na, format_or_na,
};
#[allow(unused_imports)]
pub use web::{sanitize_host_for_db_name, storage_prefix};
