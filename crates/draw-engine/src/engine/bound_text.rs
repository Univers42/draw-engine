//! Binding a text into a shape, giving a label back, and wrapping a text in a new shape:
//! the oracle's three bound-text actions (`packages/excalidraw/actions/actionBoundText.tsx@1118751f`).
//!
//! Each is one step of undo and changes only what a style may change
//! ([`DrawEngine::restylable`]): a loose locked element, or anything a peer holds, is
//! passed by. What each is offered for is read by the panel and the context menu from
//! [`DrawEngine::selection_style`], which asks the same questions as the actions below —
//! [`DrawEngine::bind_pair`], a label reached through [`DrawEngine::label_of`], a free text.

use std::cell::OnceCell;
use std::collections::HashSet;

use super::DrawEngine;
use crate::scene::{
    create_element, is_bindable_element, is_linear_element, resolved_line_height, source_text,
    DrawElement, DrawElementType, Geometry, TextAlign, VerticalAlign,
};
use crate::text::layout;

/// `isTextBindableContainer` (`packages/element/src/typeChecks.ts@1118751f:240-253`): the
/// three shapes and the arrow — the oracle's sticky note is a kind this engine does not
/// have. Never a line.
fn holds_text(element: &DrawElement) -> bool {
    is_bindable_element(element) || element.kind == DrawElementType::Arrow
}

/// A text on its own: `isTextElement && !isBoundToContainer`.
fn is_free_text(element: &DrawElement) -> bool {
    element.kind == DrawElementType::Text && element.container_id.is_none()
}

/// Makes `text` the label of `container_id`, centred in the middle and sizing itself —
/// what both binding actions write (`:178-183`, `:348-354`).
fn as_label(mut text: DrawElement, container_id: &str) -> DrawElement {
    text.container_id = Some(container_id.to_owned());
    text.text_align = Some(TextAlign::Center);
    text.vertical_align = Some(VerticalAlign::Middle);
    // `autoResize: true`, which this engine writes as unset.
    text.auto_resize = None;
    text
}

impl DrawEngine {
    /// `actionBindText`'s predicate (`:128-154`): exactly two selected, a free text and a
    /// shape that can hold one and holds none, both ones a style may change. The text
    /// comes first.
    pub(super) fn bind_pair<'a>(
        &self,
        selected: &[&'a DrawElement],
        carried: &OnceCell<HashSet<String>>,
    ) -> Option<(&'a DrawElement, &'a DrawElement)> {
        let [a, b] = selected else {
            return None;
        };
        let (text, container) = if is_free_text(a) { (*a, *b) } else { (*b, *a) };
        (is_free_text(text)
            && holds_text(container)
            && self.live_label(container).is_none()
            && self.restylable(text, carried)
            && self.restylable(container, carried))
        .then_some((text, container))
    }

    /// "Bind text to the container" (`actionBindText.perform`, `:155-215`): the text
    /// becomes the shape's label, laid out in it — which grows a shape too small to hold
    /// it — directly above it in the stack, and the shape is what is left selected. The
    /// shape's height is remembered for [`Self::unbind_text`] to give back (`:204-208`).
    ///
    /// The text keeps its own groups, as the oracle's does; a label the engine makes by
    /// double click takes its shape's (`text.rs`).
    pub fn bind_text(&mut self) {
        let carried = OnceCell::new();
        let selected = self.panel_selection();
        let Some((text, container)) = self
            .bind_pair(&selected, &carried)
            .map(|(text, container)| (text.clone(), container.clone()))
        else {
            return;
        };
        let mut container = container;
        container.bound_text_id = Some(text.id.clone());
        self.original_container_heights
            .insert(container.id.clone(), container.height);
        let label = as_label(text, &container.id);
        let (label_id, container_id) = (label.id.clone(), container.id.clone());
        self.scene.put(container);
        let laid = self.laid_out(&label);
        if let Some(grown) = laid.container {
            self.scene.put(grown);
        }
        self.scene.put(laid.text);
        // `pushTextAboveContainer` (`:218-236`).
        self.scene
            .place_above(std::slice::from_ref(&label_id), &container_id);
        self.apply_bindings();
        self.set_selection([container_id]);
        self.push_history();
        self.request_draw();
    }

