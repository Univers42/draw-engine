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
            .filter(|el| !el.locked() && crate::segment_hits_element(el, from, to, tolerance))
            .map(|el| el.id.clone())
            .collect();
        if touched.is_empty() {
            return;
        }

        let restore = self.alt_held;
        let mut changed = false;
        for id in touched {
            if restore != self.erasing.contains(&id) {
                // Restoring what is not marked, or marking what already is.
                continue;
            }
            for member in self.erased_with(&id) {
                changed |= if restore {
                    self.erasing.remove(&member)
                } else {
                    self.erasing.insert(member)
                };
            }
        }
        if changed {
            self.erasing_revision = self.erasing_revision.wrapping_add(1);
            self.request_draw();
        }
    }

    /// What goes with an element: its whole outermost group, and a container with its
    /// label either way round — as `updateElementsToBeErased` takes them. A group is
    /// one thing, and a label left behind is debris pinned to a shape that is gone.
    fn erased_with(&self, id: &str) -> Vec<String> {
        let Some(element) = self.scene.get(id) else {
            return Vec::new();
        };
        let mut members: Vec<String> = match element.group_ids.last() {
            Some(outermost) => self
                .scene
                .iter_ordered()
                .filter(|el| el.group_ids.contains(outermost))
                .map(|el| el.id.clone())
                .collect(),
            None => vec![id.to_string()],
        };
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

        let mut doomed = marked.clone();
        for id in &marked {
            if self.scene.get(id).is_some_and(crate::scene::is_frame) {
                doomed.extend(crate::scene::frame_children(self.scene.iter_ordered(), id));
            }
        }
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
