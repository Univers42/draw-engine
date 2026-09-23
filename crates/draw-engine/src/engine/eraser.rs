//! The eraser: mark what the sweep touches, delete it on release.
//!
//! Excalidraw's, in two stages (`eraser/index.ts`, `App.tsx:12154-12180`). While the
//! button is down, everything the stroke touches is **marked** and drawn at a fifth of its
//! opacity (`ELEMENT_READY_TO_ERASE_OPACITY`) — the document itself untouched. Releasing
//! deletes all of it as one step; Escape lets it all go. Sweeping back over marked
//! elements with Alt held un-marks them.
//!
//! This lived in the web host, and it was wrong in two ways. It asked the engine for the
//! element under each sample — the topmost one — so a stack of copies made with Ctrl+D
//! lost one copy per pass: the top copy, once marked, went on answering every sample
//! and the ones beneath were never reached. And it did the fading by rewriting each
//! element's opacity as if a peer had sent it, which undo then took for the element's
//! real state. Here the marking is session state beside the scene, like the selection,
//! and the fade exists only in the painter.

use std::collections::HashSet;

use crate::camera::Point;
use crate::engine::DrawEngine;

impl DrawEngine {
    /// Marks — or with Alt held, un-marks — everything the sweep from `from` to `to`
    /// touches.
    ///
    /// **Every** element it touches, not the topmost: a stack of identical copies is many
    /// elements under one point, and one sweep takes them all. A zero-length sweep is a
    /// click, and marks everything under the point, as Excalidraw's does.
    pub(crate) fn mark_along(&mut self, from: Point, to: Point) {
        let tolerance = self.collision_tolerance();
        let touched: Vec<String> = self
            .scene
            .iter_ordered()
            // Not what someone else holds: erasing a shape a colleague is moving only for
            // their next edit to bring it back is correct by the merge rule, and nothing
            // like what either of them meant.
            .filter(|el| {
                !self.untouchable(el) && crate::segment_hits_element(el, from, to, tolerance)
            })
            .map(|el| el.id.clone())
            .collect();
        if touched.is_empty() {
            return;
        }

        let restore = self.alt_held;
        // Only what would change: restoring what is not marked, or marking what already
        // is, does nothing.
        let seeds: Vec<String> = touched
            .into_iter()
            .filter(|id| restore == self.erasing.contains(id))
            .collect();
        if seeds.is_empty() {
            return;
        }
        let mut changed = false;
        for member in self.erased_with(&seeds) {
            changed |= if restore {
                self.erasing.remove(&member)
            } else {
                self.erasing.insert(member)
            };
        }
        if changed {
            self.erasing_revision = self.erasing_revision.wrapping_add(1);
            self.request_draw();
        }
    }

    /// What goes with the elements touched: each one's whole outermost group, everything
    /// a touched frame holds, and a container with its label either way round — as
    /// `updateElementsToBeErased` and `eraseElements` take them. A group is one thing, a
    /// frame is what its contents live in, and a label left behind is debris pinned to a
    /// shape that is gone.
    ///
    /// One pass over the scene for all of them, not one per group: a sweep through a
    /// pile of grouped copies touches every group at once, and a scan per group made one
    /// pointer move cost the square of the board.
    fn erased_with(&self, seeds: &[String]) -> HashSet<String> {
        let touched: Vec<&crate::scene::DrawElement> =
            seeds.iter().filter_map(|id| self.scene.get(id)).collect();
        let groups: HashSet<&str> = touched
            .iter()
            .filter_map(|el| el.group_ids.last().map(String::as_str))
            .collect();
        let frames: HashSet<&str> = touched
            .iter()
            .filter(|el| crate::scene::is_frame(el))
            .map(|el| el.id.as_str())
            .collect();

        let mut members: HashSet<String> = seeds.iter().cloned().collect();
        if !groups.is_empty() || !frames.is_empty() {
            for el in self.scene.iter_ordered() {
                let grouped = el.group_ids.iter().any(|g| groups.contains(g.as_str()));
                let framed = el
                    .frame_id
                    .as_deref()
                    .is_some_and(|frame| frames.contains(frame));
                if grouped || framed {
                    members.insert(el.id.clone());
                }
            }
        }
        // Labels last, so a frame's shapes bring theirs: a label carries no frame of its
        // own, only its container.
        let partners: Vec<String> = members
            .iter()
            .filter_map(|member| self.scene.get(member))
            .flat_map(|el| [el.bound_text_id.clone(), el.container_id.clone()])
            .flatten()
            .collect();
        members.extend(partners);
        members
    }

