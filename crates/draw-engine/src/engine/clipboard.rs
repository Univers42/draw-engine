use std::collections::HashSet;

use crate::camera::Point;
use crate::edit::clipboard::{materialize_elements, serialize_selection};
use crate::engine::DrawEngine;
use crate::export::scene_to_json;
use crate::interaction::DrawTool;
use crate::scene::geometry::scene_bounds;
use crate::scene::{DrawElement, DrawElementType};
use crate::text::layout::{self, Laid};

/// The smallest an imported label is set to fit its shape: the converter's own floor
/// (`MIN_VERTEX_LABEL_FONT_SIZE`, `@excalidraw/mermaid-to-excalidraw@2.2.2`).
const MIN_IMPORTED_LABEL_SIZE: f64 = 12.0;

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
    /// Commits the scene: settles which frame what it created belongs to, stamps what
    /// this step changed, records it, tells the host.
    ///
    /// The stamping is what makes a move reach anyone. Gestures change geometry
    /// without touching `version`, and the version is the only change signal the
    /// autosave, the server's merge and every peer have — so an unstamped move was
    /// never saved and lost on reload. See `stamp.rs`.
    pub(super) fn push_history(&mut self) {
        self.push_history_keeping_frames(&[]);
    }

    /// [`Self::push_history`], leaving the frame of `given`, created by this step, as it
    /// was set rather than judged where it landed: the rectangle "Wrap text in a
    /// container" makes takes its text's (`actionBoundText.tsx@1118751f:313`).
    pub(super) fn push_history_keeping_frames(&mut self, given: &[String]) {
        self.judge_created_frame_membership(given);
        // Only a step that changed something is recorded. A click that selected, or a
        // commit after a peer's patch and nothing of ours, used to push an entry anyway —
        // one that undid nothing and threw the redo stack away.
        let selected = std::rc::Rc::new(self.selection_state());
        if let Some(step) = self.take_local_step(&selected) {
            self.history.push(step);
        }
        // The next step begins with what this one left selected.
        self.settled_selection = selected;
        self.style_preview.clear();
        self.touch_style();
        self.emit_scene_change();
        self.keep_text_session_pending();
    }

    /// Tells the host what changed, as briefly as it can.
    ///
    /// This used to serialise the entire scene as pretty-printed JSON on **every**
    /// mutation. At 20k elements that was ~9.8MB and ~60ms per shape drawn, which is
    /// four dropped frames for the act of drawing one rectangle — and it got worse as
    /// the board filled up, which is exactly the shape of "it feels slow".
    ///
    /// A delta covers the common path, including elements placed beside another — a
    /// shape joining a frame goes below it, a new label above its shape — for which it
    /// carries the order of the ids. The rare structural changes — a z-order command, a
    /// discarded draft, a wholesale replace — still send everything, because there is no
    /// smaller honest answer for them.
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
        self.settle_selection();
    }

    fn after_history_step(&mut self, selected: &super::stamp::SelectionState) {
        // What the step recorded as selected, through `set_selection`: the step may have
        // taken the group being edited away (the oracle then drops it,
        // `packages/element/src/delta.ts@1118751f:806-818`); kept behind its back, it named a group
        // nothing carried, and no click anywhere on the board expanded to a group.
        self.restore_selection(selected);
        // A drag carries on over the scene the step left, and what it remembered of the
        // arrow under it — the far end's binding as it found it — is of the scene before.
        self.clear_binding_suggestion();
        // An undo replaces the whole scene; there is no delta that describes it.
        self.scene.invalidate_delta();
        self.events.scene_json = Some(scene_to_json(&self.scene.ordered_cloned()));
    }

    /// Undo, as a new edit of only what the step changed: see `stamp.rs`.
    pub fn undo(&mut self) {
        // While a text is being typed, undo is its editor's own (`text_session.rs`).
        if self.text_session.is_some() || !self.history.can_undo() {
            return;
        }
        let step = self.history.current().clone();
        self.history.undo();
        self.replay_step(&step, false);
        self.after_history_step(&step.selection.0);
    }

    pub fn redo(&mut self) {
        if self.text_session.is_some() {
            return;
        }
        let Some(step) = self.history.redo().cloned() else {
            return;
        };
        self.replay_step(&step, true);
        self.after_history_step(&step.selection.1);
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
        if changed {
            // A peer may have ungrouped or deleted the group being edited here.
            self.revalidate_editing();
        }
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
        // it while the element is being edited (`data/reconcile.ts@1118751f:31-33`): taking it
        // would hand the gesture's final state the peer's stamp, and the two copies
        // would then carry one stamp and different content forever. The commit stamps
        // above the refused version instead, so the later edit — this one — wins.
        let pending = self.scene.pending_ids();
        let pending_order = self.scene.order_baseline();
        let pending_placement = self.scene.placement();

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
                // The panel shows the selection and the labels it carries; a peer's edit
                // to either is one it has to show.
                if self.selected_ids.contains(&element.id)
                    || element
                        .container_id
                        .as_ref()
                        .is_some_and(|container| self.selected_ids.contains(container))
                {
                    self.touch_style();
                }
                self.scene.put(element);
                changed = true;
            }
        }

        if let Some(order_ids) = &patch.order {
            // Resolved to what this scene holds before the pass below reads it: a named id it
            // cannot produce — erased here and still live on the peer that has not been told,
            // or an edit this tab has not received — spends a slot in a positional cursor that
            // the scan can never fill. That is one O(m) pass over the order, on a path that was
            // already O(n) over the board.
            let ids: Vec<&str> = order_ids
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter(|id| self.scene.get(id).is_some_and(|e| !e.is_deleted))
                .collect();
            if !ids.is_empty() {
                // Told by a set: searched in the list once per element, a peer's order
                // cost the board squared — 533ms on 20,000 shapes — and every shape a
                // peer draws into a frame sends one.
                let listed: HashSet<&str> = ids.iter().copied().collect();
                // What the order names, in the order it names it, with what it leaves out
                // dropped back into the slot it held rather than piled on top.
                //
                // The oracle's `syncMovedIndices` returns an element the incoming order
                // does not name untouched — its own fractional index and all
                // (`packages/element/src/fractionalIndex.ts@1118751f:185-193`) — and a
                // peer cannot name what it has never seen. Sent to the top instead, a
                // shape drawn into a frame came back above the frame it was drawn in, and
                // a redo kept it there: the step that drew it records no order of its own
                // to replay, a new element's place being part of its creation
                // (`scene/store.rs` › `place_beside`).
                //
                // Each one goes back above the last of the order's elements the stack
                // put under it, so a label stays above its shape and a frame child below
                // its frame even where the peer's order moved those. The ids are resolved
                // to this scene's own live elements above, so every one of the order's slots
                // is an element the scan can meet: read as a count of the order's elements
                // met so far, which only rises, this is one pass and one stack.
                let mut live: Vec<DrawElement> = Vec::with_capacity(self.scene.total_len());
                let mut sent = 0usize;
                let mut named = 0usize;
                for element in self.scene.iter_ordered() {
                    if listed.contains(element.id.as_str()) {
                        named += 1;
                        continue;
                    }
                    while sent < named {
                        live.push(self.scene.get(ids[sent]).expect("resolved above").clone());
                        sent += 1;
                    }
                    live.push(element.clone());
                }
                for id in &ids[sent..] {
                    live.push(self.scene.get(id).expect("resolved above").clone());
                }
                self.scene.set_order(live);
                changed = true;
            }
        }

        if changed {
            // The arrows of what the peer changed, and of what is pending here, and no
            // others: a peer's edit elsewhere re-routed an arrow saved before its ends
            // were anchored, unstamped and unsent, so the server kept the old geometry
            // under the same version.
            self.apply_bindings();
            // Drop the delta this produced: the host already has these elements, and
            // emitting them would send them straight back to the peer that sent them.
            let _ = self.scene.take_delta();
            // ...but not the local changes that were pending in it, which the host has
            // not been told about yet — nor where they put things in the stack: a new
            // label above its shape, or a shape below the frame it was drawn in, told as
            // a delta without the order, went on top of the host's copy, and of the saved
            // board.
            for id in &pending {
                self.scene.mark_dirty(id);
            }
            self.scene.restore_placement(pending_placement);
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
        if !self.place_json(&payload, at, false) {
            return false;
        }
        self.clipboard_buffer = Some(payload);
        true
    }

    /// Places a scene made elsewhere — the Mermaid import — as a paste does, but with every
    /// text laid out from its source and every shape grown to hold its label: the oracle
    /// puts a skeleton's labels through `redrawTextBoundingBox` the same way
    /// (`convertToExcalidrawElements`, `packages/element/src/transform.ts@1118751f:259-298`),
    /// since a text made elsewhere has no size measured in this font. One step of undo, and
    /// the clipboard is left as it was: nothing was copied. The camera stays: the dialog's
    /// Insert fits it afterwards, a pasted definition does not (`App.tsx@1118751f:4686-4702`).
    pub fn insert_json(&mut self, json: &str, at: Option<(f64, f64)>) -> bool {
        self.place_json(json, at, true)
    }

    /// `text` laid out, a point smaller at a time while at its size it would grow the shape it
    /// labels, down to [`MIN_IMPORTED_LABEL_SIZE`] — past that the shape grows, as the oracle's
    /// always does. Each size is wrapped anew, so the largest that fits is the one found.
    ///
    /// Divergence: the converter sets only a cylinder's label smaller to fit
    /// (`computeVertexLabelFontSize`, `@excalidraw/mermaid-to-excalidraw@2.2.2`
    /// `converter/types/flowchart.js`) and the oracle grows every other shape, so one long
    /// label pushes its node over the next. Mermaid laid each node out to hold its label;
    /// keeping that size keeps Mermaid's layout.
    fn fitted(&self, text: &DrawElement) -> Laid {
        let mut laid = self.laid_out(text);
        let mut smaller = text.clone();
        while laid.container.is_some() {
            let size = layout::font_of(&smaller).size() - 1.0;
            if size < MIN_IMPORTED_LABEL_SIZE {
                break;
            }
            smaller.font_size = Some(size);
            laid = self.laid_out(&smaller);
        }
        laid
    }

    fn place_json(&mut self, payload: &str, at: Option<(f64, f64)>, lay_out: bool) -> bool {
        let mut offset_x = super::PASTE_OFFSET;
        let mut offset_y = super::PASTE_OFFSET;
        if let Some((x, y)) = at {
            if let Some(source) = materialize_elements(payload, 0.0, 0.0, self.now_ms) {
                if let Some(bounds) = scene_bounds(&source) {
                    // The oracle's `duplicateAtSceneCoords`: half-width/height off the
                    // pointer gives the bounding box's new left/top edge, snapped to the
                    // grid before it lands — not the pointer itself, and not this box's
                    // true centre (`App.duplicate.ts@1118751f:87-99`). Read the shape once
                    // more, not once per test, at `ci_paste.rs`.
                    let half_width = (bounds.max_x - bounds.min_x) / 2.0;
                    let half_height = (bounds.max_y - bounds.min_y) / 2.0;
                    let snapped = self.snap(Point {
                        x: x - half_width,
                        y: y - half_height,
                    });
                    offset_x = snapped.x - bounds.min_x;
                    offset_y = snapped.y - bounds.min_y;
                }
            }
        }
        let Some(pasted) = materialize_elements(payload, offset_x, offset_y, self.now_ms) else {
            return false;
        };
        if pasted.is_empty() {
            return false;
        }
        let ids: Vec<String> = pasted.iter().map(|el| el.id.clone()).collect();
        let texts: Vec<DrawElement> = pasted
            .iter()
            .filter(|el| lay_out && el.kind == DrawElementType::Text)
            .cloned()
            .collect();
        for element in pasted {
            self.scene.add(element);
        }
        for text in texts {
            let laid = self.fitted(&text);
            self.put_laid(laid);
        }
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
        // Ctrl+D and Alt-drag both come through here, and both copy inside the group being
        // edited: `duplicateElements` is handed the app state's `editingGroupId`, by
        // `packages/excalidraw/actions/actionDuplicateSelection.tsx@1118751f:63-72` and by
        // `packages/excalidraw/components/App.duplicate.ts@1118751f:195-199`.
        let editing = self.editing_group_id.as_deref();
        let copied =
            crate::edit::expand_for_copy_among(self.scene.iter_ordered(), &self.selected_ids);
        // Where every copy belongs, worked out before the sources are consumed: a group, a
        // frame with its children, a container with its label, each one run; anything else
        // a run of one. See `duplicate_runs`.
        let placement = crate::edit::duplicate_runs(&copied, editing);
        let Some(copies) =
            crate::edit::materialize_within(copied, offset_x, offset_y, self.now_ms, editing)
        else {
            return;
        };
        let ids: Vec<String> = copies.iter().map(|el| el.id.clone()).collect();
        for element in copies {
            self.scene.add(element);
        }
        // The oracle puts each copy directly above its original (`duplicate.ts@1118751f:
        // 430-436`), a group or a frame's own run moving together as the block it already
        // is. One call per run rather than per copy: each is an `O(board)` restack — the
        // same cost `stack_under_frame` pays once per frame a shape joins
        // (`docs/reference/zorder.md`) — so a run of one, the common Ctrl+D, pays it once.
        //
        // The anchor is resolved against the *live scene*, not the copied sources: a run
        // key (its group, frame or container) can own an element that was never selected —
        // stepped into a group and duplicated only one of its members, the anchor is the
        // group's other, untouched member above it, not the member that was copied.
        let new_copies: HashSet<&str> = ids.iter().map(String::as_str).collect();
        for (key, run) in placement {
            let run_ids: Vec<String> = run.iter().map(|&i| ids[i].clone()).collect();
            let anchor = self
                .scene
                .iter_ordered()
                .rev()
                .find(|el| !new_copies.contains(el.id.as_str()) && key.owns(el))
                .map(|el| el.id.clone());
            if let Some(anchor) = anchor {
                self.scene.place_above(&run_ids, &anchor);
            }
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
        // Deleting a frame deletes the frame, not the work in it, as Excalidraw's does
        // (`packages/excalidraw/actions/actionDeleteSelected.tsx@1118751f:57-73,115-122`): its
        // children stay — even one selected along with it, since deleting the frame is
        // taken to mean the frame — leave it, and become the selection, so a second
        // Delete takes them too if that was what was meant. This used to take them with
        // the frame, and claimed parity for it.
        //
        // Collected before anything is written, because reading a frame's children needs
        // the scene the removals below are about to change.
        let kept: HashSet<String> = self
            .selected_ids
            .iter()
            .filter(|id| self.scene.get(id).is_some_and(crate::scene::is_frame))
            .flat_map(|id| crate::scene::frame_children(self.scene.iter_ordered(), id))
            .collect();
        // A kept child's label stays with it, selected or not.
        let doomed_selected: Vec<String> = self
            .selected_ids
            .iter()
            .filter(|id| {
                !kept.contains(*id)
                    && !self
                        .scene
                        .get(id)
                        .and_then(|el| el.container_id.as_ref())
                        .is_some_and(|container| kept.contains(container))
            })
            .cloned()
            .collect();
        let mut doomed: HashSet<String> = doomed_selected.iter().cloned().collect();
        for id in &doomed_selected {
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
        // An arrow that stays is let go of a shape that is going, as the eraser and the
        // vectorize let it go: the Delete key is a delete, and the release is part of this
        // same step, so one undo binds the arrow again. An end on a shape that is not
        // going is still good and is left alone.
        crate::scene::binding::release_bindings_to_removed(&mut self.scene, &doomed);
        for id in doomed {
            // One made since the last commit — a text never typed into, left selected
            // while its session was ended the old way — was never there: dropped, not
            // tombstoned, as `remove_emptied_text` drops one abandoned through the editor
            // (`engine/text_session.rs`). A tombstone for it would be a step undo could
            // never make visible, because nothing had ever shown it.
            if self.scene.created_since_commit(&id) {
                self.scene.discard(&id);
            } else {
                self.scene.remove(&id, now);
            }
        }
        for id in &kept {
            self.scene.update(id, |child| child.frame_id = None);
        }
        if !kept.is_empty() {
            let selection = crate::edit::expand_within(
                self.scene.iter_ordered(),
                kept,
                self.editing_group_id.as_deref(),
            );
            self.set_selection(selection);
        } else {
            // Inside a group, deleting keeps you inside what is left of it, holding its next
            // member — or a level up once it is down to one. Emptied instead, the selection
            // took the group being edited with it, and the next Delete had nothing to act on.
            let (editing, held) = match self.editing_group_id.take() {
                Some(editing) => {
                    crate::edit::after_delete_within(self.scene.iter_ordered(), &editing, |el| {
                        !self.untouchable(el)
                    })
                }
                None => (None, Default::default()),
            };
            self.editing_group_id = editing;
            self.set_selection(held);
        }
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

    /// The box and the camera a whole-scene PNG export is framed by, at `scale`.
    ///
    /// The raster half is `crate::wasm::export`'s; this is the half that decides how big
    /// the picture is and what is in it, and it is here so it can be asked without a
    /// browser. See [`crate::export::ExportFrame`].
    pub fn export_frame(&self, padding: f64, scale: f64) -> crate::export::ExportFrame {
        crate::export::ExportFrame::for_scene(self.scene.iter_ordered(), padding, scale)
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
