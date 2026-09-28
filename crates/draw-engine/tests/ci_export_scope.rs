//! What an export is **of**: a selection, or one frame, in both formats.
//!
//! The oracle answers "what are the bounds?" in exactly one place, and it is worth being
//! precise about where that is, because the obvious reading of the export path is wrong.
//! There is no selection-bounds function and no frame-bounds function. There is one
//! function — `getCommonBounds` (`packages/element/src/bounds.ts@1118751f:1005-1029`) —
//! reached from one caller, `getCanvasSize` (`export.ts@1118751f:566-575`), and **both**
//! formats call it with the same argument expression:
//!
//! ```text
//! exportToCanvas  export.ts@1118751f:232-235
//! exportToSvg     export.ts@1118751f:341-344
//!   const [minX, minY, width, height] = getCanvasSize(
//!     exportingFrame ? [exportingFrame] : getRootElements(elementsForRender),
//!     exportPadding,
//!   );
//! ```
//!
//! Those two lines are the same text. So the *scope* of an export is not a second question
//! asked at the framing layer; it is decided one layer up, by which elements the caller
//! hands down, and the framing layer is told nothing about it.
//!
//! That is `prepareElementsForExport` (`packages/excalidraw/data/index.ts@1118751f:48-96`),
//! and it is the whole of the oracle's selection behaviour:
//!
//! - `:56-58` — `isExportingSelection = exportSelectionOnly && isSomeElementSelected(...)`.
//!   The flag alone is not enough; a live element has to be selected as well. **This is the
//!   empty-selection case**: with nothing selected the flag never becomes true and the
//!   ternary at `:61-69` takes its `else` arm, `exportedElements = elements` — the whole
//!   scene. What makes the competing branch not run is that the selection is empty.
//! - `:62-69` — otherwise the selection, via `getSelectedElements` with
//!   `includeBoundTextElement` (a selected shape brings its own label).
//! - `:73-79` — **exactly one** selected and it is frame-like: `exportingFrame` is that
//!   element, and `exportedElements` becomes `getElementsOverlappingFrame(elements, frame,
//!   elementsMap)` — the frame's *contents*, taken from the whole scene. What makes the
//!   competing branch not run is `exportedElements.length === 1 && isFrameLikeElement(...)`;
//!   a second selected element takes the `else if` at `:80` instead.
//! - `:80-89` — more than one: re-fetched with `includeElementsInFrames: true`, so a
//!   selected shape inside a frame brings its siblings along.
//!
//! ## The one thing this file exists to pin, and it is not what the brief assumed
//!
//! A frame export is measured by **the frame element's own box at padding 0**, not by the
//! union of what is inside it. `exportingFrame ? [exportingFrame] : …` is the whole
//! mechanism (`export.ts:233`, and `:342` for the SVG), and `:228-230` sets
//! `exportPadding = 0` first. So for a frame the two halves diverge deliberately: the
//! **painted set** is the frame's contents, the **framing box** is the frame itself.
//!
//! The consequence is a crop, and it is the oracle's: a child that pokes out past the
//! frame's edge is cut off, because the box is the frame's, not the children's union. A
//! framing built from the contents would show that child, and would be wrong.
//!
//! This is also why there is no new "export frame" mode. In the oracle the frame export is
//! not a separate entry point at all — it is what you get when the selection is exactly one
//! frame (`data/index.ts:73-79`), which is why the dialog needs one boolean and not two.

mod common;
use common::*;
use draw_engine::*;
use draw_engine::{camera::visible_world_rect, create_element_default, Geometry};
use std::collections::HashSet;

/// The oracle's eighth turn. A square turned through it spans `100 * √2`; a quarter turn
/// would land back on the box it started in and prove nothing.
const EIGHTH_TURN: f64 = std::f64::consts::FRAC_PI_4;

/// The oracle's padding, named once so a change to it is one edit.
const PADDING: f64 = draw_engine::DEFAULT_EXPORT_PADDING;

/// How far a turned 100×100 square reaches past its centre: `50 * √2`.
fn reach() -> f64 {
    50.0 * 2f64.sqrt()
}

/// The `width` attribute of an SVG's root element.
///
/// The root is the first `width="…"` in the document (`export/svg.rs` writes the root tag
/// before any element's own geometry), so this is the export's own box and not a shape's.
/// Parsed rather than compared as a string: the value is a `f64` and its printed form is
/// Rust's to decide, not the test's to pin.
fn svg_width(svg: &str) -> f64 {
    root_attribute(svg, "width")
}

