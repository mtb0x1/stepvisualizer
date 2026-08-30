//! App-wide state: one hook ([`use_step_workspace`]) owning file parsing,
//! recent-file history, and model interaction. Panels receive slices of
//! [`StepWorkspace`] as props; nothing else holds app state.
use crate::common::{
    FileId, FileIndexItem, LruCache, Metadata, StepModel, clear_all_storage, compute_bounding_box,
    convert_header, delete_model, extract_render_parts, hash_text_to_id, load_index, load_model,
    parse_units, save_index, save_model, visible_bounds,
};
use crate::error::StepVizError;
use crate::trace_span;
use gloo::file::File;
use gloo::file::callbacks::FileReader;
use ruststep::ast::{DataSection, Exchange};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Event, HtmlInputElement};
use yew::prelude::*;

use crate::common::constants::{CACHE_SIZE, DEFAULT_TOLERANCE, MAX_FILE_BYTES};

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
    pub result: UseStateHandle<Option<String>>,
    /// `true` when the current `result` message is an error (drives CSS class).
    pub result_is_error: UseStateHandle<bool>,
    pub metadata: UseStateHandle<Option<Metadata>>,
    pub files_index: UseStateHandle<Vec<FileIndexItem>>,
    pub selected_file: UseStateHandle<Option<FileId>>,
    pub step_model: UseStateHandle<Option<Rc<StepModel>>>,
    pub part_visibility: UseStateHandle<Vec<bool>>,
    pub is_processing: UseStateHandle<bool>,
    pub pending_confirm: UseStateHandle<Option<ConfirmAction>>,
    pub actions: WorkspaceActions,
}

#[hook]
fn use_workspace_storage() -> (UseStateHandle<Vec<FileIndexItem>>, Rc<RefCell<LruCache>>) {
    let files_index = use_state(Vec::<FileIndexItem>::new);
    let cache = use_mut_ref(|| LruCache::new(CACHE_SIZE));

    {
        let files_index_handle = files_index.clone();
        use_effect_with((), move |_| {
            let idx = load_index();
            files_index_handle.set(idx);
            || ()
        });
    }

    (files_index, cache)
}

/// All `use_state` handles owned by the workspace, grouped so the
/// sub-hooks (`use_file_processor`, `use_workspace_management`,
/// `use_model_actions`) can receive them without a long parameter list.
#[derive(Clone)]
struct StateHandles {
    result: UseStateHandle<Option<String>>,
    result_is_error: UseStateHandle<bool>,
    metadata: UseStateHandle<Option<Metadata>>,
    step_model: UseStateHandle<Option<Rc<StepModel>>>,
    part_visibility: UseStateHandle<Vec<bool>>,
    selected_file: UseStateHandle<Option<FileId>>,
    is_processing: UseStateHandle<bool>,
    pending_confirm: UseStateHandle<Option<ConfirmAction>>,
    file_reader: UseStateHandle<Option<FileReader>>,
}

impl StateHandles {
    /// Resets the UI after a failed load: surface `err` as an error, clear stale
    /// metadata, and drop the processing flag.
    fn fail_load(&self, err: impl std::fmt::Display) {
        self.result_is_error.set(true);
        self.result.set(Some(err.to_string()));
        self.metadata.set(None);
        self.is_processing.set(false);
    }

    /// Reset the loaded-model UI state: clear selection, metadata, model, and part
    /// visibility. Shared by deselect, delete (when the deleted file was selected),
    /// and clear-history so the four-line reset lives in one place.
    fn clear_model_state(&self) {
        self.selected_file.set(None);
        self.metadata.set(None);
        self.step_model.set(None);
        self.part_visibility.set(Vec::new());
    }

    /// Update status message and error flag.
    fn set_result(&self, msg: impl Into<String>, is_error: bool) {
        self.result_is_error.set(is_error);
        self.result.set(Some(msg.into()));
    }

