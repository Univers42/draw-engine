use crate::edit::{
    align_elements, distribute_elements, flip_elements, group_patches, is_single_group,
    reorder_elements, ungroup_patches, AlignMode, FlipAxis, ZOrderMode,
};
use crate::engine::DrawEngine;
use crate::scene::{bump_version, new_element_id, DrawElement};

impl DrawEngine {
    pub fn nudge_selection(&mut self, dx: f64, dy: f64) {
        let targets: Vec<DrawElement> = self
            .get_selected_elements()
            .into_iter()
            .filter(|el| !el.locked())
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
        let next = reorder_elements(&self.scene.ordered_cloned(), &self.selected_ids, mode);
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

    pub fn flip_selection(&mut self, axis: FlipAxis) {
        self.apply_patches(flip_elements(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
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
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    pub fn group_selection(&mut self) {
        if self.selected_ids.len() < 2 {
            return;
        }
        self.apply_patches(group_patches(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
            &new_element_id(),
        ));
    }

    pub fn ungroup_selection(&mut self) {
        self.apply_patches(ungroup_patches(
            &self.scene.ordered_cloned(),
            &self.selected_ids,
        ));
    }

    pub fn selection_is_group(&self) -> bool {
        is_single_group(&self.scene.ordered_cloned(), &self.selected_ids)
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
