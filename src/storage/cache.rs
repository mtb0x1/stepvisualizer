//! LRU cache over parsed `StepModel`s (backed by persistence storage).
use std::rc::Rc;

use smallvec::SmallVec;

use crate::common::types::{FileId, StepModel};

/// LRU over parsed models. Stores `Rc<StepModel>` so cache hits return a
/// cheap reference-count clone instead of a full deep-copy of geometry data.
pub struct LruCache {
    max_memory_bytes: usize,
    current_memory_bytes: usize,
    entries: SmallVec<[(FileId, Rc<StepModel>); 5]>,
}

#[inline]
fn estimate_model_size(model: &StepModel) -> usize {
    // 24 bytes per GpuVertex + 4 bytes per index (3 indices per triangle)
    model.metadata.vertex_count * 24 + model.metadata.triangle_count * 12
}

impl LruCache {
    /// New cache holding up to `max_memory_bytes` of models. `max_memory_bytes == 0` caches
    /// nothing: [`get_or_load`](Self::get_or_load) then falls through to the
    /// backend on every call.
    pub fn new(max_memory_bytes: usize) -> Self {
        Self { max_memory_bytes, current_memory_bytes: 0, entries: SmallVec::new() }
    }

    /// Number of items currently stored in cache.
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache contains no items.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn touch_index(&mut self, index: usize) {
        if index > 0 && index < self.entries.len() {
            let entry = self.entries.remove(index);
            self.entries.insert(0, entry);
        }
    }

    /// Returns a shared reference to the model under `id`, promoting it to
    /// most-recently-used. No geometry data is copied on a cache hit.
    pub fn get(&mut self, id: &str) -> Option<Rc<StepModel>> {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k.as_str() == id) {
            self.touch_index(pos);
            Some(self.entries[0].1.clone())
        } else {
            None
        }
    }

    /// Memory cache hit, else persistence backend, else `None`.
    ///
    /// Memory is the single in-memory layer; `load` is the persistence
    /// backend (e.g. localStorage/IndexedDB). On a miss we fall through to the
    /// backend, wrap the result in `Rc`, promote into the cache, and return it.
    pub fn get_or_load(
        &mut self, id: &str, load: impl Fn(&str) -> Option<StepModel>,
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

    /// Insert a pre-wrapped `Rc<StepModel>`, then evict beyond memory limit.
    pub fn insert_rc(&mut self, id: FileId, model: Rc<StepModel>) {
        // Limit 0 means "cache nothing": get_or_load then simply falls
        // through to the persistence backend on every call.
        if self.max_memory_bytes == 0 {
            return;
        }
        let size = estimate_model_size(&model);

        if let Some(pos) = self.entries.iter().position(|(k, _)| k.as_str() == id.as_str()) {
            let (_, old_model) = self.entries.remove(pos);
            self.current_memory_bytes =
                self.current_memory_bytes.saturating_sub(estimate_model_size(&old_model));
        }
        self.entries.insert(0, (id, model));
        self.current_memory_bytes += size;

        while self.current_memory_bytes > self.max_memory_bytes && self.entries.len() > 1 {
            if let Some((_, evicted)) = self.entries.pop() {
                self.current_memory_bytes =
                    self.current_memory_bytes.saturating_sub(estimate_model_size(&evicted));
            }
        }
    }

    /// Drop a model from the cache (the persisted copy, if any, remains).
    pub fn remove(&mut self, id: &str) {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k.as_str() == id) {
            let (_, removed) = self.entries.remove(pos);
            self.current_memory_bytes =
                self.current_memory_bytes.saturating_sub(estimate_model_size(&removed));
        }
    }

    /// Drop everything (persisted copies, if any, remain).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_memory_bytes = 0;
    }
}
