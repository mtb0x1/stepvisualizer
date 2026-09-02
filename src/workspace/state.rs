//! Internal workspace state handles and state transition methods.
use crate::common::constants::QualityPreset;
use crate::common::{FileId, Metadata, RenderablePart, StepModel, visible_bounds};
use gloo::file::callbacks::FileReader;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use yew::prelude::*;

use super::ConfirmAction;

/// Grouped state handles owned by the workspace.
///
/// UI-affecting states use [`UseStateHandle`] so components re-render when
/// they update. Concurrency control ([`load_generation`]) and asynchronous
/// readers ([`file_reader`]) use interior mutability (`Cell`/`RefCell`) to
/// avoid phantom UI re-renders and stale closure snapshots.
#[derive(Clone)]
pub(crate) struct StateHandles {
    pub result: UseStateHandle<Option<String>>,
    pub result_is_error: UseStateHandle<bool>,
    pub metadata: UseStateHandle<Option<Metadata>>,
    pub step_model: UseStateHandle<Option<Rc<StepModel>>>,
    pub part_visibility: UseStateHandle<Vec<bool>>,
    pub selected_file: UseStateHandle<Option<FileId>>,
    pub is_processing: UseStateHandle<bool>,
    pub pending_confirm: UseStateHandle<Option<ConfirmAction>>,
    pub quality_preset: UseStateHandle<QualityPreset>,
    pub load_generation: Rc<Cell<u64>>,
    pub file_reader: Rc<RefCell<Option<FileReader>>>,
}

impl StateHandles {
    /// Increments and returns the new load generation cancellation counter.
    pub fn bump_generation(&self) -> u64 {
        let next = self.load_generation.get() + 1;
        self.load_generation.set(next);
        next
    }

    /// Returns `true` if a newer load has superseded the specified `generation`.
    pub fn is_superseded(&self, generation: u64) -> bool {
        self.load_generation.get() > generation
    }

    /// Returns `true` if `generation` is still the current active load.
    pub fn is_current(&self, generation: u64) -> bool {
        self.load_generation.get() <= generation
    }

    /// Resets the UI after a failed load: surfaces `err` as an error, clears stale
    /// metadata, and drops the processing flag.
    pub fn fail_load(&self, err: impl std::fmt::Display) {
        self.result_is_error.set(true);
        self.result.set(Some(err.to_string()));
        self.metadata.set(None);
        self.is_processing.set(false);
    }

    /// Resets the loaded-model UI state: clears selection, metadata, model, and
    /// part visibility. Shared across deselect, delete, and clear-history.
    pub fn clear_model_state(&self) {
        self.selected_file.set(None);
        self.metadata.set(None);
        self.step_model.set(None);
        self.part_visibility.set(Vec::new());
    }

    /// Updates status message and error flag.
    pub fn set_result(&self, msg: impl Into<String>, is_error: bool) {
        self.result_is_error.set(is_error);
        self.result.set(Some(msg.into()));
    }

    /// Sets the active loaded model across all related state handles.
    pub fn set_loaded_model(&self, model: Rc<StepModel>, file_id: FileId, status_msg: &str) {
        let part_visibility = if model.part_visibility.len() == model.render_parts.len() {
            model.part_visibility.clone()
        } else {
            vec![true; model.render_parts.len()]
        };
        self.metadata.set(Some(model.metadata.clone()));
        self.step_model.set(Some(model));
        self.part_visibility.set(part_visibility);
        self.selected_file.set(Some(file_id));
        self.set_result(status_msg, false);
        self.is_processing.set(false);
    }
}

/// Constructs a [`StepModel`], computing totals and visible bounds.
pub(crate) fn build_step_model(
    id: FileId,
    metadata: Metadata,
    render_parts: Vec<RenderablePart>,
) -> StepModel {
    let part_count = render_parts.len();
    let part_visibility = vec![true; part_count];
    let mut model = StepModel {
        id,
        metadata,
        render_parts,
        part_visibility: part_visibility.clone(),
        visibility_generation: 0,
        cached_bounds: None,
    };
    model.metadata.vertex_count = model.total_vertices();
    model.metadata.triangle_count = model.total_triangles();
    if let Some(bbox) = visible_bounds(&model.render_parts, &part_visibility) {
        model.metadata.bounding_box = Some(bbox);
    }
    model
}
