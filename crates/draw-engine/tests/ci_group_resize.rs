//! Resizing a group keeps its drawings in their own boxes.
//!
//! A shape is generated *into* its box, so scaling the box is the whole resize. A drawing
//! — a freehand stroke, a line, a bucket fill — **is** its points, so the box and the ring
//! have to be scaled together or they come apart.
//!
//! Both paths do scale the ring. They disagree about what they scale it *from*:
//!
//! - Single-element resize (`scale_ring`, `engine/pointer_move.rs`) scales from
//!   `origin_points`, captured when the drag began. Its doc comment explains why, and
//!   ends with the sentence that matters here: "A single-step drag cannot see this, which
//!   is why the first test of it passed."
//! - Group resize (`selection/group_transform.rs`) scales the box from the captured
//!   `origin` but the ring from the **live** `element.points` — which every earlier move
//!   of the same gesture has already scaled.
//!
//! So across a drag the box grows by `sx` and the ring by `sx` per move: after three
//! moves the box is right and the drawing is `sx²` too big, sitting outside its own
//! bounds. Structural, not a slip — `GroupOrigin` was `#[derive(Copy)]` with no `points`
//! field, so there was nothing captured to scale from.
//!
//! **Every test here drags in several steps.** One step cannot detect compounding, which
//! is exactly how this survived having tests at all.

mod common;
use common::*;
use draw_engine::*;

/// A drawing whose ring fills its own box, so "did the ring keep up" is answerable.
fn stroke_at(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    let mut element = create_element_default(
        DrawElementType::Freedraw,
        Geometry {
            x,
            y,
            width: w,
            height: h,
        },
    );
    element.points = Some(vec![[0.0, 0.0], [w / 2.0, h], [w, 0.0]]);
    element
}

fn line_at(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    let mut element = create_element_default(
        DrawElementType::Line,
        Geometry {
            x,
            y,
            width: w,
            height: h,
        },
    );
    element.points = Some(vec![[0.0, 0.0], [w / 2.0, h], [w, 0.0]]);
    element
}

/// How far the ring's own extent has drifted from the box that is supposed to contain it.
///
/// The invariant, stated as a number: a drawing's box *is* the extent of its points, so
/// these agree or the element is lying about where it is — and everything downstream
/// believes the box. The marquee misses it, the eraser sweeps past it, the exporter crops
/// it, and the selection frame is drawn somewhere the drawing is not.
fn ring_vs_box(element: &DrawElement) -> (f64, f64) {
    let points = element.points.clone().unwrap_or_default();
    let [min_x, min_y, max_x, max_y] = points_bounds(&points);
    (
        (max_x - min_x) - element.width.abs(),
        (max_y - min_y) - element.height.abs(),
    )
}

fn find<'a>(scene: &'a [DrawElement], id: &str) -> &'a DrawElement {
    scene
        .iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

/// Drags the group's south-east handle so the union's corner lands on `to`.
///
/// The pointer goes down on the handle where it is drawn — `handle_offset` beyond the
/// corner, four pixels of frame margin plus half an eight-pixel handle — and every move
/// is offset the same, because a resize takes off where in the handle it was grabbed
/// (`getResizeOffsetXY`, `packages/element/src/resizeElements.ts@1118751f:497-554`): the
/// corner goes exactly as far as the pointer. Aiming at the corner itself misses by more
/// than the handle's reach, falls through to the hit test, and moves the group instead.
fn drag_se_handle(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64), steps: usize) {
    const HANDLE_OFFSET: f64 = 8.0;
    engine.begin_pointer(from.0 + HANDLE_OFFSET, from.1 + HANDLE_OFFSET, false, false);
    for step in 1..=steps {
        let t = step as f64 / steps as f64;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t + HANDLE_OFFSET,
            from.1 + (to.1 - from.1) * t + HANDLE_OFFSET,
            false,
            // Snapping to objects stays off (the default): alignment guides pull toward
            // other elements and would make this a test of the guides, not of the scale.
            false,
        );
    }
    engine.end_pointer();
}

