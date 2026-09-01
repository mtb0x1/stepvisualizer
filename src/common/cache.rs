//! LRU cache over parsed `StepModel`s (backed by persistence storage).
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use super::types::{FileId, StepModel};

/// LRU over parsed models. Stores `Rc<StepModel>` so cache hits return a
/// cheap reference-count clone instead of a full deep-copy of geometry data.
pub struct LruCache {
    capacity: usize,
    order: VecDeque<FileId>,
    map: HashMap<FileId, Rc<StepModel>>,
}

impl LruCache {
    /// New cache holding at most `capacity` models. `capacity == 0` caches
    /// nothing: [`get_or_load`](Self::get_or_load) then falls through to the
    /// backend on every call.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    // Capacity is tiny (CACHE_SIZE = 5), so a linear scan here is cheaper than
    // the pointer bookkeeping a true O(1) linked-list LRU would require.
    fn remove_from_order(&mut self, id: &str) {
        if let Some(pos) = self.order.iter().position(|k| k.as_str() == id) {
            self.order.remove(pos);
        }
    }

    fn touch(&mut self, id: &str) {
        self.remove_from_order(id);
        self.order.push_front(FileId::from(id));
    }

    /// Returns a shared reference to the model under `id`, promoting it to
    /// most-recently-used. No geometry data is copied on a cache hit.
    pub fn get(&mut self, id: &str) -> Option<Rc<StepModel>> {
        let model = self.map.get(id).cloned();
        if model.is_some() {
            self.touch(id);
        }
        model
    }

    /// Memory cache hit, else persistence backend, else `None`.
    ///
    /// Memory is the single in-memory layer; `load` is the persistence
    /// backend (e.g. localStorage/IndexedDB). On a miss we fall through to the
    /// backend, wrap the result in `Rc`, promote into the cache, and return it.
    pub fn get_or_load(
        &mut self,
        id: &str,
        load: impl Fn(&str) -> Option<StepModel>,
    ) -> Option<Rc<StepModel>> {
        if let Some(rc) = self.get(id) {
            return Some(rc);
        }
        let loaded = load(id)?;
        let rc = Rc::new(loaded);
        self.insert_rc(FileId::from(id), rc.clone());
        Some(rc)
    }

    /// Insert a plain model (wraps it in `Rc` internally), then evict
    /// least-recently-used entries beyond capacity.
    pub fn insert(&mut self, id: FileId, model: StepModel) {
        self.insert_rc(id, Rc::new(model));
    }

    /// Insert a pre-wrapped `Rc<StepModel>`, then evict beyond capacity.
    pub fn insert_rc(&mut self, id: FileId, model: Rc<StepModel>) {
        // Capacity 0 means "cache nothing": get_or_load then simply falls
        // through to the persistence backend on every call.
        if self.capacity == 0 {
            return;
        }
        let id_str = id.to_string();
        self.map.insert(id, model);
        self.touch(&id_str);
        // `>` (not `==`) so any map/order desync self-heals on the next
        // insertion instead of growing past capacity forever.
        while self.map.len() > self.capacity {
            match self.order.pop_back() {
                Some(least) => {
                    self.map.remove(&least);
                }
                None => break,
            }
        }
    }

    /// Drop a model from the cache (the persisted copy, if any, remains).
    pub fn remove(&mut self, id: &str) {
        self.remove_from_order(id);
        self.map.remove(id);
    }

    /// Drop everything (persisted copies, if any, remain).
    pub fn clear(&mut self) {
        self.order.clear();
        self.map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::{LengthUnit, Metadata, StepHeader};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    fn create_mock_model(id: &str) -> StepModel {
        StepModel {
            id: FileId::from(id),
            metadata: Metadata {
                header: StepHeader {
                    file_description: "test".to_string(),
                    implementation_level: "2;1".to_string(),
                    file_name: format!("{id}.step"),
                    time_stamp: "2026-09-01T00:00:00".to_string(),
                    author: vec!["Author".to_string()],
                    organization: vec!["Org".to_string()],
                    preprocessor_version: "1.0".to_string(),
                    originating_system: "TestSys".to_string(),
                    authorization: "None".to_string(),
                    file_schema: "AP214".to_string(),
                },
                entity_count: 10,
                bounding_box: None,
                units: Some(LengthUnit::Millimetre),
                vertex_count: 100,
                triangle_count: 50,
                volume: Some(100.0),
                surface_area: Some(250.0),
            },
            render_parts: vec![],
            part_visibility: vec![],
            visibility_generation: 0,
            cached_bounds: None,
        }
    }

    /// Verifies that querying a non-existent key in an empty cache returns None.
    #[wasm_bindgen_test]
    fn cache_empty_miss() {
        let mut cache = LruCache::new(5);
        let res = cache.get("nonexistent");
        assert!(res.is_none());
    }

    /// Verifies that inserting a model into the cache allows subsequent retrieval
    /// returning Some(Rc<StepModel>) with matching metadata and ID.
    #[wasm_bindgen_test]
    fn cache_insert_and_hit() {
        let mut cache = LruCache::new(5);
        let id = FileId::from("model_1");
        let model = create_mock_model("model_1");

        cache.insert(id.clone(), model);

        let cached = cache.get("model_1");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().id, id);
    }

