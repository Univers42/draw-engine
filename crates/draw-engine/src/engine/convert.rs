//! Tab and Shift+Tab: the selected shapes switch between rectangle, diamond and ellipse,
//! keeping their style, their label and their arrows — the generic branch of Excalidraw's
//! shape switch (`packages/excalidraw/components/ConvertElementTypePopup.tsx@1118751f`).
//!
//! The first Tab only opens the switch and the ones after it switch
//! (`App.tsx@1118751f:5543-5572`). The switch is the host's to show, so the host holds
//! whether it is open. It tells the engine when it opens and closes, because a label shrunk
//! to fit a diamond grows back to its own size when the shape comes round to a rectangle
//! again, but only while that switch stays open (`FONT_SIZE_CONVERSION_CACHE`).
//!
//! Not ported: the linear branch, which switches lines and arrows between sharp, curved
//! and elbow.

use std::collections::HashMap;

use crate::engine::DrawEngine;
use crate::scene::binding::reanchor_to_outline;
use crate::scene::element::{DrawElement, DrawElementType};
use crate::text::layout::fitting_font_size;
use crate::DrawTool;

/// `GENERIC_TYPES`, in the order Tab walks them.
const GENERIC_TYPES: [DrawElementType; 3] = [
    DrawElementType::Rectangle,
    DrawElementType::Diamond,
    DrawElementType::Ellipse,
];

fn is_generic(element: &DrawElement) -> bool {
    GENERIC_TYPES.contains(&element.kind)
}

/// The type Tab (`forward`) or Shift+Tab switches `shapes` to: the next one round from the
/// type they share, or the first one either way when they differ.
fn next_type(shapes: &[DrawElement], forward: bool) -> DrawElementType {
    let shared = shapes
        .first()
        .filter(|first| shapes.iter().all(|shape| shape.kind == first.kind))
        .and_then(|first| GENERIC_TYPES.iter().position(|kind| *kind == first.kind));
    let index = shared.map_or(-1, |index| index as isize);
    let count = GENERIC_TYPES.len() as isize;
    let step = if forward { 1 } else { -1 };
    GENERIC_TYPES[((index + count + step) % count) as usize]
}

impl DrawEngine {
    /// Whether Tab has anything to switch: a rectangle, diamond or ellipse is selected —
    /// `getConversionTypeFromElements`, generic branch.
    pub fn can_convert_selection(&self) -> bool {
        self.get_selected_elements().iter().any(is_generic)
    }

    /// The switch opened: from here until [`Self::end_conversion`], each switched shape's
    /// label comes back to the size it had now.
    pub fn begin_conversion(&mut self) {
        self.conversion_font_sizes.get_or_insert_with(HashMap::new);
        self.remember_label_sizes();
    }

    /// The switch closed: the remembered label sizes are forgotten.
    pub fn end_conversion(&mut self) {
        self.conversion_font_sizes = None;
    }

    /// Switches every selected rectangle, diamond and ellipse to `to`, or to the next type
    /// round (`forward`) or the one before — `convertElementTypes`, generic branch. One
    /// step of undo. Says whether anything changed.
    ///
    /// Each shape keeps its id, style, place and size, and its roundness on or off; an
    /// explicit corner radius goes, as the oracle's `roundness.value` does. Arrows bound to
    /// it are aimed at the new outline and routed again. A label is laid out again at the
    /// size it had when the switch opened, shrunk until its lines fit.
    pub fn convert_selection(&mut self, to: Option<DrawElementType>, forward: bool) -> bool {
        let shapes: Vec<DrawElement> = self
            .get_selected_elements()
            .into_iter()
            .filter(is_generic)
            .collect();
        if shapes.is_empty() {
            return false;
        }
        let next = match to {
            Some(kind) if GENERIC_TYPES.contains(&kind) => kind,
            Some(_) => return false,
            None => next_type(&shapes, forward),
        };
        // The popup remembers what it sees before a click switches anything.
        self.remember_label_sizes();

        let switched: Vec<DrawElement> = shapes
            .into_iter()
            .filter(|shape| shape.kind != next)
            .map(|mut shape| {
                shape.kind = next;
                shape.corner_radius = None;
                shape
            })
            .collect();
        if switched.is_empty() {
            return false;
        }
        for shape in &switched {
            self.scene.put(shape.clone());
        }
        let zoom = self.camera.scale;
        for shape in &switched {
            let arrows: Vec<DrawElement> = self
                .scene
                .iter_ordered()
                .filter_map(|element| reanchor_to_outline(element, shape, zoom))
                .collect();
            for arrow in arrows {
                self.scene.put(arrow);
            }
            self.refit_label(shape);
        }
        self.apply_bindings();
        self.set_tool(DrawTool::Select);
        self.push_history();
        self.request_draw();
        true
    }

    /// Each selected shape's label size, the first time the open switch sees it.
    fn remember_label_sizes(&mut self) {
        if self.conversion_font_sizes.is_none() {
            return;
        }
        let sizes: Vec<(String, f64)> = self
            .get_selected_elements()
            .iter()
            .filter(|shape| is_generic(shape))
            .filter_map(|shape| {
                let label = self.live_label(shape)?;
                Some((shape.id.clone(), label.font_size?))
            })
            .collect();
        if let Some(remembered) = self.conversion_font_sizes.as_mut() {
            for (id, size) in sizes {
                remembered.entry(id).or_insert(size);
            }
        }
    }

    /// `shape`'s label, back at its remembered size and shrunk to fit — then laid out in
    /// the shape as any label is.
    fn refit_label(&mut self, shape: &DrawElement) {
        let Some(mut label) = self.live_label(shape).cloned() else {
            return;
        };
        let remembered = self
            .conversion_font_sizes
            .as_ref()
            .and_then(|sizes| sizes.get(&shape.id).copied());
        if let Some(size) = remembered {
            label.font_size = Some(size);
        }
        let size = self.with_measure(|measure| fitting_font_size(&label, shape, measure));
        label.font_size = Some(size);
        let laid = self.laid_out(&label);
        self.put_laid(laid);
    }
}
