//! Whole-scene PNG export: what the export is *framed* by and how big it comes out.
//!
//! Every number below is derived from the oracle's own code, not from what this engine
//! happens to print, so a regression in the framing fails here rather than being
//! re-described as the expected value:
//!
//! - `packages/excalidraw/scene/export.ts@1118751f:566-575` — `getCanvasSize`: the union
//!   of `getCommonBounds` plus `exportPadding * 2` on each side, returned as
//!   `[minX, minY, width, height]`.
//! - `packages/excalidraw/scene/export.ts@1118751f:577-587` — `getExportSize`: the backing
//!   store is `Math.trunc(dimension * scale)`, truncating rather than rounding.
//! - `packages/excalidraw/scene/export.ts@1118751f:199-203` — the export canvas is
//!   `width * appState.exportScale` device pixels and the render is drawn at that same
//!   scale, so the scale is one number and not a device-pixel ratio.
//! - `packages/common/src/constants.ts@1118751f:397-398` — `EXPORT_SCALES = [1, 2, 3]` and
//!   `DEFAULT_EXPORT_PADDING = 10`.
//! - `packages/element/src/bounds.ts@1118751f:1009-1011` — an empty scene bounds to
//!   `[0, 0, 0, 0]`, so an empty export is the padding alone.
//! - `packages/element/src/frame.ts@1118751f:271-281` — `getRootElements`: what is inside a
//!   frame does not widen the frame the export is measured against.
//! - `packages/element/src/bounds.ts@1118751f:210-235` — a rectangle's bounds are its four
//!   turned corners, so a turned element is measured by what it draws.
//!
//! Nothing here paints: the rasterising half needs a browser. These are the numbers it is
//! given, and `e2e/exportPng.spec.ts` is what checks the pixels they produce.

mod common;
use common::*;
use draw_engine::*;

/// The oracle's padding, in one place, so a change to it is a visible edit.
const PADDING: f64 = draw_engine::DEFAULT_EXPORT_PADDING;

/// A 200x100 box at the origin, so every expected number below is arithmetic a person can do.
fn two_hundred_by_hundred() -> DrawElement {
    box_at(0.0, 0.0, 200.0, 100.0)
}

#[test]
fn default_export_padding_is_the_oracles_ten() {
    assert_eq!(PADDING, 10.0);
}

#[test]
fn an_export_is_the_scenes_own_bounds_plus_the_padding_on_each_side() {
    let engine = engine_with_scene(vec![two_hundred_by_hundred()]);
    let frame = engine.export_frame(PADDING, 1.0);

    // 200 + 10 on the left + 10 on the right, 100 + 10 + 10.
    assert_close(frame.width, 220.0);
    assert_close(frame.height, 120.0);
    assert_close(frame.bounds.min_x, 0.0);
    assert_close(frame.bounds.max_x, 200.0);
}

#[test]
fn the_scale_multiplies_the_padded_box_into_the_backing_store() {
    let engine = engine_with_scene(vec![two_hundred_by_hundred()]);

    for (scale, expected) in [(1.0, (220, 120)), (2.0, (440, 240)), (3.0, (660, 360))] {
        let frame = engine.export_frame(PADDING, scale);
        assert_eq!(
            (frame.pixel_width(), frame.pixel_height()),
            expected,
            "at {scale}x the export is {expected:?} device pixels"
        );
    }
}

#[test]
fn the_backing_store_truncates_a_fractional_dimension_rather_than_rounding_it() {
    // A 220.5-wide box at 1x is the one case the two rules disagree on: `Math.trunc` gives
    // 220 and a round would give 221. `getExportSize` truncates (export.ts:583) and a
    // canvas is an integer number of device pixels (export.test.ts:367-371 says so about
    // `exportToCanvas`), so 220.
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 200.5, 100.0)]);
    assert_eq!(engine.export_frame(PADDING, 1.0).pixel_width(), 220);
    // 1.5x: 220.5 * 1.5 = 330.75, truncated to 330 and not rounded to 331.
    assert_eq!(engine.export_frame(PADDING, 1.5).pixel_width(), 330);
}

