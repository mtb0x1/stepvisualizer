use crate::common::{
    FileIndexItem, LruCache, Metadata, StepModel, compute_bounding_box, convert_header,
    delete_model, hash_text_to_id, load_index, load_model, parse_units, save_index, save_model,
    step_extract_wsgl_reqs,
};
use crate::trace_span;
use gloo::file::File;
use gloo::file::callbacks::FileReader;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Event, HtmlInputElement};
use yew::prelude::*;

use crate::common::constants::{CACHE_SIZE, MAX_FILE_BYTES};

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

pub struct StepWorkspace {
    pub result: UseStateHandle<Option<String>>,
    pub metadata: UseStateHandle<Option<Metadata>>,
    pub files_index: UseStateHandle<Vec<FileIndexItem>>,
    pub selected_file: UseStateHandle<Option<String>>,
    pub step_model: UseStateHandle<Option<Rc<StepModel>>>,
    pub is_processing: UseStateHandle<bool>,
    pub actions: WorkspaceActions,
}

#[hook]
pub fn use_step_workspace() -> StepWorkspace {
    trace_span!("use_step_workspace");
    let result = use_state(|| None::<String>);
    let metadata = use_state(|| None::<Metadata>);
    let file_reader = use_state(|| None::<FileReader>);
    let files_index = use_state(|| Vec::<FileIndexItem>::new());
    let cache = use_mut_ref(|| LruCache::new(CACHE_SIZE));
    let step_model = use_state(|| None::<Rc<StepModel>>);
    let selected_file = use_state(|| None::<String>);
    let is_processing = use_state(|| false);

    {
        let files_index_handle = files_index.clone();
        use_effect_with((), move |_| {
            let idx = load_index();
            files_index_handle.set(idx);
            || ()
        });
    }

    let on_file_change = {
        let result_handle = result.clone();
        let metadata_handle = metadata.clone();
        let file_reader_handle = file_reader.clone();
        let files_index_handle = files_index.clone();
        let cache_handle = cache.clone();
        let step_model_handle = step_model.clone();
        let selected_file_handle = selected_file.clone();
        let is_processing_handle = is_processing.clone();
        Callback::from(move |event: Event| {
            trace_span!("on_file_change callback");
            let input: HtmlInputElement = event
                .target()
                .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
                .expect("file input event");
            if let Some(files) = input.files() {
                if let Some(web_file) = files.get(0) {
                    is_processing_handle.set(true);
                    if web_file.size() > MAX_FILE_BYTES {
                        result_handle.set(Some(
                            "File too large. Maximum allowed is 20 MB.".to_string(),
                        ));
                        metadata_handle.set(None);
                        is_processing_handle.set(false);
                        return;
                    }
                    let name = web_file.name();
                    let file = File::from(web_sys::File::from(web_file));
                    let result_state = result_handle.clone();
                    let metadata_state = metadata_handle.clone();
                    let list_state = files_index_handle.clone();
                    let cache_state = cache_handle.clone();
                    let step_model_state = step_model_handle.clone();
                    let selected_file_state = selected_file_handle.clone();
                    let processing_state = is_processing_handle.clone();
                    let reader = gloo::file::callbacks::read_as_text(&file, move |res| {
                        match res {
                            Ok(text) => match ruststep::parser::parse(&text) {
                                Ok(parsed) => {
                                    if parsed.data.is_empty() {
                                        result_state.set(Some(
                                            "No data sections found in the STEP file.".to_string(),
                                        ));
                                        processing_state.set(false);
                                        return;
                                    }
                                    let section = match parsed.data.get(0) {
                                        Some(section)
                                            if !section.entities.is_empty()
                                                || !section.meta.is_empty() =>
                                        {
                                            section
                                        }
                                        _ => {
                                            result_state.set(Some(
                                                "STEP file has no usable data sections (empty meta/entities).".to_string(),
                                            ));
                                            metadata_state.set(None);
                                            processing_state.set(false);
                                            return;
                                        }
                                    };
                                    let step_table =
                                        truck_stepio::r#in::Table::from_data_section(&section);
                                    let entity_count: usize = parsed
                                        .data
                                        .iter()
                                        .map(|section| section.entities.len())
                                        .sum();
                                    let mut step_header = convert_header(&parsed.header);
                                    if step_header.file_name.is_empty() {
                                        step_header.file_name = name.clone();
                                    }
                                    let id = hash_text_to_id(&text);
                                    let bbox = compute_bounding_box(&step_table);
                                    let units = parse_units(&parsed);
                                    let meta = Metadata {
                                        header: step_header.clone(),
                                        entity_count,
                                        bounding_box: bbox,
                                        units,
                                        vertex_count: 0,
                                        triangle_count: 0,
                                        volume: None,
                                        surface_area: None,
                                    };

                                    metadata_state.set(Some(meta.clone()));
                                    selected_file_state.set(Some(id.clone()));
                                    result_state.set(Some(
                                        "Tessellating geometry for 3D view...".to_string(),
                                    ));

                                    let metadata_future = metadata_state.clone();
                                    let step_model_future = step_model_state.clone();
                                    let cache_future = cache_state.clone();
                                    let result_future = result_state.clone();
                                    let model_meta = meta.clone();
                                    let tess_id = id.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let renderable_parts =
                                            step_extract_wsgl_reqs(&tess_id, &step_table);
                                        let vertex_count =
                                            renderable_parts.iter().map(|p| p.vertices.len()).sum();
                                        let triangle_count = renderable_parts
                                            .iter()
                                            .map(|p| p.indices.len() / 3)
                                            .sum();

                                        let mut updated_meta = model_meta;
                                        updated_meta.vertex_count = vertex_count;
                                        updated_meta.triangle_count = triangle_count;

                                        let model = StepModel {
                                            id: tess_id.clone(),
                                            metadata: updated_meta.clone(),
                                            render_parts: renderable_parts,
                                        };

                                        {
                                            let mut cache_ref = cache_future.borrow_mut();
                                            cache_ref.insert(tess_id.clone(), model.clone());
                                        }
                                        save_model(&model);

                                        metadata_future.set(Some(updated_meta));
                                        step_model_future.set(Some(Rc::new(model)));
                                        result_future.set(Some(
                                            "Parsed STEP file successfully.".to_string(),
                                        ));
                                        processing_state.set(false);
                                    });

                                    let mut list = (*list_state).clone();
                                    list.retain(|i| i.id != id);
                                    list.insert(
                                        0,
                                        FileIndexItem {
                                            id: id.clone(),
                                            name: step_header.file_name.clone(),
                                            entity_count,
                                            time_stamp: step_header.time_stamp.clone(),
                                        },
                                    );
                                    list_state.set(list.clone());
                                    save_index(&list);
                                    return;
                                }
                                Err(e) => {
                                    result_state.set(Some(format!("Failed to parse STEP: {e}")));
                                    metadata_state.set(None);
                                }
                            },
                            Err(e) => {
                                result_state.set(Some(format!("Failed to read file: {e}")));
                                metadata_state.set(None);
                            }
                        }
                        processing_state.set(false);
                    });
                    file_reader_handle.set(Some(reader));
                    return;
                }
            }
            is_processing_handle.set(false);
        })
    };

    let on_item_click = {
        let files_index_state = files_index.clone();
        let metadata_state = metadata.clone();
        let result_state = result.clone();
        let cache_state = cache.clone();
        let step_model_state = step_model.clone();
        let selected_file_state = selected_file.clone();
        Callback::from(move |id: String| {
            let maybe_model = {
                let mut c = cache_state.borrow_mut();
                c.get(&id)
            }
            .or_else(|| load_model(&id));

            match maybe_model {
                Some(model) => {
                    {
                        let mut c = cache_state.borrow_mut();
                        c.insert(id.clone(), model.clone());
                    }
                    metadata_state.set(Some(model.metadata.clone()));
                    step_model_state.set(Some(Rc::new(model)));
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
        let result_state = result.clone();
        let cache_handle = cache.clone();
        let selected_file_state = selected_file.clone();
        let metadata_state = metadata.clone();
        let step_model_state = step_model.clone();
        Callback::from(move |delete_id: String| {
            if let Some(window) = web_sys::window() {
                if let Ok(false) = window.confirm_with_message(
                    "Remove this file from history? This action cannot be undone.",
                ) {
                    result_state.set(Some("Deletion cancelled.".to_string()));
                    return;
                }
            }
            {
                let mut c = cache_handle.borrow_mut();
                c.remove(&delete_id);
            }

            delete_model(&delete_id);
            let mut list = (*files_index).clone();
            list.retain(|i| i.id != delete_id);
            files_index.set(list.clone());
            save_index(&list);
            if selected_file_state.as_ref() == Some(&delete_id) {
                selected_file_state.set(None);
                metadata_state.set(None);
                step_model_state.set(None);
            }
            result_state.set(Some("Removed file from list.".to_string()));
        })
    };

    let on_deselect = {
        let selected_file_state = selected_file.clone();
        let metadata_state = metadata.clone();
        let step_model_state = step_model.clone();
        Callback::from(move |_| {
            selected_file_state.set(None);
            metadata_state.set(None);
            step_model_state.set(None);
        })
    };

    let on_clear_history = {
        let files_index_state = files_index.clone();
        let result_state = result.clone();
        let cache_handle = cache.clone();
        let metadata_state = metadata.clone();
        let step_model_state = step_model.clone();
        let selected_file_state = selected_file.clone();
        Callback::from(move |_| {
            if let Some(window) = web_sys::window() {
                if let Ok(false) = window.confirm_with_message(
                    "Clear all cached files? This removes local copies and history.",
                ) {
                    result_state.set(Some("Clear history cancelled.".to_string()));
                    return;
                }
            }

            let existing = (*files_index_state).clone();
            for item in &existing {
                delete_model(&item.id);
            }

            {
                let mut cache_mut = cache_handle.borrow_mut();
                cache_mut.clear();
            }

            files_index_state.set(Vec::new());
            save_index(&[]);
            metadata_state.set(None);
            step_model_state.set(None);
            selected_file_state.set(None);
            result_state.set(Some("Cleared cached files.".to_string()));
        })
    };

    let on_visibility_change = {
        let step_model = step_model.clone();
        Callback::from(move |(index, visible): (usize, bool)| {
            if let Some(model) = step_model.as_ref() {
                let mut new_model = (**model).clone();
                if let Some(part) = new_model.render_parts.get_mut(index) {
                    part.visible = visible;
                    step_model.set(Some(Rc::new(new_model)));
                }
            }
        })
    };

    let on_show_all = {
        let step_model = step_model.clone();
        Callback::from(move |_| {
            if let Some(model) = step_model.as_ref() {
                let mut new_model = (**model).clone();
                for part in &mut new_model.render_parts {
                    part.visible = true;
                }
                step_model.set(Some(Rc::new(new_model)));
            }
        })
    };

    let on_hide_all = {
        let step_model = step_model.clone();
        Callback::from(move |_| {
            if let Some(model) = step_model.as_ref() {
                let mut new_model = (**model).clone();
                for part in &mut new_model.render_parts {
                    part.visible = false;
                }
                step_model.set(Some(Rc::new(new_model)));
            }
        })
    };

    let on_calculate_volume = {
        let step_model = step_model.clone();
        let metadata = metadata.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            if let Some(model) = step_model.as_ref() {
                let mut total_volume = 0.0;
                for part in &model.render_parts {
                    total_volume += part.calculate_volume();
                }

                let mut new_meta = model.metadata.clone();
                new_meta.volume = Some(total_volume);
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
        })
    };

    let on_calculate_surface = {
        let step_model = step_model.clone();
        let metadata = metadata.clone();
        let cache = cache.clone();
        Callback::from(move |_| {
            if let Some(model) = step_model.as_ref() {
                let mut total_area = 0.0;
                for part in &model.render_parts {
                    total_area += part.calculate_surface_area();
                }

                let mut new_meta = model.metadata.clone();
                new_meta.surface_area = Some(total_area);
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
        })
    };

    StepWorkspace {
        result,
        metadata,
        files_index,
        selected_file,
        step_model,
        is_processing,
        actions: WorkspaceActions {
            on_file_change,
            on_item_click,
            on_delete,
            on_deselect,
            on_clear_history,
            on_visibility_change,
            on_show_all,
            on_hide_all,
            on_calculate_volume,
            on_calculate_surface,
        },
    }
}
