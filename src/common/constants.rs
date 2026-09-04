//! Central knobs: cache sizes, localStorage/IndexedDB keys, and render parameters.
//! Storage keys are runtime functions (not consts) so they can be namespaced
//! per deployment environment without a rebuild.

/// Parsed [`StepModel`](super::types::StepModel)s kept in the in-memory LRU
/// per session.
pub const CACHE_SIZE: usize = 5;
/// Upload guard: STEP is a text format, so this caps both memory and parse time.
/// Stored as `f64` because `web_sys::File::size()` returns `f64`. 20MB fits perfectly
/// within the contiguous integer range of `f64` (up to 2^53), so there is no precision loss.
pub const MAX_FILE_BYTES: f64 = 50.0 * 1024.0 * 1024.0; // 50mb max (text file ...)
/// Detect the deployment environment from `window.location.pathname` and return
/// a namespacing prefix for storage keys:
/// - `/stepvisualizer/testing/…`   → `"testing:"`
/// - `/stepvisualizer/production/…` → `"production:"`
/// - local dev / unknown            → `""` (no prefix, fully backward-compatible)
///
/// The result is computed once per page load and cached in a thread-local.
use crate::common::utils::detect_env_prefix;

/// Returns the per-environment storage prefix (cached after first call).
pub fn env_prefix() -> &'static str {
    std::thread_local! {
        static CACHE: std::cell::OnceCell<&'static str> = const { std::cell::OnceCell::new() };
    }
    CACHE.with(|c| *c.get_or_init(detect_env_prefix))
}

/// localStorage key for the recent-files index (`Vec<FileIndexItem>`),
/// namespaced by deployment environment.
///
/// Examples: `"stepvisualizer:index"` (dev), `"testing:stepvisualizer:index"`, `"production:stepvisualizer:index"`.
pub fn ls_index_key() -> std::borrow::Cow<'static, str> {
    let prefix = env_prefix();
    if prefix.is_empty() {
        std::borrow::Cow::Borrowed("stepvisualizer:index")
    } else {
        std::borrow::Cow::Owned(format!("{prefix}stepvisualizer:index"))
    }
}

/// Prefix for per-model localStorage keys, namespaced by deployment environment.
///
/// Full key format: `<env_prefix>stepvisualizer:model:<id>`.
/// Examples: `"stepvisualizer:model:"` (dev), `"testing:stepvisualizer:model:"`, `"production:stepvisualizer:model:"`.
pub fn ls_model_key_prefix() -> std::borrow::Cow<'static, str> {
    let prefix = env_prefix();
    if prefix.is_empty() {
        std::borrow::Cow::Borrowed("stepvisualizer:model:")
    } else {
        std::borrow::Cow::Owned(format!("{prefix}stepvisualizer:model:"))
    }
}

/// IndexedDB database name, namespaced by deployment environment.
///
/// Examples: `"stepvisualizer_db"` (dev), `"stepvisualizer_db_testing"`, `"stepvisualizer_db_production"`.
pub fn db_name() -> std::borrow::Cow<'static, str> {
    let prefix = env_prefix();
    if prefix.is_empty() {
        std::borrow::Cow::Borrowed("stepvisualizer_db")
    } else {
        // Strip trailing ":" from prefix ("testing:" → "stepvisualizer_db_testing")
        std::borrow::Cow::Owned(format!(
            "stepvisualizer_db_{}",
            prefix.trim_end_matches(':')
        ))
    }
}
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
pub const NEAR_PLANE: f64 = 0.1;
/// Default tessellation tolerance: smaller = finer mesh, slower tessellation.
pub const DEFAULT_TOLERANCE: f64 = 0.005;
/// Minimum tessellation tolerance (limits excessive subdivision on very large models).
pub const MIN_TOLERANCE: f64 = 1e-4;
/// Maximum tessellation tolerance (ensures adequate curve smoothness on tiny models).
pub const MAX_TOLERANCE: f64 = 0.05;

pub use crate::common::utils::compute_adaptive_tolerance;

/// User-selectable tessellation quality trade-off. The chosen variant applies
/// a multiplier to the adaptive tolerance computed from the model bounding box.
/// A higher multiplier means coarser (faster) tessellation; lower means finer.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum QualityPreset {
    Coarse,
    #[default]
    Balanced,
    Fine,
}

impl QualityPreset {
    /// Tolerance multiplier applied after `compute_adaptive_tolerance`.
    /// Result is clamped to `[MIN_TOLERANCE, MAX_TOLERANCE]` at the call site.
    pub const fn multiplier(self) -> f64 {
        match self {
            Self::Coarse => 4.0,
            Self::Balanced => 1.0,
            Self::Fine => 0.25,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Coarse => "Coarse",
            Self::Balanced => "Balanced",
            Self::Fine => "Fine",
        }
    }

    /// Short description shown as a hover tooltip on the quality-preset button.
    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Coarse => {
                "Coarse: Fastest tessellation, lower mesh detail. \
                Best for large or complex files where speed matters."
            }
            Self::Balanced => {
                "Balanced: Default quality. \
                Good trade-off between detail and processing speed."
            }
            Self::Fine => {
                "Fine: Highest mesh detail, slower tessellation. \
                Best for inspecting small features or curved surfaces."
            }
        }
    }
}
/// Canvas clear color (RGB, alpha is always 1).
pub const CLEAR_COLOR_RGB: (f64, f64, f64) = (0.165, 0.165, 0.165);
/// Static clear color pre-formatted for wgpu render pass descriptor.
pub const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: CLEAR_COLOR_RGB.0,
    g: CLEAR_COLOR_RGB.1,
    b: CLEAR_COLOR_RGB.2,
    a: 1.0,
};

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

pub use crate::common::utils::part_color;
