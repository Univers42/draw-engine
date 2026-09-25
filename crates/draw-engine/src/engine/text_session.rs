//! The text being typed: one element open in the host's editor, laid out on every
//! keystroke and committed once.
//!
//! A port of the session Excalidraw's `textWysiwyg` runs over a text element
//! (`packages/excalidraw/wysiwyg/textWysiwyg.tsx@1118751f`) and the `onChange` /
//! `onSubmit` the app gives it (`handleTextWysiwyg`, `App.tsx@1118751f:6344-6515`):
//!
//! - every keystroke replaces the element with what was typed, wrapped and measured, and
//!   grows its shape — or shrinks it back, never below the height it had when the edit
//!   began — with no history step and no stamp: nothing leaves the engine until the
//!   commit, which records one step, as a drag does;
//! - the canvas does not paint the element being typed (`Renderer.ts@1118751f:259-267`):
//!   the editor is its only copy, so the two can never show it twice. Its shape and the
//!   arrows bound to it are live and follow it frame by frame;
//! - what is typed travels to peers on the gesture channel ([`DrawEngine::gesture_elements`]),
//!   and a peer's copy of it is refused until the commit — the pending-change rule of
//!   `stamp.rs`. A peer deleting it meanwhile loses: the commit stamps above their
//!   tombstone, as it does over any edit refused mid-gesture. A session whose edit came
//!   to nothing adopts their copy at the commit instead;
//! - undo and redo do nothing while it is open. The editor's own undo is the one that
//!   applies, as the keys go to it in the oracle.
//!
//! Divergence: the element stays selected while it is typed (the oracle deselects
//! everything, `App.tsx@1118751f:6509-6510`), so the properties panel keeps working on
//! it. Its selection frame and handles are not drawn.

use crate::engine::DrawEngine;
use crate::interaction::DrawTool;
use crate::scene::{DrawElement, DrawElementType, TextAlign, VerticalAlign};
use crate::text::layout::{self, Laid};

/// The text open in the host's editor.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditSession {
    pub id: String,
    /// Made to be typed into — a click of the text tool, a double click on the board or
    /// on a shape with no label — rather than an existing text reopened. Emptied, it was
    /// never there: no step, no tombstone.
    pub is_new: bool,
    /// The height of the label's shape when the edit began: typing grows the shape, and
    /// deleting shrinks it back down to this and no further (`originalContainerCache`,
    /// `textWysiwyg.tsx@1118751f:331-373`). The commit leaves it for "Unbind text" to give
    /// back, as the oracle's cache does (`engine/bound_text.rs`). `None` for free text.
    pub original_container_height: Option<f64>,
}

/// What the host's editor needs to sit exactly over the text it replaces: the element's
/// own box, in world units, and the camera it is seen through — `updateWysiwygStyle`
/// (`textWysiwyg.tsx@1118751f:268-450`) reads the same. Re-read after every keystroke,
/// camera change and style change.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEditLayout {
    pub id: String,
    /// Screen position of the text's unrotated top-left, canvas-relative.
    pub x: f64,
    pub y: f64,
    /// The text box in world units: the editor is scaled by `zoom`, not sized by it.
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    /// Unitless.
    pub line_height: f64,
    /// An Excalidraw family id, `0` for the system stack of a text with none.
    pub font_family: u8,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    /// Radians: the label's shape's, `0` on an arrow.
    pub angle: f64,
    /// The camera scale.
    pub zoom: f64,
    /// As the painter fills the glyphs: the stroke colour, untouched by the theme.
    pub color: String,
    /// `0..=1`.
    pub opacity: f64,
    /// Whether the lines wrap at `width` (`pre-wrap`) or keep their hard breaks (`pre`) —
    /// [`layout::wrap_width`], the rule the engine wraps by.
    pub wrap: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
}

