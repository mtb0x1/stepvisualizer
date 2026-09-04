//! History management, localStorage/IndexedDB persistence helpers, and confirmation workflows.
use crate::common::{
    FileId, FileIndexItem, LruCache, clear_all_storage, delete_model, load_model, save_index,
};
use crate::workspace::ConfirmAction;
use crate::workspace::state::StateHandles;
use std::cell::RefCell;
use std::rc::Rc;
use yew::prelude::*;

/// Mutates the history file index in state and persists it to localStorage.
pub(crate) fn update_and_persist_index(
    files_index: &UseStateHandle<Vec<FileIndexItem>>,
    update: impl FnOnce(&mut Vec<FileIndexItem>),
) {
    let mut list = (**files_index).clone();
    update(&mut list);
    files_index.set(list.clone());
    save_index(&list);
}

/// Moves an existing file item to the top of the history index.
pub(crate) fn promote_in_index(files_index: &UseStateHandle<Vec<FileIndexItem>>, id: &FileId) {
    update_and_persist_index(files_index, |list| {
        if let Some(pos) = list.iter().position(|i| &i.id == id) {
            let item = list.remove(pos);
            list.insert(0, item);
        }
    });
}

/// Prepends or updates a file in the history index.
pub(crate) fn add_to_index(files_index: &UseStateHandle<Vec<FileIndexItem>>, item: FileIndexItem) {
    update_and_persist_index(files_index, |list| {
        list.retain(|i| i.id != item.id);
        list.insert(0, item);
    });
}

/// Removes a file from the history index.
pub(crate) fn remove_from_index(files_index: &UseStateHandle<Vec<FileIndexItem>>, id: &FileId) {
    update_and_persist_index(files_index, |list| {
        list.retain(|i| &i.id != id);
    });
}

/// History-management callbacks consumed by [`StepWorkspace`].
pub(crate) struct WorkspaceManagementActions {
    pub on_item_click: Callback<FileId>,
    pub on_delete: Callback<FileId>,
    pub on_deselect: Callback<()>,
    pub on_clear_history: Callback<()>,
    pub on_confirm: Callback<()>,
    pub on_cancel_confirm: Callback<()>,
}

#[hook]
pub(crate) fn use_workspace_management(
    states: &StateHandles,
    files_index: UseStateHandle<Vec<FileIndexItem>>,
    cache: Rc<RefCell<LruCache>>,
) -> WorkspaceManagementActions {
    let on_item_click = {
        let files_index = files_index.clone();
        let states = states.clone();
        let cache = cache.clone();
        Callback::from(move |id: FileId| {
            let next_gen = states.bump_generation();

            let maybe_model = cache.borrow_mut().get_or_load(&id, load_model);

            match maybe_model {
                Some(model_rc) => {
                    states.set_loaded_model(model_rc, id.clone(), "Loaded from cache");
                    promote_in_index(&files_index, &id);
                }
                None => {
                    states.is_processing.set(true);
                    let states = states.clone();
                    let cache = cache.clone();
                    let files_index = files_index.clone();
                    let file_id = id.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Some(model) =
                            crate::common::storage::load_model_indexeddb(&file_id).await
                        {
                            if states.is_superseded(next_gen) {
                                return;
                            }
                            {
                                let mut c = cache.borrow_mut();
                                c.insert(file_id.clone(), model.clone());
                            }
                            let model_rc = Rc::new(model);
                            states.set_loaded_model(
                                model_rc,
                                file_id.clone(),
                                "Loaded from storage",
                            );
                            promote_in_index(&files_index, &file_id);
                        } else if states.is_current(next_gen) {
                            states.is_processing.set(false);
                            remove_from_index(&files_index, &file_id);
                            states.set_result(
                                "Cached data missing. Removed file from history.",
                                true,
                            );
                        }
                    });
                }
            }
        })
    };

    let on_delete = {
        let states = states.clone();
        Callback::from(move |delete_id: FileId| {
            states
                .pending_confirm
                .set(Some(ConfirmAction::DeleteFile(delete_id)));
        })
    };

    let on_deselect = {
        let states = states.clone();
        Callback::from(move |_| {
            states.bump_generation();
            states.clear_model_state();
        })
    };

    let on_clear_history = {
        let states = states.clone();
        Callback::from(move |_| {
            states
                .pending_confirm
                .set(Some(ConfirmAction::ClearHistory));
        })
    };

    let on_confirm = {
        let files_index = files_index.clone();
        let states = states.clone();
        let cache = cache.clone();
        Callback::from(move |_| match states.pending_confirm.as_ref() {
            Some(ConfirmAction::DeleteFile(delete_id)) => {
                let delete_id = delete_id.clone();
                states.pending_confirm.set(None);
                {
                    let mut c = cache.borrow_mut();
                    c.remove(&delete_id);
                }

                delete_model(&delete_id);
                remove_from_index(&files_index, &delete_id);
                if states.selected_file.as_ref() == Some(&delete_id) {
                    states.clear_model_state();
                }
                states.set_result("Removed file from list.", false);
            }
            Some(ConfirmAction::ClearHistory) => {
                states.pending_confirm.set(None);

                clear_all_storage(&files_index);

                {
                    let mut cache_mut = cache.borrow_mut();
                    cache_mut.clear();
                }

                files_index.set(Vec::new());
                states.clear_model_state();
                states.set_result("Cleared cached files.", false);
            }
            None => {}
        })
    };

    let on_cancel_confirm = {
        let states = states.clone();
        Callback::from(move |_| {
            states.pending_confirm.set(None);
            states.set_result("Action cancelled.", false);
        })
    };

    WorkspaceManagementActions {
        on_item_click,
        on_delete,
        on_deselect,
        on_clear_history,
        on_confirm,
        on_cancel_confirm,
    }
}
