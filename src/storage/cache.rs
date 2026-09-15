//! LRU cache over parsed `StepModel`s (backed by persistence storage).
use hashbrown::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::common::types::{FileId, StepModel};

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

    /// Number of items currently stored in cache.
    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the cache contains no items.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    // Capacity is tiny (CACHE_SIZE = 5), so a linear scan here is cheaper than
    // the pointer bookkeeping a true O(1) linked-list LRU would require.
    fn remove_from_order(&mut self, id: &str) -> Option<FileId> {
        if let Some(pos) = self.order.iter().position(|k| k.as_str() == id) {
            self.order.remove(pos)
        } else {
            None
        }
    }

    fn touch(&mut self, id: &str) {
        let file_id = self
            .remove_from_order(id)
            .unwrap_or_else(|| FileId::from(id));
        self.order.push_front(file_id);
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
        self.remove_from_order(id.as_str());
        self.order.push_front(id.clone());
        self.map.insert(id, model);
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
