//! Domain types and pure logic shared across the app: STEP parsing,
//! tessellation, persistence, caching, and matrix math. No Yew code lives
//! here, so most of it is unit-testable on the host target as well.
pub mod cache;
pub mod constants;
pub mod fps_meter;
pub mod parser;
pub mod render;
pub mod storage;
pub mod time;
pub mod types;
pub mod utils;

pub use cache::LruCache;
pub use glam::Vec3;
pub use glam::camera::rh::proj::opengl::perspective;
pub use glam::camera::rh::view::look_at_mat4;
#[allow(unused_imports)]
pub use parser::{
    all_usable_sections, build_initial_metadata, compute_bounding_box, convert_header, parse_units,
};
pub use render::{GpuVertex, RenderablePart, extract_render_parts, visible_bounds};
#[allow(unused_imports)]
pub use storage::{
    clear_all_storage, delete_model, hash_text_to_id, load_index, load_model, save_index,
    save_model,
};
#[allow(unused_imports)]
pub use types::{
    BoundingBox, FileId, FileIndexItem, LengthUnit, Metadata, StepModel, ViewportSize,
};
