use crate::trace_span;
use std::collections::{HashMap, VecDeque};

use super::types::StepModel;

pub struct LruCache {
    capacity: usize,
    order: VecDeque<String>,
    map: HashMap<String, StepModel>,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        trace_span!("LruCache::new");
        Self {
            capacity,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    fn touch(&mut self, id: &str) {
        trace_span!("LruCache::touch");
        if let Some(pos) = self.order.iter().position(|k| k == id) {
            self.order.remove(pos);
        }
        self.order.push_front(id.to_string());
    }

    pub fn get(&mut self, id: &str) -> Option<StepModel> {
        trace_span!("LruCache::get");
        if self.map.contains_key(id) {
            self.touch(id);
        }
        self.map.get(id).cloned()
    }

    pub fn insert(&mut self, id: String, model: StepModel) {
        trace_span!("LruCache::insert");
        if !self.map.contains_key(&id) && self.map.len() == self.capacity {
            if let Some(least) = self.order.pop_back() {
                self.map.remove(&least);
            }
        }
        self.map.insert(id.clone(), model);
        self.touch(&id);
    }

    pub fn remove(&mut self, id: &str) {
        trace_span!("LruCache::remove");
        if let Some(pos) = self.order.iter().position(|k| k == id) {
            self.order.remove(pos);
        }
        self.map.remove(id);
    }

    pub fn clear(&mut self) {
        trace_span!("LruCache::clear");
        self.order.clear();
        self.map.clear();
    }
}
