//! App-wide state: one hook ([`use_step_workspace`]) owning file parsing,
//! recent-file history, and model interaction. Panels receive slices of
//! [`StepWorkspace`] as props; nothing else holds app state.

mod actions;
mod history;
mod processor;
mod state;

use crate::common::constants::{CACHE_MAX_MEMORY_BYTES, QualityPreset};
use crate::common::{FileId, FileIndexItem, Metadata, StepModel};
use crate::storage::{LruCache, load_index_async};
use actions::use_model_actions;
use history::use_workspace_management;
use processor::use_file_processor;
use smol_str::SmolStr;
use state::StateHandles;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::Event;
use yew::prelude::*;

#[derive(Clone, PartialEq, Debug)]
pub enum ConfirmAction {
    DeleteFile(FileId),
    ClearHistory,
}

/// All UI callbacks the panels consume, aggregated so a single struct can be
/// passed down from [`StepWorkspace`]. Split internally into the file
/// processor, history management, and per-model action groups.
pub struct WorkspaceActions {
    pub on_file_change: Callback<Event>,
    pub on_item_click: Callback<FileId>,
    pub on_delete: Callback<FileId>,
    pub on_deselect: Callback<()>,
    pub on_clear_history: Callback<()>,
    pub on_confirm: Callback<()>,
    pub on_cancel_confirm: Callback<()>,
    pub on_visibility_change: Callback<(usize, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
    pub on_calculate_volume: Callback<()>,
    pub on_calculate_surface: Callback<()>,
}

/// State + actions returned by [`use_step_workspace`]: the single source of
/// truth for loaded-file state. The handles are clones of the hook's
/// internal `use_state` handles, so `set` calls from anywhere re-render the
/// components that read them.
pub struct StepWorkspace {
    pub result: UseStateHandle<Option<SmolStr>>,
    /// `true` when the current `result` message is an error (drives CSS class).
    pub result_is_error: UseStateHandle<bool>,
    pub metadata: UseStateHandle<Option<Metadata>>,
    pub files_index: UseStateHandle<Vec<FileIndexItem>>,
    pub selected_file: UseStateHandle<Option<FileId>>,
    pub step_model: UseStateHandle<Option<Rc<StepModel>>>,
    pub part_visibility: UseStateHandle<Vec<bool>>,
    pub is_processing: UseStateHandle<bool>,
    pub pending_confirm: UseStateHandle<Option<ConfirmAction>>,
    pub quality_preset: UseStateHandle<QualityPreset>,
    pub actions: WorkspaceActions,
}

#[hook]
fn use_workspace_storage() -> (UseStateHandle<Vec<FileIndexItem>>, Rc<RefCell<LruCache>>) {
    // Start empty; load asynchronously from IndexedDB on mount.
    let files_index = use_state(Vec::new);
    let cache = use_mut_ref(|| LruCache::new(CACHE_MAX_MEMORY_BYTES));

    {
        let files_index = files_index.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let index = load_index_async().await;
                files_index.set(index);
            });
            || ()
        });
    }

    (files_index, cache)
}

/// Mount point of the workspace: wires storage, file processing, history
/// management, and per-model actions, and returns the aggregate state.
/// Call once, in the root component.
#[hook]
pub fn use_step_workspace() -> StepWorkspace {
    let states = StateHandles {
        result: use_state(|| None::<SmolStr>),
        result_is_error: use_state(|| false),
        metadata: use_state(|| None::<Metadata>),
        file_reader: Rc::new(RefCell::new(None)),
        step_model: use_state(|| None::<Rc<StepModel>>),
        part_visibility: use_state(Vec::new),
        selected_file: use_state(|| None::<FileId>),
        is_processing: use_state(|| false),
        pending_confirm: use_state(|| None::<ConfirmAction>),
        load_generation: Rc::new(Cell::new(0u64)),
        quality_preset: use_state(QualityPreset::default),
    };

    {
        let states_clone = states.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::CustomEvent| {
                    // Read the panic message carried in the CustomEvent detail.
                    // Falls back to a generic message if the detail is missing.
                    let msg = event.detail().as_string().unwrap_or_else(|| {
                        "A critical error occurred. Please upload a new file.".to_string()
                    });
                    // fail_load transitions result: old → Some(msg), giving Yew a real
                    // state diff so it patches the DOM and overwrites the class-only
                    // update the panic hook made directly on the span.
                    states_clone.fail_load(msg);
                }) as Box<dyn FnMut(_)>);

            let _ = window
                .add_event_listener_with_callback("app-panic", closure.as_ref().unchecked_ref());

            move || {
                let _ = window.remove_event_listener_with_callback(
                    "app-panic",
                    closure.as_ref().unchecked_ref(),
                );
            }
        });
    }

    let (files_index, cache) = use_workspace_storage();
    let on_file_change = use_file_processor(&states, files_index.clone(), cache.clone());
    let management = use_workspace_management(&states, files_index.clone(), cache.clone());
    let model_actions = use_model_actions(&states, cache);

    StepWorkspace {
        result: states.result.clone(),
        result_is_error: states.result_is_error.clone(),
        metadata: states.metadata.clone(),
        files_index,
        selected_file: states.selected_file.clone(),
        step_model: states.step_model.clone(),
        part_visibility: states.part_visibility.clone(),
        is_processing: states.is_processing.clone(),
        pending_confirm: states.pending_confirm.clone(),
        quality_preset: states.quality_preset.clone(),
        actions: WorkspaceActions {
            on_file_change,
            on_item_click: management.on_item_click,
            on_delete: management.on_delete,
            on_deselect: management.on_deselect,
            on_clear_history: management.on_clear_history,
            on_confirm: management.on_confirm,
            on_cancel_confirm: management.on_cancel_confirm,
            on_visibility_change: model_actions.on_visibility_change,
            on_show_all: model_actions.on_show_all,
            on_hide_all: model_actions.on_hide_all,
            on_calculate_volume: model_actions.on_calculate_volume,
            on_calculate_surface: model_actions.on_calculate_surface,
        },
    }
}

pub use processor::parse_step_file_content;
pub use state::build_step_model;
