use crate::trace_span;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use super::render::RenderablePart;
use super::types::StepModel;

pub struct LruCache {
    capacity: usize,
    order: VecDeque<String>,
    map: HashMap<String, StepModel>,
}

impl LruCache {
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

    pub fn get(&mut self, id: &str) -> Option<StepModel> {
        let model = self.map.get(id).cloned();
        if model.is_some() {
            self.touch(id);
        }
        model
    }

    // Memory cache is the single in-memory layer; `load` is the persistence
    // backend (e.g. localStorage). On a miss we fall through to the backend,
    // promote the result into the cache, and return it. This removes the
    // duplicated get-or-load branching that previously lived at every caller.
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

    pub fn remove(&mut self, id: &str) {
        self.remove_from_order(id);
        self.map.remove(id);
    }

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
}

pub fn get_cached_parts(file_id: &str) -> Option<Vec<RenderablePart>> {
    trace_span!("get_cached_parts");
    RENDER_PART_CACHE.with(|cache| cache.borrow().get(file_id).map(|parts| (**parts).clone()))
}

pub fn cache_parts(file_id: &str, parts: &[RenderablePart]) {
    trace_span!("cache_parts");
    let rc = Rc::new(parts.to_vec());
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().insert(file_id.to_string(), rc);
    });
}

pub fn drop_cached_parts(file_id: &str) {
    trace_span!("drop_cached_parts");
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().remove(file_id);
    });
}

pub fn clear_cached_parts() {
    trace_span!("clear_cached_parts");
    RENDER_PART_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}