/// The `height` attribute of an SVG's root element. See [`svg_width`].
fn svg_height(svg: &str) -> f64 {
    root_attribute(svg, "height")
}

fn root_attribute(svg: &str, name: &str) -> f64 {
    let tag = svg.split_once('>').expect("an svg root element").0;
    let marker = format!("{name}=\"");
    let start = tag
        .find(&marker)
        .unwrap_or_else(|| panic!("the svg root should carry a {name}"))
        + marker.len();
    let rest = &tag[start..];
    let end = rest
        .find('"')
        .unwrap_or_else(|| panic!("the {name} attribute should be closed"));
    rest[..end]
        .parse()
        .unwrap_or_else(|_| panic!("the {name} attribute should be a number"))
}

/// A 100×100 square at the origin, turned through the eighth turn.
fn turned_square() -> DrawElement {
    let mut square = box_at(0.0, 0.0, 100.0, 100.0);
    square.angle = EIGHTH_TURN;
    square
}

/// A frame of `size` at the origin, named so it is not a blank rename.
fn frame_at_origin(size: f64) -> DrawElement {
    let mut frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: size,
            height: size,
        },
    );
    frame.name = Some("Frame 1".to_string());
    frame
}

/// A box, parented to `frame`.
fn inside(frame: &DrawElement, x: f64, y: f64, size: f64) -> DrawElement {
    let mut child = box_at(x, y, size, size);
    child.frame_id = Some(frame.id.clone());
    child
}

// ── The two formats must frame one drawing the same way ──────────────────────
//
// `exportToCanvas` and `exportToSvg` call `getCanvasSize` with the same argument
// expression (`export.ts@1118751f:232-235` and `:341-344`). The oracle has one framing
// decision, not two, so a difference between our formats on the same drawing at the same
// padding is a bug by construction — this needs no arithmetic of its own to say so, which
// is why it is the first test here.

/// The SVG export frames a turned element by what it draws.
///
/// `getCommonBounds` folds `getElementBounds`, and for a rectangle that is its four **turned**
/// corners (`bounds.ts@1118751f:210-235`). The oracle applies that to the SVG exactly as it
/// does to the PNG, because it is the same `getCanvasSize` call in both. Ours does not: the
/// SVG path measures with `scene.bounds()`, which is `scene_bounds` — the *unrotated* union
/// — so a square turned through the eighth turn exports as the 100×100 box it was drawn in,
/// with its four corners cropped off.
#[test]
fn the_svg_export_measures_a_turned_element_by_what_it_draws() {
    let engine = engine_with_scene(vec![turned_square()]);
    let scope = engine.export_scope(
        false,
        &ExportOptions {
            padding: 0.0,
            ..Default::default()
        },
    );
    let svg = engine.export_svg_of(&scope).expect("a square exports");

    assert_close(svg_width(&svg), 2.0 * reach());
    assert_close(svg_height(&svg), 2.0 * reach());
}

/// The two formats frame one drawing identically.
///
/// Not a second opinion about the right number — the same export, asked two ways, must come
/// out the same size. Today the PNG answers `100√2` and the SVG answers `100` for the very
/// same turned square, which is a picture of a diamond and a picture of the box it was
/// drawn in, from one drawing, at one padding.
#[test]
fn the_two_formats_frame_the_same_drawing_identically() {
    let engine = engine_with_scene(vec![turned_square()]);
    let options = ExportOptions {
        padding: 0.0,
        ..Default::default()
    };
    let png = engine.export_frame(&options);
    let scope = engine.export_scope(false, &options);
    let svg = engine.export_svg_of(&scope).expect("a square exports");

    assert_close(svg_width(&svg), png.width);
    assert_close(svg_height(&svg), png.height);
}

