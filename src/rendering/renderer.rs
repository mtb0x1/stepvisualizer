//! The per-frame renderer: turns parts + camera into a presented frame.
use std::rc::Rc;

use crate::{
    common::{
        BoundingBox, RenderablePart, Vec3, ViewportSize, fps_meter::FpsMeter, look_at_mat4,
        perspective,
    },
    error::StepVizError,
    rendering::camera::CameraState,
    rendering::wgpu_state::{PartGpu, WgpuState},
    trace_span,
};

/// Render one frame of `parts` onto the canvas owned by `state`.
///
/// The camera is framed on the visible parts' bounding box (hidden parts may
/// leave the remaining geometry off the model's baked-at-origin center).
/// Per-part GPU buffers are reused across frames and only reallocated when
/// a part's geometry size changes; MVP/model/color uniforms are rewritten
/// every frame; each visible part is one indexed draw.
pub async fn render_wgpu_on_canvas(
    state: Rc<WgpuState>,
    parts: &[RenderablePart],
    visibility: &[bool],
    camera: &CameraState,
    fps_meter: Rc<FpsMeter>,
) -> Result<(), StepVizError> {
    trace_span!("render_wgpu_on_canvas");
    let WgpuState {
        device,
        queue,
        surface,
        config,
        render_pipeline,
        global_bind_group,
        view_proj_buffer,
        part_bind_group_layout,
        depth_texture_view,
        depth_size: _,
        part_buffers,
        ..
    } = &*state;
    // WARNING: In WebGPU on WASM, the browser context's drawing buffer resizes whenever
    // the canvas DOM element dimensions change. To prevent WebGPU validation errors where
    // the color attachment size does not match the depth attachment size, we must ensure
    // the surface configuration and depth buffer are synchronized with the actual canvas
    // DOM dimensions before acquiring the swapchain texture.
    let current_canvas_size = ViewportSize::from_canvas(&state.canvas);
    if current_canvas_size.is_valid() && current_canvas_size != *state.depth_size.borrow() {
        state.canvas.set_width(current_canvas_size.width);
        state.canvas.set_height(current_canvas_size.height);
        state.resize(current_canvas_size);
    }

    let viewport_size = ViewportSize::new(config.borrow().width, config.borrow().height);

    let bounds = {
        let mut cached_opt = state.cached_bounds.borrow_mut();
        if let Some((cached_vis, cached_bbox)) = cached_opt.as_ref() {
            if cached_vis.as_slice() == visibility {
                *cached_bbox
            } else {
                let computed = crate::common::render::visible_bounds(parts, visibility)
                    .unwrap_or(BoundingBox::new(glam::DVec3::splat(-1.0), glam::DVec3::splat(1.0)));
                *cached_opt = Some((visibility.to_vec(), computed));
                computed
            }
        } else {
            let computed = crate::common::render::visible_bounds(parts, visibility)
                .unwrap_or(BoundingBox::new(glam::DVec3::splat(-1.0), glam::DVec3::splat(1.0)));
            *cached_opt = Some((visibility.to_vec(), computed));
            computed
        }
    };

    let max_size = bounds.max_extent_f32().max(0.1);
    let view_target = bounds.center_f32();

    // The camera distance is expressed for a reference model of size ~1 (see
    // `CameraState::DEFAULT`). Real models span arbitrary coordinate scales, so
    // scaling the orbit distance by the visible extent keeps the framing
    // identical regardless of model size — without it, a large model would
    // swallow the camera (eye ends up inside the geometry).
    let fit_distance = camera.distance * max_size;
    let camera_target = CameraState {
        target: view_target,
        distance: fit_distance,
        ..*camera
    };
    let eye = camera_target.eye_position();
    let view_matrix = look_at_mat4(eye, view_target, Vec3::Y);

    let aspect = viewport_size.aspect_ratio();
    const FOV_Y: f32 = std::f32::consts::FRAC_PI_3;
    let near = crate::common::constants::NEAR_PLANE;
    let far = max_size * 100.0;
    let projection_matrix = perspective(FOV_Y, aspect, near, far);

    // wgpu 30 returns `CurrentSurfaceTexture` instead of a `Result`:
    // - Success / Suboptimal hand us a presentable texture (Suboptimal also
    //   hints the surface should be reconfigured soon),
    // - Timeout / Occluded are transient states; skipping the frame is the
    //   documented response,
    // - Outdated / Lost are fatal surface failures that require reconfiguring
    //   the surface and retrying.
    let frame = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(texture)
        | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
        wgpu::CurrentSurfaceTexture::Timeout => return Ok(()),
        wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
        wgpu::CurrentSurfaceTexture::Outdated => {
            surface.configure(device, &config.borrow());
            return Ok(());
        }
        wgpu::CurrentSurfaceTexture::Lost => {
            surface.configure(device, &config.borrow());
            return Ok(());
        }
        wgpu::CurrentSurfaceTexture::Validation => {
            return Err(StepVizError::RenderError(
                "Surface texture validation failed".to_string(),
            ));
        }
    };

    // Recreate the depth texture when its size diverges from the surface's
    // actual swapchain size (which can lag `config` during resize).
    state.ensure_depth_texture(ViewportSize::new(
        frame.texture.width(),
        frame.texture.height(),
    ));

    let texture_view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Main Command Encoder"),
    });

    let view_proj = projection_matrix * view_matrix;
    queue.write_buffer(view_proj_buffer, 0, bytemuck::bytes_of(&view_proj));

    {
        let depth_texture_view = depth_texture_view.borrow();
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Main Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(crate::common::constants::CLEAR_COLOR),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(render_pipeline);
        render_pass.set_bind_group(0, global_bind_group, &[]);

        let mut cache = part_buffers.borrow_mut();
        if cache.len() != parts.len() {
            cache.resize_with(parts.len(), || None);
        }

        for (index, part) in parts.iter().enumerate() {
            if !visibility.get(index).copied().unwrap_or(true) || part.indices.is_empty() {
                continue;
            }

            let vertex_count = part.vertices.len();
            let index_count = part.indices.len();
            let needs_recreate = match cache[index].as_ref() {
                Some(gpu) => gpu.vertex_count != vertex_count || gpu.index_count != index_count,
                None => true,
            };
            if needs_recreate {
                cache[index] = Some(PartGpu::new(device, part_bind_group_layout, part));
            }

            let gpu = match cache[index].as_mut() {
                Some(gpu) => gpu,
                None => unreachable!("part GPU buffer slot is populated by the branch above"),
            };

            gpu.upload_uniforms(queue, part);

            render_pass.set_bind_group(1, &gpu.bind_group, &[]);
            render_pass.set_vertex_buffer(0, gpu.vertex_buffer.slice(..));
            render_pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..gpu.index_count as u32, 0, 0..1);
        }
    }

    queue.submit(Some(encoder.finish()));
    // wgpu 30 moved presentation from `SurfaceTexture::present` to the queue,
    // which consumes the texture.
    queue.present(frame);
    fps_meter.record_frame();
    Ok(())
}
