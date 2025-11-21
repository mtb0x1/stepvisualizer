use wasm_bindgen::prelude::*;
use yew::prelude::*;
mod apptracing;
mod common;
mod components;
mod header;
mod left_panel;
mod main_panel;
mod rendering;
mod right_panel;
mod workspace;
use apptracing::AppTracer;
use apptracing::AppTracerTrait;
use header::Header;
use main_panel::AppStepviz;
use right_panel::RightPanel as MetadataPanel;
use workspace::use_step_workspace;

#[function_component(App)]
fn app() -> Html {
    trace_span!("app");
    let workspace = use_step_workspace();
    let current_file_name = workspace
        .metadata
        .as_ref()
        .map(|m| m.header.file_name.clone());
    let render_error_callback = {
        let result = workspace.result.clone();
        Callback::from(move |msg: String| {
            result.set(Some(msg));
        })
    };

    html! {
        <div class="app-container">
            <Header file_name={current_file_name} />

            // Left Sidebar
            <aside class="sidebar sidebar-left">
            <crate::left_panel::LeftPanel
                files_index={(*workspace.files_index).clone()}
                selected_file={(*workspace.selected_file).clone()}
                model={(*workspace.step_model).clone()}
                on_item_click={workspace.actions.on_item_click.clone()}
                on_delete={workspace.actions.on_delete.clone()}
                on_deselect={workspace.actions.on_deselect.clone()}
                on_clear_history={workspace.actions.on_clear_history.clone()}
                on_visibility_change={workspace.actions.on_visibility_change.clone()}
                on_show_all={workspace.actions.on_show_all.clone()}
                on_hide_all={workspace.actions.on_hide_all.clone()}
            />
            </aside>

            // Main Viewport
            <main class="main-viewport">
                <div class="file-input-container">
                    <label for="file-input">{ "Select a STEP file: " }</label>
                    <input
                        type="file"
                        accept=".step,.stp"
                        id="file-input"
                        disabled={*workspace.is_processing}
                        onchange={workspace.actions.on_file_change.clone()}
                    />
                    {
                        if *workspace.is_processing {
                            html! { <span class="processing-hint">{ "Processing STEP..." }</span> }
                        } else {
                            Html::default()
                        }
                    }
                </div>

                //FIXME : invistigate the window resize issue
                // when window is resized, the canvas doesn't resize properly
                // i mean the scene resizes and it causes distortion in the rendering
                // this might be due to the canvas not being properly notified of size changes
                // consider adding a resize observer or force re-render on window resize
                <AppStepviz
                    step_model={(*workspace.step_model).clone()}
                    is_processing={*workspace.is_processing}
                    metadata={(*workspace.metadata).clone()}
                    on_render_error={render_error_callback}
                />
                <div class="result-message">
                    { workspace.result.as_ref().map(|msg| msg.as_str()).unwrap_or("") }
                </div>
            </main>

            // Right Sidebar
            <aside class="sidebar sidebar-right">
            <MetadataPanel
                metadata={(*workspace.metadata).clone()}
                on_calculate_volume={workspace.actions.on_calculate_volume.clone()}
                on_calculate_surface={workspace.actions.on_calculate_surface.clone()}
            />
            </aside>
        </div>
    }
}

#[wasm_bindgen(start)]
pub fn run_app() {
    AppTracer::init();
    trace_span!("run_app");
    yew::Renderer::<App>::new().render();
}
