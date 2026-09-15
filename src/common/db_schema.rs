//! IndexedDB schema definition, versioning, and migration runner.
//!
//! # Version Strategy
//! [`DB_VERSION`] is set at **build time** from `git rev-list --count HEAD`,
//! making it a monotonically increasing `u32` that never needs manual bumping.
//! The fallback value is `1` for environments without git (e.g. zip downloads).
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

use super::logger;
use crate::common::constants::db_name;
use crate::common::types::{FileId, StepModel};
use rexie::{ObjectStore, Rexie, TransactionMode};

// ---------------------------------------------------------------------------
// Schema version
// ---------------------------------------------------------------------------

/// IndexedDB schema version — injected at build time from `git rev-list --count HEAD`.
/// Monotonically increasing; guarantees forward-only versioning without manual bumps.
/// Falls back to `1` when the env var is absent (non-git build environments).
pub const DB_VERSION: u32 = {
    // `option_env!` is evaluated at compile time; the build.rs script sets this.
    match option_env!("DB_COMMIT_VERSION") {
        Some(s) => {
            // Inline const decimal parse (std::str::parse is not const-stable yet).
            let bytes = s.as_bytes();
            let mut val: u32 = 0;
            let mut i = 0;
            while i < bytes.len() {
                let d = bytes[i] - b'0';
                val = val * 10 + d as u32;
                i += 1;
            }
            if val == 0 { 1 } else { val }
        }
        None => 1,
    }
};

/// The oldest `DB_VERSION` from which automatic dual-DB migration is attempted.
/// DBs older than this are considered too stale and are tombstoned silently.
pub const SCHEMA_MIN_COMPATIBLE_VERSION: u32 = 1;

/// Human-readable audit log of schema versions (index = version number).
pub const SCHEMA_CHANGELOG: &[&str] = &[
    /* v0 (implicit) */
    "Unversioned origin — single 'models' object store, localStorage model fallback, \
     no host prefix in keys.",
    /* v1 */
    "Baseline versioned schema: 'models' store + 'schema_meta' store. \
     Host+port+env prefix in all storage keys and DB name. \
     localStorage model fallback removed; IndexedDB is sole model store.",
];

// ---------------------------------------------------------------------------
// Object store names
// ---------------------------------------------------------------------------

/// Object store holding serialized [`StepModel`] JSON blobs, keyed by [`FileId`].
pub const STORE_MODELS: &str = "models";

/// Single-row metadata store: holds the schema version record for audit purposes.
pub const STORE_SCHEMA_META: &str = "schema_meta";

/// The single key used in [`STORE_SCHEMA_META`].
const META_KEY: &str = "version";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Open (or upgrade) the versioned IndexedDB database.
///
/// - On first open: creates both object stores at [`DB_VERSION`].
/// - On upgrade: the rexie `onupgradeneeded` handler fires automatically.
/// - On version downgrade: the browser rejects the open; returns an `Err`.
///
/// After opening, call [`run_migrations`] if you detect `old_version < DB_VERSION`.
pub async fn open_db_versioned() -> Result<Rexie, rexie::Error> {
    let name = db_name();
    Rexie::builder(&name)
        .version(DB_VERSION)
        .add_object_store(ObjectStore::new(STORE_MODELS))
        .add_object_store(ObjectStore::new(STORE_SCHEMA_META))
        .add_object_store(ObjectStore::new(STORE_INDEX))
        .build()
        .await
}

