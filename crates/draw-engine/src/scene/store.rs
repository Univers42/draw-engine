use std::collections::HashMap;

use crate::camera::WorldBounds;
use crate::scene::element::DrawElement;
use crate::scene::geometry::scene_bounds;

#[derive(Clone, Debug, Default)]
pub struct Scene {
    by_id: HashMap<String, DrawElement>,
    order: Vec<String>,
    ordered_cache: Option<Vec<DrawElement>>,
}

impl Scene {
    pub fn new(elements: impl IntoIterator<Item = DrawElement>) -> Self {
        let mut scene = Self::default();
        for element in elements {
            scene.add(element);
        }
        scene
    }

    pub fn ordered(&mut self) -> &[DrawElement] {
        if self.ordered_cache.is_none() {
            let mut out = Vec::new();
            for id in &self.order {
                if let Some(element) = self.by_id.get(id) {
                    if !element.is_deleted {
                        out.push(element.clone());
                    }
                }
            }
            self.ordered_cache = Some(out);
        }
        self.ordered_cache.as_ref().map_or(&[], Vec::as_slice)
    }

    pub fn ordered_cloned(&self) -> Vec<DrawElement> {
        self.order
            .iter()
            .filter_map(|id| self.by_id.get(id))
            .filter(|element| !element.is_deleted)
            .cloned()
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&DrawElement> {
        self.by_id.get(id)
    }

    pub fn size(&self) -> usize {
        self.ordered_cloned().len()
    }

    pub fn add(&mut self, element: DrawElement) {
        if !self.by_id.contains_key(&element.id) {
            self.order.push(element.id.clone());
        }
        self.by_id.insert(element.id.clone(), element);
        self.ordered_cache = None;
    }

    pub fn put(&mut self, element: DrawElement) {
        if !self.by_id.contains_key(&element.id) {
            self.order.push(element.id.clone());
        }
        self.by_id.insert(element.id.clone(), element);
        self.ordered_cache = None;
    }

    pub fn remove(&mut self, id: &str, now: f64) {
        if let Some(element) = self.by_id.get_mut(id) {
            element.is_deleted = true;
            element.version += 1;
            element.updated = now;
        }
        self.ordered_cache = None;
    }

    pub fn discard(&mut self, id: &str) {
        self.by_id.remove(id);
        self.order.retain(|existing| existing != id);
        self.ordered_cache = None;
    }

    pub fn bring_to_front(&mut self, id: &str) {
        if !self.by_id.contains_key(id) {
            return;
        }
        self.order.retain(|existing| existing != id);
        self.order.push(id.to_string());
        self.ordered_cache = None;
    }

    pub fn set_order(&mut self, live: Vec<DrawElement>) {
        let tombstones: Vec<DrawElement> = self
            .by_id
            .values()
            .filter(|element| element.is_deleted)
            .cloned()
            .collect();
        self.by_id.clear();
        self.order.clear();
        for element in tombstones {
            self.order.push(element.id.clone());
            self.by_id.insert(element.id.clone(), element);
        }
        for element in live {
            self.order.retain(|existing| existing != &element.id);
            self.order.push(element.id.clone());
            self.by_id.insert(element.id.clone(), element);
        }
        self.ordered_cache = None;
    }

    pub fn bounds(&self) -> Option<WorldBounds> {
        scene_bounds(&self.ordered_cloned())
    }

    pub fn to_array(&self) -> Vec<DrawElement> {
        self.order
            .iter()
            .filter_map(|id| self.by_id.get(id).cloned())
            .collect()
    }
}
