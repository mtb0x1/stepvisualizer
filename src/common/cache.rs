//! Two cache layers behind one module: an LRU over parsed `StepModel`s
//! (backed by localStorage for persistence) and a session-only thread-local
//! cache of tessellated `RenderablePart` lists.
use crate::trace_span;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use super::constants::CACHE_SIZE;
use super::render::RenderablePart;
use super::types::StepModel;

/// LRU over parsed models. Cheap to clone: `StepModel` moves, no `Rc`
/// indirection, and the working set is bounded by `capacity`.
pub struct LruCache {
    capacity: usize,
    order: VecDeque<String>,
    map: HashMap<String, StepModel>,
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
        if let Some(pos) = self.order.iter().position(|k| k == id) {
            self.order.remove(pos);
        }
    }

    fn touch(&mut self, id: &str) {
        self.remove_from_order(id);
        self.order.push_front(id.to_string());
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
        self.insert(id.to_string(), loaded.clone());
        Some(loaded)
    }

    /// Insert or replace a model, then evict least-recently-used entries
    /// beyond capacity.
    pub fn insert(&mut self, id: String, model: StepModel) {
        // Capacity 0 means "cache nothing": get_or_load then simply falls
        // through to the persistence backend on every call.
        if self.capacity == 0 {
            return;
        }
        self.map.insert(id.clone(), model);
        self.touch(&id);
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

// ---------------------------------------------------------------------------
// Rendered-part cache (tessellation output)
//
// A separate layer from the `LruCache` above: that one holds parsed `StepModel`s
// (the persistence-backed source of truth), while this holds the GPU-ready
// `RenderablePart` lists produced by tessellation. Keeping both caches behind a
// single `cache` module avoids a third, scattered caching site.
// ---------------------------------------------------------------------------

thread_local! {
    static RENDER_PART_CACHE: RefCell<HashMap<String, Rc<Vec<RenderablePart>>>> =
        RefCell::new(HashMap::new());
    // Recency order mirroring `RENDER_PART_CACHE` so the geometry cache can be
    // bounded by the same `CACHE_SIZE` as the model LRU. A separate LRU-style
    // eviction keeps the GPU-ready geometry from growing without bound even
    // though the model LRU above evicts `StepModel`s, not this cache.
    static RENDER_PART_ORDER: RefCell<VecDeque<String>> = RefCell::new(VecDeque::new());
}

/// Promote `file_id` to most-recently-used in the recency order (no-op if absent).
fn touch_render_parts(file_id: &str) {
    RENDER_PART_ORDER.with(|order| {
        let mut order = order.borrow_mut();
        if let Some(pos) = order.iter().position(|k| k == file_id) {
            order.remove(pos);
        }
        order.push_back(file_id.to_string());
    });
}

/// Evict least-recently-used tessellations until the working set is bounded.
fn evict_render_parts() {
    RENDER_PART_ORDER.with(|order| {
        let mut order = order.borrow_mut();
        while order.len() > CACHE_SIZE {
            if let Some(least) = order.pop_front() {
                RENDER_PART_CACHE.with(|cache| {
                    cache.borrow_mut().remove(&least);
                });
            } else {
                break;
            }
        }
    });
}

/// Tessellated geometry for `file_id`, if previously computed this session.
pub fn get_cached_parts(file_id: &str) -> Option<Vec<RenderablePart>> {
    trace_span!("get_cached_parts");
    RENDER_PART_CACHE.with(|cache| {
        if cache.borrow().contains_key(file_id) {
            touch_render_parts(file_id);
            cache.borrow().get(file_id).map(|parts| (**parts).clone())
        } else {
            None
        }
    })
}

/// Store tessellated geometry for `file_id` (cloned in; the cache owns its
/// copy). Bounded by `CACHE_SIZE` — least-recently-used entries are evicted.
pub fn cache_parts(file_id: &str, parts: &[RenderablePart]) {
    trace_span!("cache_parts");
    let rc = Rc::new(parts.to_vec());
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().insert(file_id.to_string(), rc);
    });
    touch_render_parts(file_id);
    evict_render_parts();
}

/// Forget the tessellated geometry for one file (e.g. after model deletion).
pub fn drop_cached_parts(file_id: &str) {
    trace_span!("drop_cached_parts");
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().remove(file_id);
    });
    RENDER_PART_ORDER.with(|order| {
        if let Some(pos) = order.borrow().iter().position(|k| k == file_id) {
            order.borrow_mut().remove(pos);
        }
    });
}

/// Forget all tessellated geometry (e.g. after clearing history).
pub fn clear_cached_parts() {
    trace_span!("clear_cached_parts");
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
    RENDER_PART_ORDER.with(|order| {
        order.borrow_mut().clear();
    });
}