impl DrawEngine {
    /// Opens `element` for typing, committing any other text still open first — only one
    /// is ever typed at a time.
    ///
    /// The element is marked changed from the start, so a peer's copy is refused from the
    /// first frame rather than only after the first keystroke, and the painter's cached
    /// pictures, which hold it drawn, are not taken for pictures without it.
    pub(crate) fn open_text_session(&mut self, element: &DrawElement) {
        if let Some(open) = self.text_session.as_ref() {
            if open.id == element.id {
                return;
            }
            let typed = self
                .scene
                .get(&open.id)
                .map(|open| crate::scene::source_text(open).to_owned())
                .unwrap_or_default();
            // What the caller selected for the new one outlives the old one's ending.
            let selected: Vec<String> = self.selected_ids.iter().cloned().collect();
            self.commit_text_edit(&typed, false);
            let live: Vec<String> = selected
                .into_iter()
                .filter(|id| self.scene.get(id).is_some_and(|el| !el.is_deleted))
                .collect();
            self.set_selection(live);
        }
        let original_container_height = self
            .container_of(element)
            .map(|container| container.height.abs());
        self.text_session = Some(TextEditSession {
            id: element.id.clone(),
            is_new: self.scene.created_since_commit(&element.id),
            original_container_height,
        });
        // Live before it is touched, so the touch does not throw the static layer away.
        self.refresh_live();
        self.scene.update(&element.id, |_| {});
        self.touch_style();
        self.request_draw();
    }

    /// The text open for typing, if one is.
    pub fn text_edit_session(&self) -> Option<&TextEditSession> {
        self.text_session.as_ref()
    }

    /// The open text with `text` typed into it: re-wrapped, measured and placed, its shape
    /// grown or shrunk to hold it and the arrows bound to that shape following. No step and
    /// no stamp. `false` when no text is open — the host's editor has nothing left to type
    /// into, and closes.
    pub fn update_text_edit(&mut self, text: &str) -> bool {
        let Some(session) = self.text_session.clone() else {
            return false;
        };
        let Some(element) = self.live_element(&session.id) else {
            // Gone from under the editor, a session left open would keep undo off.
            self.drop_text_session(None);
            self.refresh_live();
            return false;
        };
        self.type_into(&element, text, session.original_container_height);
        self.refresh_live();
        self.request_draw();
        true
    }

    /// Ends the edit with `text` as what was typed, as one step of history.
    ///
    /// Emptied, a text goes: one made for this edit leaves no trace at all — no step, no
    /// tombstone, its shape as it was — and an existing one is deleted, as a step
    /// (`textWysiwyg.tsx@1118751f:818-872`, `App.tsx@1118751f:6439-6498`).
    ///
    /// `via_keyboard` — Escape or Ctrl/Cmd+Enter — leaves the label's shape selected, or
    /// the text, so the keyboard carries on from it (Enter opens it again); any other
    /// ending lets go of it, as the oracle's selection is empty then. With the tool locked,
    /// or the autoshape tool, which stays on through the edit, nothing is selected either
    /// way (`App.tsx@1118751f:6446-6453`).
    ///
    /// A preview still showing is given back first: a family only hovered in the font
    /// list is not what the edit commits. A press on the board ends the edit before the
    /// list closes, where the oracle's picker puts back the text it cached when it opened
    /// (`actionProperties.tsx@1118751f:1483-1500`); committed with the edit instead, the
    /// hovered family was stamped and sent, and the list closing had nothing to give back.
    pub fn commit_text_edit(&mut self, text: &str, via_keyboard: bool) {
        let Some(session) = self.text_session.take() else {
            return;
        };
        self.give_back_style_preview();
        let Some(element) = self.live_element(&session.id) else {
            // Deleted meanwhile, here: nothing is left to write into.
            self.push_history();
            self.refresh_live();
            self.request_draw();
            return;
        };
        let keep = via_keyboard && !self.tool_locked && self.tool != DrawTool::AutoShape;
        if text.trim().is_empty() {
            // Selected before the step is taken, so redo selects it again.
            match element.container_id.clone().filter(|_| keep) {
                // The shape stays selected even though its label is gone.
                Some(container) => self.set_selection(vec![container]),
                None => self.clear_selection(),
            }
            self.remove_emptied_text(&element);
            self.refresh_live();
            self.request_draw();
            return;
        }
        self.type_into(&element, text, session.original_container_height);
        // The shape's height when the editor opened is what "Unbind text" gives back,
        // unless it remembers one already (`textWysiwyg.tsx@1118751f:326-345`). The oracle
        // notes it at the editor's first layout; here at the commit, so an edit that
        // leaves no trace leaves none here either.
        if let (Some(container), Some(height)) = (
            element.container_id.clone(),
            session.original_container_height,
        ) {
            self.original_container_heights
                .entry(container)
                .or_insert(height);
        }
        if keep {
            let id = element.container_id.clone().unwrap_or(element.id);
            self.set_selection(vec![id]);
        } else {
            self.clear_selection();
        }
        self.push_history();
        self.refresh_live();
        self.request_draw();
    }