    /// Deletes everything marked, as one step, and clears the marks.
    ///
    /// Also what a marked frame holds, as Excalidraw's `eraseElements` takes it; and an
    /// arrow that stays is let go of a shape that does not, so it does not go on naming
    /// an element nobody can see. That release is part of the same step, so undo binds
    /// it again.
    pub(super) fn erase_marked(&mut self) {
        let marked = std::mem::take(&mut self.erasing);
        self.erasing_revision = self.erasing_revision.wrapping_add(1);
        if marked.is_empty() {
            self.request_draw();
            return;
        }

        // What a marked frame holds was marked with it; taken again here, with every
        // label of what is going, so nothing marked while a peer's patch moved things
        // around is left behind as an orphan.
        let mut doomed = marked.clone();
        for id in &marked {
            if self.scene.get(id).is_some_and(crate::scene::is_frame) {
                doomed.extend(crate::scene::frame_children(self.scene.iter_ordered(), id));
            }
        }
        let labels: Vec<String> = doomed
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter_map(|el| el.bound_text_id.clone())
            .collect();
        doomed.extend(labels);
        let released: Vec<(String, bool, bool)> = self
            .scene
            .iter_ordered()
            .filter(|el| !doomed.contains(&el.id))
            .filter_map(|el| {
                let start = el
                    .start_binding
                    .as_ref()
                    .is_some_and(|b| doomed.contains(b));
                let end = el.end_binding.as_ref().is_some_and(|b| doomed.contains(b));
                (start || end).then(|| (el.id.clone(), start, end))
            })
            .collect();
        for (id, start, end) in released {
            self.scene.update(&id, |arrow| {
                if start {
                    arrow.start_binding = None;
                }
                if end {
                    arrow.end_binding = None;
                }
            });
        }

        let now = self.now_ms;
        let mut selection_changed = false;
        for id in &doomed {
            if self.scene.get(id).is_some_and(|el| !el.is_deleted) {
                self.scene.remove(id, now);
                selection_changed |= self.selected_ids.remove(id);
            }
        }
        if selection_changed {
            self.events.selection = Some(self.get_selection());
        }
        self.push_history();
        self.request_draw();
    }

    /// Lets go of every mark without deleting anything — Escape, or a gesture that ends
    /// some other way.
    pub(super) fn clear_erasing(&mut self) {
        if !self.erasing.is_empty() {
            self.erasing.clear();
            self.erasing_revision = self.erasing_revision.wrapping_add(1);
            self.request_draw();
        }
    }

    /// Ends a sweep without deleting anything: drops the gesture and lets its marks go.
    /// For leaving the eraser by any route, as Excalidraw's `endPath` on a tool change.
    pub(super) fn abandon_sweep(&mut self) {
        if matches!(self.interaction, Some(super::Interaction::Erase { .. })) {
            self.interaction = None;
        }
        self.clear_erasing();
    }

    /// What the sweep in progress has marked, sorted. Empty between sweeps.
    pub fn marked_for_erasure(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.erasing.iter().cloned().collect();
        ids.sort();
        ids
    }

    /// Alt, as held during the current pointer move.
    ///
    /// Only the eraser reads it: sweeping back over marked elements with Alt held
    /// un-marks them, as in Excalidraw. Set by the host before each move rather than
    /// passed to `move_pointer`, which a hundred call sites use without it.
    pub fn set_alt_held(&mut self, held: bool) {
        self.alt_held = held;
    }
}
