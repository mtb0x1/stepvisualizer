use crate::{
    apptracing::{AppTracer, AppTracerTrait},
    common::{RenderablePart, ViewportSize},
    error::StepVizError,
    trace_span,
};
use bytemuck::cast_slice;
use std::cell::RefCell;
use wasm_bindgen::JsValue;
use web_sys::HtmlCanvasElement;
use wgpu::{
    self, SurfaceTarget,
    util::{BufferInitDescriptor, DeviceExt},
};

/// Whether the browser exposes the `navigator.gpu` entry point.
///
/// This gates the entire application, not just the viewport: wgpu's
/// `BROWSER_WEBGPU` backend cannot create a surface at all without it, so
/// probing first lets the app swap itself for an explanatory page instead of
/// mounting a shell whose every feature dead-ends at the renderer.
pub fn browser_has_webgpu() -> bool {
    web_sys::window()
        .map(|window| {
            js_sys::Reflect::has(&window.navigator(), &JsValue::from_str("gpu")).unwrap_or(false)
        })
        // No JS window (e.g. non-browser embedding): treat as unsupported
        // rather than letting the app mount and fail asynchronously later.
        .unwrap_or(false)
}

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
    pub model_buffer: wgpu::Buffer,
    pub color_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub uniforms_uploaded: bool,
}

impl PartGpu {
    /// Allocate vertex, index, and uniform buffers on `device` for `part`, and
    /// construct the matching part bind group under `layout`.
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        part: &RenderablePart,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: cast_slice(&part.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: cast_slice(&part.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let model_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Model Uniform Buffer"),
            contents: bytemuck::bytes_of(&[0.0f32; 16]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let color_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Color Uniform Buffer"),
            contents: bytemuck::bytes_of(&[0.0f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: model_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: color_buffer.as_entire_binding(),
                },
            ],
            label: Some("part_bind_group"),
        });

        Self {
            vertex_buffer,
            index_buffer,
            vertex_count: part.vertices.len(),
            index_count: part.indices.len(),
            model_buffer,
            color_buffer,
            bind_group,
            uniforms_uploaded: false,
        }
    }

    /// Upload model transform and RGBA color uniforms to the GPU queue once per part.
    pub fn upload_uniforms(&mut self, queue: &wgpu::Queue, part: &RenderablePart) {
        if !self.uniforms_uploaded {
            queue.write_buffer(
                &self.model_buffer,
                0,
                bytemuck::bytes_of(&part.model_matrix),
            );
            queue.write_buffer(&self.color_buffer, 0, bytemuck::bytes_of(&part.color));
            self.uniforms_uploaded = true;
        }
    }
}

/// Owned WebGPU context for one canvas: device/queue/surface, pipeline and
/// bind group layout, plus interior-mutable per-frame state (surface config,
/// depth view, per-part buffer cache). Shared as `Rc<WgpuState>`; the
/// `RefCell` fields let `resize` and the renderer mutate it without unique
/// ownership.
pub struct WgpuState {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    /// Surface configuration (canvas size). Interior-mutable so `resize` can
    /// update it without requiring unique ownership of the whole `WgpuState`.
    pub config: RefCell<wgpu::SurfaceConfiguration>,
    pub render_pipeline: wgpu::RenderPipeline,
    pub global_bind_group: wgpu::BindGroup,
    pub view_proj_buffer: wgpu::Buffer,
    pub part_bind_group_layout: wgpu::BindGroupLayout,
    /// Depth texture view, sized to the canvas. Recreated on resize and kept
    /// in sync with the surface's actual swapchain size (see `ensure_depth_texture`).
    pub depth_texture_view: RefCell<wgpu::TextureView>,
    /// Dimensions of the currently allocated `depth_texture_view`. Tracked so
    /// the renderer can detect when the surface swapchain size diverges from
    /// the configured size (browser-driven resizes / zoom) and rebuild the
    /// depth attachment to match, avoiding a depth/color size-mismatch error.
    pub depth_size: RefCell<ViewportSize>,
    /// Per-part GPU buffers, keyed by the part's position index in the frame's
    /// part list. Slots are (re)created lazily when missing or when the part's
    /// geometry size changes, and truncated to match the live part count.
    pub part_buffers: RefCell<Vec<Option<PartGpu>>>,
}

impl WgpuState {
    /// Reconfigure the surface and rebuild the depth texture for a new canvas
    /// size. Safe to call between frames; callers should trigger a re-render
    /// afterwards so the next frame uses the updated dimensions/aspect ratio.
    pub fn resize(&self, size: ViewportSize) {
        if !size.is_valid() {
            return;
        }
        {
            let mut config = self.config.borrow_mut();
            config.width = size.width;
            config.height = size.height;
            self.surface.configure(&self.device, &config);
        }
        *self.depth_texture_view.borrow_mut() =
            create_depth_texture_view(&self.device, size.width, size.height);
        *self.depth_size.borrow_mut() = size;
    }

