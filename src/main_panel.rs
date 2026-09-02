//! The WebGPU viewport: canvas setup, orbit/drag handling, camera presets,
//! and the effect that renders a frame whenever inputs change.
use crate::apptracing::{AppTracer, AppTracerTrait};
use crate::{
    common::fps_meter::FpsMeter,
    common::{FileId, Metadata, StepModel, ViewportSize, constants::WEBGPU_INIT_FAILED_MSG},
    components::fps_graph::FpsGraph,
    rendering::{
        camera::{CAMERA_PRESETS, CameraPreset, CameraState},
        renderer::render_wgpu_on_canvas,
        wgpu_state::{WgpuState, init_wgpu},
    },
    trace_span,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlCanvasElement, ResizeObserver};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MainPanelProps {
    pub step_model: Option<Rc<StepModel>>,
    #[prop_or(false)]
    pub is_processing: bool,
    pub metadata: Option<Metadata>,
    pub part_visibility: Vec<bool>,
    /// Transient per-frame errors, surfaced in the app's result message.
    pub on_render_error: Callback<String>,
    /// Fatal GPU init errors: the app cannot render at all, so the whole
    /// shell is replaced by the WebGPU-unavailable page.
    pub on_gpu_unavailable: Callback<String>,
}

/// Screen coordinate of an ongoing pointer drag interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DragState {
    last_x: i32,
    last_y: i32,
}

use std::rc::Rc;

