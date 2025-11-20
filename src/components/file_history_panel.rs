use crate::common::FileIndexItem;
use crate::trace_span;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct FileHistoryPanelProps {
    pub files_index: Vec<FileIndexItem>,
    pub on_item_click: Callback<String>,
    pub on_delete: Callback<String>,
    pub on_clear_history: Callback<()>,
}

#[function_component(FileHistoryPanel)]
pub fn file_history_panel(props: &FileHistoryPanelProps) -> Html {
    trace_span!("file_history_panel");
    let on_item_click = props.on_item_click.clone();
    let on_delete = props.on_delete.clone();

    html! {
        <div class="panel">
            <div class="panel-header">
                <span>{ "File History " }</span>
                <span class="icon fas fa-file"></span>
                <button
                    class="clear-history-button"
                    onclick={Callback::from({
                        let on_clear = props.on_clear_history.clone();
                        move |_| on_clear.emit(())
                    })}
                    disabled={props.files_index.is_empty()}
                >
                    { "Clear" }
                </button>
            </div>
            <div class="panel-content">
                if props.files_index.is_empty() {
                    <div class="empty-files-message">{ "No files yet. Upload a .stp/.step to begin." }</div>
                } else {
                    <ul class="files-list">
                        { for props.files_index.iter().map(|item| {
                            let id = item.id.clone();
                            let click = {
                                let on_item_click = on_item_click.clone();
                                Callback::from(move |_| on_item_click.emit(id.clone()))
                            };
                            let delete_id = item.id.clone();
                            let ondelete = {
                                let on_delete = on_delete.clone();
                                Callback::from(move |e: MouseEvent| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    on_delete.emit(delete_id.clone());
                                })
                            };
                            html!{
                                <li>
                                    <div class="file-item-container">
                                        <button onclick={click} class="file-item-button">
                                            <div class="file-item-name">{ &item.name }</div>
                                            <div class="file-item-details">{ format!("{} entities • {}", item.entity_count, item.time_stamp) }</div>
                                        </button>
                                        <button title="Remove" onclick={ondelete} class="delete-button">
                                            <i class="fa-solid fa-trash delete-icon"></i>
                                        </button>
                                    </div>
                                </li>
                            }
                        }) }
                    </ul>
                }
            </div>
        </div>
    }
}
