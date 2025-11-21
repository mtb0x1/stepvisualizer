pub const CACHE_SIZE: usize = 5;
pub const MAX_FILE_BYTES: f64 = 20.0 * 1024.0 * 1024.0; //20mb max (text file ...)
pub const LS_INDEX_KEY: &str = "stepviz:index";
pub const NA: &str = "N/A";
pub const STEP_TRACER: &str = "[STEP_TRACER]";
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
var<uniform> mvp_matrix: mat4x4<f32>;

@group(0) @binding(1)
var<uniform> model_matrix: mat4x4<f32>;

@group(0) @binding(2)
var<uniform> color: vec3<f32>;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
var out: VertexOutput;
    out.clip_position = mvp_matrix * vec4<f32>(input.position, 1.0);
    out.normal = (model_matrix * vec4<f32>(input.normal, 0.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let intensity = max(dot(normalize(in.normal), light_dir), 0.0);
    let shaded_color = color * intensity;
    return vec4<f32>(shaded_color, 1.0);
}
"#;

pub const NEAR_PLANE: f32 = 0.1;
pub const DEFAULT_TOLERANCE: f64 = 0.1;
pub const CLEAR_COLOR_RGB: (f64, f64, f64) = (0.165, 0.165, 0.165);

pub const COLORS: [[f32; 3]; 10] = [
    [0.8, 0.2, 0.2],
    [0.2, 0.8, 0.2],
    [0.2, 0.2, 0.8],
    [0.8, 0.8, 0.2],
    [0.8, 0.2, 0.8],
    [0.2, 0.8, 0.8],
    [0.6, 0.4, 0.2],
    [0.4, 0.6, 0.8],
    [0.8, 0.6, 0.4],
    [0.6, 0.8, 0.4],
];
