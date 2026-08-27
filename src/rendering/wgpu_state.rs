use crate::{apptracing::AppTracer, apptracing::AppTracerTrait, trace_span};
use std::cell::RefCell;
use web_sys::HtmlCanvasElement;
use wgpu::{self, SurfaceTarget};

/// GPU-side buffers for a single rendered part.
///
/// Geometry buffers (`vertex_buffer`, `index_buffer`) are immutable for the
/// lifetime of a given part, so they are allocated once and reused across
/// frames. The uniform buffers (`mvp_buffer`, `model_buffer`, `color_buffer`)
/// are rewritten every frame; the `bind_group` references them and is only
/// rebuilt when the geometry buffers are (re)allocated.
pub struct PartGpu {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub vertex_count: usize,
    pub index_count: usize,
    pub mvp_buffer: wgpu::Buffer,
    pub model_buffer: wgpu::Buffer,
    pub color_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

pub struct WgpuState {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub render_pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub depth_texture_view: wgpu::TextureView,
    /// Per-part GPU buffers, keyed by the part's position index in the frame's
    /// part list. Slots are (re)created lazily when missing or when the part's
    /// geometry size changes, and truncated to match the live part count.
    pub part_buffers: RefCell<Vec<Option<PartGpu>>>,
}

impl PartialEq for WgpuState {
    fn eq(&self, other: &Self) -> bool {
        // `part_buffers` is a per-frame runtime cache and is intentionally
        // excluded: two `WgpuState`s are equivalent when their GPU resources
        // (device, pipeline, surfaces, ...) match, regardless of cached buffers.
        self.device == other.device
            && self.queue == other.queue
            && self.surface == other.surface
            && self.config == other.config
            && self.render_pipeline == other.render_pipeline
            && self.bind_group_layout == other.bind_group_layout
            && self.depth_texture_view == other.depth_texture_view
    }
}

use crate::common::constants::WGSL_SHADER;

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

    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

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
                    min_binding_size: std::num::NonZeroU64::new(16),
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
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
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
        depth_texture_view,
        part_buffers: RefCell::new(Vec::new()),
    })
}
