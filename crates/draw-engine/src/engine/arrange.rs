use crate::edit::{
    align_elements, can_toggle_polygon, distribute_elements, flip_elements, gather, group_patches,
    is_single_group, reorder_positions, toggle_polygon, ungroup_patches, AlignMode, FlipAxis,
    ZOrderMode,
};
use std::collections::HashMap;

use crate::engine::DrawEngine;
use crate::scene::frame::FrameOwners;
use crate::scene::{bump_version, is_frame, new_element_id, DrawElement};

impl DrawEngine {
    /// The arrow keys: the same set a drag moves, so a locked group member and a
    /// frame's children come along — the oracle's arrow keys move every selected element
    /// and what their frames hold (`packages/excalidraw/components/App.tsx@1118751f:5771-5776`).
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

    /// Moves the selection through the stack: see [`crate::edit::zorder`].
    ///
    /// What the carried set holds moves, as a drag's: a locked group member comes with its
    /// group, and a loose locked element — held after a right-click or a peer's lock —
    /// stays, as does a label whose shape stays. The labels of what moves and the children
    /// of a moving frame come too, even one a peer holds: the stack is not stamped, so it
    /// takes nothing from their edit.
    pub fn reorder_selection(&mut self, mode: ZOrderMode) {
        let carried = self.carried_selection();
        if carried.is_empty() {
            return;
        }
        let next: Vec<DrawElement> = {
            let live = self.scene.ordered_refs();
            let order = reorder_positions(&live, &carried, mode, self.editing_group_id.as_deref());
            // Nothing moved, so nothing to record or send — the oracle's store records no
            // step for unchanged elements either.
            if order.iter().enumerate().all(|(at, &was)| at == was) {
                return;
            }
            order.into_iter().map(|i| live[i].clone()).collect()
        };
        // No element's content changes, only where it stands, so the host hears a delta
        // carrying the order rather than the whole scene (`Scene::reorder_live`).
        self.scene.reorder_live(next);
        self.push_history();
        self.request_draw();
    }

    /// Lines up what the selection carries, a group as one unit: see
    /// [`crate::edit::align::units`]. The carried set, as a drag's, so a locked group
    /// member comes with its group and a loose locked element stays.
    pub fn align_selection(&mut self, mode: AlignMode) {
        self.apply_patches(align_elements(
            &self.scene.ordered_cloned(),
            &self.carried_selection(),
            self.editing_group_id.as_deref(),
            mode,
        ));
    }

    pub fn distribute_selection(&mut self, axis: char) {
        self.apply_patches(distribute_elements(
            &self.scene.ordered_cloned(),
            &self.carried_selection(),
            self.editing_group_id.as_deref(),
            axis,
        ));
    }

    /// How many units align and distribute would move — none with a frame selected.
    pub(super) fn arrange_units(&self) -> usize {
        crate::edit::units(
            self.scene.iter_ordered(),
            &self.carried_selection(),
            self.editing_group_id.as_deref(),
        )
        .len()
    }

    /// Whether align has two units to line up (`alignActionsPredicate`,
    /// `actionAlign.tsx@1118751f:39-52`), so the host offers it when it would do something.
    pub fn can_align(&self) -> bool {
        self.arrange_units() > 1
    }

    /// Whether distribute has three units to space (`actionDistribute.tsx@1118751f:35-46`).
    pub fn can_distribute(&self) -> bool {
        self.arrange_units() > 2
    }

    /// Flips what a drag would move: a frame's children and a locked group member come along,
    /// as the oracle's flip takes them (`actionFlip.ts@1118751f:87-94`,
    /// `groups.ts@1118751f:94-132`).
    pub fn flip_selection(&mut self, axis: FlipAxis) {
        let flipped = self.moving_selection();
        self.forget_original_heights(&flipped);
        self.apply_patches(flip_elements(&self.scene.ordered_cloned(), &flipped, axis));
    }

    /// Whether the polygon toggle would do anything to the current selection — a line
    /// with at least four points, and only lines (`actionLinearEditor.tsx@1118751f:127-138`).
    pub fn can_toggle_polygon(&self) -> bool {
        let targets: Vec<&DrawElement> = self
            .selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .collect();
        can_toggle_polygon(&targets)
    }

