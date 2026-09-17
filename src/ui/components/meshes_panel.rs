//! Part list of the loaded model with per-part visibility toggles.
use crate::common::Color;
use smol_str::{SmolStr, format_smolstr};
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MeshItemProps {
    pub index: usize,
    #[prop_or_default]
    pub name: Option<SmolStr>,
    pub triangle_count: usize,
    pub vertex_count: usize,
    pub visible: bool,
    pub color: Color,
    pub on_toggle_visibility: Callback<(usize, bool)>,
}

#[function_component(MeshItem)]
fn mesh_item(props: &MeshItemProps) -> Html {
    let on_visibility_change = {
        let index = props.index;
        let on_toggle = props.on_toggle_visibility.clone();
        Callback::from(move |e: Event| {
            if let Some(input) = e.target_dyn_into::<HtmlInputElement>() {
                on_toggle.emit((index, input.checked()));
            }
        })
    };

    let display_name = props
        .name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(SmolStr::new)
        .unwrap_or_else(|| format_smolstr!("Mesh {}", props.index + 1));

    html! {
        <div class="mesh-item">
            <div class="mesh-header">
                <input
                    type="checkbox"
                    checked={props.visible}
                    onchange={on_visibility_change}
                    class="mesh-visibility"
                />
                <span
                    class="mesh-color-swatch"
                    style={format!("background-color: {};", props.color.to_css_rgba())}
                    title={props.color.to_hex()}
                />
                <span class="mesh-name">{ display_name.as_str() }</span>
            </div>
            <div class="mesh-details">
                <span class="mesh-stats">
                    { props.triangle_count }{ " triangles | " }{ props.vertex_count }{ " vertices" }
                </span>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct MeshesPanelProps {
    pub meshes: Vec<MeshData>,
    pub on_visibility_change: Callback<(usize, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
}

#[derive(Clone, Debug, PartialEq, Properties)]
pub struct MeshData {
    pub index: usize,
    pub name: Option<SmolStr>,
    pub triangle_count: usize,
    pub vertex_count: usize,
    pub visible: bool,
    pub color: Color,
}

// Scene centering on the visible subset is handled in the renderer
// (renderer.rs derives the orbit target from the visible bounding box).

#[function_component(MeshesPanel)]
pub fn meshes_panel(props: &MeshesPanelProps) -> Html {
    let meshes_list = props
        .meshes
        .iter()
        .map(|mesh| {
            html! {
                <MeshItem
                    key={mesh.index}
                    index={mesh.index}
                    name={mesh.name.clone()}
                    triangle_count={mesh.triangle_count}
                    vertex_count={mesh.vertex_count}
                    visible={mesh.visible}
                    color={mesh.color}
                    on_toggle_visibility={props.on_visibility_change.clone()}
                />
            }
        })
        .collect::<Html>();

    html! {
        <div class="meshes-container">
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
    }
}
