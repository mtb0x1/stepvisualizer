//! App-wide state: one hook ([`use_step_workspace`]) owning file parsing,
//! recent-file history, and model interaction. Panels receive slices of
//! [`StepWorkspace`] as props; nothing else holds app state.
use crate::common::cache::{clear_cached_parts, drop_cached_parts};
use crate::common::{
    FileIndexItem, LruCache, Metadata, RenderablePart, StepModel, compute_bounding_box,
    convert_header, delete_model, extract_render_parts, hash_text_to_id, load_index, load_model,
    parse_units, save_index, save_model,
};
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

/// All UI callbacks the panels consume, aggregated so a single struct can be
/// passed down from [`StepWorkspace`]. Split internally into the file
/// processor, history management, and per-model action groups.
pub struct WorkspaceActions {
    pub on_file_change: Callback<Event>,
    pub on_item_click: Callback<String>,
    pub on_delete: Callback<String>,
    pub on_deselect: Callback<()>,
    pub on_clear_history: Callback<()>,
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
    pub metadata: UseStateHandle<Option<Metadata>>,
    pub files_index: UseStateHandle<Vec<FileIndexItem>>,
    pub selected_file: UseStateHandle<Option<String>>,
    pub step_model: UseStateHandle<Option<Rc<StepModel>>>,
    pub part_visibility: UseStateHandle<Vec<bool>>,
    pub is_processing: UseStateHandle<bool>,
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
struct StateHandles {
    result: UseStateHandle<Option<String>>,
    metadata: UseStateHandle<Option<Metadata>>,
    step_model: UseStateHandle<Option<Rc<StepModel>>>,
    part_visibility: UseStateHandle<Vec<bool>>,
    selected_file: UseStateHandle<Option<String>>,
    is_processing: UseStateHandle<bool>,
    file_reader: UseStateHandle<Option<FileReader>>,
}

/// Extracts the first selected file from an `<input type="file">` change event.
fn input_file(event: &Event) -> Option<web_sys::File> {
    let input: HtmlInputElement = event
        .target()
        .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())?;
    input.files()?.get(0)
}

/// Resets the UI after a failed load: surface `msg`, clear stale metadata,
/// and drop the processing flag. Every failure path funnels through here.
fn fail_load(
    msg: String,
    result: &UseStateHandle<Option<String>>,
    metadata: &UseStateHandle<Option<Metadata>>,
    is_processing: &UseStateHandle<bool>,
) {
    result.set(Some(msg));
    metadata.set(None);
    is_processing.set(false);
}