    /// Sets the active loaded model across all related state handles.
    fn set_loaded_model(&self, model: Rc<StepModel>, file_id: FileId, status_msg: &str) {
        let part_visibility = model.part_visibility.clone();
        self.metadata.set(Some(model.metadata.clone()));
        self.step_model.set(Some(model));
        self.part_visibility.set(part_visibility);
        self.selected_file.set(Some(file_id));
        self.set_result(status_msg, false);
        self.is_processing.set(false);
    }
}

/// Extracts the first selected file from an `<input type="file">` change event.
fn input_file(event: &Event) -> Option<web_sys::File> {
    let input: HtmlInputElement = event
        .target()
        .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())?;
    input.files()?.get(0)
}

/// Returns the first data section carrying usable STEP content, or a
/// domain error explaining why the file has none.
fn first_usable_section(parsed: &Exchange) -> Result<&DataSection, StepVizError> {
    if parsed.data.is_empty() {
        return Err(StepVizError::EmptyDataSection);
    }
    match parsed.data.first() {
        Some(section) if !section.entities.is_empty() || !section.meta.is_empty() => Ok(section),
        _ => Err(StepVizError::EmptyDataSection),
    }
}

/// Assembles the pre-tessellation metadata (header, entity count, bounding
/// box, units) for a parsed STEP file, together with its content-hash id.
/// The tessellated counts (vertices/triangles) are filled in later, once
/// the geometry pass has produced them.
fn build_initial_metadata(
    fallback_name: &str,
    parsed: &Exchange,
    step_table: &truck_stepio::r#in::Table,
    text: &str,
) -> Result<(Metadata, FileId), StepVizError> {
    let entity_count: usize = parsed
        .data
        .iter()
        .map(|section| section.entities.len())
        .sum();
    let mut step_header = convert_header(&parsed.header)?;
    if step_header.file_name.is_empty() {
        step_header.file_name = fallback_name.to_string();
    }
    let meta = Metadata {
        header: step_header,
        entity_count,
        bounding_box: compute_bounding_box(step_table),
        units: parse_units(parsed),
        vertex_count: 0,
        triangle_count: 0,
        volume: None,
        surface_area: None,
    };
    Ok((meta, hash_text_to_id(text)))
}

/// Spawns the async tessellation pass: tessellates the STEP table, wraps the
/// result in a `StepModel`, persists it to the cache and localStorage, then
/// publishes the updated metadata and model to the UI.
fn spawn_tessellation(
    step_table: truck_stepio::r#in::Table,
    file_id: FileId,
    meta: Metadata,
    states: StateHandles,
    cache: Rc<RefCell<LruCache>>,
) {
    let tolerance = DEFAULT_TOLERANCE;
    wasm_bindgen_futures::spawn_local(async move {
        let renderable_parts = extract_render_parts(&step_table, tolerance);
        let part_count = renderable_parts.len();

        let mut model = StepModel {
            id: file_id.clone(),
            metadata: meta,
            render_parts: renderable_parts,
            part_visibility: vec![true; part_count],
            visibility_generation: 0,
            cached_bounds: None,
        };
        model.metadata.vertex_count = model.total_vertices();
        model.metadata.triangle_count = model.total_triangles();
        if let Some(bbox) = visible_bounds(&model.render_parts, &[]) {
            model.metadata.bounding_box = Some(bbox);
        }

        {
            let mut cache_ref = cache.borrow_mut();
            cache_ref.insert(file_id.clone(), model.clone());
        }
        save_model(&model);

        states.metadata.set(Some(model.metadata.clone()));
        states.step_model.set(Some(Rc::new(model)));
        states.part_visibility.set(vec![true; part_count]);
        states.set_result("Parsed STEP file successfully.", false);
        states.is_processing.set(false);
    });
}

/// Mutates the history file index in state and persists it to localStorage.
fn update_and_persist_index(
    files_index: &UseStateHandle<Vec<FileIndexItem>>,
    mut update: impl FnMut(&mut Vec<FileIndexItem>),
) {
    let mut list = (**files_index).clone();
    update(&mut list);
    files_index.set(list.clone());
    save_index(&list);
}

