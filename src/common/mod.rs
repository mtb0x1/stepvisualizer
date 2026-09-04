//! Domain types and pure logic shared across the app: STEP parsing,
//! tessellation, persistence, caching, and matrix math. No Yew code lives
//! here, so most of it is unit-testable on the host target as well.
pub mod cache;
pub mod color;
pub mod constants;
pub mod error;
pub mod fps_meter;
pub mod logger;
pub mod parser;
pub mod render;
pub mod storage;
pub mod time;
pub mod types;
pub mod utils;

pub use cache::LruCache;
pub use color::{Color, PART_COLORS, PART_COLORS_COUNT, StepColorMap, part_color};
pub use glam::dcamera::rh::proj::opengl::perspective;
pub use glam::dcamera::rh::view::look_at_mat4;
pub use glam::{DMat4, DVec3, Mat4, Vec3, Vec4};
#[allow(unused_imports)]
pub use parser::{
    StepSchema, all_usable_sections, build_initial_metadata, compute_bounding_box, convert_header,
    normalize_exchange, parse_units, probe_validate_step_buffer, validate_schema,
};
pub use render::{
    GpuVertex, RenderablePart, TessellationOutput, extract_render_parts, visible_bounds,
};
#[allow(unused_imports)]
pub use storage::{
    clear_all_storage, delete_model, hash_text_to_id, load_index, load_model, save_index,
    save_model,
};
#[allow(unused_imports)]
pub use types::{
    BoundingBox, FileId, FileIndexItem, LengthUnit, Metadata, StepModel, ViewportSize,
};
#[allow(unused_imports)]
pub use utils::{
    build_svg_polyline_points, clean_unit_name, contains_ignore_ascii_case, extract_entity_refs,
    find_ignore_ascii_case, format_bbox_coordinates, format_list_or_na, format_or_na,
    geometric_normal, param_as_enum, param_as_list, param_as_real, param_as_ref, param_as_str,
    spherical_to_cartesian, triangle_area, triangle_signed_volume,
};