/// Two boxes plus a drawing, selected together. Union is (0,0)-(300,100).
fn group_with_drawing(drawing: DrawElement) -> (DrawEngine, String) {
    let anchor = filled(box_at(0.0, 0.0, 100.0, 100.0));
    let id = drawing.id.clone();
    let mut engine = engine_with_scene(vec![anchor.clone(), drawing]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![anchor.id, id.clone()]);
    (engine, id)
}

// ---------------------------------------------------------------------------
// The bug
// ---------------------------------------------------------------------------

#[test]
fn a_stroke_stays_in_its_box_across_a_multi_move_resize() {
    let (mut engine, id) = group_with_drawing(stroke_at(200.0, 0.0, 100.0, 100.0));

    drag_se_handle(&mut engine, (300.0, 100.0), (600.0, 200.0), 4);

    let scene = engine.get_scene();
    let (dw, dh) = ring_vs_box(find(&scene, &id));
    assert!(
        dw.abs() < 0.001 && dh.abs() < 0.001,
        "the ring drifted out of its box by ({dw}, {dh}) — the box is scaled from the \
         geometry captured at drag start, so the ring has to be too"
    );
}

#[test]
fn a_line_stays_in_its_box_across_a_multi_move_resize() {
    let (mut engine, id) = group_with_drawing(line_at(200.0, 0.0, 100.0, 100.0));

    drag_se_handle(&mut engine, (300.0, 100.0), (600.0, 200.0), 4);

    let scene = engine.get_scene();
    let (dw, dh) = ring_vs_box(find(&scene, &id));
    assert!(
        dw.abs() < 0.001 && dh.abs() < 0.001,
        "drifted by ({dw}, {dh})"
    );
}

/// The control, and the reason this went unnoticed. One move gives the live ring and the
/// captured origin the same value, so the two implementations agree exactly. If this ever
/// fails, the fix broke the case that always worked.
#[test]
fn a_single_move_resize_was_always_correct() {
    let (mut engine, id) = group_with_drawing(stroke_at(200.0, 0.0, 100.0, 100.0));

    drag_se_handle(&mut engine, (300.0, 100.0), (600.0, 200.0), 1);

    let scene = engine.get_scene();
    let (dw, dh) = ring_vs_box(find(&scene, &id));
    assert!(
        dw.abs() < 0.001 && dh.abs() < 0.001,
        "drifted by ({dw}, {dh})"
    );
}

/// The scale itself has to be right, not merely self-consistent. A ring and a box that
/// are both wrong by the same factor would pass every check above.
#[test]
fn the_drawing_grows_by_the_factor_the_handle_asked_for() {
    let (mut engine, id) = group_with_drawing(stroke_at(200.0, 0.0, 100.0, 100.0));

    // Union (0,0)-(300,100) dragged to (600,200): twice as wide, twice as tall.
    drag_se_handle(&mut engine, (300.0, 100.0), (600.0, 200.0), 4);

    let scene = engine.get_scene();
    let drawing = find(&scene, &id);
    assert_close(drawing.width, 200.0);
    assert_close(drawing.height, 200.0);
}

/// Shrinking compounds the same way, in the direction that hides it: the ring ends up
/// *smaller* than its box rather than bursting out of it, which reads as a drawing that
/// quietly lost its proportions rather than as an obvious break.
#[test]
fn a_stroke_stays_in_its_box_while_the_group_shrinks() {
    let (mut engine, id) = group_with_drawing(stroke_at(200.0, 0.0, 100.0, 100.0));

    drag_se_handle(&mut engine, (300.0, 100.0), (150.0, 50.0), 4);

    let scene = engine.get_scene();
    let (dw, dh) = ring_vs_box(find(&scene, &id));
    assert!(
        dw.abs() < 0.001 && dh.abs() < 0.001,
        "drifted by ({dw}, {dh})"
    );
}