    /// Closes the selected line(s) into a filled polygon, or opens them back up — the
    /// panel toggle and its keyboard shortcut (`actionTogglePolygon`).
    pub fn toggle_polygon_selection(&mut self) {
        let targets: Vec<DrawElement> = self
            .selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id).cloned())
            .collect();
        self.apply_patches(toggle_polygon(&targets));
    }

    pub(super) fn apply_patches(&mut self, patches: Vec<DrawElement>) {
        self.apply_patches_then(patches, |_| {});
    }

    /// [`Self::apply_patches`], running `then` on the patched scene before the step is
    /// recorded — so a selection it sets is the step's own, which undo and redo put back.
    pub(super) fn apply_patches_then(
        &mut self,
        patches: Vec<DrawElement>,
        then: impl FnOnce(&mut Self),
    ) {
        // A patch that changes nothing is not an edit: stamped, it would be saved, sent to
        // every peer and take a step of undo that puts nothing back — a lone unturned box
        // flipped, shapes aligned already. The oracle's `mutateElement` keeps the version
        // when no value changed (`packages/element/src/mutateElement.ts@1118751f:129-131`).
        let patches: Vec<DrawElement> = patches
            .into_iter()
            .filter(|patch| {
                self.scene
                    .get(&patch.id)
                    .is_none_or(|live| !super::stamp::same_content(patch, live))
            })
            .collect();
        if patches.is_empty() {
            return;
        }
        let now = self.now_ms;
        for element in patches {
            self.scene.put(bump_version(element, now));
        }
        then(self);
        // No frame membership pass: outside a drag the oracle's align, distribute and
        // flip leave `frameId` alone (`isElementInFrame` is true unless the selection is
        // being dragged, `packages/element/src/frame.ts@1118751f:845-855`), and a lock has no frame
        // logic at all. Grouping settles its own members in `group_selection`.
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    pub fn group_selection(&mut self) {
        // Labels are not counted: a shape and its own words are one thing to group, as
        // `enableActionGroup` reads the selection without them (`actionGroup.tsx@1118751f:73-83`).
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
        // (`packages/excalidraw/actions/actionGroup.tsx@1118751f:138-150`): each member is judged
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
            self.scene.iter_ordered(),
            &self.selected_ids,
            self.editing_group_id.as_deref(),
        )
    }

    /// The menu's Lock / Unlock — `actionToggleElementLock` (`actionElementLock.ts@1118751f:
    /// 26-150`). Locks when nothing it acts on is locked, and unlocks all of it otherwise
    /// (`shouldLock`, `:22-23`): a group holding one locked member is unlocked whole, where
    /// locking whatever was not locked yet left that member no way back but on its own.
    ///
    /// Locking lets go of the selection, as the oracle's does (`:108-110`): a locked
    /// element is not there to be held, and still held it took the Delete, the Ctrl+D and
    /// the style meant for whatever came next. Unlocking keeps it, so what was just freed
    /// is in hand. Either way it is the step's own selection, which undo puts back.
    pub fn toggle_lock_selection(&mut self) {
        let targets = self.lock_targets();
        if targets.is_empty() {
            return;
        }
        let lock = !targets.iter().any(DrawElement::locked);
        let patches = targets
            .into_iter()
            .filter(|el| el.locked() != lock)
            .map(|mut el| {
                el.locked = Some(lock);
                el
            })
            .collect();
        self.apply_patches_then(patches, |engine| {
            if lock {
                engine.clear_selection();
            }
        });
    }

    /// Whether the toggle would unlock: something it acts on is locked, so the menu reads
    /// Unlock (`label`, `actionElementLock.ts@1118751f:28-37`).
    pub fn selection_locked(&self) -> bool {
        self.lock_targets().iter().any(DrawElement::locked)
    }

    /// The board menu's "Unlock all" — `actionUnlockAllElements` (`actionElementLock.ts@1118751f:
    /// 161-217`), offered by [`Self::can_unlock_all`]: every locked element unlocked and
    /// selected, grown to groups, so what was just freed is in hand. One step of undo.
    pub fn unlock_all(&mut self) {
        let patches: Vec<DrawElement> = self
            .scene
            .iter_ordered()
            .filter(|el| el.locked())
            .cloned()
            .map(|mut el| {
                el.locked = Some(false);
                el
            })
            .collect();
        let freed: Vec<String> = patches
            .iter()
            .filter(|el| el.container_id.is_none())
            .map(|el| el.id.clone())
            .collect();
        self.apply_patches_then(patches, |engine| {
            engine.editing_group_id = None;
            let ids = crate::edit::expand_within(engine.scene.iter_ordered(), freed, None);
            engine.set_selection(ids);
        });
    }

    /// Whether "Unlock all" is on offer: nothing selected, and something locked
    /// (`predicate`, `actionElementLock.ts@1118751f:166-171`).
    pub fn can_unlock_all(&self) -> bool {
        self.selected_ids.is_empty() && self.scene.iter_ordered().any(DrawElement::locked)
    }

    /// What Lock and Unlock act on: the selection, what its frames hold and the labels of
    /// all of it — `getSelectedElements` with `includeElementsInFrames` and
    /// `includeBoundTextElement` (`actionElementLock.ts@1118751f:52-56`). Left out, a locked
    /// shape's words and a locked frame's children stayed free, and were picked up and
    /// dragged out of the frame.
    fn lock_targets(&self) -> Vec<DrawElement> {
        let mut ids = self.selected_ids.clone();
        for id in &self.selected_ids {
            if self.scene.get(id).is_some_and(is_frame) {
                ids.extend(crate::scene::frame_children(self.scene.iter_ordered(), id));
            }
        }
        let ids = crate::edit::with_labels(self.scene.iter_ordered(), &ids);
        self.scene
            .iter_ordered()
            .filter(|el| ids.contains(&el.id))
            .cloned()
            .collect()
    }
}
