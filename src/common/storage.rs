//! Hybrid persistence: recent-files index in localStorage, full models in IndexedDB.
//! Persistence is best-effort — failures are logged as warnings and the app keeps running.
use crate::{
    apptracing::{AppTracer, AppTracerTrait},
    trace_span,
};
use gloo_storage::{LocalStorage, Storage, errors::StorageError};
use rexie::{ObjectStore, Rexie, TransactionMode};
use wasm_bindgen_futures::spawn_local;

use super::types::{FileId, FileIndexItem, StepModel};
use crate::common::constants::{LS_INDEX_KEY, LS_MODEL_KEY_PREFIX};

const DB_NAME: &str = "stepviz_db";
const STORE_MODELS: &str = "models";

/// Open or initialize the IndexedDB database instance.
pub async fn open_db() -> Result<Rexie, rexie::Error> {
    Rexie::builder(DB_NAME)
        .version(1)
        .add_object_store(ObjectStore::new(STORE_MODELS))
        .build()
        .await
}

/// Persist the recent-files index. Failure is logged, not propagated.
pub fn save_index(index: &[FileIndexItem]) {
    trace_span!("save_index");
    if let Err(err) = LocalStorage::set(LS_INDEX_KEY, index) {
        AppTracer::warn(&format!("Failed to save file index to localStorage: {err}"));
    }
}

/// Load the recent-files index; an empty history on first visit or on any storage failure.
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

/// Persist a whole model asynchronously to IndexedDB.
pub async fn save_model_indexeddb(model: &StepModel) -> Result<(), String> {
    let db = open_db().await.map_err(|e| e.to_string())?;
    let transaction = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = transaction.store(STORE_MODELS).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(model).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(&model.id);
    let val = wasm_bindgen::JsValue::from_str(&json);
    store
        .put(&val, Some(&key))
        .await
        .map_err(|e| e.to_string())?;
    transaction.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Load a model asynchronously from IndexedDB.
pub async fn load_model_indexeddb(id: &str) -> Option<StepModel> {
    let db = open_db().await.ok()?;
    let transaction = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadOnly)
        .ok()?;
    let store = transaction.store(STORE_MODELS).ok()?;
    let key = wasm_bindgen::JsValue::from_str(id);
    let val = store.get(key).await.ok()??;
    let json = val.as_string()?;
    serde_json::from_str(&json).ok()
}

/// Remove a model from IndexedDB.
pub async fn delete_model_indexeddb(id: &str) -> Result<(), String> {
    let db = open_db().await.map_err(|e| e.to_string())?;
    let transaction = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = transaction.store(STORE_MODELS).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(id);
    store.delete(key).await.map_err(|e| e.to_string())?;
    transaction.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear all models from IndexedDB.
pub async fn clear_indexeddb() -> Result<(), String> {
    let db = open_db().await.map_err(|e| e.to_string())?;
    let transaction = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = transaction.store(STORE_MODELS).map_err(|e| e.to_string())?;
    store.clear().await.map_err(|e| e.to_string())?;
    transaction.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Persist a whole model under its id. Spawns async IndexedDB write and writes to localStorage if small.
pub fn save_model(model: &StepModel) {
    trace_span!("save_model");
    let model_clone = model.clone();
    spawn_local(async move {
        if let Err(e) = save_model_indexeddb(&model_clone).await {
            AppTracer::warn(&format!("Failed to save model to IndexedDB: {e}"));
        }
    });

    let key = model_key(&model.id);
    let _ = LocalStorage::set(key, model);
}

/// Load a previously saved model from localStorage (sync fallback).
pub fn load_model(id: &str) -> Option<StepModel> {
    trace_span!("load_model");
    let key = model_key(id);
    LocalStorage::get::<StepModel>(key).ok()
}

/// Remove a model's persisted copy from both IndexedDB and localStorage.
pub fn delete_model(id: &str) {
    trace_span!("delete_model");
    let id_string = id.to_string();
    spawn_local(async move {
        let _ = delete_model_indexeddb(&id_string).await;
    });
    let key = model_key(id);
    LocalStorage::delete(key);
}

/// Remove all persisted models and the file index from localStorage and IndexedDB.
pub fn clear_all_storage(items: &[FileIndexItem]) {
    trace_span!("clear_all_storage");
    spawn_local(async move {
        let _ = clear_indexeddb().await;
    });

    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let count = storage.length().unwrap_or(0);
        let mut keys_to_delete = Vec::new();
        for i in 0..count {
            if let Ok(Some(key)) = storage.key(i)
                && (key.starts_with(LS_MODEL_KEY_PREFIX) || key == LS_INDEX_KEY)
            {
                keys_to_delete.push(key);
            }
        }
        for key in keys_to_delete {
            LocalStorage::delete(key);
        }
    } else {
        for item in items {
            delete_model(&item.id);
        }
        save_index(&[]);
    }
}

/// Content-based model identity (16 hex chars) used as the localStorage key.
pub fn hash_text_to_id(text: &str) -> FileId {
    FileId::from_content(text)
}
