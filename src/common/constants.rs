pub const CACHE_SIZE: usize = 5;
pub const MAX_FILE_BYTES: f64 = 20.0 * 1024.0 * 1024.0; //20mb max (text file ...)
pub const LS_INDEX_KEY: &str = "stepviz:index";
pub const NA: &str = "N/A";
pub const STEP_TRACER: &str = "[STEP_TRACER]";
pub const WGSL_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
}

struct CameraUniforms {
    view_proj: mat4x4<f32>,
    view_position: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniforms;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.world_position = model.position;
    out.world_normal = model.normal;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let ambient = 0.2;
    let diffuse = max(dot(in.world_normal, light_dir), 0.0) * 0.7;
    let color = vec3<f32>(0.8, 0.8, 0.8) * (ambient + diffuse);
    return vec4<f32>(color, 1.0);
}
"#;
