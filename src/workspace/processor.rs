//! File upload handling, STEP parsing pipeline, and async tessellation.
use crate::common::constants::{
    MAX_FILE_BYTES, MAX_TOLERANCE, MIN_TOLERANCE, compute_adaptive_tolerance,
};
use crate::common::utils::input_file;
use crate::common::{
    FileId, FileIndexItem, LruCache, Metadata, all_usable_sections, build_initial_metadata,
    extract_render_parts, load_model, save_model,
};
use crate::error::StepVizError;
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
) -> Result<(Metadata, FileId, Vec<truck_stepio::r#in::Table>), StepVizError> {
    let parsed = ruststep::parser::parse(text).map_err(|e| StepVizError::Parse(e.to_string()))?;
    let sections = all_usable_sections(&parsed)?;
    let step_tables: Vec<truck_stepio::r#in::Table> = sections
        .into_iter()
        .map(truck_stepio::r#in::Table::from_data_section)
        .collect();
    let (meta, id) = build_initial_metadata(name, &parsed, &step_tables, text)?;
    Ok((meta, id, step_tables))
}

fn format_tessellation_status(total_triangles: usize, skipped_shells: usize) -> String {
    if total_triangles == 0 {
        "File loaded but no renderable geometry was found.".to_string()
    } else if skipped_shells > 0 {
        format!(
            "Parsed STEP file. {skipped_shells} shell(s) could not be tessellated and were skipped."
        )
    } else {
        "Parsed STEP file successfully.".to_string()
    }
}

/// Spawns the async tessellation pass: tessellates the STEP tables, wraps the
/// result in a [`StepModel`], persists it to the cache and localStorage, then
/// publishes the updated metadata and model to the UI.
pub(crate) fn spawn_tessellation(
    step_tables: Vec<truck_stepio::r#in::Table>,
    file_id: FileId,
    meta: Metadata,
    states: StateHandles,
    cache: Rc<RefCell<LruCache>>,
    generation: u64,
) {
    let base_tolerance = compute_adaptive_tolerance(meta.bounding_box.as_ref());
    let multiplier = states.quality_preset.multiplier();
    let tolerance = (base_tolerance * multiplier).clamp(MIN_TOLERANCE, MAX_TOLERANCE);
    wasm_bindgen_futures::spawn_local(async move {
        let total_shell_count: usize = step_tables.iter().map(|t| t.shell.len()).sum();
        let (renderable_parts, skipped_shells) = extract_render_parts(&step_tables, tolerance);

        if states.is_superseded(generation) {
            return;
        }

        if skipped_shells > 0 && renderable_parts.is_empty() && skipped_shells == total_shell_count
        {
            states.fail_load(
                "Tessellation produced no geometry. The file may be too complex or use unsupported geometry.",
            );
            return;
        }

        let model = build_step_model(file_id.clone(), meta, renderable_parts);
        save_model(&model);

        let total_triangles = model.metadata.triangle_count;
        let model_rc = Rc::new(model);
        {
            let mut cache_ref = cache.borrow_mut();
            cache_ref.insert_rc(file_id.clone(), model_rc.clone());
        }

        if states.is_superseded(generation) {
            return;
        }

        let status_msg = format_tessellation_status(total_triangles, skipped_shells);
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
            if states_for_reader.is_superseded(next_gen) {
                return;
            }
            let fail = |err: StepVizError| {
                if states_for_reader.is_current(next_gen) {
                    states_for_reader.fail_load(err);
                }
            };

            let text = match res {
                Ok(text) => text,
                Err(e) => return fail(StepVizError::FileRead(e.to_string())),
            };

            let (meta, id, step_tables) = match parse_step_file_content(&name, &text) {
                Ok(parsed) => parsed,
                Err(err) => return fail(err),
            };

            if let Some(model_rc) = cache.borrow_mut().get_or_load(&id, load_model) {
                states_for_reader.set_loaded_model(model_rc, id.clone(), "Loaded from cache");
                promote_in_index(&files_index, &id);
                return;
            }

            states_for_reader.metadata.set(Some(meta.clone()));
            states_for_reader.selected_file.set(Some(id.clone()));
            states_for_reader.set_result("Tessellating geometry for 3D view...", false);

            spawn_tessellation(
                step_tables,
                id.clone(),
                meta.clone(),
                states_for_reader.clone(),
                cache.clone(),
                next_gen,
            );

            // Record the file in the history index (most recent first).
            add_to_index(
                &files_index,
                FileIndexItem {
                    id,
                    name: meta.header.file_name,
                    entity_count: meta.entity_count,
                    time_stamp: meta.header.time_stamp,
                },
            );
        });
        *states.file_reader.borrow_mut() = Some(reader);
    })
}