    /// Where the open text is and how it looks, for the host's editor. `None` when none
    /// is open.
    pub fn text_edit_layout(&self) -> Option<TextEditLayout> {
        let session = self.text_session.as_ref()?;
        let element = self.scene.get(&session.id).filter(|el| !el.is_deleted)?;
        let screen = crate::world_to_screen(self.camera, element.x, element.y);
        Some(TextEditLayout {
            id: element.id.clone(),
            x: screen.x,
            y: screen.y,
            width: element.width,
            height: element.height,
            font_size: layout::font_size_of(element),
            line_height: crate::scene::resolved_line_height(element),
            font_family: crate::scene::resolved_font_family(element)
                .unwrap_or(crate::text::FontKey::LEGACY),
            text_align: crate::scene::resolved_text_align(element),
            vertical_align: crate::scene::resolved_vertical_align(element),
            angle: element.angle,
            zoom: self.camera.scale,
            color: element.stroke_color.clone(),
            opacity: element.opacity / 100.0,
            wrap: layout::wrap_width(element, self.container_of(element)).is_some(),
            container_id: element.container_id.clone(),
        })
    }

    /// The id of the text open for typing, which the painter leaves out.
    pub(crate) fn editing_text_id(&self) -> Option<&str> {
        self.text_session
            .as_ref()
            .map(|session| session.id.as_str())
    }

    /// Ends the session without writing anything — for the legacy one-shot
    /// [`Self::set_element_text`], which then commits as it always has, and for a scene
    /// replaced wholesale.
    pub(crate) fn drop_text_session(&mut self, id: Option<&str>) {
        if id.is_none_or(|id| self.editing_text_id() == Some(id)) {
            self.text_session = None;
        }
    }

    /// Marks the open text changed again after a step taken while it is typed — a text
    /// wrapped in a shape from the panel commits it as typed so far; a style does not
    /// ([`Self::commit_style`]) — so a peer's copy of it is still refused until the edit
    /// ends.
    ///
    /// A step that deleted it — `delete_selection` from a host that ends an edit the way
    /// it did before the session — ends the session instead: nothing is left to type
    /// into, and a session left open would keep undo and redo off.
    pub(crate) fn keep_text_session_pending(&mut self) {
        let Some(id) = self.editing_text_id().map(str::to_owned) else {
            return;
        };
        if self.live_element(&id).is_some() {
            self.scene.update(&id, |_| {});
        } else {
            self.drop_text_session(None);
            self.refresh_live();
        }
    }

    fn live_element(&self, id: &str) -> Option<DrawElement> {
        self.scene.get(id).filter(|el| !el.is_deleted).cloned()
    }

