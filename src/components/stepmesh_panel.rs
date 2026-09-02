//! Loaded-model panel: wraps the part list, show/hide-all controls, and the
//! deselect action.
use crate::common::types::StepModel;
use crate::{
    components::meshes_panel::{MeshData, MeshesPanel},
    trace_span,
};
use std::rc::Rc;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct StepMeshPanelProps {
    pub on_deselect: Callback<()>,
    pub model: Option<Rc<StepModel>>,
    pub part_visibility: Vec<bool>,
    pub on_visibility_change: Callback<(usize, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
}

#[function_component(StepMeshPanel)]
pub fn step_mesh_panel(props: &StepMeshPanelProps) -> Html {
    trace_span!("step_mesh_panel");

    let meshes = use_memo(
        (props.model.clone(), props.part_visibility.clone()),
        |(model, part_visibility)| {
            model.as_ref().map_or_else(Vec::new, |m| {
                m.render_parts
                    .iter()
                    .enumerate()
                    .filter(|(_, part)| !part.vertices.is_empty() && !part.indices.is_empty())
                    .map(|(i, part)| MeshData {
                        index: i,
                        name: format!("Mesh {}", i + 1),
                        triangle_count: part.triangle_count(),
                        vertex_count: part.vertex_count(),
                        visible: part_visibility.get(i).copied().unwrap_or(true),
                    })
                    .collect()
            })
        },
    );

    html! {
        <div class="panel panel-meshes">
            <div class="panel-header">
                <button
                    class="back-button"
                    onclick={props.on_deselect.reform(|_| ())}
                    title="Back to file history"
                >
                    <span class="fas fa-arrow-left"></span>
                    <span>{ "Back" }</span>
                </button>
                <div class="panel-header-title">
                    <span class="icon fas fa-cubes"></span>
                    <span>{ "Meshes" }</span>
                </div>
            </div>
            <div class="panel-content">
                <MeshesPanel
                    meshes={(*meshes).clone()}
                    on_visibility_change={props.on_visibility_change.clone()}
                    on_show_all={props.on_show_all.clone()}
                    on_hide_all={props.on_hide_all.clone()}
                />
            </div>
        </div>
    }
}
