//! What an export is **of**: the oracle's `prepareElementsForExport`
//! (`packages/excalidraw/data/index.ts@1118751f:48-96`).
//!
//! The oracle has no selection-bounds function and no frame-bounds function. `getCanvasSize`
//! takes an element list, and the two things that decide the list both happen before it is
//! called. This module is those two decisions, and the framing itself is still
//! [`ExportFrame`]'s — there is one path to an export's box and this is only how the
//! elements are chosen on the way to it.

use std::collections::HashSet;

use crate::camera::WorldBounds;
use crate::export::{ExportFrame, ExportOptions};
use crate::scene::element::DrawElement;
use crate::scene::geometry::element_outline_bounds;
use crate::scene::is_frame;
use crate::scene::Scene;

/// One export's two halves: the box it is cut to, and the elements it paints.
///
/// Both are decided together because the oracle decides them together — `exportedElements`
/// and `exportingFrame` come out of one function and are used by both formats
/// (`data/index.ts@1118751f:92-95`). A frame export is the case where the two disagree on
/// purpose: what is painted is the frame's contents, and what it is measured by is the
/// frame.
pub struct ExportScope<'a> {
    /// The target: the box, the size, the scale. See [`ExportFrame`].
    pub frame: ExportFrame,
    /// Exactly what gets painted, in scene order. The oracle's SVG entry point states the
    /// requirement: "it also requires that the exportToSvg is being supplied with only the
    /// elements that we're exporting, and no extra" (`export.ts@1118751f:384-385`).
    pub elements: Vec<&'a DrawElement>,
    /// Which of the three answers this is, said by the function that decided it.
    ///
    /// Here so that nobody has to ask the question a second way. A host that wanted to
    /// print "selection" or "canvas" — which the oracle's copy toast does
    /// (`actionClipboard.tsx@1118751f:165-167, 225-227`) — and re-derived it from its own
    /// `selectedCount` would be making a scope decision in the front, off a number that is
    /// not the same number (a selection of ids that are not on the board is not a
    /// selection, `selection.ts@1118751f:141-143`). The kind is two-valued, like the
    /// oracle's two words, and a lone frame is [`Self::Selection`].
    pub kind: ExportScopeKind,
}

/// The two answers a caller can report about a scope. See [`ExportScope::kind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportScopeKind {
    /// The selection, or one frame's contents — the `isExportingSelection` arm.
    Selection,
    /// Every live element — the `else` arm at `data/index.ts@1118751f:69`.
    Scene,
}

/// `prepareElementsForExport` (`data/index.ts@1118751f:48-96`).
///
/// `selection_only` is the oracle's `exportSelectionOnly`, the dialog's checkbox. It is the
/// **only** thing a front passes: whether that means the whole scene, a selection or one
/// frame is decided here, from the state, and not by a second flag the host has to get
/// right. That is also why there is no separate "export this frame" call — the oracle has
/// none either, and the frame export is what a selection of exactly one frame *is*.
///
/// The branches, in the oracle's order:
///
/// - **nothing is selected**, or the checkbox is off → the whole live scene
///   (`:61-69`, and the guard at `:56-58`);
/// - **exactly one selected and it is a frame** → that frame's contents, framed by the
///   frame at no padding (`:73-79`);
/// - **otherwise the selection** → `:80-89`, which re-fetches with
///   `includeElementsInFrames` so a selected frame brings its children.
///
/// Empty is a deliberate answer and not a degenerate one: `isExportingSelection` is
/// `exportSelectionOnly && isSomeElementSelected(...)` (`:56-58`), so a selection export of
/// nothing never becomes a selection and falls through to the scene.
pub fn export_scope<'a>(
    scene: &'a Scene,
    selected_ids: &[String],
    selection_only: bool,
    options: &ExportOptions,
) -> ExportScope<'a> {
    let selected = live_selected(scene, selected_ids);
    if !(selection_only && !selected.is_empty()) {
        return of_the_whole_scene(scene, options);
    }
    match single_frame(&selected) {
        Some(frame) => of_the_frame(scene, frame, options),
        None => of_the_selection(scene, &selected, options),
    }
}

/// The whole live scene, in z-order.
fn of_the_whole_scene<'a>(scene: &'a Scene, options: &ExportOptions) -> ExportScope<'a> {
    let elements: Vec<&DrawElement> = scene.iter_ordered().collect();
    ExportScope {
        frame: ExportFrame::for_elements(scene.iter_ordered(), options),
        elements,
        kind: ExportScopeKind::Scene,
    }
}

