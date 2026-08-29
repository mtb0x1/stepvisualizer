//! Domain types and pure logic shared across the app: STEP parsing,
//! tessellation, persistence, caching, and matrix math. No Yew code lives
//! here, so most of it is unit-testable on the host target as well.
pub mod cache;
pub mod constants;
pub mod fps_meter;
pub mod math;
pub mod parser;
pub mod render;
pub mod storage;
pub mod time;
pub mod types;

pub use cache::LruCache;
pub use math::{create_look_at_matrix, create_perspective_matrix, multiply_matrices};
pub use parser::{compute_bounding_box, convert_header, parse_units};
pub use render::{GpuVertex, RenderablePart, extract_render_parts};
pub use storage::{delete_model, hash_text_to_id, load_index, load_model, save_index, save_model};
pub use types::{FileIndexItem, Metadata, StepModel};
