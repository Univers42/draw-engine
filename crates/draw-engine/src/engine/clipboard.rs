use crate::edit::clipboard::{materialize_elements, serialize_selection};
use crate::engine::DrawEngine;
use crate::export::scene_to_json;
use crate::interaction::DrawTool;
use crate::scene::geometry::scene_bounds;
use crate::scene::DrawElement;

impl DrawEngine {
    pub(super) fn push_history(&mut self) {
        self.history.push(self.scene.to_array());
        self.events.scene_json = Some(scene_to_json(&self.scene.ordered_cloned()));
    }

    pub(super) fn reset_history(&mut self) {
        self.history.reset(self.scene.to_array());
    }

    fn apply_snapshot(&mut self, snapshot: Option<Vec<DrawElement>>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        self.scene = crate::scene::Scene::new(snapshot);
        self.selected_ids.clear();
        self.events.selection = Some(Vec::new());
        self.request_draw();
        self.events.scene_json = Some(scene_to_json(&self.scene.ordered_cloned()));
    }

    pub fn undo(&mut self) {
        let snap = self.history.undo().cloned();
        self.apply_snapshot(snap);
    }

    pub fn redo(&mut self) {
        let snap = self.history.redo().cloned();
        self.apply_snapshot(snap);
    }

    pub fn copy_selection(&mut self) -> Option<String> {
        let json = serialize_selection(&self.scene.ordered_cloned(), &self.selected_ids)?;
        self.clipboard_buffer = Some(json.clone());
        Some(json)
    }

    pub fn cut_selection(&mut self) -> Option<String> {
        let json = self.copy_selection()?;
        self.delete_selection();
        Some(json)
    }

    pub fn paste_json(&mut self, json: Option<&str>, at: Option<(f64, f64)>) -> bool {
        let payload = json
            .map(str::to_string)
            .or_else(|| self.clipboard_buffer.clone());
        let Some(payload) = payload else {
            return false;
        };
        let mut offset_x = super::PASTE_OFFSET;
        let mut offset_y = super::PASTE_OFFSET;
        if let Some((x, y)) = at {
            if let Some(source) = materialize_elements(&payload, 0.0, 0.0, self.now_ms) {
                if let Some(bounds) = scene_bounds(&source) {
                    offset_x = x - (bounds.min_x + bounds.max_x) / 2.0;
                    offset_y = y - (bounds.min_y + bounds.max_y) / 2.0;
                }
            }
        }
        let Some(pasted) = materialize_elements(&payload, offset_x, offset_y, self.now_ms) else {
            return false;
        };
        if pasted.is_empty() {
            return false;
        }
        let ids: Vec<String> = pasted.iter().map(|el| el.id.clone()).collect();
        for element in pasted {
            self.scene.add(element);
        }
        self.clipboard_buffer = Some(payload);
        self.set_tool(DrawTool::Select);
        self.set_selection(ids);
        self.apply_bindings();
        self.push_history();
        true
    }

    pub fn duplicate_selection(&mut self, offset_x: f64, offset_y: f64) {
        let Some(json) = serialize_selection(&self.scene.ordered_cloned(), &self.selected_ids)
        else {
            return;
        };
        let Some(copies) = materialize_elements(&json, offset_x, offset_y, self.now_ms) else {
            return;
        };
        let ids: Vec<String> = copies.iter().map(|el| el.id.clone()).collect();
        for element in copies {
            self.scene.add(element);
        }
        self.set_selection(ids);
        self.apply_bindings();
        self.push_history();
    }

    pub fn delete_selection(&mut self) {
        if self.selected_ids.is_empty() {
            return;
        }
        let now = self.now_ms;
        let mut doomed = self.selected_ids.clone();
        for id in &self.selected_ids {
            if let Some(element) = self.scene.get(id) {
                if let Some(bound) = &element.bound_text_id {
                    doomed.insert(bound.clone());
                }
                if let Some(container_id) = &element.container_id {
                    if !doomed.contains(container_id) {
                        if let Some(mut container) = self.scene.get(container_id).cloned() {
                            container.bound_text_id = None;
                            self.scene.put(container);
                        }
                    }
                }
            }
        }
        for id in doomed {
            self.scene.remove(&id, now);
        }
        self.set_selection(Vec::new());
        self.push_history();
    }

    pub fn export_json(&self) -> String {
        crate::scene_to_json(&self.scene.ordered_cloned())
    }

    pub fn export_svg(&self, padding: f64) -> Option<String> {
        let bounds = self.scene.bounds()?;
        Some(crate::scene_to_svg(
            &self.scene.ordered_cloned(),
            bounds,
            padding,
            &self.theme.background,
        ))
    }

    pub fn load_scene(&mut self, json: &str) -> bool {
        let Some(elements) = crate::elements_from_json(json) else {
            return false;
        };
        self.set_scene(crate::scene::Scene::new(elements));
        true
    }

    pub fn clear(&mut self) {
        self.set_scene(crate::scene::Scene::default());
        self.set_selection(Vec::new());
        self.events.scene_json = Some(self.export_json());
    }
}