#[hook]
fn use_file_processor(
    states: &StateHandles,
    files_index: UseStateHandle<Vec<FileIndexItem>>,
    cache: Rc<RefCell<LruCache>>,
) -> Callback<Event> {
    let states = states.clone();

    Callback::from(move |event: Event| {
        trace_span!("on_file_change callback");
        let Some(web_file) = input_file(&event) else {
            states.is_processing.set(false);
            return;
        };

        if let Some(input) = event
            .target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
        {
            input.set_value("");
        }

        states.is_processing.set(true);
        if web_file.size() > MAX_FILE_BYTES {
            states.fail_load(StepVizError::FileTooLarge {
                size_bytes: web_file.size(),
                max_bytes: MAX_FILE_BYTES,
            });
            return;
        }

        let name = web_file.name();
        let file = File::from(web_file);

        let states_for_reader = states.clone();
        let cache = cache.clone();
        let files_index = files_index.clone();

        let reader = gloo::file::callbacks::read_as_text(&file, move |res| {
            let fail = |err: StepVizError| states_for_reader.fail_load(err);

            let text = match res {
                Ok(text) => text,
                Err(e) => return fail(StepVizError::FileRead(e.to_string())),
            };
            let parsed = match ruststep::parser::parse(&text) {
                Ok(parsed) => parsed,
                Err(e) => return fail(StepVizError::Parse(e.to_string())),
            };
            let section = match first_usable_section(&parsed) {
                Ok(section) => section,
                Err(err) => return fail(err),
            };
            let step_table = truck_stepio::r#in::Table::from_data_section(section);
            let (meta, id) = match build_initial_metadata(&name, &parsed, &step_table, &text) {
                Ok(v) => v,
                Err(err) => return fail(err),
            };

            if let Some(cached_model) = cache.borrow_mut().get_or_load(&id, load_model) {
                let model_rc = Rc::new(cached_model);
                states_for_reader.set_loaded_model(model_rc, id.clone(), "Loaded from cache");
                update_and_persist_index(&files_index, |list| {
                    if let Some(pos) = list.iter().position(|i| i.id == id) {
                        let item = list.remove(pos);
                        list.insert(0, item);
                    }
                });
                return;
            }

            states_for_reader.metadata.set(Some(meta.clone()));
            states_for_reader.selected_file.set(Some(id.clone()));
            states_for_reader.set_result("Tessellating geometry for 3D view...", false);

            spawn_tessellation(
                step_table,
                id.clone(),
                meta.clone(),
                states_for_reader.clone(),
                cache.clone(),
            );

            // Record the file in the history index (most recent first).
            update_and_persist_index(&files_index, |list| {
                list.retain(|i| i.id != id);
                list.insert(
                    0,
                    FileIndexItem {
                        id: id.clone(),
                        name: meta.header.file_name.clone(),
                        entity_count: meta.entity_count,
                        time_stamp: meta.header.time_stamp.clone(),
                    },
                );
            });
        });
        states.file_reader.set(Some(reader));
    })
}

/// History-management callbacks returned by `use_workspace_management`.
pub struct WorkspaceManagementActions {
    pub on_item_click: Callback<FileId>,
    pub on_delete: Callback<FileId>,
    pub on_deselect: Callback<()>,
    pub on_clear_history: Callback<()>,
    pub on_confirm: Callback<()>,
    pub on_cancel_confirm: Callback<()>,
}

