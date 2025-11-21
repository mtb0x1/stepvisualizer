use crate::trace_span;
use gloo_storage::{LocalStorage, Storage};
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use super::types::{FileIndexItem, StepModel};

use crate::common::constants::LS_INDEX_KEY;

pub fn save_index(index: &[FileIndexItem]) {
    trace_span!("save_index");
    let _ = LocalStorage::set(LS_INDEX_KEY, index);
}

pub fn load_index() -> Vec<FileIndexItem> {
    trace_span!("load_index");
    LocalStorage::get(LS_INDEX_KEY).unwrap_or_else(|_| vec![])
}

fn model_key(id: &str) -> String {
    trace_span!("model_key");
    format!("stepviz:model:{}", id)
}

pub fn save_model(model: &StepModel) {
    trace_span!("save_model");
    let key = model_key(&model.id);
    let _ = LocalStorage::set(key, model);
}

pub fn load_model(id: &str) -> Option<StepModel> {
    trace_span!("load_model");
    let key = model_key(id);
    LocalStorage::get::<StepModel>(key).ok()
}

pub fn delete_model(id: &str) {
    trace_span!("delete_model");
    let key = model_key(id);
    let _ = LocalStorage::delete(key);
}

pub fn hash_text_to_id(text: &str) -> String {
    trace_span!("hash_text_to_id");
    let mut hasher = DefaultHasher::new();
    std::hash::Hash::hash(&text, &mut hasher);
    format!("{:016x}", hasher.finish())
}
