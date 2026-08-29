//! The per-frame renderer: turns parts + camera into a presented frame.
use std::rc::Rc;

use crate::{
    apptracing::{AppTracer, AppTracerTrait},
    common::{
        RenderablePart, create_look_at_matrix, create_perspective_matrix, fps_meter::FpsMeter,
        multiply_matrices,
    },
    error::StepVizError,
    rendering::camera::{CameraState, compute_eye_position},
    rendering::wgpu_state::WgpuState,
    trace_span,
};
use bytemuck::cast_slice;
use wgpu::util::{BufferInitDescriptor, DeviceExt};

/// Render one frame of `parts` onto the canvas owned by `state`.
///
/// The camera is framed on the visible parts' bounding box (hidden parts may
/// leave the remaining geometry off the model's baked-at-origin center).
/// Per-part GPU buffers are reused across frames and only reallocated when
/// a part's geometry size changes; MVP/model/color uniforms are rewritten
/// every frame; each visible part is one indexed draw.
pub async fn render_wgpu_on_canvas(
    state: Rc<WgpuState>,
    parts: Vec<RenderablePart>,
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
        bind_group_layout,
        depth_texture_view,
        depth_size: _,
        part_buffers,
    } = &*state;

    let canvas_width = config.borrow().width;
    let canvas_height = config.borrow().height;

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    let mut visible_count = 0;

    // Frame on the visible subset only: when parts are hidden the remaining
    // geometry may sit off the (baked-at-origin) model center, so we derive
    // the orbit target from the visible bounding box instead of the origin.
    for (index, part) in parts.iter().enumerate() {
        if !visibility.get(index).copied().unwrap_or(true) {
            continue;
        }
        if part.vertices.is_empty() {
            continue;
        }
        visible_count += 1;
        for vertex in &part.vertices {
            let pos = vertex.position;
            min_x = min_x.min(pos[0]);
            min_y = min_y.min(pos[1]);
            min_z = min_z.min(pos[2]);
            max_x = max_x.max(pos[0]);
            max_y = max_y.max(pos[1]);
            max_z = max_z.max(pos[2]);
        }
    }

    if visible_count == 0 {
        min_x = -1.0;
        max_x = 1.0;
        min_y = -1.0;
        max_y = 1.0;
        min_z = -1.0;
        max_z = 1.0;
    }

    let size_x = (max_x - min_x).max(0.1);
    let size_y = (max_y - min_y).max(0.1);
    let size_z = (max_z - min_z).max(0.1);
    let max_size = size_x.max(size_y).max(size_z);

    let view_target = [
        (min_x + max_x) * 0.5,
        (min_y + max_y) * 0.5,
        (min_z + max_z) * 0.5,
    ];

    // The camera distance is expressed for a reference model of size ~1 (see
    // `CameraState::DEFAULT`). Real models span arbitrary coordinate scales, so
    // scaling the orbit distance by the visible extent keeps the framing
    // identical regardless of model size — without it, a large model would
    // swallow the camera (eye ends up inside the geometry).
    let fit_distance = camera.distance * max_size;
    let camera_target = CameraState {
        target: view_target,
        distance: fit_distance,
        ..(*camera).clone()
    };
    let eye = compute_eye_position(&camera_target);
    let view_matrix = create_look_at_matrix(eye, view_target, [0.0, 1.0, 0.0]);

    let aspect = canvas_width as f32 / canvas_height as f32;
    let fov_y = std::f32::consts::PI / 3.0;
    let near = crate::common::constants::NEAR_PLANE;
    let far = max_size * 100.0;
    let projection_matrix = create_perspective_matrix(fov_y, aspect, near, far);

    // wgpu 30 returns `CurrentSurfaceTexture` instead of a `Result`:
    // - Success / Suboptimal hand us a presentable texture (Suboptimal also
    //   hints the surface should be reconfigured soon),
    // - Timeout / Occluded are transient states; skipping the frame is the
    //   documented response,
    // - Outdated / Lost / Validation are real failures the user should see.
    let frame = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) => frame,
        wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
            AppTracer::warn("Suboptimal surface texture; rendering frame, reconfigure soon");
            frame
        }
        status @ (wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded) => {
            AppTracer::warn(&format!(
                "Skipping frame: transient surface state {status:?}"
            ));
            return Ok(());
        }
        status @ (wgpu::CurrentSurfaceTexture::Outdated
        | wgpu::CurrentSurfaceTexture::Lost
        | wgpu::CurrentSurfaceTexture::Validation) => {
            let msg = format!("Failed to acquire surface texture: {status:?}");
            AppTracer::error(&msg);
            return Err(msg.into());
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    // Keep the depth attachment in lockstep with the surface's actual size:
    // the swapchain texture can diverge from `config` after browser-driven
    // resizes (zoom), which would otherwise make the depth attachment smaller
    // than the color attachment and fail render-pass validation.
    state.ensure_depth_texture(frame.texture.width(), frame.texture.height());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });

    {
        let depth_texture_view = depth_texture_view.borrow();
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: crate::common::constants::CLEAR_COLOR_RGB.0,
                        g: crate::common::constants::CLEAR_COLOR_RGB.1,
                        b: crate::common::constants::CLEAR_COLOR_RGB.2,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: Some(0),
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &*depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            // wgpu 30 addition; single-view render passes use `None`.
            multiview_mask: None,
        });

        render_pass.set_pipeline(render_pipeline);

        // Keep the buffer cache sized to the live part list. Truncating only
        // drops tail slots, so surviving parts keep their allocated buffers.
        let mut cache = part_buffers.borrow_mut();
        cache.truncate(parts.len());

        for (index, part) in parts.iter().enumerate() {
            if !visibility.get(index).copied().unwrap_or(true) {
                continue;
            }
            if part.indices.is_empty() {
                AppTracer::warn("Skipping render of part with empty indices");
                continue;
            }

            while cache.len() <= index {
                cache.push(None);
            }

            // (Re)allocate geometry + uniform buffers only when the slot is
            // empty or the part's geometry size has changed; otherwise reuse.
            let vertex_count = part.vertices.len();
            let index_count = part.indices.len();
            let needs_recreate = match cache[index].as_ref() {
                Some(gpu) => gpu.vertex_count != vertex_count || gpu.index_count != index_count,
                None => true,
            };
            if needs_recreate {
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
                let mvp_buffer = device.create_buffer_init(&BufferInitDescriptor {
                    label: Some("MVP Uniform Buffer"),
                    contents: bytemuck::bytes_of(&[0.0f32; 16]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: mvp_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: model_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: color_buffer.as_entire_binding(),
                        },
                    ],
                    label: Some("bind_group"),
                });

                cache[index] = Some(crate::rendering::wgpu_state::PartGpu {
                    vertex_buffer,
                    index_buffer,
                    vertex_count,
                    index_count,
                    mvp_buffer,
                    model_buffer,
                    color_buffer,
                    bind_group,
                });
            }

            let gpu = match cache[index].as_ref() {
                Some(gpu) => gpu,
                None => unreachable!("part GPU buffer slot is populated by the branch above"),
            };

            let mvp_matrix = multiply_matrices(
                &projection_matrix,
                &multiply_matrices(&view_matrix, &part.model_matrix),
            );
            queue.write_buffer(&gpu.mvp_buffer, 0, bytemuck::bytes_of(&mvp_matrix));
            queue.write_buffer(&gpu.model_buffer, 0, bytemuck::bytes_of(&part.model_matrix));
            queue.write_buffer(&gpu.color_buffer, 0, bytemuck::bytes_of(&part.color));

            render_pass.set_bind_group(0, &gpu.bind_group, &[]);
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
