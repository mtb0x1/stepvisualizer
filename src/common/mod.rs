//! Domain types and pure logic shared across the app: STEP parsing,
//! tessellation, persistence, caching, and matrix math. No Yew code lives
//! here, so most of it is unit-testable on the host target as well.

pub mod core;
pub mod graphics;
pub mod math;
pub mod step;
pub mod sys;

// Re-export old module paths for backwards compatibility
pub use core::{constants, error, fast_hash, types, utils};
pub use graphics::{color, fps_meter, render};
pub use step::{ast_helpers, exchange_index, parser, step_names};
pub use sys::{logger, time, web};

pub use core::fast_hash::{FastBuildHasher, FastU64Hasher, FastU64Map, FastU64Set};
#[allow(unused_imports)]
pub use core::types::{
    AuditMetadata, BoundingBox, FileId, FileIndexItem, LengthUnit, Metadata, StepModel,
    ViewportSize,
};
#[allow(unused_imports)]
pub use core::utils::{
    build_svg_polyline_points, clean_unit_name, contains_ignore_ascii_case, find_ignore_ascii_case,
    format_bbox_coordinates, format_list_or_na, format_or_na,
};
pub use glam::dcamera::rh::proj::directx::perspective;
pub use glam::dcamera::rh::view::look_at_mat4;
pub use glam::{DMat4, DVec3, Mat4, Vec3, Vec4};
pub use graphics::color::{Color, PART_COLORS, PART_COLORS_COUNT, StepColorMap, part_color};
pub use graphics::render::{
    GpuVertex, RenderablePart, TessellationOutput, extract_render_parts, visible_bounds,
};
#[allow(unused_imports)]
pub use math::{
    geometric_normal, ray_aabb_intersect, ray_triangle_intersect, raycast_parts,
    screen_point_to_ray, spherical_to_cartesian, triangle_area, triangle_signed_volume,
};
#[allow(unused_imports)]
pub use step::ast_helpers::{ParameterExt, TryExtractParam, extract_entity_refs};
pub use step::exchange_index::ExchangeIndex;
#[allow(unused_imports)]
pub use step::parser::{
    StepSchema, all_usable_sections, build_initial_metadata, compute_bounding_box, convert_header,
    extract_header_and_count, parse_units, probe_validate_step_buffer, validate_schema,
};
pub use step::step_names::StepNameMap;
#[allow(unused_imports)]
pub use sys::web::{sanitize_host_for_db_name, storage_prefix};
