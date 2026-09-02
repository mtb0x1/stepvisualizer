//! StepViz — a WebAssembly STEP file viewer.
//!
//! Parses STEP files in the browser (ruststep), tessellates the geometry
//! (truck), and renders it with WebGPU. The UI is Yew (CSR); all app state
//! is owned by the `workspace` hook and passed down to the panels as props.
//! Browsers without WebGPU never get past [`App`]'s gate: the whole UI is
//! replaced by the `WebGpuUnavailable` fallback page.
//!
//! Module map:
//! - `workspace`: the app-wide state hook wiring parsing → storage → UI
//! - `ui`: UI panels, viewport, dialogs, and reusable components
//! - `rendering`: wgpu device/pipeline setup, frame renderer, orbit camera
//! - `common`: domain types + pure logic (parsing, tessellation, caches, math)
//! - `apptracing`: URL-query-driven tracing setup
//! - `error`: the crate-wide error type
use wasm_bindgen::prelude::*;
use yew::prelude::*;
mod apptracing;
pub mod common;
mod error;
mod rendering;
mod ui;
mod workspace;
use apptracing::{AppTracer, AppTracerTrait};
use common::constants::NO_WEBGPU_MSG;
use rendering::wgpu_state::browser_has_webgpu;
use ui::{
    AppStepviz, ConfirmModal, LeftPanel, RightPanel as MetadataPanel, UploadBar, WebGpuUnavailable,
};
use workspace::{ConfirmAction, use_step_workspace};

/// Root component: a WebGPU gate in front of the real application.
///
/// The probe runs synchronously on first render. When the browser has no
/// WebGPU, the entire app is swapped for [`WebGpuUnavailable`] — no workspace,
/// canvas, observers, or upload handling ever mount, which is the graceful
/// shutdown path. The probe passing only means `navigator.gpu` exists; a
/// later adapter/device failure is routed to the same page via the
/// `on_gpu_unavailable` callback.
#[function_component(App)]
fn app() -> Html {
    trace_span!("app");
    // Single hook, called unconditionally before the early return, so hook
    // order is stable across renders.
    let gpu_unavailable =
        use_state(|| (!browser_has_webgpu()).then_some(AttrValue::Static(NO_WEBGPU_MSG)));
    if let Some(reason) = gpu_unavailable.as_ref() {
        return html! { <WebGpuUnavailable reason={reason.clone()} /> };
    }

    let on_gpu_unavailable = {
        let gpu_unavailable = gpu_unavailable.clone();
        Callback::from(move |reason: String| gpu_unavailable.set(Some(AttrValue::from(reason))))
    };

    html! { <MainApp {on_gpu_unavailable} /> }
}

#[derive(Properties, PartialEq)]
struct MainAppProps {
    /// Fatal GPU failure channel from the viewport (init_wgpu errors).
    on_gpu_unavailable: Callback<String>,
}

/// The application shell: workspace hook + sidebars, viewport.
/// Mounted only after the WebGPU gate passes.

#[function_component(MainApp)]
fn main_app(props: &MainAppProps) -> Html {
    let workspace = use_step_workspace();
    let render_error_callback = {
        let result = workspace.result.clone();
        let result_is_error = workspace.result_is_error.clone();
        Callback::from(move |msg: String| {
            result_is_error.set(true);
            result.set(Some(msg));
        })
    };

    let on_quality_change = {
        let quality_preset = workspace.quality_preset.clone();
        Callback::from(move |preset| quality_preset.set(preset))
    };

    let confirm_modal_view = match workspace.pending_confirm.as_ref() {
        Some(ConfirmAction::DeleteFile(_)) => {
            html! {
                <ConfirmModal
                    title="Delete File"
                    message="Remove this file from history? This action cannot be undone."
                    confirm_label="Delete"
                    on_confirm={workspace.actions.on_confirm.clone()}
                    on_cancel={workspace.actions.on_cancel_confirm.clone()}
                />
            }
        }
        Some(ConfirmAction::ClearHistory) => {
            html! {
                <ConfirmModal
                    title="Clear All History"
                    message="Clear all cached files? This removes all local copies and history."
                    confirm_label="Clear All"
                    on_confirm={workspace.actions.on_confirm.clone()}
                    on_cancel={workspace.actions.on_cancel_confirm.clone()}
                />
            }
        }
        None => html! {},
    };

    html! {
        <div class="app-container">
            // Left Sidebar: file history and model parts
            <aside class="sidebar sidebar-left">
                <LeftPanel
                    files_index={(*workspace.files_index).clone()}
                    selected_file={(*workspace.selected_file).clone()}
                    model={(*workspace.step_model).clone()}
                    part_visibility={(*workspace.part_visibility).clone()}
                    on_item_click={workspace.actions.on_item_click.clone()}
                    on_delete={workspace.actions.on_delete.clone()}
                    on_deselect={workspace.actions.on_deselect.clone()}
                    on_clear_history={workspace.actions.on_clear_history.clone()}
                    on_visibility_change={workspace.actions.on_visibility_change.clone()}
                    on_show_all={workspace.actions.on_show_all.clone()}
                    on_hide_all={workspace.actions.on_hide_all.clone()}
                />
            </aside>

            // Main Viewport: file upload and 3D WebGPU canvas
            <main class="main-viewport">
                <UploadBar
                    is_processing={*workspace.is_processing}
                    on_file_change={workspace.actions.on_file_change.clone()}
                    quality_preset={*workspace.quality_preset}
                    on_quality_change={on_quality_change}
                />

                <AppStepviz
                    step_model={(*workspace.step_model).clone()}
                    is_processing={*workspace.is_processing}
                    metadata={(*workspace.metadata).clone()}
                    part_visibility={(*workspace.part_visibility).clone()}
                    on_render_error={render_error_callback}
                    on_gpu_unavailable={props.on_gpu_unavailable.clone()}
                />
                <div class={if *workspace.result_is_error { "result-message result-error" } else { "result-message result-success" }}>
                    { workspace.result.as_ref().map(|msg| msg.as_str()).unwrap_or("") }
                </div>
            </main>

            // Right Sidebar: model metadata and calculated metrics
            <aside class="sidebar sidebar-right">
                <MetadataPanel
                    metadata={(*workspace.metadata).clone()}
                    on_calculate_volume={workspace.actions.on_calculate_volume.clone()}
                    on_calculate_surface={workspace.actions.on_calculate_surface.clone()}
                />
            </aside>

            { confirm_modal_view }
        </div>
    }
}

#[wasm_bindgen(start)]
pub fn run_app() {
    AppTracer::init();
    trace_span!("run_app");
    yew::Renderer::<App>::new().render();
}
