use crate::edit::clipboard::{materialize_elements, serialize_selection};
use crate::engine::DrawEngine;
use crate::export::scene_to_json;
use crate::interaction::DrawTool;
use crate::scene::geometry::scene_bounds;
use crate::scene::{DrawElement, DrawElementType};

/// Last-writer-wins between two versions of the same element.
///
/// Higher `version` wins; a tie is broken by higher `versionNonce`, then by `updated`.
/// Comparing in that order means a skewed clock can only ever decide a tie that the
/// edit counts could not — the ordering is deterministic, so every client reaches the
/// same answer regardless of the order patches arrive in.
pub(super) fn remote_wins(incoming: &DrawElement, existing: &DrawElement) -> bool {
    if incoming.version != existing.version {
        return incoming.version > existing.version;
    }
    if incoming.version_nonce != existing.version_nonce {
        return incoming.version_nonce > existing.version_nonce;
    }
    incoming.updated > existing.updated
}

/// A peer's patch as it arrives: its elements and order are read one at a time, so what
/// cannot be read costs only itself.
#[derive(serde::Deserialize)]
struct RemotePatch {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    elements: Vec<serde_json::Value>,
    #[serde(default)]
    order: Option<Vec<serde_json::Value>>,
}

/// Whether `copy` is a live image the scene's `existing` shows, missing only a picture.
fn lacks_picture(copy: &DrawElement, existing: &DrawElement) -> bool {
    copy.kind == DrawElementType::Image
        && !copy.is_deleted
        && !existing.is_deleted
        && copy.data_url.is_none()
        && existing.data_url.is_some()
}

/// Gives `copy` the picture this scene already has for it.
///
/// An image's picture never changes once it has one, so peers send it once and leave it
/// off every later edit of the same image: moving a photo is then a few hundred bytes
/// rather than megabytes, every time. The copy without it keeps the one here.
pub(super) fn inherit_picture(copy: &mut DrawElement, existing: &DrawElement) {
    if lacks_picture(copy, existing) {
        copy.data_url = existing.data_url.clone();
    }
}

/// Whether the only thing to take from `incoming` is its picture: the same edit as the
/// scene's, which arrived without one — an edit that overtook the picture's first
/// arrival. The stamps tie, so the merge alone would never take it.
fn brings_picture(incoming: &DrawElement, existing: &DrawElement) -> bool {
    lacks_picture(existing, incoming)
        && incoming.version == existing.version
        && incoming.version_nonce == existing.version_nonce
}

impl DrawEngine {
    /// Commits the scene: stamps what this step changed, records it, tells the host.
    ///
    /// The stamping is what makes a move reach anyone. Gestures change geometry
    /// without touching `version`, and the version is the only change signal the
    /// autosave, the server's merge and every peer have — so an unstamped move was
    /// never saved and lost on reload. See `stamp.rs`.
    pub(super) fn push_history(&mut self) {
        // Only a step that changed something is recorded. A click that selected, or a
        // commit after a peer's patch and nothing of ours, used to push an entry anyway —
        // one that undid nothing and threw the redo stack away.
        if let Some(step) = self.take_local_step() {
            self.history.push(step);
        }
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
        // A scene loaded wholesale is nobody's local edit.
        let _ = self.scene.take_baseline();
        let _ = self.scene.take_order_baseline();
        self.remote_refused.clear();
        self.history.reset(super::stamp::HistoryEntry::default());
    }

    fn after_history_step(&mut self) {
        self.selected_ids.clear();
        self.events.selection = Some(Vec::new());
        self.request_draw();
        // An undo replaces the whole scene; there is no delta that describes it.
        self.scene.invalidate_delta();
        self.events.scene_json = Some(scene_to_json(&self.scene.ordered_cloned()));
    }

    /// Undo, as a new edit of only what the step changed: see `stamp.rs`.
    pub fn undo(&mut self) {
        if !self.history.can_undo() {
            return;
        }
        let step = self.history.current().clone();
        self.history.undo();
        self.replay_step(&step, false);
        self.after_history_step();
    }

