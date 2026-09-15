//! Persistence layer: all data stored in IndexedDB only.
//!
//! - [`FileIndexItem`] recent-files index → [`STORE_INDEX`] in IndexedDB
//! - [`StepModel`] blobs → [`STORE_MODELS`] in IndexedDB
//!
//! There is no localStorage usage. Pre-refactor `localStorage` keys are silently
//! abandoned (they will remain in the browser until the user clears site data).
//!
//! Persistence is best-effort — failures are logged as warnings and the app
//! continues running with whatever state it already has in memory.
use super::logger;
use crate::trace_span;
use wasm_bindgen_futures::spawn_local;

use super::db_schema::{
    clear_index_in_db, clear_models_in_db, delete_index_item_from_db, delete_model_from_db,
    load_index_from_db, load_model_from_db, open_db_versioned, save_index_item_to_db,
    save_model_json_to_db,
};
use super::types::{FileId, FileIndexItem, StepModel};

/// Persist a single recent-files index item to IndexedDB (fire-and-forget).
/// The write is async; the in-memory state in Yew is already updated by the caller.
pub fn save_index_item(item: &FileIndexItem) {
    trace_span!("save_index_item");
    let item_owned = item.clone();
    spawn_local(async move {
        match open_db_versioned().await {
            Ok(db) => {
                if let Err(e) = save_index_item_to_db(&db, &item_owned).await {
                    logger::warn(&format!("Failed to save file index item to IndexedDB: {e}"));
                }
            }
            Err(e) => logger::warn(&format!("Failed to open DB for index item save: {e}")),
        }
    });
}

/// Remove a single recent-files index item from IndexedDB (fire-and-forget).
pub fn delete_index_item(id: &str) {
    trace_span!("delete_index_item");
    let id_string = id.to_string();
    spawn_local(async move {
        match open_db_versioned().await {
            Ok(db) => {
                if let Err(e) = delete_index_item_from_db(&db, &id_string).await {
                    logger::warn(&format!(
                        "Failed to delete file index item from IndexedDB: {e}"
                    ));
                }
            }
            Err(e) => logger::warn(&format!("Failed to open DB for index item delete: {e}")),
        }
    });
}

/// Load the recent-files index from IndexedDB asynchronously.
/// Returns an empty vec on first visit or on any storage failure.
pub async fn load_index_async() -> Vec<FileIndexItem> {
    trace_span!("load_index_async");
    match open_db_versioned().await {
        Ok(db) => load_index_from_db(&db).await,
        Err(e) => {
            logger::warn(&format!(
                "Failed to open DB for index load, starting with empty history: {e}"
            ));
            vec![]
        }
    }
}

/// Persist a serialized model JSON blob asynchronously to IndexedDB.
pub async fn save_model_json_indexeddb(id: &str, json: &str) -> Result<(), String> {
    let db = open_db_versioned().await.map_err(|e| e.to_string())?;
    save_model_json_to_db(&db, id, json).await
}

/// Persist a whole model asynchronously to IndexedDB.
#[allow(dead_code)]
pub async fn save_model_indexeddb(model: &StepModel) -> Result<(), String> {
    let json = serde_json::to_string(model).map_err(|e| e.to_string())?;
    save_model_json_indexeddb(&model.id, &json).await
}

/// Load a model asynchronously from IndexedDB.
pub async fn load_model_indexeddb(id: &str) -> Option<StepModel> {
    let db = open_db_versioned().await.ok()?;
    load_model_from_db(&db, &FileId::from(id)).await
}

/// Remove a model from IndexedDB.
pub async fn delete_model_indexeddb(id: &str) -> Result<(), String> {
    let db = open_db_versioned().await.map_err(|e| e.to_string())?;
    delete_model_from_db(&db, id).await
}

/// Clear all models from IndexedDB.
pub async fn clear_indexeddb() -> Result<(), String> {
    let db = open_db_versioned().await.map_err(|e| e.to_string())?;
    clear_models_in_db(&db).await
}

/// Persist a whole model (fire-and-forget async IndexedDB write).
pub fn save_model(model: &StepModel) {
    trace_span!("save_model");
    let id = model.id.clone();
    let json = match serde_json::to_string(model) {
        Ok(j) => j,
        Err(e) => {
            logger::warn(&format!("Failed to serialize model: {e}"));
            return;
        }
    };
    spawn_local(async move {
        if let Err(e) = save_model_json_indexeddb(&id, &json).await {
            logger::warn(&format!("Failed to save model to IndexedDB: {e}"));
        }
    });
}

/// Remove a model's persisted copy from IndexedDB (fire-and-forget).
pub fn delete_model(id: &str) {
    trace_span!("delete_model");
    let id_string = id.to_string();
    spawn_local(async move {
        let _ = delete_model_indexeddb(&id_string).await;
    });
}

/// Remove all persisted models and the file index from IndexedDB (fire-and-forget).
pub fn clear_all_storage(_items: &[FileIndexItem]) {
    trace_span!("clear_all_storage");
    spawn_local(async move {
        match open_db_versioned().await {
            Ok(db) => {
                let _ = clear_models_in_db(&db).await;
                let _ = clear_index_in_db(&db).await;
            }
            Err(e) => logger::warn(&format!("Failed to open DB for clear: {e}")),
        }
    });
}

/// Content-based model identity (16 hex chars) used as the IndexedDB key.
pub fn hash_text_to_id(text: &str) -> FileId {
    FileId::from_content(text)
}