/// One frame: its contents painted, its own box measured, at `exportPadding = 0`.
///
/// `getElementsOverlappingFrame` (`packages/element/src/frame.ts@1118751f:983-998`) takes
/// from the **whole scene** everything whose turned bounds cross the frame's, minus what
/// belongs to a different frame. The bounds come from the frame element alone — see
/// [`ExportFrame::of_frame`] — so the frame is in the set: it overlaps itself.
fn of_the_frame<'a>(
    scene: &'a Scene,
    frame: &'a DrawElement,
    options: &ExportOptions,
) -> ExportScope<'a> {
    let frame_box = element_outline_bounds(frame);
    let elements: Vec<&DrawElement> = scene
        .iter_ordered()
        .filter(|element| {
            let own_frame = element
                .frame_id
                .as_deref()
                .is_none_or(|holder| holder == frame.id.as_str());
            own_frame && overlaps(element, frame_box)
        })
        .collect();
    ExportScope {
        frame: ExportFrame::of_frame(frame, options),
        elements,
        kind: ExportScopeKind::Selection,
    }
}

/// `getElementsOverlappingFrame`'s half of the predicate: `doBoundsIntersect` over the two
/// turned boxes (`frame.ts@1118751f:993-996`).
fn overlaps(element: &DrawElement, frame_box: WorldBounds) -> bool {
    let own = element_outline_bounds(element);
    own.min_x <= frame_box.max_x
        && own.max_x >= frame_box.min_x
        && own.min_y <= frame_box.max_y
        && own.max_y >= frame_box.min_y
}

/// The selection, in scene order, with a selected frame's children brought along.
///
/// `getSelectedElements` with `includeElementsInFrames` (`packages/element/src/selection.ts
/// @1118751f:196-210`) adds each selected frame's children, which is what a selection of a
/// frame *and* one of its shapes comes to. One pass over the scene in z-order rather than
/// the oracle's per-element walk, so the paint order is the scene's and the set is the
/// same: what `getRootElements` then drops for the bounds is in the set either way.
fn of_the_selection<'a>(
    scene: &'a Scene,
    selected: &[&'a DrawElement],
    options: &ExportOptions,
) -> ExportScope<'a> {
    let wanted = wanted_ids(scene, selected);
    let elements: Vec<&DrawElement> = scene
        .iter_ordered()
        .filter(|element| wanted.contains(element.id.as_str()))
        .collect();
    ExportScope {
        frame: ExportFrame::for_elements(elements.iter().copied(), options),
        elements,
        kind: ExportScopeKind::Selection,
    }
}

/// The selection, plus the two things the oracle adds to it, each in one direction only.
///
/// `getSelectedElements` with `includeBoundTextElement` and `includeElementsInFrames`
/// (`packages/element/src/selection.ts@1118751f:184-193, 196-210`), and both are additions to
/// the selection rather than a walk over everything:
///
/// - a **label bound to a selected shape** joins it, because a shape exported without its
///   own text is a shape with a hole in it;
/// - the **children of a selected frame** join it, because otherwise the selection of a frame
///   and a distant shape would be a picture of an empty box.
///
/// The direction matters in both. A shape inside a frame does **not** bring the frame, and a
/// frame does not bring the frames that hold it: `getFrameChildren(elements, element.id)` is
/// the frame's own children, and a label's `containerId` is the shape it is written on.
fn wanted_ids<'a>(scene: &'a Scene, selected: &[&'a DrawElement]) -> HashSet<&'a str> {
    let mut wanted: HashSet<&str> = selected.iter().map(|el| el.id.as_str()).collect();
    for element in scene.iter_ordered() {
        let label_of_a_selected_shape = element
            .container_id
            .as_deref()
            .is_some_and(|container| selected.iter().any(|el| el.id == container));
        let held_by_a_selected_frame = element
            .frame_id
            .as_deref()
            .is_some_and(|holder| selected.iter().any(|el| is_frame(el) && el.id == holder));
        if label_of_a_selected_shape || held_by_a_selected_frame {
            wanted.insert(element.id.as_str());
        }
    }
    wanted
}

/// `isSomeElementSelected` made concrete: the ids that name a live element.
///
/// `elements.some(el => selectedElementIds[el.id])` (`selection.ts@1118751f:141-143`), so a
/// selection pointing at nothing that is on the board is not a selection. That is what makes
/// the empty case fall through to the scene rather than export an empty canvas.
fn live_selected<'a>(scene: &'a Scene, selected_ids: &[String]) -> Vec<&'a DrawElement> {
    selected_ids
        .iter()
        .filter_map(|id| scene.get(id))
        .filter(|element| !element.is_deleted)
        .collect()
}

/// The one-frame case, and the only one the oracle treats specially: a single selected
/// frame-like element. `exportedElements.length === 1 && isFrameLikeElement(firstElement)`
/// (`data/index.ts@1118751f:73`). What makes the competing branch not run is the count —
/// a second selected element takes the `else if` at `:80` instead, and an ordinary shape
/// fails the `isFrameLikeElement` half.
fn single_frame<'a>(selected: &[&'a DrawElement]) -> Option<&'a DrawElement> {
    match selected {
        [only] if is_frame(only) => Some(only),
        _ => None,
    }
}