/// Many small moves are what a real drag is — a pointer sends one per animation frame, so
/// dragging a corner across the screen is tens of them, not four. If the scale compounds
/// at all, this is where it becomes enormous.
#[test]
fn twenty_moves_drift_no_further_than_four() {
    let (mut engine, id) = group_with_drawing(stroke_at(200.0, 0.0, 100.0, 100.0));

    drag_se_handle(&mut engine, (300.0, 100.0), (600.0, 200.0), 20);

    let scene = engine.get_scene();
    let drawing = find(&scene, &id);
    let (dw, dh) = ring_vs_box(drawing);
    assert!(
        dw.abs() < 0.001 && dh.abs() < 0.001,
        "drifted by ({dw}, {dh})"
    );
    assert_close(drawing.width, 200.0);
}

/// A shape has no ring, and must keep behaving as it always did — this is the half a
/// points-focused fix could quietly break.
#[test]
fn a_plain_shape_still_scales_with_the_group() {
    let anchor = filled(box_at(0.0, 0.0, 100.0, 100.0));
    let other = filled(box_at(200.0, 0.0, 100.0, 100.0));
    let (anchor_id, other_id) = (anchor.id.clone(), other.id.clone());
    let mut engine = engine_with_scene(vec![anchor, other]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![anchor_id.clone(), other_id.clone()]);

    drag_se_handle(&mut engine, (300.0, 100.0), (600.0, 200.0), 4);

    let scene = engine.get_scene();
    assert_close(find(&scene, &anchor_id).width, 200.0);
    assert_close(find(&scene, &other_id).width, 200.0);
}

/// A group's handle taken where it is drawn does not jump the frame the eight pixels out
/// to it: pressed and released without moving, nothing is resized.
#[test]
fn a_groups_handle_taken_where_it_is_drawn_does_not_jump() {
    let (mut engine, id) = group_with_drawing(stroke_at(200.0, 0.0, 100.0, 100.0));

    drag_se_handle(&mut engine, (300.0, 100.0), (300.0, 100.0), 3);

    let scene = engine.get_scene();
    let drawing = find(&scene, &id);
    assert_close(drawing.width, 100.0);
    assert_close(drawing.height, 100.0);
    assert_close(drawing.x, 200.0);
}

// ---------------------------------------------------------------------------
// The multi-selection frame is the union of *turned* bounds (docs/reference/resize.md
// › the multi-selection frame)
// ---------------------------------------------------------------------------

/// The frame a multi-selection's handles sit on is the union of each member's turned
/// bounds — Excalidraw's `getCommonBounds` (`bounds.ts@1118751f:1005-1027`) — not the
/// union of their unrotated boxes.
#[test]
fn the_frame_is_the_union_of_turned_bounds_not_unturned_boxes() {
    let mut bar = box_at(0.0, 0.0, 200.0, 20.0);
    bar.angle = std::f64::consts::FRAC_PI_2;
    let square = box_at(300.0, 0.0, 50.0, 50.0);
    let (bar_id, square_id) = (bar.id.clone(), square.id.clone());
    let mut engine = engine_with_scene(vec![bar, square]);
    engine.select(vec![bar_id, square_id]);

    let drawn = engine.paint_view().group_box.expect("a box around both");

    // The bar's *unturned* box reaches x=0..200, so the unturned union starts the frame
    // at x=0. Turned 90 degrees about its own centre it only reaches x=90..110 — where
    // the union starts once turned bounds are used, as the oracle's are.
    assert_close(drawn.min_x, 90.0);
    assert_close(drawn.max_x, 350.0);
}

// ---------------------------------------------------------------------------
// Alt: resize from the centre (docs/reference/resize.md › "Alt: resize from the centre")
// ---------------------------------------------------------------------------

/// Drags the group's south-east handle with Alt held throughout, as
/// `session.engine.setAltHeld(event.altKey)` does before every move
/// (`engine/src/host/pointerInput.ts:19`).
fn drag_se_handle_alt(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64), steps: usize) {
    const HANDLE_OFFSET: f64 = 8.0;
    engine.set_alt_held(true);
    engine.begin_pointer(from.0 + HANDLE_OFFSET, from.1 + HANDLE_OFFSET, false, true);
    for step in 1..=steps {
        let t = step as f64 / steps as f64;
        engine.set_alt_held(true);
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t + HANDLE_OFFSET,
            from.1 + (to.1 - from.1) * t + HANDLE_OFFSET,
            false,
            false,
        );
    }
    engine.end_pointer();
}

