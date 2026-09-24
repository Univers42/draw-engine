use crate::edit::{
    align_elements, distribute_elements, flip_elements, gather, group_patches, is_single_group,
    reorder_within, ungroup_patches, AlignMode, FlipAxis, ZOrderMode,
};
use std::collections::HashMap;

use crate::engine::DrawEngine;
use crate::scene::frame::FrameOwners;
use crate::scene::{bump_version, is_frame, new_element_id, DrawElement};

impl DrawEngine {
    /// The arrow keys: the same set a drag moves, so a locked group member and a
    /// frame's children come along — the oracle's arrow keys move every selected element
    /// and what their frames hold (`packages/excalidraw/components/App.tsx:5770-5775`).
    pub fn nudge_selection(&mut self, dx: f64, dy: f64) {
        let targets: Vec<DrawElement> = self
            .moving_selection()
            .iter()
            .filter_map(|id| self.scene.get(id).cloned())
            .collect();
        if targets.is_empty() {
            return;
        }
        for mut element in targets {
            element.x += dx;
            element.y += dy;
            self.scene.put(element);
        }
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    pub fn reorder_selection(&mut self, mode: ZOrderMode) {
        if self.selected_ids.is_empty() {
            return;
        }
        let next = reorder_within(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
            mode,
            self.editing_group_id.as_deref(),
        );
        self.scene.set_order(next);
        self.push_history();
        self.request_draw();
    }

    pub fn align_selection(&mut self, mode: AlignMode) {
        self.apply_patches(align_elements(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
            mode,
        ));
    }

    pub fn distribute_selection(&mut self, axis: char) {
        self.apply_patches(distribute_elements(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
            axis,
        ));
    }

    /// Flips what a drag would move: a frame's children and a locked group member come
    /// along, as the oracle's flip takes them (`actionFlip.ts:87-94`, `groups.ts:94-132`).
    pub fn flip_selection(&mut self, axis: FlipAxis) {
        self.apply_patches(flip_elements(
            &self.scene.ordered_cloned(),
            &self.moving_selection(),
            axis,
        ));
    }

    pub(super) fn apply_patches(&mut self, patches: Vec<DrawElement>) {
        if patches.is_empty() {
            return;
        }
        let now = self.now_ms;
        for element in patches {
            self.scene.put(bump_version(element, now));
        }
        // No frame membership pass: outside a drag the oracle's align, distribute and
        // flip leave `frameId` alone (`isElementInFrame` is true unless the selection is
        // being dragged, `packages/element/src/frame.ts:845-855`), and a lock has no frame
        // logic at all. Grouping settles its own members in `group_selection`.
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    pub fn group_selection(&mut self) {
        // Labels are not counted: a shape and its own words are one thing to group, as
        // `enableActionGroup` reads the selection without them (`actionGroup.tsx:73-83`).
        let shapes = self
            .selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| el.container_id.is_none())
            .count();
        if shapes < 2 {
            return;
        }
        let editing = self.editing_group_id.clone();
        let live = self.scene.ordered_cloned();
        let mut patches = group_patches(
            &live,
            &self.selected_ids,
            &new_element_id(),
            editing.as_deref(),
        );
        // A label comes along with its shape, but not one a peer holds — they may be
        // typing into it, and their commit would stamp above this and take it back out.
        patches.retain(|el| !self.held.contains_key(&el.id));
        // Grouping across a frame's edge takes the group out whole
        // (`packages/excalidraw/actions/actionGroup.tsx:138-150`): each member is judged
        // with its new group's box, and nothing else on the board is touched.
        let owners: Vec<Option<String>> = {
            let patched: HashMap<&str, &DrawElement> =
                patches.iter().map(|el| (el.id.as_str(), el)).collect();
            let next: Vec<&DrawElement> = live
                .iter()
                .map(|el| patched.get(el.id.as_str()).copied().unwrap_or(el))
                .collect();
            let frames = FrameOwners::new(next.iter().copied());
            patches.iter().map(|el| frames.of(el)).collect()
        };
        for (element, frame_id) in patches.iter_mut().zip(owners) {
            if !is_frame(element) {
                element.frame_id = frame_id;
            }
        }
        // Gathered in the same step as the grouping, so one undo takes both away. Only
        // when it moves something: a reorder sends the host the whole scene, where a
        // grouping alone is a delta.
        let members = patches.iter().map(|el| el.id.clone()).collect();
        let gathered = gather(&live, &members);
        if gathered
            .iter()
            .map(|el| &el.id)
            .ne(live.iter().map(|el| &el.id))
        {
            self.scene.set_order(gathered);
        }
        self.apply_patches(patches);
    }

    pub fn ungroup_selection(&mut self) {
        let editing = self.editing_group_id.clone();
        self.apply_patches(ungroup_patches(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
            editing.as_deref(),
        ));
    }

    /// Ctrl+G: group what is loose, ungroup what is already exactly one group.
    ///
    /// A deliberate divergence. Excalidraw's Ctrl+G on an already-grouped selection is a
    /// **no-op** — observed on excalidraw.com, not assumed — which leaves the key with no
    /// inverse, so there is no way out of a group using the key you reached for. A toggle
    /// costs nothing and makes the key undo itself; Ctrl+Shift+G still matches the
    /// oracle exactly, so nothing is given up.
    ///
    /// On a nested selection it peels **one** level, because `ungroup_selection` removes
    /// only the level `selected_group_for` would have selected. Flattening would make one
    /// grouping impossible to undo without losing the structure beneath it.
    pub fn toggle_group_selection(&mut self) {
        if self.selection_is_group() {
            self.ungroup_selection();
        } else {
            self.group_selection();
        }
    }

    pub fn selection_is_group(&self) -> bool {
        is_single_group(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
            self.editing_group_id.as_deref(),
        )
    }

    pub fn toggle_lock_selection(&mut self) {
        let selected = self.get_selected_elements();
        if selected.is_empty() {
            return;
        }
        let lock = selected.iter().any(|el| !el.locked());
        self.apply_patches(
            selected
                .into_iter()
                .map(|mut el| {
                    el.locked = Some(lock);
                    el
                })
                .collect(),
        );
    }

    pub fn selection_locked(&self) -> bool {
        let selected = self.get_selected_elements();
        !selected.is_empty() && selected.iter().all(DrawElement::locked)
    }
}
