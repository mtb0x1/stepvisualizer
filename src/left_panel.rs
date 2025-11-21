use crate::common::types::StepModel;
use crate::{
    common::FileIndexItem,
    components::{file_history_panel::FileHistoryPanel, stepmesh_panel::StepMeshPanel},
    trace_span,
};
use std::rc::Rc;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct LeftPanelProps {
    pub files_index: Vec<FileIndexItem>,
    pub selected_file: Option<String>,
    pub model: Option<Rc<StepModel>>,
    pub on_item_click: Callback<String>,
    pub on_delete: Callback<String>,
    pub on_deselect: Callback<()>,
    pub on_clear_history: Callback<()>,
    pub on_visibility_change: Callback<(usize, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
}

#[function_component(LeftPanel)]
pub fn left_panel(props: &LeftPanelProps) -> Html {
    trace_span!("left_panel");
    html! {
        <div class="left-panel">
            if props.selected_file.is_none() {
                <FileHistoryPanel
                    files_index={props.files_index.clone()}
                    on_item_click={props.on_item_click.clone()}
                    on_delete={props.on_delete.clone()}
                    on_clear_history={props.on_clear_history.clone()}
                />
            } else {
                <StepMeshPanel
                    on_deselect={props.on_deselect.clone()}
                    model={props.model.clone()}
                    on_visibility_change={props.on_visibility_change.clone()}
                    on_show_all={props.on_show_all.clone()}
                    on_hide_all={props.on_hide_all.clone()}
                />
            }
        </div>
    }
}
