use std::rc::Rc;

use crate::{
    apptracing::{AppTracer, AppTracerTrait},
    common::{RenderablePart, create_look_at_matrix, create_perspective_matrix, multiply_matrices},
    rendering::camera::{CameraState, compute_eye_position},
    rendering::wgpu_state::WgpuState,
    trace_span,
};
use bytemuck::cast_slice;
use wgpu::util::{BufferInitDescriptor, DeviceExt};

pub async fn render_wgpu_on_canvas(
    state: Rc<WgpuState>,
    mut parts: Vec<RenderablePart>,
    camera: &CameraState,
) -> Result<(), Box<dyn std::error::Error>> {
    trace_span!("render_wgpu_on_canvas");
    let WgpuState {
        device,
        queue,
        surface,
        config,
        render_pipeline,
        bind_group_layout,
    } = &*state;

    let canvas_width = config.width;
    let canvas_height = config.height;
    // AppTracer::debug(&format!(
    //     "Canvas dimensions: {}x{}",
    //     canvas_width, canvas_height
    // ));
    // AppTracer::debug(&format!("Rendering {} parts", parts.len()));

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut max_z = f32::NEG_INFINITY;

    for part in &parts {
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

    if parts.is_empty() || (min_x == f32::INFINITY) {
        min_x = -1.0;
        max_x = 1.0;
        min_y = -1.0;
        max_y = 1.0;
        min_z = -1.0;
        max_z = 1.0;
    }

    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let center_z = (min_z + max_z) * 0.5;
    let size_x = (max_x - min_x).max(0.1);
    let size_y = (max_y - min_y).max(0.1);
    let size_z = (max_z - min_z).max(0.1);
    let max_size = size_x.max(size_y).max(size_z); //?

    for part in &mut parts {
        part.model_matrix[12] -= center_x;
        part.model_matrix[13] -= center_y;
        part.model_matrix[14] -= center_z;
    }

    // AppTracer::debug(&format!(
    //     "Model bounds: ({:.2}, {:.2}, {:.2}) to ({:.2}, {:.2}, {:.2}), center: ({:.2}, {:.2}, {:.2})",
    //     min_x, min_y, min_z, max_x, max_y, max_z, center_x, center_y, center_z
    // ));

    let eye = compute_eye_position(camera);
    let view_matrix = create_look_at_matrix(eye, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);

    let aspect = canvas_width as f32 / canvas_height as f32;
    let fov_y = std::f32::consts::PI / 3.0;
    let near = 0.1;
    let far = max_size * 100.0;
    let projection_matrix = create_perspective_matrix(fov_y, aspect, near, far);
    
    let frame = match surface.get_current_texture() {
        Ok(frame) => frame,
        Err(e) => {
           panic!("Failed to acquire swap chain texture: {e}");
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });

    let mut parts_drawn = 0;
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.165,
                        g: 0.165,
                        b: 0.165,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: Some(0),
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(render_pipeline);

        for part in parts.iter().filter(|p| p.visible) {
            if part.indices.is_empty() {
                AppTracer::warn("Skipping render of part with empty indices");
                continue;
            }

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

            let mvp_matrix = multiply_matrices(
                &projection_matrix,
                &multiply_matrices(&view_matrix, &part.model_matrix),
            );

            let mvp_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("MVP Uniform Buffer"),
                contents: bytemuck::bytes_of(&mvp_matrix),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let model_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Model Uniform Buffer"),
                contents: bytemuck::bytes_of(&part.model_matrix),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let color_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Color Uniform Buffer"),
                contents: bytemuck::bytes_of(&part.color),
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

            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..part.indices.len() as u32, 0, 0..1);
            parts_drawn += 1;
        }
    }

    queue.submit(Some(encoder.finish()));
    //AppTracer::debug(&format!("Rendering complete, {} parts drawn", parts_drawn));
    frame.present();
    Ok(())
}