    /// Recreate the depth texture view when its size does not match the
    /// surface's actual swapchain size. The surface texture returned by
    /// `get_current_texture` can diverge from `config` after browser-driven
    /// resizes (zoom, layout changes), which would otherwise make the depth
    /// attachment smaller than the color attachment and fail validation.
    /// Calling this each frame keeps the two in lockstep without paying for a
    /// reallocation unless the size actually changed.
    pub fn ensure_depth_texture(&self, size: ViewportSize) {
        if !size.is_valid() || *self.depth_size.borrow() == size {
            return;
        }
        *self.depth_texture_view.borrow_mut() =
            create_depth_texture_view(&self.device, size.width, size.height);
        *self.depth_size.borrow_mut() = size;
    }
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
            && self.global_bind_group == other.global_bind_group
            && self.view_proj_buffer == other.view_proj_buffer
            && self.part_bind_group_layout == other.part_bind_group_layout
            && self.depth_texture_view == other.depth_texture_view
            && self.depth_size == other.depth_size
    }
}

use crate::common::constants::{POWER_PREFERENCE, WGSL_SHADER};

/// Build the depth attachment view sized to the current canvas dimensions.
/// Shared by `init_wgpu` and `WgpuState::resize` so both paths stay in sync
/// (same format, usage flags, and single-mip/single-sample configuration).
fn create_depth_texture_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    depth_texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Create the full WebGPU context for `canvas`: surface, adapter
/// (high-performance preference), device, depth texture, and the shader +
/// render pipeline. Fails with a descriptive [`StepVizError`] on any GPU
/// init step (no WebGPU support, adapter/device request failure, ...).
pub async fn init_wgpu(canvas: HtmlCanvasElement) -> Result<WgpuState, StepVizError> {
    trace_span!("init_wgpu");

    // wgpu 30 dropped `Default` for `InstanceDescriptor` (the new `display`
    // field has no universal default), so start from the display-less
    // constructor; the canvas reaches wgpu via `create_surface` below. The
    // descriptor is now also consumed by value.
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });

    let target = SurfaceTarget::Canvas(canvas.clone());
    let surface = match instance.create_surface(target) {
        Ok(surface) => surface,
        Err(err) => {
            let msg = format!("Failed to create WebGPU surface: {err}");
            AppTracer::error(&msg);
            return Err(StepVizError::GpuInitFailed(msg));
        }
    };

    let adapter = match instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            // Named in `constants.rs::POWER_PREFERENCE` so the trade-off (frame
            // rate vs. battery) is documented and adjustable in one place.
            power_preference: POWER_PREFERENCE,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            // Anti-fingerprinting measure (reports bucketed instead of exact
            // adapter limits); keep it off so we see the real limits.
            apply_limit_buckets: false,
        })
        .await
    {
        Ok(adapter) => adapter,
        Err(err) => {
            let msg = format!("Failed to request WebGPU adapter: {err}");
            AppTracer::error(&msg);
            return Err(StepVizError::GpuInitFailed(msg));
        }
    };
    let (device, queue) = match adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
    {
        Ok((device, queue)) => (device, queue),
        Err(err) => {
            let msg = format!("Failed to request adapter device: {err}");
            AppTracer::error(&msg);
            return Err(StepVizError::GpuInitFailed(msg));
        }
    };

    let size = ViewportSize::from_canvas(&canvas);
    canvas.set_width(size.width);
    canvas.set_height(size.height);

    //FIXME : Params below may not be the best choice
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface.get_capabilities(&adapter).formats[0],
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        // Required in wgpu 30. `Auto` reproduces the historical behavior
        // (the presentation engine keeps treating the canvas as sRGB).
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    let depth_texture_view = create_depth_texture_view(&device, size.width, size.height);
    let depth_size = size;

    let shader_module_descriptor = wgpu::ShaderModuleDescriptor {
        label: Some("shader"),
        source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
    };
    let shader = device.create_shader_module(shader_module_descriptor);
    let global_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(64),
                },
                count: None,
            }],
            label: Some("global_bind_group_layout"),
        });

    let view_proj_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("View Projection Buffer"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let global_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &global_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: view_proj_buffer.as_entire_binding(),
        }],
        label: Some("global_bind_group"),
    });

    let part_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        // vec4<f32> is exactly 16 bytes.
                        min_binding_size: std::num::NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
            label: Some("part_bind_group_layout"),
        });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        // wgpu 30: each layout slot is optional (None skips that set), and
        // `immediate_size` replaces `push_constant_ranges` (we use neither
        // feature, so a single mandatory layout and zero immediate bytes).
        bind_group_layouts: &[
            Some(&global_bind_group_layout),
            Some(&part_bind_group_layout),
        ],
        immediate_size: 0,
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        cache: None,
        vertex: wgpu::VertexState {
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            module: &shader,
            entry_point: Some("vs_main"),
            // wgpu 30: each vertex buffer slot is optional (None skips the
            // slot); ours stays mandatory.
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::common::GpuVertex>()
                    as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            })],
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
            // wgpu 30 makes the depth options optional (None disables the
            // depth aspect); we keep depth testing and writes as before.
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        // wgpu 30 replaced `multiview` with `multiview_mask`; single-view
        // rendering uses `None`.
        multiview_mask: None,
    });

    Ok(WgpuState {
        device,
        queue,
        surface,
        config: RefCell::new(config),
        render_pipeline,
        global_bind_group,
        view_proj_buffer,
        part_bind_group_layout,
        depth_texture_view: RefCell::new(depth_texture_view),
        depth_size: RefCell::new(depth_size),
        part_buffers: RefCell::new(Vec::new()),
    })
}
