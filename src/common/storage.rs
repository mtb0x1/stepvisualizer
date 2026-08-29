//! localStorage persistence: the recent-files index plus whole models keyed
//! by content hash. Persistence is best-effort — failures are logged as
//! warnings and the app keeps running without stored data.
use crate::{
    apptracing::{AppTracer, AppTracerTrait},
    trace_span,
};
use gloo_storage::{LocalStorage, Storage, errors::StorageError};

use super::types::{FileId, FileIndexItem, StepModel};

use crate::common::constants::{LS_INDEX_KEY, LS_MODEL_KEY_PREFIX};

/// Persist the recent-files index. Failure is logged, not propagated.
pub fn save_index(index: &[FileIndexItem]) {
    trace_span!("save_index");
    if let Err(err) = LocalStorage::set(LS_INDEX_KEY, index) {
        AppTracer::warn(&format!("Failed to save file index to localStorage: {err}"));
    }
}

/// Load the recent-files index; an empty history on first visit or on any
/// storage failure.
pub fn load_index() -> Vec<FileIndexItem> {
    trace_span!("load_index");
    match LocalStorage::get(LS_INDEX_KEY) {
        Ok(index) => index,
        // A missing index is the normal first-visit case, not a failure.
        Err(StorageError::KeyNotFound(_)) => vec![],
        Err(err) => {
            AppTracer::warn(&format!(
                "Failed to load file index from localStorage, starting with an empty history: {err}"
            ));
            vec![]
        }
    }
}

fn model_key(id: &str) -> String {
    format!("{}{}", LS_MODEL_KEY_PREFIX, id)
}

/// Persist a whole model under its id. Models can be large; a quota
/// exhaustion is the expected failure mode and is logged with a hint.
pub fn save_model(model: &StepModel) {
    trace_span!("save_model");
    let key = model_key(&model.id);
    if let Err(err) = LocalStorage::set(key, model) {
        AppTracer::warn(&format!(
            "Failed to save model {} to localStorage (storage quota may be exceeded): {err}",
            model.id
        ));
    }
}

/// Load a previously saved model; `None` when absent or unreadable.
pub fn load_model(id: &str) -> Option<StepModel> {
    trace_span!("load_model");
    let key = model_key(id);
    LocalStorage::get::<StepModel>(key).ok()
}

/// Remove a model's persisted copy. Deleting an absent key is a no-op.
pub fn delete_model(id: &str) {
    trace_span!("delete_model");
    let key = model_key(id);
    LocalStorage::delete(key);
}

/// Remove all persisted models and the file index from localStorage.
pub fn clear_all_storage(items: &[FileIndexItem]) {
    trace_span!("clear_all_storage");
    for item in items {
        delete_model(&item.id);
    }
    save_index(&[]);
}

/// Content-based model identity (16 hex chars) used as the localStorage key.
pub fn hash_text_to_id(text: &str) -> FileId {
    FileId::from_content(text)
}
