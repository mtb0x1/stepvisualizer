use crate::trace_span;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MeshItemProps {
    pub id: String,
    pub name: String,
    pub triangle_count: usize,
    pub vertex_count: usize,
    pub visible: bool,
    pub on_toggle_visibility: Callback<(String, bool)>,
}

#[function_component(MeshItem)]
fn mesh_item(props: &MeshItemProps) -> Html {
    let on_visibility_change = {
        let id = props.id.clone();
        let on_toggle = props.on_toggle_visibility.clone();
        Callback::from(move |e: Event| {
            if let Some(input) = e.target_dyn_into::<HtmlInputElement>() {
                on_toggle.emit((id.clone(), input.checked()));
            }
        })
    };

    html! {
        <div class="mesh-item">
            <div class="mesh-header">
                <input
                    type="checkbox"
                    checked={props.visible}
                    onchange={on_visibility_change}
                    class="mesh-visibility"
                />
                <span class="mesh-name">{&props.name}</span>
            </div>
            <div class="mesh-details">
                <span class="mesh-stats">
                    {format!("{} triangles", props.triangle_count)}
                    {" | "}
                    {format!("{} vertices", props.vertex_count)}
                </span>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct MeshesPanelProps {
    pub meshes: Vec<MeshData>,
    pub on_visibility_change: Callback<(String, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
}

#[derive(Clone, Debug, PartialEq, Properties)]
pub struct MeshData {
    pub id: String,
    pub name: String,
    pub triangle_count: usize,
    pub vertex_count: usize,
    pub visible: bool,
}

// FIXME : Scene need to be centered on visible meshes
// example if only one mesh is visible, center on that mesh
// if multiple meshes are visible, center on the bounding box of all visible meshes

// TODO: Implement actual centering logic based on visible meshes
// Consider performance - only recompute when visible meshes change

#[function_component(MeshesPanel)]
pub fn meshes_panel(props: &MeshesPanelProps) -> Html {
    trace_span!("meshes_panel");

    let meshes_list = props
        .meshes
        .iter()
        .map(|mesh| {
            html! {
                <MeshItem
                    id={mesh.id.clone()}
                    name={mesh.name.clone()}
                    triangle_count={mesh.triangle_count}
                    vertex_count={mesh.vertex_count}
                    visible={mesh.visible}
                    on_toggle_visibility={props.on_visibility_change.clone()}
                />
            }
        })
        .collect::<Html>();

    html! {
        <div class="panel panel-meshes">
            <div class="panel-header">
                <span class="panel-header-left">
                    <span class="toggle toggle-closed"></span>
                    <span>{"Meshes "}</span>
                    <span class="icon fas fa-cubes"></span>
                </span>
            </div>
            <div class="panel-content">
                <div class="mesh-controls">
                    <button
                        class="btn btn-small"
                        onclick={props.on_show_all.reform(|_| ())}
                    >
                        <span class="fas fa-eye"></span> {" Show All"}
                    </button>
                    <button
                        class="btn btn-small"
                        onclick={props.on_hide_all.reform(|_| ())}
                    >
                        <span class="fas fa-eye-slash"></span> {" Hide All"}
                    </button>
                </div>
                <div class="meshes-list">
                    {meshes_list}
                </div>
            </div>
        </div>
    }
}
