//! LRU cache over parsed `StepModel`s (backed by persistence storage).
use std::collections::{HashMap, VecDeque};

use super::types::{FileId, StepModel};

/// LRU over parsed models. Cheap to clone: `StepModel` moves, no `Rc`
/// indirection, and the working set is bounded by `capacity`.
pub struct LruCache {
    capacity: usize,
    order: VecDeque<FileId>,
    map: HashMap<FileId, StepModel>,
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

    /// Clone of the model under `id`, promoting it to most-recently-used.
    pub fn get(&mut self, id: &str) -> Option<StepModel> {
        let model = self.map.get(id).cloned();
        if model.is_some() {
            self.touch(id);
        }
        model
    }

    /// Memory cache hit, else persistence backend, else `None`.
    ///
    /// Memory is the single in-memory layer; `load` is the persistence
    /// backend (e.g. localStorage). On a miss we fall through to the backend,
    /// promote the result into the cache, and return it. This removes the
    /// duplicated get-or-load branching that previously lived at every caller.
    pub fn get_or_load(
        &mut self,
        id: &str,
        load: impl Fn(&str) -> Option<StepModel>,
    ) -> Option<StepModel> {
        if let Some(model) = self.get(id) {
            return Some(model);
        }
        let loaded = load(id)?;
        self.insert(FileId::from(id), loaded.clone());
        Some(loaded)
    }

    /// Insert or replace a model, then evict least-recently-used entries
    /// beyond capacity.
    pub fn insert(&mut self, id: FileId, model: StepModel) {
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