/// Both formats, of a selection, are the same picture at the same size.
///
/// The scope is the engine's for both, which is the property rather than a number: the PNG
/// and the SVG take the one [`ExportScope`], so there is no arrangement of inputs that gives
/// one format a different box than the other.
#[test]
fn both_formats_of_a_selection_are_the_same_picture() {
    let left = box_at(0.0, 0.0, 60.0, 40.0);
    let right = box_at(500.0, 300.0, 30.0, 30.0);
    let mut engine = engine_with_scene(vec![left.clone(), right.clone(), turned_square()]);
    engine.select(vec![left.id.clone(), right.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());
    let svg = engine.export_svg_of(&scope).expect("a selection exports");

    assert_close(svg_width(&svg), scope.frame.width);
    assert_close(svg_height(&svg), scope.frame.height);
}

// ── Nothing selected is the whole scene, and that is the answer ───────────────

/// A selection export of **nothing** is the whole-scene export, and that is deliberate.
///
/// This is the case most likely to be wrong and least likely to be noticed: it exports a
/// picture, it downloads, and nothing about it looks wrong. It is the branch at
/// `data/index.ts@1118751f:56-58` — `isExportingSelection` is
/// `exportSelectionOnly && isSomeElementSelected(...)`, and `isSomeElementSelected` is
/// `elements.some(el => selectedElementIds[el.id])` (`selection.ts@1118751f:141-143`). An
/// empty selection is not "a selection of zero elements" as far as the oracle is concerned:
/// the flag never becomes true, so the ternary at `:61-69` takes its `else` arm and
/// `exportedElements = elements`, the whole scene. **What makes the competing branch not run
/// is that no live element is selected** — not that the checkbox is off, and not that the
/// list is empty in the abstract.
///
/// The alternative — a 20×20 canvas of nothing, which is what an empty element list would
/// give (`bounds.ts@1118751f:1009-1011` answers `[0,0,0,0]`) — is what the front would show
/// a person who ticked "selection only" with nothing ticked on the board, which is a
/// question the oracle never asks and a picture nobody wants.
#[test]
fn a_selection_export_of_nothing_is_the_whole_scene_export() {
    let shape = box_at(0.0, 0.0, 200.0, 100.0);
    let engine = engine_with_scene(vec![shape]);
    let options = ExportOptions::default();

    let whole = engine.export_scope(false, &options);
    let nothing_selected = engine.export_scope(true, &options);

    assert_eq!(nothing_selected.elements.len(), 1);
    assert_eq!(nothing_selected.frame, whole.frame);
}

/// The same, with the selection pointing at ids that are not on the board.
///
/// `isSomeElementSelected` is asked about the **elements**, not the ids: a selection
/// referring to something deleted, or to nothing at all, is not a selection either, and the
/// export must not become an empty canvas. Deleted elements are already out of
/// `iter_ordered`, so this is the same answer by the same path — pinned because "the ids
/// were not empty" is exactly the wrong reasoning someone would reach for.
#[test]
fn a_selection_of_ids_that_are_not_on_the_board_is_the_whole_scene() {
    let shape = box_at(0.0, 0.0, 200.0, 100.0);
    let mut ghost = shape.clone();
    ghost.id = "not-here".to_string();
    let mut engine = engine_with_scene(vec![shape]);
    let options = ExportOptions::default();

    engine.select(vec![ghost.id.clone()]);
    let scope = engine.export_scope(true, &options);

    assert_eq!(scope.elements.len(), 1);
    assert_eq!(scope.frame, engine.export_scope(false, &options).frame);
}

/// The checkbox being off means the scene, whatever is selected.
///
/// The other half of the `&&` at `data/index.ts@1118751f:56-58`, and the reason the flag is
/// enough for the host: a false `exportSelectionOnly` reaches the same `else` arm.
#[test]
fn an_unset_selection_flag_ignores_a_live_selection() {
    let first = box_at(0.0, 0.0, 60.0, 40.0);
    let second = box_at(500.0, 300.0, 30.0, 30.0);
    let mut engine = engine_with_scene(vec![first.clone(), second.clone()]);
    engine.select(vec![first.id.clone()]);

    let scope = engine.export_scope(false, &ExportOptions::default());

    assert_eq!(scope.elements.len(), 2);
}

// ── One frame, and the measurement that is the whole point ───────────────────
//
// A frame export is the case where the two halves of an export deliberately disagree, and
// the disagreement is the answer. What is **painted** is the frame's contents
// (`getElementsOverlappingFrame`, `data/index.ts@1118751f:75-79`); what it is **measured
// by** is the frame element's own box at padding 0 — `exportingFrame ? [exportingFrame] : …`
// with `exportPadding = 0` in front of it (`export.ts@1118751f:228-233`, and `:337-342` for
// the SVG, which is the same line twice).
//
// So a frame is **not** exported as the union of what it holds, and building it that way is
// the mistake this section exists to catch. The test below is the one that fails if it does.

/// A selected frame exports **its own contents**.
///
/// `getElementsOverlappingFrame` (`packages/element/src/frame.ts@1118751f:983-998`) takes
/// from the whole scene everything whose turned box crosses the frame's, and drops what
/// belongs to a different frame. The loose shape at x=500 is the discriminator: it overlaps
/// nothing, so it must not be in the picture, even though it is on the same board.
#[test]
fn a_selected_frame_exports_its_own_contents() {
    let frame = frame_at_origin(200.0);
    let child = inside(&frame, 20.0, 20.0, 40.0);
    let loose = box_at(500.0, 500.0, 30.0, 30.0);
    let mut engine = engine_with_scene(vec![frame.clone(), child, loose.clone()]);
    engine.select(vec![frame.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());
    let ids: Vec<&str> = scope.elements.iter().map(|el| el.id.as_str()).collect();

    assert!(
        ids.contains(&frame.id.as_str()),
        "the frame is in its own export"
    );
    assert_eq!(
        scope.elements.len(),
        2,
        "the frame and the one child inside it"
    );
    assert!(!ids.contains(&loose.id.as_str()));
}

/// A frame is measured by **the frame**, at no padding, not by the union of its contents.
///
/// The child here is deliberately much bigger than the frame that holds it, which the oracle
/// permits: a frame captures on overlap, not on containment
/// (`frame.ts@1118751f:988-996`), so a 400-wide child in a 200-wide frame is a child.
///
/// If the framing were the union of the contents, the width would be 400. It is 200 — the
/// frame's own box, and `exportPadding = 0`, so not 220. Two numbers, and the
/// `exportingFrame ? [exportingFrame]` ternary is what produces both.
#[test]
fn a_frame_is_measured_by_its_own_box_at_no_padding() {
    let frame = frame_at_origin(200.0);
    // Poking well out past the frame's right edge, which the frame still holds.
    let child = inside(&frame, 150.0, 20.0, 400.0);
    let mut engine = engine_with_scene(vec![frame.clone(), child]);
    engine.select(vec![frame.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    assert_close(scope.frame.width, 200.0);
    assert_close(scope.frame.height, 200.0);
    assert_close(scope.frame.padding, 0.0);
    assert_close(scope.frame.bounds.min_x, 0.0);
    assert_close(scope.frame.bounds.max_x, 200.0);
}

/// A frame export crops a child that pokes out, and that is the oracle's answer.
///
/// The consequence of the test above, stated as its own case because it is the thing a
/// reader will assume is a bug. The box is the frame's, so the part of the child outside it
/// is outside the picture. `getCommonBounds` was handed `[exportingFrame]`, not the contents,
/// and it has no opinion about what is in them. A framing built from the contents would
/// *show* the poking part and would not match.
#[test]
fn a_child_poking_out_of_its_frame_is_cropped_by_the_frames_own_box() {
    let frame = frame_at_origin(200.0);
    let child = inside(&frame, 150.0, 20.0, 400.0);
    let mut engine = engine_with_scene(vec![frame.clone(), child.clone()]);
    engine.select(vec![frame.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    // Painted, because the frame does hold it…
    assert!(scope.elements.iter().any(|el| el.id == child.id));
    // …and the box is the frame's, so the world rect does not reach for it.
    assert!(scope.frame.bounds.max_x < child.x + child.width);
}

/// A frame export does not clip its contents to the frame.
///
/// `frameRendering.clip = false` when `exportingFrame` is set — "for canvas export, don't
/// clip if exporting a specific frame as it would clip the corners of the content"
/// (`export.ts@1118751f:217-219`). This is the reason: a frame's clip box is its own box,
/// and applying it to content that fills the frame cuts the content at the frame's very
/// edge, corners included. Everywhere else the clip stands, which is what crops a child on
/// screen.
#[test]
fn a_frame_export_does_not_clip_what_the_frame_holds() {
    let frame = frame_at_origin(200.0);
    let child = inside(&frame, 10.0, 10.0, 180.0);
    let mut engine = engine_with_scene(vec![frame, child]);
    engine.select(vec![engine_selected_frame(&engine)]);
    let options = ExportOptions::default();

    let scope = engine.export_scope(true, &options);
    let view = engine.export_view_of(&scope.frame, &scope.elements);

    assert!(
        view.frame_clips.is_empty(),
        "a frame export clips nothing, or the content's corners are cut"
    );
    // And the flag is what decided it, rather than a lucky element.
    assert!(!scope.frame.frame_clip);
}

/// Everywhere else, a frame export's clip stands: the flag is on by default.
///
/// The other side of the test above, so a change that made `frame_clip` false everywhere
/// would fail here rather than pass by accident.
#[test]
fn a_selection_export_keeps_its_frame_clips() {
    let frame = frame_at_origin(200.0);
    // Pokes out past the frame's right edge, so it is one of the children that needs a clip.
    let child = inside(&frame, 10.0, 10.0, 400.0);
    let loose = box_at(600.0, 600.0, 20.0, 20.0);
    let mut engine = engine_with_scene(vec![frame.clone(), child, loose.clone()]);
    let options = ExportOptions::default();

    // Two live elements, so this is a selection and not a frame export — and the clip has to
    // come from a frame that is *in the export*, which is why the frame is selected here.
    engine.select(vec![frame.id.clone(), loose.id.clone()]);
    let scope = engine.export_scope(true, &options);
    let view = engine.export_view_of(&scope.frame, &scope.elements);

    assert!(
        scope.frame.frame_clip,
        "only a frame export turns the clip off"
    );
    assert!(
        !view.frame_clips.is_empty(),
        "a selection export still crops what pokes out of a frame"
    );
}

/// A frame selected **with** one of its children takes the selection branch, not the frame
/// branch.
///
/// `exportedElements.length === 1` is the whole condition at `data/index.ts@1118751f:73`, so
/// a second element — the frame's own child — is enough to leave it, and the `else if` at
/// `:80` re-fetches with `includeElementsInFrames`. What the export is then is bounded by
/// `getRootElements`, which drops the child because its holder frame is in the set
/// (`frame.ts@1118751f:272-281`) — so the frame's box stands for both, and the width is
/// 200 either way. The child is painted; it is not what widens the picture.
#[test]
fn a_frame_selected_with_its_own_child_is_a_selection_not_a_frame_export() {
    let frame = frame_at_origin(200.0);
    let child = inside(&frame, 20.0, 20.0, 40.0);
    let mut engine = engine_with_scene(vec![frame.clone(), child.clone()]);
    engine.select(vec![frame.id.clone(), child.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    // Two elements to paint, both of them.
    assert_eq!(scope.elements.len(), 2);
    // The frame's box, and the padding back — which is what makes this the selection branch.
    assert_close(scope.frame.width, 200.0 + 2.0 * PADDING);
    assert_close(scope.frame.padding, PADDING);
}

/// A frame selected with a **loose** shape beside it is a selection too, and the frame
/// brings its children along.
///
/// The `includeElementsInFrames` arm (`packages/element/src/selection.ts@1118751f:196-210`):
/// each selected frame's children are added to the export. Without it, selecting a frame and
/// a distant shape would produce a picture of the frame's *box* with nothing in it, since
/// `getRootElements` drops the children the selection never named.
#[test]
fn a_selected_frame_brings_its_children_when_it_is_not_alone() {
    let frame = frame_at_origin(200.0);
    let child = inside(&frame, 20.0, 20.0, 40.0);
    let loose = box_at(500.0, 500.0, 30.0, 30.0);
    let mut engine = engine_with_scene(vec![frame.clone(), child.clone(), loose.clone()]);
    engine.select(vec![frame.id.clone(), loose.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());
    let ids: Vec<&str> = scope.elements.iter().map(|el| el.id.as_str()).collect();

    assert!(
        ids.contains(&child.id.as_str()),
        "the frame's child came along"
    );
    assert!(ids.contains(&loose.id.as_str()));
    // The frame is in it too: the oracle pushes each selected element itself after its
    // children (`selection.ts@1118751f:206`), and `getRootElements` keeps a frame rather than
    // dropping it — the box has to be drawn for the region to read as a frame at all.
    assert!(ids.contains(&frame.id.as_str()));
    // The frame ends at x=200 and the loose shape is 30 wide at x=500, so the union is 530.
    // The child at x=20 is inside the frame and does not widen it (`getRootElements`).
    assert_close(scope.frame.width, 530.0 + 2.0 * PADDING);
}

/// Two frames, neither alone, is a selection of both — not a frame export of either.
///
/// The count is the only thing that makes a frame special, so two selected frames take the
/// `else if` at `data/index.ts@1118751f:80` and are measured as a union of both boxes.
#[test]
fn two_selected_frames_are_a_selection_of_both() {
    let first = frame_at_origin(100.0);
    let mut second = frame_at_origin(100.0);
    second.x = 300.0;
    let mut engine = engine_with_scene(vec![first.clone(), second.clone()]);
    engine.select(vec![first.id.clone(), second.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    assert_eq!(scope.elements.len(), 2);
    assert_close(scope.frame.bounds.max_x, 400.0);
    assert_close(scope.frame.padding, PADDING);
}

/// A frame nested inside another frame: the inner one is selected, and the outer is not.
///
/// The competing branch here is `getRootElements`' second clause — an element is kept
/// "unless a frame **in the set** holds it" (`frame.ts@1118751f:272-281`). The outer's frame
/// is not in this export's set, so the inner frame's own box is what the picture is measured
/// by, and the outer frame does not widen it. Pinned because the reading that gets this
/// wrong — "a frame is always measured by its holder" — is the intuitive one and is not the
/// oracle's.
#[test]
fn a_frame_nested_in_an_unselected_frame_is_measured_on_its_own() {
    let outer = frame_at_origin(600.0);
    let mut inner = frame_at_origin(200.0);
    inner.frame_id = Some(outer.id.clone());
    let mut leaf = box_at(20.0, 20.0, 40.0, 40.0);
    leaf.frame_id = Some(inner.id.clone());
    let mut engine = engine_with_scene(vec![outer, inner.clone(), leaf]);
    engine.select(vec![inner.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    assert_close(scope.frame.width, 200.0);
    assert_close(scope.frame.padding, 0.0);
}

/// A selected shape inside a frame that is not selected still exports.
///
/// The other half of the same clause: only a frame **in the set** takes its children out of
/// the measurement. A shape alone, in a frame that is not part of the export, is measured by
/// its own box and nothing else — the frame it lives in is invisible to this export.
#[test]
fn a_shape_inside_an_unselected_frame_exports_at_its_own_bounds() {
    let frame = frame_at_origin(600.0);
    let mut child = box_at(20.0, 20.0, 40.0, 30.0);
    child.frame_id = Some(frame.id.clone());
    let mut engine = engine_with_scene(vec![frame, child.clone()]);
    engine.select(vec![child.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    assert_eq!(scope.elements.len(), 1);
    assert_close(scope.frame.width, 40.0 + 2.0 * PADDING);
    assert_close(scope.frame.height, 30.0 + 2.0 * PADDING);
}

/// The id of the frame in a one-frame scene, read off the engine rather than hard-written.
///
/// `DrawEngine` has no accessor for the scene's frames, and the tests above select by a
/// `frame.id` they built themselves. This one selects the frame it does not have a handle
/// on, so it asks the engine. Kept tiny and separate rather than adding a public accessor
/// for a test's convenience.
fn engine_selected_frame(engine: &DrawEngine) -> String {
    engine
        .export_scope(
            true,
            &ExportOptions {
                padding: 0.0,
                ..Default::default()
            },
        )
        .elements
        .first()
        .map(|el| el.id.clone())
        .expect("the frame is the only element")
}

// ── The invariants, over every shape of input ────────────────────────────────
//
// A number test can be made to pass by a number that is right for the case it was written
// from. These two cannot: the first holds for any input at all, the second for any input
// drawn from the set below. The bounds a scope reports are a function of five things — empty,
// one, many, turned, and nested in a frame — so the cases below walk that set rather than
// asserting one scenario twice.

/// The one that catches a cull aimed at the wrong rectangle.
///
/// An export's world rect is `padding - bounds.min` away from the content it was asked to
/// paint, so every element in the export must lie **inside** it. This is the invariant that
/// a cull aimed at the *camera* rather than at the export's own box breaks: a shape scrolled
/// off screen is in the export, and a camera-culled view would drop it and leave a hole in
/// the picture while every size assertion still passed.
///
/// The inputs are the five that have bitten: nothing, one, many, a turned element, and
/// things inside frames — including a child that pokes out of its frame, which a wrong clip
/// would cut and which this must still find accounted for. `frame_clip` is read, not assumed,
/// so a frame export is allowed to keep its children outside the rect.
#[test]
fn an_exports_world_rect_contains_everything_it_was_asked_to_paint() {
    for (name, engine) in scenes() {
        for selection_only in [false, true] {
            let scope = engine.export_scope(selection_only, &ExportOptions::default());
            // A frame export is measured by the frame and crops its contents, so a child
            // poking out is *meant* to fall outside the rect — that is the answer, and
            // `a_child_poking_out_of_its_frame_is_cropped_by_the_frames_own_box` is the case
            // that says so. The frame itself is what must be inside, and the frame export
            // keeps no clips.
            let rect = world_rect(&scope.frame);
            if let Some(frame) = scope.elements.iter().find(|el| is_frame(el)) {
                let box_ = draw_engine::scene_outline_bounds([*frame]).expect("a frame");
                assert!(
                    contains(rect, box_),
                    "{name}: a frame is outside its own export"
                );
            }
            // The view is what gets painted, so the property is about the view and not
            // about the numbers: it must carry every element the scope chose, and it must
            // see them through the export's own camera. A cull aimed at the editor's camera
            // fails the first; one aimed at a rect the wrong size fails the second.
            let view = engine.export_view_of(&scope.frame, &scope.elements);
            let painted: HashSet<&str> = view.elements.iter().map(|el| el.id.as_str()).collect();
            for element in &scope.elements {
                let box_ = draw_engine::scene_outline_bounds([*element]).expect("an element");
                assert!(
                    contains(rect, box_),
                    "{name} (selection_only={selection_only}): the export's world rect \
                     {rect:?} leaves out {:?} at {box_:?}",
                    element.id
                );
                assert!(
                    painted.contains(element.id.as_str()),
                    "{name} (selection_only={selection_only}): {:?} was chosen for the export \
                     and then not painted — the subset was culled to something other than \
                     the export's own rect",
                    element.id
                );
            }
            assert_eq!(
                view.camera,
                scope.frame.camera(),
                "{name}: the export is not seen through its own camera"
            );
        }
    }
}

/// The same walk, one element at a time, so a scope is pinned against a scope of one.
///
/// A multi-element export is measured by the union; a single-element one by that element. If
/// the two ever disagreed, a selection of one would frame differently from that element
/// exported alone, and the PNG and the SVG would both be "right" about a different picture.
#[test]
fn a_scope_of_one_element_frames_it_as_exported_alone() {
    for (name, engine) in scenes() {
        let whole = engine.export_scope(false, &ExportOptions::default());
        for element in &whole.elements {
            let mut alone = engine_with_scene(vec![(*element).clone()]);
            alone.select(vec![element.id.clone()]);
            let one = alone.export_scope(true, &ExportOptions::default());
            // The same element, so the same box, unless the selection is a frame — which is
            // measured by itself at no padding and is therefore a different picture.
            if !engine
                .export_scope(true, &ExportOptions::default())
                .frame
                .is_frame_export()
            {
                assert_eq!(
                    one.frame.bounds,
                    whole.frame.bounds_of(&[element]),
                    "{name}"
                );
            }
        }
    }
}

/// A box, and whether it holds another.
///
/// Closed on the edges: an element exactly on the export's edge is inside it, since the
/// padding is what keeps anything off the boundary and a turned corner can land on it.
fn contains(outer: WorldBounds, inner: WorldBounds) -> bool {
    outer.min_x <= inner.min_x
        && outer.min_y <= inner.min_y
        && outer.max_x >= inner.max_x
        && outer.max_y >= inner.max_y
}

/// The world rect an export paints into: its box, put `padding` in from its own corner.
///
/// [`ExportFrame::camera`] in reverse — the camera is what the picture is seen through, and
/// the rect is what it shows.
fn world_rect(frame: &ExportFrame) -> WorldBounds {
    let visible = visible_world_rect(frame.camera(), frame.width, frame.height);
    // Rounded outwards by a hair, so a value that is a hair off the boundary from
    // floating-point multiplication is not read as an element that fell outside.
    WorldBounds {
        min_x: visible.min_x - 1e-6,
        min_y: visible.min_y - 1e-6,
        max_x: visible.max_x + 1e-6,
        max_y: visible.max_y + 1e-6,
    }
}

/// The cases the walk runs over, each named so a failure says which one broke.
fn scenes() -> Vec<(&'static str, DrawEngine)> {
    let frame = frame_at_origin(200.0);
    let inner = frame_at_origin(100.0);
    let mut nested = inner.clone();
    nested.frame_id = Some(frame.id.clone());
    let leaf = inside(&nested, 20.0, 20.0, 30.0);
    vec![
        ("empty", engine_with_scene(vec![])),
        (
            "one",
            engine_with_scene(vec![box_at(10.0, 20.0, 60.0, 40.0)]),
        ),
        (
            "many",
            engine_with_scene(vec![
                box_at(0.0, 0.0, 50.0, 50.0),
                ellipse_at(300.0, 20.0, 80.0, 30.0),
                diamond_at(-120.0, 200.0, 40.0, 40.0),
                connector(0.0, 0.0, 500.0, 400.0, DrawElementType::Arrow),
            ]),
        ),
        ("turned", engine_with_scene(vec![turned_square()])),
        (
            "two turned",
            engine_with_scene(vec![turned_square(), turned_square_at(400.0, 400.0)]),
        ),
        (
            "framed",
            engine_with_scene(vec![
                frame_at_origin(200.0),
                inside(&frame_at_origin(200.0), 10.0, 10.0, 40.0),
            ]),
        ),
        (
            "nested",
            engine_with_scene(vec![
                frame_at_origin(600.0),
                nested,
                leaf,
                box_at(700.0, 700.0, 20.0, 20.0),
            ]),
        ),
        (
            "a child poking out",
            engine_with_scene(vec![
                frame_at_origin(200.0),
                inside(&frame_at_origin(200.0), 150.0, 0.0, 400.0),
            ]),
        ),
        ("a deleted element", {
            let mut gone = box_at(0.0, 0.0, 50.0, 50.0);
            gone.is_deleted = true;
            engine_with_scene(vec![gone, box_at(100.0, 100.0, 20.0, 20.0)])
        }),
        (
            "a zero-sized element",
            engine_with_scene(vec![
                box_at(50.0, 50.0, 0.0, 0.0),
                box_at(0.0, 0.0, 80.0, 80.0),
            ]),
        ),
    ]
}

/// The turned square again, somewhere else, so a two-element turned case is not one element.
fn turned_square_at(x: f64, y: f64) -> DrawElement {
    let mut square = turned_square();
    square.x = x;
    square.y = y;
    square
}

// ── What a selection is ───────────────────────────────────────────────────────

/// The selection is what gets exported, and nothing beside it.
///
/// `getSelectedElements` (`data/index.ts@1118751f:61-69`) hands the exporter the selection
/// rather than the scene, and the oracle's own SVG entry point insists on it in as many
/// words — "it also requires that the exportToSvg is being supplied with only the elements
/// that we're exporting, and no extra" (`export.ts:1118751f:384-385`). The shape left out
/// here is the turned square, and the box being 80×60 rather than the scene's proves the
/// framing followed.
#[test]
fn a_selection_exports_the_selection_and_nothing_else() {
    let wanted = box_at(0.0, 0.0, 60.0, 40.0);
    let unwanted = box_at(900.0, 900.0, 20.0, 20.0);
    let mut engine = engine_with_scene(vec![wanted.clone(), unwanted.clone()]);
    engine.select(vec![wanted.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    assert_eq!(scope.elements.len(), 1);
    assert_eq!(scope.elements[0].id, wanted.id);
    // 60 + 10 either side, 40 + 10 + 10 — the selected box and not the scene's.
    assert_close(scope.frame.width, 80.0);
    assert_close(scope.frame.height, 60.0);
}

/// One selected element is measured at its own turned bounds.
///
/// One is not a special case in the oracle, and it is not one here: the same `getCommonBounds`
/// folds whatever list it is given (`bounds.ts@1118751f:1020-1026`). The eighth turn is the
/// angle that can tell this from a stored box — a quarter turn lands back where it started.
#[test]
fn one_element_exports_at_its_own_turned_bounds() {
    let square = turned_square();
    let mut engine = engine_with_scene(vec![square.clone()]);
    engine.select(vec![square.id.clone()]);

    let scope = engine.export_scope(true, &ExportOptions::default());

    assert_close(scope.frame.width, 2.0 * reach() + 2.0 * PADDING);
    assert_close(scope.frame.bounds.min_x, 50.0 - reach());
}

/// A turned element in a selection is measured by what it draws, not by its stored box.
///
/// The property `element_bounds` gets wrong and `element_outline_bounds` gets right, and the
/// one the two formats disagreed about. Asserted on the box rather than the pixels, because
/// a crop of four corners is arithmetic here and a set of black pixels in a browser.
#[test]
fn a_turned_element_in_a_selection_is_measured_by_what_it_draws() {
    let square = turned_square();
    let plain = box_at(0.0, 0.0, 100.0, 100.0);
    let mut turned = engine_with_scene(vec![square.clone()]);
    let mut unturned = engine_with_scene(vec![plain.clone()]);
    turned.select(vec![square.id.clone()]);
    unturned.select(vec![plain.id.clone()]);

    let turned_scope = turned.export_scope(true, &ExportOptions::default());
    let plain_scope = unturned.export_scope(true, &ExportOptions::default());

    assert!(turned_scope.frame.width > plain_scope.frame.width);
    assert_close(turned_scope.frame.width, 2.0 * reach() + 2.0 * PADDING);
}