/// Write (or overwrite) the [`STORE_SCHEMA_META`] version record.
/// Called after every successful migration and on fresh DB creation.
pub async fn write_schema_meta(db: &Rexie) -> Result<(), String> {
    let tx = db
        .transaction(&[STORE_SCHEMA_META], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.store(STORE_SCHEMA_META).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(META_KEY);
    let val = wasm_bindgen::JsValue::from_f64(DB_VERSION as f64);
    store.put(&val, Some(&key)).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Dual-DB tombstone migration
// ---------------------------------------------------------------------------

/// Attempt to migrate all model records from `old_db_name` into `new_db`.
///
/// Follows the **dual-DB tombstone** strategy:
/// 1. Open the old DB read-only at version 1 (minimum; browser uses stored version if higher).
/// 2. Fetch all keys via `get_all_keys`, then fetch each value individually.
/// 3. Deserialize via `serde_json` — missing fields get `#[serde(default)]` values.
/// 4. Re-serialize and write into `new_db`.
/// 5. Skip (log a warning for) any record that fails deserialization.
/// 6. The old DB is **never deleted** — it is simply abandoned after this call.
///
/// Returns the number of records successfully migrated.
pub async fn migrate_models_from(old_db_name: &str, new_db: &Rexie) -> usize {
    let old_db = match Rexie::builder(old_db_name)
        .version(1)
        .add_object_store(ObjectStore::new(STORE_MODELS))
        .build()
        .await
    {
        Ok(db) => db,
        Err(e) => {
            logger::warn(&format!(
                "[db_schema] Cannot open old DB '{old_db_name}' for migration: {e}"
            ));
            return 0;
        }
    };

    // Collect all keys from the old store.
    let keys: Vec<wasm_bindgen::JsValue> = {
        let tx = match old_db.transaction(&[STORE_MODELS], TransactionMode::ReadOnly) {
            Ok(t) => t,
            Err(e) => {
                logger::warn(&format!("[db_schema] Migration read tx failed: {e}"));
                return 0;
            }
        };
        let store = match tx.store(STORE_MODELS) {
            Ok(s) => s,
            Err(e) => {
                logger::warn(&format!("[db_schema] Migration store access failed: {e}"));
                return 0;
            }
        };
        match store.get_all_keys(None, None).await {
            Ok(k) => k,
            Err(e) => {
                logger::warn(&format!("[db_schema] Migration get_all_keys failed: {e}"));
                return 0;
            }
        }
    };

    if keys.is_empty() {
        return 0;
    }

    let mut migrated = 0usize;

    for key_js in keys {
        let id_str = match key_js.as_string() {
            Some(s) => s,
            None => continue,
        };

        // Fetch the individual value from the old DB.
        let json: String = {
            let tx = match old_db.transaction(&[STORE_MODELS], TransactionMode::ReadOnly) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let store = match tx.store(STORE_MODELS) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let val_key = wasm_bindgen::JsValue::from_str(&id_str);
            let val_js = match store.get(val_key).await {
                Ok(Some(v)) => v,
                _ => continue,
            };
            match val_js.as_string() {
                Some(s) => s,
                None => continue,
            }
        };

        // Deserialize with serde — missing fields in old schema get their Default values.
        let model: StepModel = match serde_json::from_str(&json) {
            Ok(m) => m,
            Err(e) => {
                logger::warn(&format!(
                    "[db_schema] Skipping '{id_str}' (deserialize failed): {e}"
                ));
                continue;
            }
        };

        let new_json = match serde_json::to_string(&model) {
            Ok(j) => j,
            Err(e) => {
                logger::warn(&format!(
                    "[db_schema] Skipping '{id_str}' (serialize failed): {e}"
                ));
                continue;
            }
        };

        // Write to new DB.
        if let Err(e) = save_model_json_to_db(new_db, &id_str, &new_json).await {
            logger::warn(&format!(
                "[db_schema] Could not write '{id_str}' to new DB: {e}"
            ));
        } else {
            migrated += 1;
        }
    }

    logger::warn(&format!(
        "[db_schema] Migration from '{old_db_name}': {migrated} record(s) migrated."
    ));
    migrated
}

/// Run schema migrations for the upgrade path `old_version → DB_VERSION`.
///
/// `new_db` is the freshly opened, upgraded DB at [`DB_VERSION`].
pub async fn run_migrations(old_version: u32, new_version: u32, new_db: &Rexie) {
    logger::warn(&format!(
        "[db_schema] Running migrations: v{old_version} -> v{new_version}"
    ));

    if old_version == 0 {
        // v0 → v1+: legacy DBs used env-prefix only (no host) in their DB name.
        // Try all known legacy candidates; migrate models from whichever has records.
        let legacy_candidates: &[&str] = &[
            "stepvisualizer_db",
            "stepvisualizer_db_testing",
            "stepvisualizer_db_production",
        ];
        for &candidate in legacy_candidates {
            let n = migrate_models_from(candidate, new_db).await;
            if n > 0 {
                logger::warn(&format!(
                    "[db_schema] Migrated {n} records from legacy DB '{candidate}'."
                ));
            }
        }

        // Attempt to migrate the file index from localStorage (best-effort).
        // The old localStorage key format was "<env_prefix>stepvisualizer:index".
        // We try the three known variants; silently skip if not found.
        let index_key_candidates: &[&str] = &[
            "stepvisualizer:index",
            "testing:stepvisualizer:index",
            "production:stepvisualizer:index",
        ];
        'index_migration: for &ls_key in index_key_candidates {
            if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
                && let Ok(Some(json)) = storage.get_item(ls_key)
                && let Ok(items) = serde_json::from_str::<Vec<crate::common::types::FileIndexItem>>(&json)
                && !items.is_empty()
            {
                if let Err(e) = save_index_to_db(new_db, &items).await {
                    logger::warn(&format!(
                        "[db_schema] Index migration from localStorage failed: {e}"
                    ));
                } else {
                    logger::warn(&format!(
                        "[db_schema] Migrated {} index entries from localStorage key '{ls_key}'.",
                        items.len()
                    ));
                }
                break 'index_migration;
            }
        }
    }

    if let Err(e) = write_schema_meta(new_db).await {
        logger::warn(&format!("[db_schema] Could not write schema_meta: {e}"));
    }
}

// ---------------------------------------------------------------------------
// Low-level helpers (used by storage.rs)
// ---------------------------------------------------------------------------

/// Load a [`StepModel`] by its [`FileId`] from the given open DB.
pub async fn load_model_from_db(db: &Rexie, id: &FileId) -> Option<StepModel> {
    let tx = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadOnly)
        .ok()?;
    let store = tx.store(STORE_MODELS).ok()?;
    let key = wasm_bindgen::JsValue::from_str(id.as_str());
    let val = store.get(key).await.ok()??;
    let json = val.as_string()?;
    serde_json::from_str(&json).ok()
}