#[function_component(AppStepviz)]
pub fn stepviz_viewer(props: &MainPanelProps) -> Html {
    trace_span!("stepviz_viewer");
    let canvas_ref = use_node_ref();
    let wgpu_state = use_state(|| None::<Rc<WgpuState>>);
    let camera_state = use_state(CameraState::default);
    let drag_state = use_state(|| None::<DragState>);
    let canvas_size = use_state(|| ViewportSize::ZERO);
    let last_model_id = use_state(|| None::<FileId>);
    let fps_meter = use_state(|| Rc::new(FpsMeter::new()));
    let is_rendering = use_mut_ref(|| false);
    let pending_render = use_mut_ref(|| false);

    {
        let canvas_ref = canvas_ref.clone();
        let wgpu_state = wgpu_state.clone();
        let gpu_unavailable_cb = props.on_gpu_unavailable.clone();

        use_effect_with((), move |_| {
            if let Some(canvas) = canvas_ref.cast::<HtmlCanvasElement>() {
                spawn_local(async move {
                    match init_wgpu(canvas).await {
                        Ok(state) => {
                            wgpu_state.set(Some(Rc::new(state)));
                        }
                        Err(e) => {
                            // Init failure is fatal for the whole app (every
                            // feature dead-ends at the renderer), so it goes
                            // to the dedicated channel rather than the
                            // transient per-frame error message.
                            gpu_unavailable_cb.emit(format!("{}: {e}", WEBGPU_INIT_FAILED_MSG));
                        }
                    }
                });
            }
            || ()
        });
    }

    {
        let canvas_ref = canvas_ref.clone();
        let wgpu_state = wgpu_state.clone();
        let canvas_size = canvas_size.clone();
        use_effect_with(canvas_ref.clone(), move |canvas_ref| {
            let Some(canvas) = canvas_ref.cast::<HtmlCanvasElement>() else {
                return Box::new(|| {}) as Box<dyn Fn()>;
            };
            let canvas_for_closure = canvas.clone();
            let on_resize = Closure::wrap(Box::new(move |_entries: js_sys::Array| {
                let size = ViewportSize::from_canvas(&canvas_for_closure);
                canvas_for_closure.set_width(size.width);
                canvas_for_closure.set_height(size.height);
                if let Some(state) = &*wgpu_state {
                    state.resize(size);
                }
                canvas_size.set(size);
            }) as Box<dyn Fn(js_sys::Array)>);
            let observer_res = ResizeObserver::new(on_resize.as_ref().unchecked_ref());
            if let Ok(observer) = observer_res {
                observer.observe(&canvas);
                // Keep the closure alive until the observer is disconnected.
                Box::new(move || {
                    observer.disconnect();
                    let _ = &on_resize;
                }) as Box<dyn Fn()>
            } else {
                AppTracer::error("Failed to create ResizeObserver");
                Box::new(move || {
                    let _ = &on_resize;
                }) as Box<dyn Fn()>
            }
        });
    }

    {
        let wgpu_state_handle = wgpu_state.clone();
        let camera_state = camera_state.clone();
        let render_error_cb = props.on_render_error.clone();
        let step_model = props.step_model.clone();
        let part_visibility = props.part_visibility.clone();
        let last_model_id = last_model_id.clone();
        let fps_meter = (*fps_meter).clone();

        use_effect_with(
            (
                wgpu_state_handle,
                camera_state,
                step_model,
                part_visibility,
                canvas_size,
            ),
            move |(wgpu_handle, camera, model, vis, _size)| {
                // Discard cached per-part GPU buffers whenever the loaded model
                // changes: index-keyed buffers would otherwise be reused for a
                // different model that happens to have identical part counts.
                let model_id = model.as_ref().map(|m| m.id.clone());
                if model_id != *last_model_id {
                    if let Some(wgpu_state) = &**wgpu_handle {
                        wgpu_state.part_buffers.borrow_mut().clear();
                        *wgpu_state.cached_bounds.borrow_mut() = None;
                    }
                    last_model_id.set(model_id);
                }

                if let (Some(wgpu_state), Some(model)) = (&**wgpu_handle, model.as_ref())
                    && !model.render_parts.is_empty()
                {
                    if *is_rendering.borrow() {
                        *pending_render.borrow_mut() = true;
                    } else {
                        *is_rendering.borrow_mut() = true;

                        let is_rendering = is_rendering.clone();
                        let pending_render = pending_render.clone();
                        let model = model.clone();
                        let vis_vec = vis.clone();
                        let camera_value = **camera;
                        let state = wgpu_state.clone();
                        let error_cb = render_error_cb.clone();
                        let meter = fps_meter.clone();

                        spawn_local(async move {
                            let res = render_wgpu_on_canvas(
                                state.clone(),
                                &model.render_parts,
                                &vis_vec,
                                &camera_value,
                                meter.clone(),
                            )
                            .await;
                            *is_rendering.borrow_mut() = false;
                            if let Err(e) = res {
                                error_cb.emit(format!("Render error: {e}"));
                            }
                            if *pending_render.borrow() {
                                *pending_render.borrow_mut() = false;
                                let _ = render_wgpu_on_canvas(
                                    state,
                                    &model.render_parts,
                                    &vis_vec,
                                    &camera_value,
                                    meter,
                                )
                                .await;
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
        } else if props
            .step_model
            .as_ref()
            .is_some_and(|m| m.render_parts.is_empty())
        {
            html! { <div class="empty-canvas-message">{ "Parsing geometry..." }</div> }
        } else {
            Html::default()
        }
    };

    let preset_button = |preset: &CameraPreset| {
        let camera_state = camera_state.clone();
        let preset = *preset;
        html! {
            <button
                class="camera-button"
                onclick={Callback::from(move |_| {
                    let new_camera = preset.apply(&camera_state);
                    camera_state.set(new_camera);
                })}
            >{ preset.label }</button>
        }
    };

    let camera_toolbar = html! {
        <div class="camera-toolbar">
            { for CAMERA_PRESETS.iter().map(preset_button) }
        </div>
    };

    let stats_overlay = if let Some(meta) = props.metadata.as_ref() {
        html! {
            <div class="canvas-stats">
                <div>{ meta.triangle_count }{ " triangles" }</div>
                <div>{ meta.vertex_count }{ " vertices" }</div>
                { meta.units.as_ref().map(|u| html!{ <div>{ "Units: " }{ u }</div> }).unwrap_or_default() }
            </div>
        }
    } else {
        Html::default()
    };
    let on_mouse_down = {
        let drag_state = drag_state.clone();
        Callback::from(move |e: MouseEvent| {
            drag_state.set(Some(DragState {
                last_x: e.client_x(),
                last_y: e.client_y(),
            }));
        })
    };

    let on_mouse_up = {
        let drag_state = drag_state.clone();
        Callback::from(move |_| {
            drag_state.set(None);
        })
    };

    let on_mouse_leave = {
        let drag_state = drag_state.clone();
        Callback::from(move |_| {
            drag_state.set(None);
        })
    };

    let on_mouse_move = {
        let drag_state = drag_state.clone();
        let camera_state = camera_state.clone();
        Callback::from(move |e: MouseEvent| {
            if let Some(drag) = *drag_state {
                let dx = (e.client_x() - drag.last_x) as f64;
                let dy = (e.client_y() - drag.last_y) as f64;
                drag_state.set(Some(DragState {
                    last_x: e.client_x(),
                    last_y: e.client_y(),
                }));
                camera_state.set(camera_state.orbit(dx, dy));
            }
        })
    };

    let on_wheel = {
        let camera_state = camera_state.clone();
        Callback::from(move |e: WheelEvent| {
            let factor = if e.delta_y() > 0.0 { 1.1 } else { 0.9 };
            camera_state.set(camera_state.zoom(factor));
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
                onmouseleave={on_mouse_leave}
                onmousemove={on_mouse_move}
                onwheel={on_wheel}
            />
            <div class="canvas-ui">
                { stats_overlay }
                { camera_toolbar }
            </div>
            { canvas_overlay }
            <FpsGraph meter={(*fps_meter).clone()} />
        </div>
    }
}
