use core::panic;

use crate::{apptracing::AppTracer, apptracing::AppTracerTrait, trace_span};
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;
use wgpu::{self, SurfaceTarget, util::DeviceExt};

#[derive(PartialEq)]
pub struct WgpuState {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub render_pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

const WGSL_SHADER: &str = r#"
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

pub async fn init_wgpu(canvas: HtmlCanvasElement) -> Result<WgpuState, Box<dyn std::error::Error>> {
    trace_span!("init_wgpu");

    let instance_descriptor = wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..Default::default()
    };

    let instance = wgpu::Instance::new(&instance_descriptor);

    let target = SurfaceTarget::Canvas(canvas.clone());
    let surface = match instance.create_surface(target) {
        Ok(surface) => surface,
        Err(err) => {
            let msg = format!("Failed to create WebGPU surface: {}", err);
            AppTracer::error(&msg);
            return Err(msg.into());
        }
    };

    //FIXME : we request an adapter with the surface
    // and activate high performance mode ? (does it make sense for all hardwares, who knows)
    let adapter = match instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
    {
        Ok(adapter) => adapter,
        Err(err) => {
            let msg = format!("Failed to request WebGPU adapter: {}", err);
            AppTracer::error(&msg);
            return Err(msg.into());
        }
    };
    //FIXME: we should request an adapter with the surface
    // let adapter = instance
    //     .request_adapter(&wgpu::RequestAdapterOptions {
    //         compatible_surface: Some(&surface),
    //         ..Default::default()
    //     })
    //     .await
    //     .expect("No adapter found");
    let (device, queue) = match adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
    {
        Ok((device, queue)) => (device, queue),
        Err(err) => {
            let msg = format!("Failed to request adapter device: {}", err);
            AppTracer::error(&msg);
            return Err(msg.into());
        }
    };

    let canvas_width = canvas.client_width().max(1) as u32;
    let canvas_height = canvas.client_height().max(1) as u32;
    canvas.set_width(canvas_width);
    canvas.set_height(canvas_height);

    //FIXME : Params below may not be the best choice
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface.get_capabilities(&adapter).formats[0],
        width: canvas_width,
        height: canvas_height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    let shader_module_descriptor = wgpu::ShaderModuleDescriptor {
        label: Some("shader"),
        source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
    };
    let shader = device.create_shader_module(shader_module_descriptor);
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    //FIXME: this should be 64 but it crashes
                    min_binding_size: std::num::NonZeroU64::new(12),
                },
                count: None,
            },
        ],
        label: Some("bind_group_layout"),
    });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        cache: None,
        vertex: wgpu::VertexState {
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::common::GpuVertex>()
                    as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    Ok(WgpuState {
        device,
        queue,
        surface,
        config,
        render_pipeline,
        bind_group_layout,
    })
}
