use crate::{
    common::{Metadata, StepModel},
    rendering::{
        camera::CameraState,
        renderer::render_wgpu_on_canvas,
        wgpu_state::{WgpuState, init_wgpu},
    },
    trace_span,
};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlCanvasElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MainPanelProps {
    pub step_model: Option<Rc<StepModel>>,
    #[prop_or(false)]
    pub is_processing: bool,
    pub metadata: Option<Metadata>,
    pub on_render_error: Callback<String>,
}

use std::rc::Rc;

#[function_component(AppStepviz)]
pub fn stepviz_viewer(props: &MainPanelProps) -> Html {
    trace_span!("stepviz_viewer");
    let canvas_ref = use_node_ref();
    let wgpu_state = use_state(|| None::<Rc<WgpuState>>);
    let camera_state = use_state(CameraState::default);
    let is_dragging = use_state(|| false);
    let last_mouse_pos = use_state(|| (0, 0));
    let render_parts = use_state(Vec::new);

    {
        let canvas_ref = canvas_ref.clone();
        let wgpu_state = wgpu_state.clone();
        let render_error_cb = props.on_render_error.clone();

        use_effect_with((), move |_| {
            if let Some(canvas) = canvas_ref.cast::<HtmlCanvasElement>() {
                spawn_local(async move {
                    match init_wgpu(canvas).await {
                        Ok(state) => {
                            wgpu_state.set(Some(Rc::new(state)));
                        }
                        Err(e) => {
                            render_error_cb.emit(format!("WebGPU init failed: {e}"));
                        }
                    }
                });
            }
            || ()
        });
    }

    {
        let render_parts = render_parts.clone();
        use_effect_with(props.step_model.clone(), move |maybe_model| {
            if let Some(model) = maybe_model {
                render_parts.set((**model).render_parts.clone());
            } else {
                render_parts.set(Vec::new());
            }
            || ()
        });
    }

    {
        let wgpu_state_handle = wgpu_state.clone();
        let camera_state = camera_state.clone();
        let render_parts = render_parts.clone();
        let render_error_cb = props.on_render_error.clone();

        use_effect_with(
            (wgpu_state_handle, camera_state, render_parts),
            move |(wgpu_handle, camera, parts)| {
                if let Some(wgpu_state) = &**wgpu_handle {
                    if !parts.is_empty() {
                        let parts_vec = (**parts).clone();
                        let camera_value = (**camera).clone();
                        let state = wgpu_state.clone();
                        let error_cb = render_error_cb.clone();
                        spawn_local(async move {
                            if let Err(e) =
                                render_wgpu_on_canvas(state, parts_vec, &camera_value).await
                            {
                                error_cb.emit(format!("Render error: {e}"));
                            }
                        });
                    }
                }
                || ()
            },
        );
    }

    let canvas_overlay = {
        if props.is_processing {
            html! { <div class="canvas-processing-overlay">{ "Preparing 3D view..." }</div> }
        } else if props.step_model.is_none() {
            html! { <div class="empty-canvas-message">{ "Upload a STEP file to visualize it." }</div> }
        } else if render_parts.is_empty() {
            html! { <div class="empty-canvas-message">{ "Parsing geometry..." }</div> }
        } else {
            Html::default()
        }
    };

    let preset_button = |label: &str, azimuth: f32, elevation: f32, distance: f32| {
        let camera_state = camera_state.clone();
        html! {
            <button
                class="camera-button"
                onclick={Callback::from(move |_| {
                    let mut new_camera = (*camera_state).clone();
                    new_camera.azimuth = azimuth;
                    new_camera.elevation = elevation;
                    new_camera.distance = distance;
                    camera_state.set(new_camera);
                })}
            >{ label }</button>
        }
    };

    let camera_toolbar = html! {
        <div class="camera-toolbar">
            { preset_button("Reset", CameraState::default().azimuth, CameraState::default().elevation, CameraState::default().distance) }
            { preset_button("Iso", 0.8, 0.9, 3.0) }
            { preset_button("Top", 0.0, 1.3, 2.5) }
            { preset_button("Front", 0.0, 0.0, 3.0) }
        </div>
    };

    let stats_overlay = if let Some(meta) = props.metadata.as_ref() {
        html! {
            <div class="canvas-stats">
                <div>{ format!("{} triangles", meta.triangle_count) }</div>
                <div>{ format!("{} vertices", meta.vertex_count) }</div>
                { meta.units.as_ref().map(|u| html!{ <div>{ format!("Units: {}", u) }</div> }).unwrap_or(Html::default()) }
            </div>
        }
    } else {
        Html::default()
    };
    let on_mouse_down = {
        let is_dragging = is_dragging.clone();
        let last_mouse_pos = last_mouse_pos.clone();
        Callback::from(move |e: MouseEvent| {
            is_dragging.set(true);
            last_mouse_pos.set((e.client_x(), e.client_y()));
        })
    };

    let on_mouse_up = {
        let is_dragging = is_dragging.clone();
        Callback::from(move |_| {
            is_dragging.set(false);
        })
    };

    let on_mouse_move = {
        let is_dragging = is_dragging.clone();
        let last_mouse_pos = last_mouse_pos.clone();
        let camera_state = camera_state.clone();
        Callback::from(move |e: MouseEvent| {
            if *is_dragging {
                let (last_x, last_y) = *last_mouse_pos;
                let dx = e.client_x() - last_x;
                let dy = e.client_y() - last_y;
                last_mouse_pos.set((e.client_x(), e.client_y()));

                let mut new_camera_state = (*camera_state).clone();
                new_camera_state.azimuth -= dx as f32 * 0.01;
                new_camera_state.elevation =
                    (new_camera_state.elevation - dy as f32 * 0.01).clamp(-1.57, 1.57);
                camera_state.set(new_camera_state);
            }
        })
    };

    html! {
        <div class="canvas-wrapper">
            <canvas
                id="step3D"
                ref={canvas_ref}
                class="main-panel-canvas"
                onmousedown={on_mouse_down}
                onmouseup={on_mouse_up}
                onmousemove={on_mouse_move}
                onwheel={Callback::from(move |e: WheelEvent| {
                    let mut new_camera_state = (*camera_state).clone();
                    new_camera_state.distance *= if e.delta_y() > 0.0 { 1.1 } else { 0.9 };
                    camera_state.set(new_camera_state);
                })}
            />
            <div class="canvas-ui">
                { stats_overlay }
                { camera_toolbar }
            </div>
            { canvas_overlay }
        </div>
    }
}