#[test]
fn the_camera_puts_the_scenes_corner_in_the_padding() {
    // `scrollX = -minX + exportPadding` at zoom 1 (export.ts:264-266) is what frames the
    // scene rather than the viewport, and the camera is how this engine says the same thing.
    let engine = engine_with_scene(vec![box_at(40.0, 25.0, 200.0, 100.0)]);
    let camera = engine.export_frame(PADDING, 2.0).camera();

    // World x=40 has to land on the left edge of the padding, 10 CSS px in.
    assert_close(camera.x, -30.0);
    assert_close(camera.y, -15.0);
    // Zoom 1: the scale is the device multiplier, not the camera's — a camera scaled by 2
    // would frame a different scene.
    assert_close(camera.scale, 1.0);
}

#[test]
fn an_empty_scene_exports_the_padding_alone() {
    // `getCommonBounds` answers `[0, 0, 0, 0]` for no elements (bounds.ts:1009-1011), so
    // the width is the padding twice over rather than zero — a zero-sized canvas cannot be
    // encoded at all.
    let engine = engine_with_scene(vec![]);
    let frame = engine.export_frame(PADDING, 1.0);

    assert_close(frame.width, 20.0);
    assert_close(frame.height, 20.0);
    assert_eq!((frame.pixel_width(), frame.pixel_height()), (20, 20));
    assert_close(frame.camera().x, 10.0);
}

#[test]
fn one_element_is_no_special_case() {
    let engine = engine_with_scene(vec![box_at(10.0, 10.0, 50.0, 50.0)]);
    let frame = engine.export_frame(PADDING, 1.0);

    assert_close(frame.width, 70.0);
    assert_close(frame.height, 70.0);
    assert_close(frame.bounds.min_x, 10.0);
    assert_close(frame.bounds.min_y, 10.0);
}

#[test]
fn a_zero_width_element_still_produces_a_canvas() {
    // A rectangle dragged to nothing has no width, so the scene's own width is zero and the
    // export is the padding on each side. Nothing special-cases it: the oracle adds
    // `exportPadding * 2` whatever the bounds came out as (export.ts:571-572).
    let engine = engine_with_scene(vec![box_at(30.0, 30.0, 0.0, 40.0)]);
    let frame = engine.export_frame(PADDING, 1.0);

    assert_close(frame.width, 20.0);
    assert_close(frame.height, 60.0);
    assert_eq!((frame.pixel_width(), frame.pixel_height()), (20, 60));
}

#[test]
fn a_deleted_element_does_not_widen_the_export() {
    let mut deleted = box_at(-500.0, -500.0, 100.0, 100.0);
    deleted.is_deleted = true;
    let engine = engine_with_scene(vec![deleted, two_hundred_by_hundred()]);
    let frame = engine.export_frame(PADDING, 1.0);

    assert_close(frame.bounds.min_x, 0.0);
    assert_close(frame.width, 220.0);
}

#[test]
fn the_export_spans_every_element_not_just_the_first() {
    let engine = engine_with_scene(vec![
        box_at(-100.0, 0.0, 50.0, 50.0),
        two_hundred_by_hundred(),
    ]);
    let frame = engine.export_frame(PADDING, 1.0);

    assert_close(frame.bounds.min_x, -100.0);
    assert_close(frame.bounds.max_x, 200.0);
    assert_close(frame.width, 320.0);
}