    /// Writes `text` into `element`: laid out as every writer lays text out, its shape
    /// resized to hold it — shrunk back no lower than `floor`, the session's
    /// [`TextEditSession::original_container_height`] — and the arrows bound to that shape
    /// following. Unstamped: the commit stamps it.
    pub(crate) fn type_into(&mut self, element: &DrawElement, text: &str, floor: Option<f64>) {
        let Laid {
            text: mut next,
            container,
        } = self.with_text(element, text);
        if let Some(current) = self.container_of(&next).cloned() {
            // A shape a peer took while its label was being typed stays as they have it.
            // ponytail: the label may overflow it until it is next laid out.
            let shape = if self.untouchable(&current) {
                current
            } else {
                let shape = shrunk(container.unwrap_or_else(|| current.clone()), &next, floor);
                if shape != current {
                    self.scene.put(shape.clone());
                }
                shape
            };
            // The label sits in its shape as the shape now is.
            let at = layout::bound_text_position(&shape, &next);
            next.x = at.x;
            next.y = at.y;
        }
        self.scene.put(next);
        // The arrows bound to a shape that grew follow it.
        self.apply_bindings();
    }

    /// A text emptied, as one step: gone. One made since the last commit — for this edit —
    /// was never there: no tombstone, and no step. The session's commit and the one-shot
    /// [`Self::set_element_text`] both empty a text through here.
    pub(crate) fn remove_emptied_text(&mut self, element: &DrawElement) {
        if self.scene.created_since_commit(&element.id) {
            // Everything made or moved for it goes back as it was, as an abandoned gesture
            // puts back what it changed: the label, its shape told of it and grown to one
            // line, the arrows that followed the shape and their own labels. Divergence:
            // the oracle leaves a new label's shape grown to one line
            // (`App.tsx@1118751f:6974-7006`), uncaptured until the next step.
            self.scene.discard(&element.id);
            for (id, _) in self.scene.baseline() {
                self.drop_local_change(&id);
            }
            // Nor does the shape remember a height a style grew it to meanwhile.
            self.forget_original_heights(element.container_id.as_slice());
            // Its place above its shape was the only reorder, and it went with it.
            let before = self.scene.order_baseline().map(|mut order| {
                order.retain(|id| *id != element.id);
                order
            });
            if before.is_some_and(|before| before == self.scene.ids()) {
                self.scene.set_order_baseline(None);
            }
            self.push_history();
            return;
        }
        if let Some(container_id) = &element.container_id {
            let unbound = self
                .scene
                .get(container_id)
                .filter(|container| container.bound_text_id.as_deref() == Some(&element.id))
                .cloned();
            if let Some(mut container) = unbound {
                container.bound_text_id = None;
                self.scene.put(container);
            }
        }
        // A committed text emptied is a deletion, and a deletion is a tombstone: the server
        // and the peers have to be told, and undo has to be able to stamp it back.
        self.scene.remove(&element.id, self.now_ms);
        self.push_history();
    }
}

/// `shape` shrunk back toward the label in it, never below `floor` — the auto-shrink of
/// `updateWysiwygStyle` (`textWysiwyg.tsx@1118751f:359-373`). A line or arrow never
/// resizes for its label.
///
/// Divergence: the oracle shrinks to fit the label in one step once the shape is taller
/// than it was, which can take it below its height at the start of the edit; the floor
/// keeps it there.
fn shrunk(mut shape: DrawElement, label: &DrawElement, floor: Option<f64>) -> DrawElement {
    let Some(floor) = floor else {
        return shape;
    };
    if crate::scene::is_linear_element(&shape) || shape.kind == DrawElementType::Text {
        return shape;
    }
    let height = shape.height.abs();
    if height <= floor || label.height >= layout::bound_text_max_height(&shape, label.height) {
        return shape;
    }
    let target = layout::container_dimension_for_bound_text(label.height, shape.kind).max(floor);
    if target < height {
        let rect = crate::scene::normalize_rect(shape.x, shape.y, shape.width, shape.height);
        shape.x = rect.x;
        shape.y = rect.y;
        shape.width = rect.width;
        shape.height = target;
    }
    shape
}
