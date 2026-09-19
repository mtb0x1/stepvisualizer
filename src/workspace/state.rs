//! Internal workspace state handles and state transition methods.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gloo::file::callbacks::FileReader;
use smol_str::{SmolStr, format_smolstr};
use yew::prelude::*;

use super::ConfirmAction;
use crate::common::{
    FileId, Metadata, RenderablePart, StepModel, constants::QualityPreset, visible_bounds,
};

/// Grouped state handles owned by the workspace.
///
/// UI-affecting states use [`UseStateHandle`] so components re-render when
/// they update. Concurrency control ([`load_generation`]) and asynchronous
/// readers ([`file_reader`]) use interior mutability (`Cell`/`RefCell`) to
/// avoid phantom UI re-renders and stale closure snapshots.
#[derive(Clone)]
pub(crate) struct StateHandles {
    pub result: UseStateHandle<Option<SmolStr>>,
    pub result_is_error: UseStateHandle<bool>,
    pub metadata: UseStateHandle<Option<Metadata>>,
    pub step_model: UseStateHandle<Option<ModelView>>,
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
        self.file_reader.borrow_mut().take();
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
        self.result.set(Some(format_smolstr!("{err}")));
        self.metadata.set(None);
        self.is_processing.set(false);
    }

    /// Resets the loaded-model UI state: clears selection, metadata, model, and
    /// part visibility. Shared across deselect, delete, and clear-history.
    pub fn clear_model_state(&self) {
        self.selected_file.set(None);
        self.metadata.set(None);
        self.step_model.set(None);
    }

    /// Updates status message and error flag.
    pub fn set_result(&self, msg: impl Into<SmolStr>, is_error: bool) {
        self.result_is_error.set(is_error);
        self.result.set(Some(msg.into()));
    }

    /// Sets the active loaded model across all related state handles.
    pub fn set_loaded_model(&self, mut model_rc: Rc<StepModel>, file_id: FileId, status_msg: &str) {
        if model_rc.part_visibility.len() != model_rc.render_parts.len() {
            let mut model = (*model_rc).clone();
            model.part_visibility = vec![true; model.render_parts.len()];
            model_rc = Rc::new(model);
        }
        self.metadata.set(Some(model_rc.metadata.clone()));

        let generation = model_rc.visibility_generation;
        let view = ModelView { model: model_rc, generation };

        self.step_model.set(Some(view));
        self.selected_file.set(Some(file_id));
        self.set_result(status_msg, false);
        self.is_processing.set(false);
    }
}

/// Constructs a [`StepModel`], computing totals and visible bounds.
pub fn build_step_model(
    id: FileId, metadata: Metadata, render_parts: Vec<RenderablePart>,
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
        audit: crate::common::types::AuditMetadata::default(),
    };
    model.metadata.vertex_count = model.total_vertices();
    model.metadata.triangle_count = model.total_triangles();
    if let Some(bbox) = visible_bounds(&model.render_parts, &model.part_visibility) {
        model.metadata.bounding_box = Some(bbox);
    }
    model
}

#[derive(Clone)]
pub struct ModelView {
    pub model: Rc<StepModel>,
    pub generation: u64,
}

impl PartialEq for ModelView {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Rc::ptr_eq(&self.model, &other.model)
    }
}
