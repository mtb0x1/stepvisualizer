use crate::{
    components::meshes_panel::{MeshesPanel, MeshData},
    trace_span,
};
use std::rc::Rc;
use yew::prelude::*;
use crate::common::types::StepModel;

#[derive(Properties, PartialEq)]
pub struct StepMeshPanelProps {
    pub on_deselect: Callback<()>,
    pub model: Option<Rc<StepModel>>,
    pub on_visibility_change: Callback<(usize, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
}

#[function_component(StepMeshPanel)]
pub fn step_mesh_panel(props: &StepMeshPanelProps) -> Html {
    trace_span!("step_mesh_panel");
    
    let meshes = use_memo(
        (props.model.clone(),),
        |(model,)| {
            model.as_ref().map_or_else(Vec::new, |m| {
                m.render_parts
                    .iter()
                    .enumerate()
                    .filter(|(_, part)| !part.vertices.is_empty() && !part.indices.is_empty())
                    .map(|(i, part)| MeshData {
                        id: i.to_string(),
                        name: format!("Mesh {}", i + 1),
                        triangle_count: part.indices.len() / 3,
                        vertex_count: part.vertices.len(),
                        visible: part.visible,
                    })
                    .collect()
            })
        },
    );

    let on_visibility_change = {
        let cb = props.on_visibility_change.clone();
        Callback::from(move |(id, visible): (String, bool)| {
            if let Ok(index) = id.parse::<usize>() {
                cb.emit((index, visible));
            }
        })
    };

    html! {
        <div class="panel panel-meshes">
            <div class="panel-content">
                <button 
                    class="back-button"
                    onclick={props.on_deselect.reform(|_| ())}
                >
                    <span class="fas fa-arrow-left"></span> { " Back"}
                </button>
                <MeshesPanel 
                    meshes={(*meshes).clone()}
                    on_visibility_change={on_visibility_change}
                    on_show_all={props.on_show_all.clone()}
                    on_hide_all={props.on_hide_all.clone()}
                />
            </div>
        </div>
    }
}