#[test]
fn a_turned_element_is_measured_by_what_it_draws() {
    // The bounds are the oracle's `getCommonBounds` (`bounds.ts@1118751f:1005-1027`), which
    // folds each element's *turned* box, not the box it is stored in. For a rectangle the
    // oracle turns all four corners and takes the extremes (`bounds.ts@1118751f:210-235`),
    // so a 100x100 square turned through 45° spans `100 * √2` about the same centre — a
    // framing that used the stored box would crop its corners off.
    //
    // A quarter turn would prove nothing about that: a square turned 90° lands on exactly
    // the box it started in, which is why the angle here is the eighth turn.
    let mut square = box_at(0.0, 0.0, 100.0, 100.0);
    square.angle = std::f64::consts::FRAC_PI_4;
    let engine = engine_with_scene(vec![square]);
    let frame = engine.export_frame(0.0, 1.0);

    let reach = 50.0 * 2f64.sqrt();
    assert_close(frame.bounds.min_x, 50.0 - reach);
    assert_close(frame.bounds.min_y, 50.0 - reach);
    assert_close(frame.bounds.max_x, 50.0 + reach);
    assert_close(frame.width, 2.0 * reach);
}

#[test]
fn a_quarter_turned_square_is_framed_exactly_where_it_was() {
    // The companion to the 45° case, and the one that catches a `sin`/`cos` that dropped its
    // sign: a square turned a quarter occupies the box it started in, and one whose corners
    // had gone the wrong way round the centre would come out 100 wide on each side of it
    // instead.
    let mut square = box_at(0.0, 0.0, 100.0, 100.0);
    square.angle = std::f64::consts::FRAC_PI_2;
    let frame = engine_with_scene(vec![square]).export_frame(0.0, 1.0);

    assert_close(frame.bounds.min_x, 0.0);
    assert_close(frame.bounds.max_x, 100.0);
    assert_close(frame.width, 100.0);
}

#[test]
fn what_is_inside_a_frame_does_not_widen_the_export() {
    // `getCanvasSize` measures `getRootElements(elementsForRender)` (export.ts:232-235), and
    // that drops an element whose frame is in the scene (frame.ts:271-281): a frame is a
    // window onto a region, and the region is inside it by construction.
    let mut frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    );
    let mut child = box_at(10.0, 10.0, 20.0, 20.0);
    child.frame_id = Some(frame.id.clone());
    // A child poking out past the frame's own edge — which is the case that would be cropped
    // if the frame were the whole story. The oracle crops it, and so does this.
    child.x = -80.0;
    let mut outside = box_at(400.0, 400.0, 10.0, 10.0);
    outside.frame_id = None;
    frame.name = Some("Frame 1".to_string());

    let engine = engine_with_scene(vec![frame, child, outside]);
    let measured = engine.export_frame(PADDING, 1.0);

    // Only the frame and the loose shape, so the box runs from the frame's edge to the
    // loose shape's — and not to the child at x = -80.
    assert_close(measured.bounds.min_x, 0.0);
    assert_close(measured.bounds.min_y, 0.0);
    assert_close(measured.bounds.max_x, 410.0);
    assert_close(measured.bounds.max_y, 410.0);
    assert_close(measured.width, 430.0);
}

/// A scale is taken as given rather than clamped to the three the dialog offers — the
/// oracle does not clamp either. `getExportSize` multiplies by whatever `exportScale`
/// holds (`export.ts:582-584`), and the app's own default is the device pixel ratio when
/// that happens to be one of the three and 1 otherwise (`appState.ts:20-22`). So which chips
/// to offer is the front's business; what they mean is arithmetic, and it is here.
#[test]
fn a_scale_is_taken_as_given_rather_than_clamped_to_the_offered_ones() {
    let engine = engine_with_scene(vec![two_hundred_by_hundred()]);
    assert_eq!(engine.export_frame(PADDING, 0.5).pixel_width(), 110);
    assert_eq!(engine.export_frame(PADDING, 4.0).pixel_width(), 880);
}

#[test]
fn padding_of_zero_is_the_way_to_export_with_no_margin() {
    // What a frame export does (`exportPadding = 0`, export.ts:228-230) and what a test that
    // wants the arithmetic to show uses.
    let engine = engine_with_scene(vec![two_hundred_by_hundred()]);
    let frame = engine.export_frame(0.0, 1.0);

    assert_close(frame.width, 200.0);
    assert_close(frame.camera().x, 0.0);
}