/// Alt scales a group about the frame's own centre instead of the corner or side
/// opposite the handle — the oracle's `resizeFromCenterScale = 2`
/// (`resizeElements.ts@1118751f:1150-1155`).
#[test]
fn alt_resizes_a_group_from_the_frames_centre() {
    let anchor = filled(box_at(0.0, 0.0, 100.0, 100.0));
    let other = filled(box_at(200.0, 0.0, 100.0, 100.0));
    let (anchor_id, other_id) = (anchor.id.clone(), other.id.clone());
    let mut engine = engine_with_scene(vec![anchor, other]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![anchor_id.clone(), other_id.clone()]);
    // Frame is (0,0)-(300,100), centre (150,50). Holding y at the frame's own edge (100)
    // keeps the vertical scale at 1, so only the horizontal doubling is under test.

    drag_se_handle_alt(&mut engine, (300.0, 100.0), (450.0, 100.0), 4);

    let scene = engine.get_scene();
    let a = find(&scene, &anchor_id);
    let b = find(&scene, &other_id);
    // The SE corner asked to reach x=450 is 300 past the centre at x=150; doubled from
    // the centre, the frame's new half-width is 300, so it spans -150..450 — twice its
    // original 300 width, about the same centre.
    assert_close(a.x, -150.0);
    assert_close(a.width, 200.0);
    assert_close(b.x + b.width, 450.0);
    assert_close(b.width, 200.0);
    // y was held at the original edge: no vertical scale.
    assert_close(a.height, 100.0);
}

// ---------------------------------------------------------------------------
// Side handles on a multi-selection (docs/reference/resize.md › "a side of a
// multi-selection")
// ---------------------------------------------------------------------------

/// The group frame's east handle resizes it along one axis, as every other handle does
/// (`resizeMultipleElements`, `getTransformHandles`,
/// `transformHandles.ts@1118751f:218-244`) — "the group frame has corners only" was the
/// gap.
#[test]
fn the_groups_east_handle_resizes_it_along_one_axis() {
    let anchor = filled(box_at(0.0, 0.0, 100.0, 100.0));
    let other = filled(box_at(200.0, 0.0, 100.0, 100.0));
    let (anchor_id, other_id) = (anchor.id.clone(), other.id.clone());
    let mut engine = engine_with_scene(vec![anchor, other]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![anchor_id.clone(), other_id.clone()]);
    // Frame is (0,0)-(300,100); the east handle is drawn at (300, 50), 8px further out.

    const HANDLE_OFFSET: f64 = 8.0;
    engine.begin_pointer(300.0 + HANDLE_OFFSET, 50.0, false, false);
    engine.move_pointer(450.0 + HANDLE_OFFSET, 50.0, false, false);
    engine.move_pointer(600.0 + HANDLE_OFFSET, 50.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    let a = find(&scene, &anchor_id);
    let b = find(&scene, &other_id);
    // The west edge (x=0) stays put; the frame doubles from 300 to 600 wide.
    assert_close(a.x, 0.0);
    assert_close(a.width, 200.0);
    assert_close(b.x, 400.0);
    assert_close(b.width, 200.0);
    // An east handle must not touch height.
    assert_close(a.height, 100.0);
    assert_close(b.height, 100.0);
}

/// A cardinal handle is skipped on a selection too small along that axis to hold it
/// without crowding its corners — Excalidraw's `minimumSizeForEightHandles`
/// (`transformHandles.ts@1118751f:218-244`).
#[test]
fn a_cardinal_handle_is_hidden_on_a_tiny_selection() {
    let a = filled(box_at(0.0, 0.0, 5.0, 5.0));
    let b = filled(box_at(10.0, 0.0, 5.0, 5.0));
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![a_id.clone(), b_id.clone()]);
    // Frame (0,0)-(15,5): far under the 40-unit threshold on both axes, so the east
    // handle a bigger selection would offer at this same spot is skipped entirely.

    engine.begin_pointer(23.0, 2.5, false, false);
    engine.move_pointer(200.0, 2.5, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    assert_close(find(&scene, &a_id).width, 5.0);
    assert_close(find(&scene, &b_id).width, 5.0);
}
