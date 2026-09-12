//! File upload handling, STEP parsing pipeline, and async tessellation.
use crate::common::constants::{
    MAX_FILE_BYTES, MAX_TOLERANCE, MIN_TOLERANCE, compute_adaptive_tolerance,
};
use crate::common::utils::input_file;
use crate::common::{
    ExchangeIndex, FileId, FileIndexItem, LruCache, Metadata, StepColorMap, StepNameMap,
    all_usable_sections, build_initial_metadata, extract_render_parts, load_model,
    probe_validate_step_buffer, save_model,
};
use crate::error::StepError;
use crate::trace_span;
use crate::workspace::history::{add_to_index, promote_in_index};
use crate::workspace::state::{StateHandles, build_step_model};
use gloo::file::File;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Event, HtmlInputElement};
use yew::prelude::*;

/// Parses STEP text into metadata, a content-based FileId, and entity tables.
pub(crate) fn parse_step_file_content(
    name: &str,
    text: &str,
) -> Result<
    (
        Metadata,
        FileId,
        Vec<truck_stepio::r#in::Table>,
        StepColorMap,
        StepNameMap,
    ),
    StepError,
> {
    // Fast pre-check on the in-memory buffer: validates the ISO exchange structure header
    // and FILE_SCHEMA before running full AST parsing, avoiding downstream tokenizer crashes
    // on unsupported schemas.
    probe_validate_step_buffer(text)?;

    let mut parsed =
        crate::ruststep::parser::parse(text).map_err(|e| StepError::Parse(e.to_string()))?;

    // Single combined pass: normalises INTERSECTION/BOUNDARY_CURVE → SURFACE_CURVE,
    // and simultaneously collects all data for color, name, and unit extraction.
    let index = ExchangeIndex::build(&mut parsed);
    let color_map = StepColorMap::from_index(&index);
    let name_map = StepNameMap::from_index(&index);
    let units = index.resolved_unit();
    // Drop the index before building step tables to free intermediate HashMap memory.
    drop(index);

    let sections = all_usable_sections(&parsed)?;
    let step_tables: Vec<truck_stepio::r#in::Table> = sections
        .into_iter()
        .map(truck_stepio::r#in::Table::from_data_section)
        .collect();
    let (meta, id) = build_initial_metadata(name, &parsed, &step_tables, text, units)?;
    Ok((meta, id, step_tables, color_map, name_map))
}

fn format_tessellation_status(
    total_triangles: usize,
    skipped_shells: usize,
    warnings: &[String],
) -> String {
    if total_triangles == 0 {
        if !warnings.is_empty() {
            format!(
                "File loaded but no renderable geometry found: {}.",
                warnings.join("; ")
            )
        } else {
            "File loaded but no renderable geometry was found.".to_string()
        }
    } else if skipped_shells > 0 {
        if !warnings.is_empty() {
            format!(
                "Parsed STEP file. {skipped_shells} shell(s) could not be tessellated and were skipped: {}.",
                warnings.join("; ")
            )
        } else {
            format!(
                "Parsed STEP file. {skipped_shells} shell(s) could not be tessellated and were skipped."
            )
        }
    } else {
        "Parsed STEP file successfully.".to_string()
    }
}

/// Job parameters for asynchronous geometry tessellation.
pub(crate) struct TessellationJob {
    pub step_tables: Vec<truck_stepio::r#in::Table>,
    pub color_map: StepColorMap,
    pub name_map: StepNameMap,
    pub file_id: FileId,
    pub meta: Metadata,
    pub generation: u64,
}

/// Spawns the async tessellation pass: tessellates the STEP tables, wraps the
/// result in a [`StepModel`], persists it to the cache and localStorage, then
/// publishes the updated metadata and model to the UI.
pub(crate) fn spawn_tessellation(
    job: TessellationJob,
    states: StateHandles,
    files_index: UseStateHandle<Vec<FileIndexItem>>,
    cache: Rc<RefCell<LruCache>>,
) {
    let TessellationJob {
        step_tables,
        color_map,
        name_map,
        file_id,
        meta,
        generation,
    } = job;
    let base_tolerance = compute_adaptive_tolerance(meta.bounding_box.as_ref());
    let multiplier = states.quality_preset.multiplier();
    let tolerance = (base_tolerance * multiplier).clamp(MIN_TOLERANCE, MAX_TOLERANCE);
    wasm_bindgen_futures::spawn_local(async move {
        let total_shell_count: usize = step_tables.iter().map(|t| t.shell.len()).sum();
        let output =
            extract_render_parts(&step_tables, Some(&color_map), Some(&name_map), tolerance);
        let renderable_parts = output.parts;
        let skipped_shells = output.skipped_shells;
        let warnings = output.warnings;

        if states.is_superseded(generation) {
            return;
        }

        if skipped_shells > 0 && renderable_parts.is_empty() && skipped_shells == total_shell_count
        {
            let fail_msg = if !warnings.is_empty() {
                format!(
                    "Tessellation produced no geometry. Skipped {skipped_shells} shell(s): {}.",
                    warnings.join("; ")
                )
            } else {
                "Tessellation produced no geometry. The file may be too complex or use unsupported geometry.".to_string()
            };
            states.fail_load(fail_msg);
            return;
        }

        let model = build_step_model(file_id.clone(), meta, renderable_parts);
        save_model(&model);

        // Record the file in the history index ONLY after successful tessellation and model persistence.
        add_to_index(
            &files_index,
            FileIndexItem {
                id: file_id.clone(),
                name: model.metadata.header.file_name.clone(),
                entity_count: model.metadata.entity_count,
                time_stamp: model.metadata.header.time_stamp.clone(),
            },
        );

        let total_triangles = model.metadata.triangle_count;
        let model_rc = Rc::new(model);
        {
            let mut cache_ref = cache.borrow_mut();
            cache_ref.insert_rc(file_id.clone(), model_rc.clone());
        }

        if states.is_superseded(generation) {
            return;
        }

        let status_msg = format_tessellation_status(total_triangles, skipped_shells, &warnings);
        states.set_loaded_model(model_rc, file_id, &status_msg);
    });
}

#[hook]
pub(crate) fn use_file_processor(
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

        let next_gen = states.bump_generation();
        states.is_processing.set(true);
        if web_file.size() > MAX_FILE_BYTES {
            states.fail_load(StepError::FileTooLarge {
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
            if states_for_reader.is_superseded(next_gen) {
                return;
            }
            let fail = |err: StepError| {
                if states_for_reader.is_current(next_gen) {
                    states_for_reader.fail_load(err);
                }
            };

            let text = match res {
                Ok(text) => text,
                Err(e) => return fail(StepError::FileRead(e.to_string())),
            };

            let id = crate::common::storage::hash_text_to_id(&text);

            // Fast-path 1: Check synchronous in-memory LruCache / localStorage
            if let Some(model_rc) = cache.borrow_mut().get_or_load(&id, load_model) {
                states_for_reader.set_loaded_model(model_rc, id.clone(), "Loaded from cache");
                promote_in_index(&files_index, &id);
                return;
            }

            // Fast-path 2: Check asynchronous IndexedDB before falling back to full AST parsing & tessellation
            let states_async = states_for_reader.clone();
            let cache_async = cache.clone();
            let files_index_async = files_index.clone();
            let file_id = id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(model) = crate::common::storage::load_model_indexeddb(&file_id).await {
                    if states_async.is_superseded(next_gen) {
                        return;
                    }
                    let model_rc = Rc::new(model);
                    cache_async
                        .borrow_mut()
                        .insert_rc(file_id.clone(), model_rc.clone());
                    states_async.set_loaded_model(model_rc, file_id.clone(), "Loaded from storage");
                    promote_in_index(&files_index_async, &file_id);
                    return;
                }

                if states_async.is_superseded(next_gen) {
                    return;
                }

                let (meta, _id, step_tables, color_map, name_map) =
                    match parse_step_file_content(&name, &text) {
                        Ok(parsed) => parsed,
                        Err(err) => {
                            if states_async.is_current(next_gen) {
                                states_async.fail_load(err);
                            }
                            return;
                        }
                    };

                if states_async.is_superseded(next_gen) {
                    return;
                }

                states_async.metadata.set(Some(meta.clone()));
                states_async.selected_file.set(Some(file_id.clone()));
                states_async.set_result("Tessellating geometry for 3D view...", false);

                spawn_tessellation(
                    TessellationJob {
                        step_tables,
                        color_map,
                        name_map,
                        file_id,
                        meta,
                        generation: next_gen,
                    },
                    states_async,
                    files_index_async,
                    cache_async,
                );
            });
        });
        *states.file_reader.borrow_mut() = Some(reader);
    })
}