#[hook]
fn use_workspace_management(
    states: &StateHandles,
    files_index: UseStateHandle<Vec<FileIndexItem>>,
    cache: Rc<RefCell<LruCache>>,
) -> WorkspaceManagementActions {
    let on_item_click = {
        let files_index = files_index.clone();
        let states = states.clone();
        let cache = cache.clone();
        Callback::from(move |id: FileId| {
            let maybe_model = cache.borrow_mut().get_or_load(&id, load_model);

            match maybe_model {
                Some(model) => {
                    let model_rc = Rc::new(model);
                    states.set_loaded_model(model_rc, id.clone(), "Loaded from cache");
                    update_and_persist_index(&files_index, |list| {
                        if let Some(pos) = list.iter().position(|i| i.id == id) {
                            let item = list.remove(pos);
                            list.insert(0, item);
                        }
                    });
                }
                None => {
                    states.set_result("Cached data missing.", true);
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
            states.clear_model_state();
        })
    };

    let on_clear_history = {
        let states = states.clone();
        Callback::from(move |_| {
            states.pending_confirm.set(Some(ConfirmAction::ClearHistory));
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
                update_and_persist_index(&files_index, |list| {
                    list.retain(|i| i.id != delete_id);
                });
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

// Calculates a metric over the current model, updates the matching field on
// `Metadata`, then persists and republishes the model. Shared by the volume and
// surface-area actions which differ only in the metric and the field written.
fn recompute_and_store_metric(
    states: &StateHandles,
    cache: &Rc<RefCell<LruCache>>,
    compute: impl Fn(&StepModel) -> f64,
    apply: impl Fn(&mut Metadata, f64),
) {
    if let Some(model) = states.step_model.as_ref() {
        let total = compute(model);

        let mut new_meta = model.metadata.clone();
        apply(&mut new_meta, total);
        states.metadata.set(Some(new_meta.clone()));

        let mut new_model = (**model).clone();
        new_model.metadata = new_meta;

        {
            let mut c = cache.borrow_mut();
            c.insert(new_model.id.clone(), new_model.clone());
        }
        save_model(&new_model);

        states.step_model.set(Some(Rc::new(new_model)));
    }
}

/// Per-model interaction callbacks returned by `use_model_actions`.
pub struct ModelActions {
    pub on_visibility_change: Callback<(usize, bool)>,
    pub on_show_all: Callback<()>,
    pub on_hide_all: Callback<()>,
    pub on_calculate_volume: Callback<()>,
    pub on_calculate_surface: Callback<()>,
}

#[hook]
fn use_model_actions(states: &StateHandles, cache: Rc<RefCell<LruCache>>) -> ModelActions {
    let on_visibility_change = {
        let part_visibility = states.part_visibility.clone();
        Callback::from(move |(index, visible): (usize, bool)| {
            let mut new_visibility = (*part_visibility).clone();
            if index < new_visibility.len() {
                new_visibility[index] = visible;
                part_visibility.set(new_visibility);
            }
        })
    };

    let on_show_all = {
        let part_visibility = states.part_visibility.clone();
        Callback::from(move |_| {
            part_visibility.set(vec![true; part_visibility.len()]);
        })
    };

    let on_hide_all = {
        let part_visibility = states.part_visibility.clone();
        Callback::from(move |_| {
            part_visibility.set(vec![false; part_visibility.len()]);
        })
    };

    let on_calculate_volume = {
        let states = states.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            recompute_and_store_metric(
                &states,
                &cache,
                |m| m.calculate_total_volume(),
                |meta, value| meta.volume = Some(value),
            );
        })
    };

    let on_calculate_surface = {
        let states = states.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            recompute_and_store_metric(
                &states,
                &cache,
                |m| m.calculate_total_surface_area(),
                |meta, value| meta.surface_area = Some(value),
            );
        })
    };

    ModelActions {
        on_visibility_change,
        on_show_all,
        on_hide_all,
        on_calculate_volume,
        on_calculate_surface,
    }
}

/// Mount point of the workspace: wires storage, file processing, history
/// management, and per-model actions, and returns the aggregate state.
/// Call once, in the root component.
#[hook]
pub fn use_step_workspace() -> StepWorkspace {
    trace_span!("use_step_workspace");
    let states = StateHandles {
        result: use_state(|| None::<String>),
        result_is_error: use_state(|| false),
        metadata: use_state(|| None::<Metadata>),
        file_reader: use_state(|| None::<FileReader>),
        step_model: use_state(|| None::<Rc<StepModel>>),
        part_visibility: use_state(Vec::new),
        selected_file: use_state(|| None::<FileId>),
        is_processing: use_state(|| false),
        pending_confirm: use_state(|| None::<ConfirmAction>),
    };

    let (files_index, cache) = use_workspace_storage();

    let on_file_change = use_file_processor(&states, files_index.clone(), cache.clone());

    let management = use_workspace_management(&states, files_index.clone(), cache.clone());

    let model_actions = use_model_actions(&states, cache.clone());

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
