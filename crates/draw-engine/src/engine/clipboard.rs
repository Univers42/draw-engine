use crate::edit::clipboard::{materialize_elements, serialize_selection};
use crate::engine::DrawEngine;
use crate::export::scene_to_json;
use crate::interaction::DrawTool;
use crate::scene::geometry::scene_bounds;
use crate::scene::DrawElement;

/// Last-writer-wins between two versions of the same element.
///
/// Higher `version` wins; a tie is broken by higher `versionNonce`, then by `updated`.
/// Comparing in that order means a skewed clock can only ever decide a tie that the
/// edit counts could not — the ordering is deterministic, so every client reaches the
/// same answer regardless of the order patches arrive in.
fn remote_wins(incoming: &DrawElement, existing: &DrawElement) -> bool {
    if incoming.version != existing.version {
        return incoming.version > existing.version;
    }
    if incoming.version_nonce != existing.version_nonce {
        return incoming.version_nonce > existing.version_nonce;
    }
    incoming.updated > existing.updated
}

impl DrawEngine {
    pub(super) fn push_history(&mut self) {
        self.history.push(self.scene.snapshot());
        self.emit_scene_change();
    }

    /// Tells the host what changed, as briefly as it can.
    ///
    /// This used to serialise the entire scene as pretty-printed JSON on **every**
    /// mutation. At 20k elements that was ~9.8MB and ~60ms per shape drawn, which is
    /// four dropped frames for the act of drawing one rectangle — and it got worse as
    /// the board filled up, which is exactly the shape of "it feels slow".
    ///
    /// A delta covers the common path. The rare structural changes — a z-order command,
    /// a discarded draft, a wholesale replace — still send everything, because there is
    /// no smaller honest answer for them.
    pub(super) fn emit_scene_change(&mut self) {
        match self.scene.take_delta() {
            Some(delta) => self.events.scene_delta = Some(delta),
            None => self.events.scene_json = Some(scene_to_json(&self.scene.ordered_cloned())),
        }
    }

    pub(super) fn reset_history(&mut self) {
        self.history.reset(self.scene.snapshot());
    }

    fn apply_snapshot(&mut self, snapshot: Option<Vec<std::rc::Rc<DrawElement>>>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        self.scene = crate::scene::Scene::from_snapshot(snapshot);
        self.selected_ids.clear();
        self.events.selection = Some(Vec::new());
        self.request_draw();
        // An undo replaces the whole scene; there is no delta that describes it.
        self.scene.invalidate_delta();
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

    /// Merges elements from another client into this scene.
    ///
    /// This is **not** a paste, and the difference is the whole point: `paste_json`
    /// mints a fresh id for every element and offsets it, because that is what pasting
    /// means. Feeding remote edits through it turned every incoming element into a
    /// duplicate — and because the resulting change was then broadcast back, two
    /// clients grew a board without bound. A four-element board reached 8,273 elements
    /// and three frames a second that way.
    ///
    /// Merge is by id, last-writer-wins on `(version, versionNonce)` — Excalidraw's
    /// reconciliation rule, and the same one `packages/contract` applies server-side,
    /// so a client and the server converge on the same answer.
    ///
    /// No history entry: a remote edit is not a step in *your* undo stack. No scene
    /// event either — the caller is told through the return value, so nothing
    /// re-broadcasts what it was just sent.
    pub fn apply_remote_patch(&mut self, json: &str) -> bool {
        let Some(incoming) = crate::export::elements_from_json(json) else {
            return false;
        };

        let mut changed = false;
        for element in incoming {
            let accept = match self.scene.get(&element.id) {
                None => true,
                Some(existing) => remote_wins(&element, existing),
            };
            if accept {
                self.scene.put(element);
                changed = true;
            }
        }

        if changed {
            self.apply_bindings();
            // Drop the delta this produced: the host already has these elements, and
            // emitting them would send them straight back to the peer that sent them.
            let _ = self.scene.take_delta();
            self.request_draw();
        }
        changed
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
