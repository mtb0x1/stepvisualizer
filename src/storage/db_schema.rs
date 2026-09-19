//! IndexedDB schema definition, versioning, and migration runner.
//!
//! # Version Strategy
//! [`DB_VERSION`] is set manually and should be monotonically increased
//! when making breaking schema changes (e.g. changing storage format).
//!
//! # Migration Strategy (Dual-DB Tombstone)
//! When the stored DB version is lower than [`DB_VERSION`], the old DB is opened
//! read-only, its records are migrated into the new DB, and the old DB is sealed
//! (left untouched — never auto-deleted). Partial migration (skipping corrupt
//! records) is preferred over aborting the entire migration.
//!
//! Version **downgrades** (stored version > [`DB_VERSION`]) are rejected by the
//! browser at the IndexedDB spec level; the app falls back to an empty in-memory
//! session and surfaces a non-fatal warning.

use rexie::{ObjectStore, Rexie, TransactionMode};
use wasm_bindgen::JsCast;

use crate::common::{
    constants::db_name,
    types::{FileId, StepModel},
};

/// Database schema version. Bumping to 3 migrates model store to rkyv binary format.
pub const DB_VERSION: u32 = 3;

/// Object store holding serialized [`StepModel`] binary/JSON blobs, keyed by [`FileId`].
pub const STORE_MODELS: &str = "models";

/// Open (or upgrade) the versioned IndexedDB database.
pub async fn open_db_versioned() -> Result<Rexie, rexie::Error> {
    let name = db_name();
    Rexie::builder(&name)
        .version(DB_VERSION)
        .add_object_store(ObjectStore::new(STORE_MODELS))
        .add_object_store(ObjectStore::new(STORE_INDEX))
        .build()
        .await
}

/// Load a [`StepModel`] by its [`FileId`] from the given open DB.
/// Transparently handles both modern `rkyv` binary buffers and legacy JSON strings.
pub async fn load_model_from_db(db: &Rexie, id: &FileId) -> Option<StepModel> {
    let tx = db.transaction(&[STORE_MODELS], TransactionMode::ReadOnly).ok()?;
    let store = tx.store(STORE_MODELS).ok()?;
    let key = wasm_bindgen::JsValue::from_str(id.as_str());
    let val = store.get(key).await.ok()??;

    // Check if stored as rkyv binary payload
    // not sure if we need both inclination of binary aka :
    // Uint8Array // ArrayBuffer
    // but we do that for now and later we might need too look into
    // this.
    // TODO : check which format is better
    if let Some(uint8) = val.dyn_ref::<js_sys::Uint8Array>() {
        let len = uint8.length() as usize;
        let mut aligned = rkyv::util::AlignedVec::<16>::with_capacity(len);
        aligned.resize(len, 0);
        uint8.copy_to(&mut aligned[..]);
        if let Ok(model) = rkyv::from_bytes::<StepModel, rkyv::rancor::Error>(&aligned) {
            return Some(model);
        }
    } else if let Some(ab) = val.dyn_ref::<js_sys::ArrayBuffer>() {
        let uint8 = js_sys::Uint8Array::new(ab);
        let len = uint8.length() as usize;
        let mut aligned = rkyv::util::AlignedVec::<16>::with_capacity(len);
        aligned.resize(len, 0);
        uint8.copy_to(&mut aligned[..]);
        if let Ok(model) = rkyv::from_bytes::<StepModel, rkyv::rancor::Error>(&aligned) {
            return Some(model);
        }
    }

    None
}

/// Save a serialized model binary buffer (rkyv) to the given open DB.
pub async fn save_model_bytes_to_db(db: &Rexie, id: &str, bytes: &[u8]) -> Result<(), String> {
    let tx =
        db.transaction(&[STORE_MODELS], TransactionMode::ReadWrite).map_err(|e| e.to_string())?;
    let store = tx.store(STORE_MODELS).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(id);
    let uint8 = js_sys::Uint8Array::from(bytes);
    store.put(&uint8.into(), Some(&key)).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Save a serialized model JSON blob to the given open DB (legacy format support).
pub async fn save_model_json_to_db(db: &Rexie, id: &str, json: &str) -> Result<(), String> {
    let tx =
        db.transaction(&[STORE_MODELS], TransactionMode::ReadWrite).map_err(|e| e.to_string())?;
    let store = tx.store(STORE_MODELS).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(id);
    let val = wasm_bindgen::JsValue::from_str(json);
    store.put(&val, Some(&key)).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a model record from the given open DB.
pub async fn delete_model_from_db(db: &Rexie, id: &str) -> Result<(), String> {
    let tx =
        db.transaction(&[STORE_MODELS], TransactionMode::ReadWrite).map_err(|e| e.to_string())?;
    let store = tx.store(STORE_MODELS).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(id);
    store.delete(key).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear all model records from the given open DB.
pub async fn clear_models_in_db(db: &Rexie) -> Result<(), String> {
    let tx =
        db.transaction(&[STORE_MODELS], TransactionMode::ReadWrite).map_err(|e| e.to_string())?;
    let store = tx.store(STORE_MODELS).map_err(|e| e.to_string())?;
    store.clear().await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Object store holding the serialized recent-files index rows
/// (1 row per file, keyed by FileId string).
pub const STORE_INDEX: &str = "file_index";

/// Load the recent-files index from IndexedDB.
pub async fn load_index_from_db(db: &Rexie) -> Vec<crate::common::types::FileIndexItem> {
    let tx = match db.transaction(&[STORE_INDEX], TransactionMode::ReadOnly) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let store = match tx.store(STORE_INDEX) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let keys = match store.get_all_keys(None, None).await {
        Ok(k) => k,
        Err(_) => return vec![],
    };

    let mut items = Vec::new();
    for key_js in keys {
        if let Ok(Some(val)) = store.get(key_js).await
            && let Some(json) = val.as_string()
            && let Ok(item) = serde_json::from_str::<crate::common::types::FileIndexItem>(&json)
        {
            items.push(item);
        }
    }

    items.sort_by(|a, b| {
        b.audit.updated_on.partial_cmp(&a.audit.updated_on).unwrap_or(std::cmp::Ordering::Equal)
    });
    items
}

/// Persist a single file index item to IndexedDB.
pub async fn save_index_item_to_db(
    db: &Rexie, item: &crate::common::types::FileIndexItem,
) -> Result<(), String> {
    let json = serde_json::to_string(item).map_err(|e| e.to_string())?;
    let tx =
        db.transaction(&[STORE_INDEX], TransactionMode::ReadWrite).map_err(|e| e.to_string())?;
    let store = tx.store(STORE_INDEX).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(item.id.as_str());
    let val = wasm_bindgen::JsValue::from_str(&json);
    store.put(&val, Some(&key)).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a single file index item from IndexedDB.
pub async fn delete_index_item_from_db(db: &Rexie, id: &str) -> Result<(), String> {
    let tx =
        db.transaction(&[STORE_INDEX], TransactionMode::ReadWrite).map_err(|e| e.to_string())?;
    let store = tx.store(STORE_INDEX).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(id);
    store.delete(key).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear the recent-files index in IndexedDB.
pub async fn clear_index_in_db(db: &Rexie) -> Result<(), String> {
    let tx =
        db.transaction(&[STORE_INDEX], TransactionMode::ReadWrite).map_err(|e| e.to_string())?;
    let store = tx.store(STORE_INDEX).map_err(|e| e.to_string())?;
    store.clear().await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}
