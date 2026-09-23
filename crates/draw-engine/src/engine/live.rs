//! Which elements the gesture in progress changes from frame to frame.
//!
//! The painter caches everything else and draws only these each frame, so a drag or a
//! stroke on a board of thousands of elements costs what it touches. Measured at 15% zoom
//! on 2,000 freehand strokes, every frame of drawing a new stroke repainted all 2,000 of
//! them.
//!
//! The set has to be complete. Anything that changes during the gesture but is left out
//! of it would be frozen in the cached layer at its old place — and the scene's
//! `static_revision` would give it away, since a change to a non-live element moves it
//! and throws the cache out, so the failure is a slow frame rather than a wrong one. The
//! set is recomputed at every input step, because a gesture can change what it touches:
//! an Alt-drag creates the copies it then moves.

use std::collections::HashSet;

use crate::engine::{DrawEngine, Interaction};

impl DrawEngine {
    /// Recomputes the live set and hands it to the scene.
    pub(crate) fn refresh_live(&mut self) {
        let live = self.live_ids();
        self.scene.set_live(live);
    }

    fn live_ids(&self) -> HashSet<String> {
        let mut ids: HashSet<String> = HashSet::new();
        match &self.interaction {
            Some(
                Interaction::Draft { id, .. }
                | Interaction::TextDraft { id, .. }
                | Interaction::Linear { id, .. }
                | Interaction::Freedraw { id, .. }
                | Interaction::Resize { id, .. }
                | Interaction::Rotate { id }
                | Interaction::LinearPoint { id, .. }
                | Interaction::CornerRadius { id, .. },
            ) => {
                ids.insert(id.clone());
            }
            Some(
                Interaction::Move { ids: moving, .. }
                | Interaction::ResizeGroup { ids: moving, .. }
                | Interaction::RotateGroup { ids: moving, .. },
            ) => ids.extend(moving.iter().cloned()),
            _ => {}
        }
        // A path placed point by point follows the pointer between clicks, with no
        // gesture in progress at all.
        if let Some(path) = self.linear_in_progress() {
            ids.insert(path);
        }
        if ids.is_empty() {
            return ids;
        }

        // What moves with them: a label with its shape and a shape with its label, an
        // arrow re-routed after a shape it is bound to, the contents of a frame. One
        // pass, then a second for the labels of what the first pass brought in.
        let mut follow: Vec<String> = Vec::new();
        for element in self.scene.iter_ordered() {
            let attached =
                |other: &Option<String>| other.as_ref().is_some_and(|id| ids.contains(id));
            if attached(&element.container_id)
                || attached(&element.start_binding)
                || attached(&element.end_binding)
                || attached(&element.frame_id)
            {
                follow.push(element.id.clone());
            }
            if ids.contains(&element.id) {
                if let Some(label) = &element.bound_text_id {
                    follow.push(label.clone());
                }
            }
        }
        ids.extend(follow);
        let labels: Vec<String> = ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter_map(|element| element.bound_text_id.clone())
            .collect();
        ids.extend(labels);
        ids
    }

    /// The live set, for tests and the debug snapshot.
    pub fn debug_live(&self) -> HashSet<String> {
        self.scene.live().clone()
    }

    /// The scene's `static_revision`, for tests.
    pub fn debug_static_revision(&self) -> u64 {
        self.scene.static_revision()
    }
}
