//! Storage management: in-memory LRU caching and IndexedDB durable persistence.
pub mod cache;
pub mod db_schema;
pub mod persistence;

pub use cache::LruCache;
#[allow(unused_imports)]
pub use db_schema::{DB_VERSION, STORE_INDEX, STORE_MODELS, open_db_versioned};
#[allow(unused_imports)]
pub use persistence::{
    clear_all_storage, clear_indexeddb, delete_index_item, delete_model, delete_model_indexeddb,
    hash_text_to_id, load_index_async, load_model_indexeddb, save_index_item, save_model,
    save_model_indexeddb, save_model_json_indexeddb,
};
