//! Renaming a frame: Excalidraw parity for `editingFrame`
//! (`App.tsx@1118751f:2166-2170` `resetEditingFrame`, `:2219-2261` the input it opens,
//! `:2334-2340` the double click that starts it) and `getFrameLikeTitle`
//! (`packages/element/src/frame.ts@1118751f:974-981`).
//!
//! Unlike a text edit ([`super::text_session`]), typing a name has nothing to lay out on
//! the board as it goes — no shape to grow, no wrap to recompute — so there is no session
//! here at all: the host's input holds the value uncommitted, exactly as the text editor's
//! textarea does before its own commit, and one call writes it, once, as one step.

use crate::engine::{DrawEngine, FrameRenameRequest};
use crate::scene::{frame_display_name, frame_name_anchor, is_frame, DrawElement};
use crate::text::layout::Measure;
use crate::text::FontKey;

impl DrawEngine {
    /// The screen-space box the frame's name label occupies — left, top, width, height —
    /// exactly as `paint_frame_names` (`engine/frame.rs`) draws it: left-aligned, baseline
    /// at the anchor `world_to_screen` puts on screen.
    fn frame_name_box(&self, frame: &DrawElement) -> (f64, f64, f64, f64) {
        let name = frame_display_name(frame);
        let anchor = frame_name_anchor(frame);
        let at = crate::world_to_screen(self.camera, anchor.x, anchor.y);
        let font = FontKey::legacy(crate::scene::FRAME_NAME_FONT_SIZE);
        let (width, height) = self.with_measure(|measure: &Measure| {
            measure.size(&name, font, crate::scene::FRAME_NAME_LINE_HEIGHT)
        });
        (at.x, at.y - height, width, height)
    }

    /// The topmost frame whose painted name label contains the screen point `(sx, sy)`,
    /// if any — the hit test a double click on the name runs before anything else
    /// (`handle_double_click`, `engine/text.rs`).
    pub fn frame_name_at(&self, sx: f64, sy: f64) -> Option<String> {
        self.scene
            .iter_ordered()
            .filter(|element| is_frame(element) && !element.is_deleted)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .find_map(|frame| {
                let (x, y, width, height) = self.frame_name_box(frame);
                (sx >= x && sx <= x + width && sy >= y && sy <= y + height)
                    .then(|| frame.id.clone())
            })
    }

    /// Opens the host's input over `frame_id`'s name — pushes one [`FrameRenameRequest`],
    /// the way [`super::text::DrawEngine::request_text_edit`] opens a text.
    pub(crate) fn request_frame_rename(&mut self, frame_id: &str) {
        let Some(frame) = self.scene.get(frame_id).filter(|el| !el.is_deleted) else {
            return;
        };
        let (x, y, width, height) = self.frame_name_box(frame);
        self.events.frame_rename = Some(FrameRenameRequest {
            id: frame.id.clone(),
            name: frame_display_name(frame),
            x,
            y,
            width,
            height,
            font_size: crate::scene::FRAME_NAME_FONT_SIZE,
            color: self.theme.frame_name.clone(),
        });
    }

    /// Commits `name`, trimmed, as one step: syncs to peers and autosave like any edit,
    /// the stamp moving with it (`apply_patches`, `engine/arrange.rs`). Emptied, the name
    /// falls back to the generic default the way the oracle's `resetEditingFrame` does
    /// (`frame.name?.trim() || null`) — never blank, only ever unnamed. A no-op, with no
    /// step taken, when nothing actually changes, or when `id` is not a live frame.
    pub fn rename_frame(&mut self, id: &str, name: &str) {
        let Some(mut next) = self
            .scene
            .get(id)
            .filter(|el| !el.is_deleted && is_frame(el))
            .cloned()
        else {
            return;
        };
        let trimmed = name.trim();
        next.name = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.apply_patches(vec![next]);
    }
}