/// Returns the first data section carrying usable STEP content, or a
/// user-facing message explaining why the file has none.
fn first_usable_section(parsed: &Exchange) -> Result<&DataSection, String> {
    if parsed.data.is_empty() {
        return Err("No data sections found in the STEP file.".to_string());
    }
    match parsed.data.first() {
        Some(section) if !section.entities.is_empty() || !section.meta.is_empty() => Ok(section),
        _ => Err("STEP file has no usable data sections (empty meta/entities).".to_string()),
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
) -> Result<(Metadata, String), String> {
    let entity_count: usize = parsed
        .data
        .iter()
        .map(|section| section.entities.len())
        .sum();
    let mut step_header = convert_header(&parsed.header).map_err(|e| e.0)?;
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

/// UI state targets that receive the results of the async tessellation pass.
struct TessellationTargets {
    metadata: UseStateHandle<Option<Metadata>>,
    step_model: UseStateHandle<Option<Rc<StepModel>>>,
    part_visibility: UseStateHandle<Vec<bool>>,
    result: UseStateHandle<Option<String>>,
    is_processing: UseStateHandle<bool>,
    cache: Rc<RefCell<LruCache>>,
}

/// Spawns the async tessellation pass: tessellates the STEP table, wraps the
/// result in a `StepModel`, persists it to the cache and localStorage, then
/// publishes the updated metadata and model to the UI.
fn spawn_tessellation(
    step_table: truck_stepio::r#in::Table,
    file_id: String,
    meta: Metadata,
    targets: TessellationTargets,
) {
    let tolerance = DEFAULT_TOLERANCE;
    wasm_bindgen_futures::spawn_local(async move {
        let renderable_parts = extract_render_parts(&file_id, &step_table, tolerance);
        let vertex_count = renderable_parts.iter().map(|p| p.vertices.len()).sum();
        let triangle_count = renderable_parts.iter().map(|p| p.indices.len() / 3).sum();

        let mut updated_meta = meta;
        updated_meta.vertex_count = vertex_count;
        updated_meta.triangle_count = triangle_count;

        let part_count = renderable_parts.len();
        let model = StepModel {
            id: file_id.clone(),
            metadata: updated_meta.clone(),
            render_parts: renderable_parts,
            part_visibility: vec![true; part_count],
        };

        {
            let mut cache_ref = targets.cache.borrow_mut();
            cache_ref.insert(file_id.clone(), model.clone());
        }
        save_model(&model);

        targets.metadata.set(Some(updated_meta));
        targets.step_model.set(Some(Rc::new(model)));
        targets.part_visibility.set(vec![true; part_count]);
        targets
            .result
            .set(Some("Parsed STEP file successfully.".to_string()));
        targets.is_processing.set(false);
    });
}

#[hook]
fn use_file_processor(
    states: &StateHandles,
    files_index: UseStateHandle<Vec<FileIndexItem>>,
    cache: Rc<RefCell<LruCache>>,
) -> Callback<Event> {
    let result_handle = states.result.clone();
    let metadata_handle = states.metadata.clone();
    let file_reader_handle = states.file_reader.clone();
    let files_index_handle = files_index.clone();
    let cache_handle = cache.clone();
    let step_model_handle = states.step_model.clone();
    let selected_file_handle = states.selected_file.clone();
    let part_visibility_handle = states.part_visibility.clone();
    let is_processing_handle = states.is_processing.clone();

    Callback::from(move |event: Event| {
        trace_span!("on_file_change callback");
        let Some(web_file) = input_file(&event) else {
            is_processing_handle.set(false);
            return;
        };

        is_processing_handle.set(true);
        if web_file.size() > MAX_FILE_BYTES {
            fail_load(
                "File too large. Maximum allowed is 20 MB.".to_string(),
                &result_handle,
                &metadata_handle,
                &is_processing_handle,
            );
            return;
        }

        let name = web_file.name();
        let file = File::from(web_file);

        // Clone the handles the reader callback will own: the outer closure
        // must stay `Fn` (it is invoked for every file-selection event), so it
        // cannot move its own captures into the one-shot reader callback.
        let result_state = result_handle.clone();
        let metadata_state = metadata_handle.clone();
        let processing_state = is_processing_handle.clone();
        let selected_file_state = selected_file_handle.clone();
        let step_model_state = step_model_handle.clone();
        let part_visibility_state = part_visibility_handle.clone();
        let cache_state = cache_handle.clone();
        let files_index_state = files_index_handle.clone();

        let reader = gloo::file::callbacks::read_as_text(&file, move |res| {
            // Every early exit below resets the UI the same way.
            let fail = |msg: String| {
                fail_load(msg, &result_state, &metadata_state, &processing_state);
            };

            let text = match res {
                Ok(text) => text,
                Err(e) => return fail(format!("Failed to read file: {e}")),
            };
            let parsed = match ruststep::parser::parse(&text) {
                Ok(parsed) => parsed,
                Err(e) => return fail(format!("Failed to parse STEP: {e}")),
            };
            let section = match first_usable_section(&parsed) {
                Ok(section) => section,
                Err(msg) => return fail(msg),
            };
            let step_table = truck_stepio::r#in::Table::from_data_section(section);
            let (meta, id) = match build_initial_metadata(&name, &parsed, &step_table, &text) {
                Ok(v) => v,
                Err(msg) => return fail(msg),
            };

            metadata_state.set(Some(meta.clone()));
            selected_file_state.set(Some(id.clone()));
            result_state.set(Some("Tessellating geometry for 3D view...".to_string()));

            spawn_tessellation(
                step_table,
                id.clone(),
                meta.clone(),
                TessellationTargets {
                    metadata: metadata_state.clone(),
                    step_model: step_model_state.clone(),
                    part_visibility: part_visibility_state.clone(),
                    result: result_state.clone(),
                    is_processing: processing_state.clone(),
                    cache: cache_state.clone(),
                },
            );

            // Record the file in the history index (most recent first).
            let mut list = (*files_index_state).clone();
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
            files_index_state.set(list.clone());
            save_index(&list);
        });
        file_reader_handle.set(Some(reader));
    })
}

/// History-management callbacks returned by `use_workspace_management`.
pub struct WorkspaceManagementActions {
    pub on_item_click: Callback<String>,
    pub on_delete: Callback<String>,
    pub on_deselect: Callback<()>,
    pub on_clear_history: Callback<()>,
}

#[hook]
fn use_workspace_management(
    states: &StateHandles,
    files_index: UseStateHandle<Vec<FileIndexItem>>,
    cache: Rc<RefCell<LruCache>>,
) -> WorkspaceManagementActions {
    let on_item_click = {
        let files_index_state = files_index.clone();
        let metadata_state = states.metadata.clone();
        let result_state = states.result.clone();
        let cache_state = cache.clone();
        let step_model_state = states.step_model.clone();
        let selected_file_state = states.selected_file.clone();
        let part_visibility_state = states.part_visibility.clone();
        Callback::from(move |id: String| {
            let maybe_model = cache_state.borrow_mut().get_or_load(&id, load_model);

            match maybe_model {
                Some(model) => {
                    let model_rc = Rc::new(model);
                    let part_visibility = model_rc.part_visibility.clone();
                    metadata_state.set(Some(model_rc.metadata.clone()));
                    step_model_state.set(Some(model_rc));
                    part_visibility_state.set(part_visibility);
                    selected_file_state.set(Some(id.clone()));
                    result_state.set(Some("Loaded from cache".to_string()));
                    let mut list = (*files_index_state).clone();
                    if let Some(pos) = list.iter().position(|i| i.id == id) {
                        let item = list.remove(pos);
                        list.insert(0, item);
                        files_index_state.set(list.clone());
                        save_index(&list);
                    }
                }
                None => {
                    result_state.set(Some("Cached data missing.".to_string()));
                }
            }
        })
    };

    let on_delete = {
        let files_index = files_index.clone();
        let result_state = states.result.clone();
        let cache_handle = cache.clone();
        let selected_file_state = states.selected_file.clone();
        let metadata_state = states.metadata.clone();
        let step_model_state = states.step_model.clone();
        let part_visibility_state = states.part_visibility.clone();
        Callback::from(move |delete_id: String| {
            if let Some(window) = web_sys::window()
                && let Ok(false) = window.confirm_with_message(
                    "Remove this file from history? This action cannot be undone.",
                )
            {
                result_state.set(Some("Deletion cancelled.".to_string()));
                return;
            }
            {
                let mut c = cache_handle.borrow_mut();
                c.remove(&delete_id);
            }

            // Free the tessellated geometry held for this file so it is not
            // retained for the lifetime of the page.
            drop_cached_parts(&delete_id);

            delete_model(&delete_id);
            let mut list = (*files_index).clone();
            list.retain(|i| i.id != delete_id);
            files_index.set(list.clone());
            save_index(&list);
            if selected_file_state.as_ref() == Some(&delete_id) {
                selected_file_state.set(None);
                metadata_state.set(None);
                step_model_state.set(None);
                part_visibility_state.set(Vec::new());
            }
            result_state.set(Some("Removed file from list.".to_string()));
        })
    };

    let on_deselect = {
        let selected_file_state = states.selected_file.clone();
        let metadata_state = states.metadata.clone();
        let step_model_state = states.step_model.clone();
        let part_visibility_state = states.part_visibility.clone();
        Callback::from(move |_| {
            selected_file_state.set(None);
            metadata_state.set(None);
            step_model_state.set(None);
            part_visibility_state.set(Vec::new());
        })
    };

    let on_clear_history = {
        let files_index_state = files_index.clone();
        let result_state = states.result.clone();
        let cache_handle = cache.clone();
        let metadata_state = states.metadata.clone();
        let step_model_state = states.step_model.clone();
        let selected_file_state = states.selected_file.clone();
        let part_visibility_state = states.part_visibility.clone();
        Callback::from(move |_| {
            if let Some(window) = web_sys::window()
                && let Ok(false) = window.confirm_with_message(
                    "Clear all cached files? This removes local copies and history.",
                )
            {
                result_state.set(Some("Clear history cancelled.".to_string()));
                return;
            }

            let existing = (*files_index_state).clone();
            for item in &existing {
                delete_model(&item.id);
            }

            {
                let mut cache_mut = cache_handle.borrow_mut();
                cache_mut.clear();
            }

            // Drop every cached tessellation alongside the model cache.
            clear_cached_parts();

            files_index_state.set(Vec::new());
            save_index(&[]);
            metadata_state.set(None);
            step_model_state.set(None);
            selected_file_state.set(None);
            part_visibility_state.set(Vec::new());
            result_state.set(Some("Cleared cached files.".to_string()));
        })
    };

    WorkspaceManagementActions {
        on_item_click,
        on_delete,
        on_deselect,
        on_clear_history,
    }
}

// Sums a per-part metric over the current model, updates the matching field on
// `Metadata`, then persists and republishes the model. Shared by the volume and
// surface-area actions which differ only in the metric and the field written.
fn recompute_and_store_metric(
    step_model: &UseStateHandle<Option<Rc<StepModel>>>,
    metadata: &UseStateHandle<Option<Metadata>>,
    cache: &Rc<RefCell<LruCache>>,
    compute: impl Fn(&RenderablePart) -> f64,
    apply: impl Fn(&mut Metadata, f64),
) {
    if let Some(model) = step_model.as_ref() {
        let total: f64 = model.render_parts.iter().map(compute).sum();

        let mut new_meta = model.metadata.clone();
        apply(&mut new_meta, total);
        metadata.set(Some(new_meta.clone()));

        let mut new_model = (**model).clone();
        new_model.metadata = new_meta;

        {
            let mut c = cache.borrow_mut();
            c.insert(new_model.id.clone(), new_model.clone());
        }
        save_model(&new_model);

        step_model.set(Some(Rc::new(new_model)));
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
        let step_model = states.step_model.clone();
        let metadata = states.metadata.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            recompute_and_store_metric(
                &step_model,
                &metadata,
                &cache,
                |p| p.calculate_volume(),
                |meta, value| meta.volume = Some(value),
            );
        })
    };

    let on_calculate_surface = {
        let step_model = states.step_model.clone();
        let metadata = states.metadata.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            recompute_and_store_metric(
                &step_model,
                &metadata,
                &cache,
                |p| p.calculate_surface_area(),
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
        metadata: use_state(|| None::<Metadata>),
        file_reader: use_state(|| None::<FileReader>),
        step_model: use_state(|| None::<Rc<StepModel>>),
        part_visibility: use_state(Vec::new),
        selected_file: use_state(|| None::<String>),
        is_processing: use_state(|| false),
    };

    let (files_index, cache) = use_workspace_storage();

    let on_file_change = use_file_processor(&states, files_index.clone(), cache.clone());

    let management = use_workspace_management(&states, files_index.clone(), cache.clone());

    let model_actions = use_model_actions(&states, cache.clone());

    StepWorkspace {
        result: states.result.clone(),
        metadata: states.metadata.clone(),
        files_index,
        selected_file: states.selected_file.clone(),
        step_model: states.step_model.clone(),
        part_visibility: states.part_visibility.clone(),
        is_processing: states.is_processing.clone(),
        actions: WorkspaceActions {
            on_file_change,
            on_item_click: management.on_item_click,
            on_delete: management.on_delete,
            on_deselect: management.on_deselect,
            on_clear_history: management.on_clear_history,
            on_visibility_change: model_actions.on_visibility_change,
            on_show_all: model_actions.on_show_all,
            on_hide_all: model_actions.on_hide_all,
            on_calculate_volume: model_actions.on_calculate_volume,
            on_calculate_surface: model_actions.on_calculate_surface,
        },
    }
}
