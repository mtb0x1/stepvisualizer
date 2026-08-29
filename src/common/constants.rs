//! Central knobs: cache sizes, localStorage keys, and render parameters.
//! No logic here — just named values shared across modules.

/// Parsed [`StepModel`](super::types::StepModel)s kept in the in-memory LRU
/// per session.
pub const CACHE_SIZE: usize = 5;
/// Upload guard: STEP is a text format, so this caps both memory and parse time.
/// Stored as `f64` because `web_sys::File::size()` returns `f64`. 20MB fits perfectly
/// within the contiguous integer range of `f64` (up to 2^53), so there is no precision loss.
pub const MAX_FILE_BYTES: f64 = 20.0 * 1024.0 * 1024.0; // 20mb max (text file ...)
const _: () = assert!(
    MAX_FILE_BYTES == 20971520.0,
    "MAX_FILE_BYTES must exactly represent 20MB"
);
/// localStorage key of the recent-files index (Vec<FileIndexItem>).
pub const LS_INDEX_KEY: &str = "stepviz:index";
/// Prefix on every persisted-model localStorage key (`stepviz:model:<id>`),
/// kept alongside `LS_INDEX_KEY` so the key format is documented in one place
/// rather than hard-coded in `storage.rs::model_key`.
pub const LS_MODEL_KEY_PREFIX: &str = "stepviz:model:";
/// Placeholder shown for missing metadata fields in the UI.
pub const NA: &str = "N/A";
/// Prefix on every tracing message/span, so logs are greppable in the console.
pub const STEP_TRACER: &str = "[STEP_TRACER]";
/// Vertex/fragment shader: MVP transform, then two-light diffuse + ambient
/// shading (main light top-right-front, weaker back light, 0.2 ambient).
pub const WGSL_SHADER: &str = r#"
struct VertexInput {
@location(0) position: vec3<f32>,
@location(1) normal: vec3<f32>,
};

struct VertexOutput {
@builtin(position) clip_position: vec4<f32>,
@location(0) normal: vec3<f32>,
};

@group(0) @binding(0)
var<uniform> view_proj_matrix: mat4x4<f32>;

@group(1) @binding(0)
var<uniform> model_matrix: mat4x4<f32>;

@group(1) @binding(1)
var<uniform> color: vec4<f32>;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
var out: VertexOutput;
    out.clip_position = view_proj_matrix * model_matrix * vec4<f32>(input.position, 1.0);
    out.normal = (model_matrix * vec4<f32>(input.normal, 0.0)).xyz;
    return out;
}

    @fragment
    fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
        let norm_len = length(in.normal);
        let norm = select(in.normal / norm_len, vec3<f32>(0.0, 1.0, 0.0), norm_len < 1e-6);
        
        // Primary light from top-right-front
        let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
        let diffuse = max(dot(norm, light_dir), 0.0);
        
        // Secondary "back" light to reveal details in shadows
        let light_dir_back = normalize(vec3<f32>(-1.0, -0.5, -1.0));
        let diffuse_back = max(dot(norm, light_dir_back), 0.0);
        
        // Base ambient light level
        let ambient = 0.2;
        
        // Combine lighting components
        let intensity = ambient + 0.6 * diffuse + 0.2 * diffuse_back;
        
        let shaded_color = color.xyz * intensity;
        return vec4<f32>(shaded_color, 1.0);
    }
    "#;

/// Projection near plane (far plane is derived per frame from model size).
pub const NEAR_PLANE: f32 = 0.1;
/// Default tessellation tolerance: smaller = finer mesh, slower tessellation.
pub const DEFAULT_TOLERANCE: f64 = 0.1;
/// Canvas clear color (RGB, alpha is always 1).
pub const CLEAR_COLOR_RGB: (f64, f64, f64) = (0.165, 0.165, 0.165);

/// GPU adapter power preference requested at init.
///
/// `HighPerformance` favours the discrete GPU for higher, steadier frame rates,
/// at the cost of battery life. For an interactive 3D viewer that trade-off is
/// the right default; switch to `wgpu::PowerPreference::LowPower` if power draw
/// on mobile/portables matters more than frame rate.
pub const POWER_PREFERENCE: wgpu::PowerPreference = wgpu::PowerPreference::HighPerformance;

/// Fatal reason shown when the browser has no `navigator.gpu` at all.
pub const NO_WEBGPU_MSG: &str = "This browser does not expose the WebGPU API \
    (navigator.gpu is missing). Rendering 3D models is not possible.";
/// Prefix for the fatal reason shown when `navigator.gpu` exists but GPU
/// initialization (surface/adapter/device) fails; the underlying wgpu error
/// is appended at the call site.
pub const WEBGPU_INIT_FAILED_MSG: &str = "This browser exposes WebGPU, but the \
    GPU could not be initialized";

/// Per-part palette, cycled by part index (alpha forced to 1 at the use site).
pub const COLORS: [[f32; 4]; 10] = [
    [0.8, 0.2, 0.2, 1.0],
    [0.2, 0.8, 0.2, 1.0],
    [0.2, 0.2, 0.8, 1.0],
    [0.8, 0.8, 0.2, 1.0],
    [0.8, 0.2, 0.8, 1.0],
    [0.2, 0.8, 0.8, 1.0],
    [0.6, 0.4, 0.2, 1.0],
    [0.4, 0.6, 0.8, 1.0],
    [0.8, 0.6, 0.4, 1.0],
    [0.6, 0.8, 0.4, 1.0],
];