    /// "Unbind text" (`actionUnbindText.perform`, `:69-121`): each selected shape's label
    /// is free text again — its typed lines, measured as they are, where it stood — and
    /// the shape takes back the height it had before a text was bound into it this
    /// session, or keeps the one it has.
    ///
    /// Divergence: an arrow keeps its geometry. The oracle writes the remembered height
    /// onto any container, and an arrow's extent is its points, which would contradict it.
    pub fn unbind_text(&mut self) {
        let carried = OnceCell::new();
        let pairs: Vec<(DrawElement, DrawElement)> = self
            .panel_selection()
            .into_iter()
            .filter(|element| {
                element.kind != DrawElementType::Text && self.restylable(element, &carried)
            })
            .filter_map(|element| {
                self.label_of(element, &carried)
                    .map(|label| (element.clone(), label.clone()))
            })
            .collect();
        if pairs.is_empty() {
            return;
        }
        for (mut container, mut text) in pairs {
            // `computeBoundTextPosition` with the label as it is: where it is drawn.
            let at = layout::bound_text_position(&container, &text);
            let source = source_text(&text).to_owned();
            let (width, height) = self.with_measure(|measure| {
                measure.size(&source, layout::font_of(&text), resolved_line_height(&text))
            });
            text.container_id = None;
            // A label's own switch, which a free text says with `auto_resize`.
            text.wrap = None;
            text.text = Some(source);
            text.width = width;
            text.height = height;
            text.x = at.x;
            text.y = at.y;
            container.bound_text_id = None;
            // `resetOriginalContainerCache` whatever the shape.
            if let Some(height) = self.original_container_heights.remove(&container.id) {
                if !is_linear_element(&container) {
                    container.height = height;
                }
            }
            self.scene.put(container);
            self.scene.put(text);
        }
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    /// "Wrap text in a container" (`actionWrapTextInContainer.perform`, `:269-376`): each
    /// selected free text gets a rectangle in the next element's style — fully opaque —
    /// a padding clear of it all round, in its groups and frame and at its angle,
    /// directly below it in the stack. The text is its label; the arrows bound to the
    /// text are bound to the rectangle instead. The rectangles are what is left selected.
    pub fn wrap_text_in_container(&mut self) {
        let carried = OnceCell::new();
        let texts: Vec<DrawElement> = self
            .panel_selection()
            .into_iter()
            .filter(|element| is_free_text(element) && self.restylable(element, &carried))
            .cloned()
            .collect();
        if texts.is_empty() {
            return;
        }
        let style = self.get_next_style();
        let pad = layout::BOUND_TEXT_PADDING;
        let mut containers = Vec::with_capacity(texts.len());
        for text in texts {
            let mut container = create_element(
                DrawElementType::Rectangle,
                Geometry {
                    x: text.x - pad,
                    y: text.y - pad,
                    width: layout::container_dimension_for_bound_text(
                        text.width,
                        DrawElementType::Rectangle,
                    ),
                    height: layout::container_dimension_for_bound_text(
                        text.height,
                        DrawElementType::Rectangle,
                    ),
                },
                style.clone(),
                self.now_ms,
            );
            container.opacity = 100.0;
            container.angle = text.angle;
            container.group_ids.clone_from(&text.group_ids);
            container.frame_id.clone_from(&text.frame_id);
            container.bound_text_id = Some(text.id.clone());
            let container_id = container.id.clone();

            let bound: Vec<DrawElement> = self
                .scene
                .iter_ordered()
                .filter(|element| {
                    element.kind == DrawElementType::Arrow
                        && (element.start_binding.as_deref() == Some(text.id.as_str())
                            || element.end_binding.as_deref() == Some(text.id.as_str()))
                })
                .cloned()
                .collect();
            for mut arrow in bound {
                for end in [&mut arrow.start_binding, &mut arrow.end_binding] {
                    if end.as_deref() == Some(text.id.as_str()) {
                        *end = Some(container_id.clone());
                    }
                }
                self.scene.put(arrow);
            }

            let text_id = text.id.clone();
            self.scene.add(container);
            // `pushContainerBelowText` (`:238-256`): added on top, lifted to just above
            // the text, then the text lifted above it — the pair where the text was.
            self.scene
                .place_above(std::slice::from_ref(&container_id), &text_id);
            self.scene
                .place_above(std::slice::from_ref(&text_id), &container_id);
            let laid = self.laid_out(&as_label(text, &container_id));
            if let Some(grown) = laid.container {
                self.scene.put(grown);
            }
            self.scene.put(laid.text);
            containers.push(container_id);
        }
        self.apply_bindings();
        self.set_selection(containers);
        self.push_history();
        self.request_draw();
    }
}