    pub fn redo(&mut self) {
        let Some(step) = self.history.redo().cloned() else {
            return;
        };
        self.replay_step(&step, true);
        self.after_history_step();
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
    /// An optional `order` array of live ids rewrites z-order after the element merge.
    /// Without it, `put` keeps each element's existing position — a style change must
    /// not float a shape to the front, and a peer's reorder would never arrive.
    ///
    /// No history entry: a remote edit is not a step in *your* undo stack. No scene
    /// event either — the caller is told through the return value, so nothing
    /// re-broadcasts what it was just sent.
    pub fn apply_remote_patch(&mut self, json: &str) -> bool {
        let changed = self.apply_remote_patch_step(json);
        // A peer's element can bind to one being dragged here, and then moves with it.
        self.refresh_live();
        changed
    }

    fn apply_remote_patch_step(&mut self, json: &str) -> bool {
        let Ok(patch) = serde_json::from_str::<RemotePatch>(json) else {
            return false;
        };
        if patch.kind != "osidraw" {
            return false;
        }
        // One by one, so an element this engine cannot read costs only itself. Read as
        // one array, a single element from a newer engine — a shape type, a field of the
        // wrong kind — refused the whole patch, and every other edit in it was lost.
        let incoming: Vec<DrawElement> = patch
            .elements
            .into_iter()
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect();

        // Elements this client has changed and not yet committed — a drag in progress,
        // a path being placed. A peer's copy of one is refused, as Excalidraw refuses
        // it while the element is being edited (`data/reconcile.ts:31-33`): taking it
        // would hand the gesture's final state the peer's stamp, and the two copies
        // would then carry one stamp and different content forever. The commit stamps
        // above the refused version instead, so the later edit — this one — wins.
        let pending = self.scene.pending_ids();
        let pending_order = self.scene.order_baseline();

        let mut changed = false;
        for mut element in incoming {
            if self
                .scene
                .get(&element.id)
                .is_some_and(|existing| brings_picture(&element, existing))
            {
                let picture = element.data_url.take();
                self.scene
                    .update(&element.id, |ours| ours.data_url = picture);
                changed = true;
                continue;
            }
            if pending.contains(&element.id) {
                // Kept, not dropped: see `stamp.rs` for what the commit does with it.
                match self.remote_refused.get(&element.id) {
                    Some(kept) if !remote_wins(&element, kept) => {}
                    _ => {
                        self.remote_refused.insert(element.id.clone(), element);
                    }
                }
                continue;
            }
            let accept = match self.scene.get(&element.id) {
                None => true,
                Some(existing) => {
                    let wins = remote_wins(&element, existing);
                    if wins {
                        inherit_picture(&mut element, existing);
                    }
                    wins
                }
            };
            if accept {
                self.scene.put(element);
                changed = true;
            }
        }

        if let Some(order_ids) = &patch.order {
            let ids: Vec<&str> = order_ids
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect();
            if !ids.is_empty() {
                let mut live: Vec<DrawElement> = Vec::with_capacity(ids.len());
                for id in &ids {
                    if let Some(element) = self.scene.get(id) {
                        if !element.is_deleted {
                            live.push(element.clone());
                        }
                    }
                }
                for element in self.scene.ordered_cloned() {
                    if element.is_deleted {
                        continue;
                    }
                    if !ids.iter().any(|id| *id == element.id) {
                        live.push(element);
                    }
                }
                self.scene.set_order(live);
                changed = true;
            }
        }

        if changed {
            self.apply_bindings();
            // Drop the delta this produced: the host already has these elements, and
            // emitting them would send them straight back to the peer that sent them.
            let _ = self.scene.take_delta();
            // ...but not the local changes that were pending in it, which the host has
            // not been told about yet.
            for id in &pending {
                self.scene.mark_dirty(id);
            }
            self.request_draw();
        }
        // What the peer's edit changed here — its elements, arrows re-routed to follow
        // them, its z-order — is the peer's edit, not ours, and must not be recorded or
        // stamped as ours at the next commit. Local changes already pending stay.
        self.scene.retain_baseline(|id| pending.contains(id));
        self.scene.set_order_baseline(pending_order);
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
        // Straight from the scene, with no JSON in between. This used to deep-clone every
        // element on the board, serialise the selection, and parse it back — so the cost
        // of one Ctrl+D was proportional to the whole document, and a run of them (which
        // is how a board full of one shape gets made) was quadratic in its own output.
        let copied =
            crate::edit::expand_for_copy_among(self.scene.iter_ordered(), &self.selected_ids);
        // Ctrl+D and Alt-drag both come through here, and both copy inside the group being
        // edited: `duplicateElements` is handed the app state's `editingGroupId`, by
        // `packages/excalidraw/actions/actionDuplicateSelection.tsx:63-72` and by
        // `packages/excalidraw/components/App.duplicate.ts:195-199`.
        let editing = self.editing_group_id.as_deref();
        let Some(copies) =
            crate::edit::materialize(copied, offset_x, offset_y, self.now_ms, editing)
        else {
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
        // Deleting a frame deletes what it holds, as Excalidraw's does. A frame is the
        // thing those elements live in, not a label on them: leaving the contents behind
        // would scatter a diagram you had deliberately gathered.
        //
        // Collected before the removals rather than inside the loop above, because
        // reading a frame's children needs the scene while the loop above is already
        // writing to it.
        let orphaned: Vec<String> = self
            .selected_ids
            .iter()
            .filter(|id| self.scene.get(id).is_some_and(crate::scene::is_frame))
            .flat_map(|id| crate::scene::frame_children(self.scene.iter_ordered(), id))
            .collect();
        doomed.extend(orphaned);
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
        // Unlike a scene the host set, this one came from a file the host has never
        // seen: it is told, whole, or its copy of the board — the one it saves — stays
        // the board that was open before.
        self.events.scene_json = Some(self.export_json());
        true
    }

    pub fn clear(&mut self) {
        self.set_scene(crate::scene::Scene::default());
        self.set_selection(Vec::new());
        self.events.scene_json = Some(self.export_json());
    }
}