/// Save a serialized model JSON blob to the given open DB.
pub async fn save_model_json_to_db(db: &Rexie, id: &str, json: &str) -> Result<(), String> {
    let tx = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.store(STORE_MODELS).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(id);
    let val = wasm_bindgen::JsValue::from_str(json);
    store.put(&val, Some(&key)).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a model record from the given open DB.
pub async fn delete_model_from_db(db: &Rexie, id: &str) -> Result<(), String> {
    let tx = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.store(STORE_MODELS).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(id);
    store.delete(key).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear all model records from the given open DB.
pub async fn clear_models_in_db(db: &Rexie) -> Result<(), String> {
    let tx = db
        .transaction(&[STORE_MODELS], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.store(STORE_MODELS).map_err(|e| e.to_string())?;
    store.clear().await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Index store helpers (FileIndexItem[])
// ---------------------------------------------------------------------------

/// Object store holding the serialized recent-files index (`Vec<FileIndexItem>`),
/// keyed by a single well-known key so the whole list is one IDB record.
pub const STORE_INDEX: &str = "file_index";

/// The single key under which the index array is stored in [`STORE_INDEX`].
const INDEX_KEY: &str = "index";

/// Load the recent-files index from IndexedDB.
/// Returns an empty vec on first visit or if the record does not yet exist.
pub async fn load_index_from_db(db: &Rexie) -> Vec<crate::common::types::FileIndexItem> {
    let tx = match db.transaction(&[STORE_INDEX], TransactionMode::ReadOnly) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let store = match tx.store(STORE_INDEX) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let key = wasm_bindgen::JsValue::from_str(INDEX_KEY);
    let val = match store.get(key).await {
        Ok(Some(v)) => v,
        _ => return vec![],
    };
    let json = match val.as_string() {
        Some(s) => s,
        None => return vec![],
    };
    serde_json::from_str(&json).unwrap_or_default()
}

/// Persist the recent-files index to IndexedDB (replaces the whole array in one write).
pub async fn save_index_to_db(
    db: &Rexie,
    index: &[crate::common::types::FileIndexItem],
) -> Result<(), String> {
    let json = serde_json::to_string(index).map_err(|e| e.to_string())?;
    let tx = db
        .transaction(&[STORE_INDEX], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.store(STORE_INDEX).map_err(|e| e.to_string())?;
    let key = wasm_bindgen::JsValue::from_str(INDEX_KEY);
    let val = wasm_bindgen::JsValue::from_str(&json);
    store.put(&val, Some(&key)).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear the recent-files index in IndexedDB.
pub async fn clear_index_in_db(db: &Rexie) -> Result<(), String> {
    let tx = db
        .transaction(&[STORE_INDEX], TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.store(STORE_INDEX).map_err(|e| e.to_string())?;
    store.clear().await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}
