//! The presentation path: the order a presentation visits the board's frames in.
//!
//! Prezi's path, over Excalidraw's frames. Each frame carries its own step
//! ([`DrawElement::path_step`](crate::scene::DrawElement::path_step)), so moving one
//! frame on the path changes only the frames whose step changed. Two people reordering
//! at once then merge like any other edit, element by element. Ordering the steps into
//! slides is the host's (`presentation.ts` › `slidesFromScene`); the engine only stores
//! them.

use crate::engine::DrawEngine;
use crate::scene::is_frame;

impl DrawEngine {
    /// Numbers the live frames among `ids` 0, 1, 2… in the order given, as one step that
    /// syncs and autosaves like any edit. Ids that are not live frames are skipped without
    /// taking a number. Frames not listed keep the step they had. A no-op, with no step
    /// taken, when every listed frame already has its number.
    pub fn set_presentation_path(&mut self, ids: &[String]) {
        let frames = ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| !el.is_deleted && is_frame(el));
        let changed: Vec<_> = frames
            .enumerate()
            .filter_map(|(step, frame)| {
                let step = u32::try_from(step).ok()?;
                (frame.path_step != Some(step)).then(|| {
                    let mut next = frame.clone();
                    next.path_step = Some(step);
                    next
                })
            })
            .collect();
        if !changed.is_empty() {
            self.apply_patches(changed);
        }
    }
}
