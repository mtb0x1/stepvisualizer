//! Domain types and pure logic shared across the app: STEP parsing,
//! tessellation, persistence, caching, and matrix math. No Yew code lives
//! here, so most of it is unit-testable on the host target as well.

pub mod core;
pub mod graphics;
pub mod math;
pub mod step;
pub mod sys;

// Re-export old module paths for backwards compatibility
pub use core::{
    constants, error, fast_hash,
    fast_hash::{FastBuildHasher, FastU64Hasher, FastU64Map, FastU64Set},
    types,
    types::{
        AuditMetadata, BoundingBox, FileId, FileIndexItem, LengthUnit, Metadata, StepModel,
        ViewportSize,
    },
    utils,
    utils::{
        build_svg_polyline_points, clean_unit_name, contains_ignore_ascii_case,
        find_ignore_ascii_case, format_bbox_coordinates, format_list_or_na, format_or_na,
    },
};

pub use glam::{DMat4, DVec3, Mat4, Vec3, Vec4};
pub use graphics::{
    color,
    color::{Color, PART_COLORS, PART_COLORS_COUNT, StepColorMap, part_color},
    fps_meter, render,
    render::{GpuVertex, RenderablePart, TessellationOutput, extract_render_parts, visible_bounds},
};
pub use math::{
    compute_bounding_box, geometric_normal, ray_aabb_intersect, ray_triangle_intersect,
    raycast_parts, screen_point_to_ray, spherical_to_cartesian, triangle_area,
    triangle_signed_volume,
};
pub use step::{
    ast_helpers,
    ast_helpers::{
        ParameterExt, TryExtractParam, extract_direction_coords, extract_entity_refs,
        is_collinear_with_x, is_unit_z_direction,
    },
    exchange_index,
    exchange_index::ExchangeIndex,
    parser,
    parser::{StepParser, StepSchema, convert_header, convert_header_from_ast, validate_schema},
    step_names,
    step_names::StepNameMap,
};
pub use sys::{
    logger, time, web,
    web::{sanitize_host_for_db_name, storage_prefix},
};