    /// Verifies that multiple cache hits return reference-counted Rc pointers to the
    /// same underlying heap allocation without performing deep clones of geometry data.
    #[wasm_bindgen_test]
    fn cache_rc_pointer_equality() {
        let mut cache = LruCache::new(5);
        let id = FileId::from("model_1");
        cache.insert(id.clone(), create_mock_model("model_1"));

        let rc1 = cache.get("model_1").expect("cache hit 1");
        let rc2 = cache.get("model_1").expect("cache hit 2");

        assert!(Rc::ptr_eq(&rc1, &rc2));
    }

    /// Verifies that removing an entry from the cache deletes it from both the internal
    /// lookup map and eviction order, causing subsequent get calls to return None.
    #[wasm_bindgen_test]
    fn cache_remove_entry() {
        let mut cache = LruCache::new(5);
        cache.insert(FileId::from("model_1"), create_mock_model("model_1"));

        assert!(cache.get("model_1").is_some());
        cache.remove("model_1");
        assert!(cache.get("model_1").is_none());
    }

    /// Verifies that clearing the cache empties all stored models and reset internal order tracking.
    #[wasm_bindgen_test]
    fn cache_clear_all() {
        let mut cache = LruCache::new(5);
        cache.insert(FileId::from("model_1"), create_mock_model("model_1"));
        cache.insert(FileId::from("model_2"), create_mock_model("model_2"));
        cache.insert(FileId::from("model_3"), create_mock_model("model_3"));

        assert_eq!(cache.map.len(), 3);
        assert_eq!(cache.order.len(), 3);

        cache.clear();

        assert!(cache.get("model_1").is_none());
        assert!(cache.get("model_2").is_none());
        assert!(cache.get("model_3").is_none());
        assert_eq!(cache.map.len(), 0);
        assert_eq!(cache.order.len(), 0);
    }

    /// Verifies that inserting entries beyond maximum capacity evicts the oldest
    /// least-recently-used item (A evicted when inserting C with capacity 2).
    #[wasm_bindgen_test]
    fn cache_eviction_at_capacity() {
        let mut cache = LruCache::new(2);
        cache.insert(FileId::from("model_A"), create_mock_model("model_A"));
        cache.insert(FileId::from("model_B"), create_mock_model("model_B"));
        cache.insert(FileId::from("model_C"), create_mock_model("model_C"));

        assert!(cache.get("model_A").is_none());
        assert!(cache.get("model_B").is_some());
        assert!(cache.get("model_C").is_some());
        assert_eq!(cache.map.len(), 2);
    }

    /// Verifies that accessing an entry via get promotes it to most-recently-used,
    /// so the untouched item is evicted on the next insertion beyond capacity.
    #[wasm_bindgen_test]
    fn cache_touch_promotes_mru() {
        let mut cache = LruCache::new(2);
        cache.insert(FileId::from("model_A"), create_mock_model("model_A"));
        cache.insert(FileId::from("model_B"), create_mock_model("model_B"));

        // Touch A to promote it to MRU
        assert!(cache.get("model_A").is_some());

        // Insert C -> B is now LRU and should be evicted
        cache.insert(FileId::from("model_C"), create_mock_model("model_C"));

        assert!(cache.get("model_A").is_some());
        assert!(cache.get("model_B").is_none());
        assert!(cache.get("model_C").is_some());
    }

    /// Verifies that re-inserting an existing key replaces its payload and promotes it
    /// without increasing cache size or prematurely evicting other entries.
    #[wasm_bindgen_test]
    fn cache_reinsert_existing_key() {
        let mut cache = LruCache::new(2);
        cache.insert(FileId::from("model_A"), create_mock_model("model_A"));
        cache.insert(FileId::from("model_B"), create_mock_model("model_B"));

        let mut updated_a = create_mock_model("model_A");
        updated_a.metadata.entity_count = 999;
        cache.insert(FileId::from("model_A"), updated_a);

        assert_eq!(cache.map.len(), 2);
        let a = cache.get("model_A").unwrap();
        assert_eq!(a.metadata.entity_count, 999);
        assert!(cache.get("model_B").is_some());
    }

    /// Verifies that a cache created with zero capacity does not retain any inserted items.
    #[wasm_bindgen_test]
    fn cache_zero_capacity() {
        let mut cache = LruCache::new(0);
        cache.insert(FileId::from("model_A"), create_mock_model("model_A"));

        assert!(cache.get("model_A").is_none());
        assert_eq!(cache.map.len(), 0);
        assert_eq!(cache.order.len(), 0);
    }

    /// Verifies that a cache with capacity 1 immediately evicts the previous item on each new insert.
    #[wasm_bindgen_test]
    fn cache_capacity_one() {
        let mut cache = LruCache::new(1);
        cache.insert(FileId::from("model_A"), create_mock_model("model_A"));
        assert!(cache.get("model_A").is_some());

        cache.insert(FileId::from("model_B"), create_mock_model("model_B"));
        assert!(cache.get("model_A").is_none());
        assert!(cache.get("model_B").is_some());
        assert_eq!(cache.map.len(), 1);
    }
}
