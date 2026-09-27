mod common;
use common::*;
use draw_engine::*;

/// The corner opposite the handle is the anchor, and resizing must not move it.
///
/// Asserted as a **point**, not as a named handle. If the drag carries the pointer past
/// the anchor the element turns through and comes out mirrored, and the anchor point is
/// then the opposite corner of the new box — still in exactly the same place, but no
/// longer the corner it started as. Checking `handle_world(resized, opposite)` would
/// report that legitimate flip as the anchor having moved.
fn assert_resize_anchor(handle: HandleKind, target: Point, aspect: Option<f64>, angle: f64) {
    let mut element = box_at(20.0, 10.0, 80.0, 40.0);
    element.angle = angle;
    let anchor = handle_world(&element, opposite_handle(handle));

    let geom = resize_element(&element, handle, target.x, target.y, 4.0, aspect, false);
    let mut resized = element.clone();
    resized.x = geom.x;
    resized.y = geom.y;
    resized.width = geom.width;
    resized.height = geom.height;

    assert!(
        corner_stays_put(&resized, anchor),
        "the anchor at ({}, {}) moved; box is now {:?}",
        anchor.x,
        anchor.y,
        (geom.x, geom.y, geom.width, geom.height)
    );
    // Magnitude, not value: a negative extent is a mirror, and it is still that size.
    assert!(geom.width.abs() >= 4.0);
    assert!(geom.height.abs() >= 4.0);
}

/// Whether `point` is still one of the element's four corners.
fn corner_stays_put(element: &DrawElement, point: Point) -> bool {
    [
        HandleKind::Nw,
        HandleKind::Ne,
        HandleKind::Se,
        HandleKind::Sw,
    ]
    .into_iter()
    .any(|kind| {
        let c = handle_world(element, kind);
        (c.x - point.x).abs() < EPS && (c.y - point.y).abs() < EPS
    })
}

#[test]
fn resize_nw_handle() {
    assert_resize_anchor(HandleKind::Nw, Point { x: 0.0, y: -5.0 }, None, 0.0);
}

#[test]
fn resize_ne_handle() {
    assert_resize_anchor(HandleKind::Ne, Point { x: 120.0, y: -5.0 }, None, 0.0);
}

#[test]
fn resize_se_handle() {
    assert_resize_anchor(HandleKind::Se, Point { x: 130.0, y: 70.0 }, None, 0.0);
}

#[test]
fn resize_sw_handle() {
    assert_resize_anchor(HandleKind::Sw, Point { x: -10.0, y: 70.0 }, None, 0.0);
}

#[test]
fn resize_n_handle_preserves_width() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::N, 60.0, -10.0, 4.0, None, false);
    assert_close(geom.width, 80.0);
    assert_close(geom.height, 60.0);
}

#[test]
fn resize_s_handle_preserves_width() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::S, 60.0, 90.0, 4.0, None, false);
    assert_close(geom.width, 80.0);
    assert_close(geom.height, 80.0);
}

#[test]
fn resize_e_handle_preserves_height() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::E, 140.0, 30.0, 4.0, None, false);
    assert_close(geom.height, 40.0);
    assert_close(geom.width, 120.0);
}

#[test]
fn resize_w_handle_preserves_height() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::W, -20.0, 30.0, 4.0, None, false);
    assert_close(geom.height, 40.0);
    assert_close(geom.width, 120.0);
}

/// Dragging a handle past its anchor turns the element through and out the other side,
/// mirrored — it does not stop dead against the anchor.
///
/// This used to assert a clamp to the minimum size on the *original* side, which is what
/// the rebound was: the element shrank to nothing as the pointer approached the anchor
/// and then grew again on the side it started from, so it appeared to bounce off.
#[test]
fn a_handle_dragged_past_its_anchor_turns_the_element_through() {
    let element = box_at(50.0, 50.0, 60.0, 60.0);

    // Just short of the anchor: still the right way round, nearly collapsed.
    let near = resize_element(&element, HandleKind::Se, 54.0, 54.0, 1.0, None, false);
    assert!(near.width > 0.0 && near.width < 6.0);

    // Well past it: the same size, on the other side, and mirrored.
    let through = resize_element(&element, HandleKind::Se, 10.0, 10.0, 1.0, None, false);
    assert_close(through.width, -40.0);
    assert_close(through.height, -40.0);
    assert_close(
        normalize_rect(through.x, through.y, through.width, through.height).x,
        10.0,
    );

    // The anchor stays exactly where it was through all of it — though once the element
    // has turned through, the point it sits on is the box's opposite corner.
    let anchor = handle_world(&element, HandleKind::Nw);
    for geom in [near, through] {
        let mut probe = element.clone();
        probe.x = geom.x;
        probe.y = geom.y;
        probe.width = geom.width;
        probe.height = geom.height;
        assert!(corner_stays_put(&probe, anchor));
    }
}

/// Turning through a second time returns the element the right way round.
#[test]
fn turning_through_twice_comes_back() {
    let element = box_at(50.0, 50.0, 60.0, 60.0);
    let once = resize_element(&element, HandleKind::Se, 10.0, 10.0, 1.0, None, false);

    let mut mirrored = element.clone();
    mirrored.x = once.x;
    mirrored.y = once.y;
    mirrored.width = once.width;
    mirrored.height = once.height;
    assert!(mirrored.width < 0.0 && mirrored.height < 0.0);

    // It now occupies [10, 50]; its south-east handle is at (50, 50) and the anchor
    // opposite that is (10, 10). Drag the handle out through *that* anchor.
    let twice = resize_element(&mirrored, HandleKind::Se, -30.0, -30.0, 1.0, None, false);
    assert!(twice.width > 0.0, "back the right way round");
    assert!(twice.height > 0.0);
}

/// Growing a mirrored element does not quietly un-mirror it. Only turning it back
/// through its anchor does that.
#[test]
fn growing_a_mirrored_element_keeps_it_mirrored() {
    let element = box_at(50.0, 50.0, 60.0, 60.0);
    let once = resize_element(&element, HandleKind::Se, 10.0, 10.0, 1.0, None, false);
    let mut mirrored = element.clone();
    mirrored.x = once.x;
    mirrored.y = once.y;
    mirrored.width = once.width;
    mirrored.height = once.height;

    // Away from the anchor at (10, 10), so no flip.
    let bigger = resize_element(&mirrored, HandleKind::Se, 110.0, 110.0, 1.0, None, false);
    assert!(bigger.width < 0.0, "still mirrored");
    assert_close(bigger.width.abs(), 100.0);
}

/// The minimum is a floor on size, not a wall the pointer collides with.
#[test]
fn the_minimum_size_applies_to_either_side() {
    let element = box_at(50.0, 50.0, 60.0, 60.0);
    let geom = resize_element(&element, HandleKind::Se, 49.0, 49.0, 4.0, None, false);
    assert_close(geom.width.abs(), 4.0);
    assert!(geom.width < 0.0, "and on the far side of the anchor");
}

#[test]
fn resize_aspect_ratio_lock_se() {
    let element = box_at(0.0, 0.0, 100.0, 50.0);
    let ratio = 100.0 / 50.0;
    let geom = resize_element(
        &element,
        HandleKind::Se,
        200.0,
        80.0,
        4.0,
        Some(ratio),
        false,
    );
    assert_close(geom.width / geom.height, ratio);
}

#[test]
fn resize_aspect_ratio_lock_nw() {
    let element = box_at(50.0, 50.0, 80.0, 40.0);
    let ratio = 80.0 / 40.0;
    let geom = resize_element(
        &element,
        HandleKind::Nw,
        10.0,
        20.0,
        4.0,
        Some(ratio),
        false,
    );
    assert_close(geom.width / geom.height, ratio);
}

#[test]
fn resize_rotated_element_30deg() {
    assert_resize_anchor(
        HandleKind::Se,
        Point { x: 120.0, y: 80.0 },
        None,
        std::f64::consts::PI / 6.0,
    );
}

#[test]
fn resize_rotated_element_90deg() {
    assert_resize_anchor(
        HandleKind::Nw,
        Point { x: -10.0, y: -20.0 },
        None,
        std::f64::consts::PI / 2.0,
    );
}

// ---------------------------------------------------------------------------
// Alt: resize from the centre (docs/reference/resize.md › "Alt: resize from the centre")
// ---------------------------------------------------------------------------

/// Alt scales a single element about its own centre: both edges move by the same amount
/// in opposite directions, so the centre never moves (`shouldResizeFromCenter`,
/// `packages/common/src/keys.ts@1118751f:145-146`).
#[test]
fn alt_resizes_a_single_element_from_its_centre() {
    let element = box_at(0.0, 0.0, 100.0, 100.0);
    let centre = (
        element.x + element.width / 2.0,
        element.y + element.height / 2.0,
    );

    let geom = resize_element(&element, HandleKind::Se, 200.0, 200.0, 1.0, None, true);

    // The SE corner asked to reach (200, 200) is 100 past the centre on each axis; from
    // the centre that reach is doubled, so the box grows to 300x300 evenly about (50,50).
    assert_close(geom.width, 300.0);
    assert_close(geom.height, 300.0);
    assert_close(geom.x + geom.width / 2.0, centre.0);
    assert_close(geom.y + geom.height / 2.0, centre.1);
}

/// Shift still keeps proportions with Alt held: the anchor is the centre, but a corner
/// still takes the larger of its two scales (`getResizeAnchor` returns "center" whenever
/// `shouldResizeFromCenter`, whatever `shouldMaintainAspectRatio` says —
/// `resizeElements.ts@1118751f:621-627`).
#[test]
fn shift_and_alt_compose_on_a_single_element() {
    let element = box_at(0.0, 0.0, 100.0, 50.0);

    // Only x moves under the pointer; with Shift the y follows at the same ratio.
    let geom = resize_element(&element, HandleKind::Se, 200.0, 25.0, 1.0, Some(2.0), true);

    assert_close(geom.width, 300.0);
    assert_close(geom.height, 150.0);
    assert_close(geom.x, -100.0);
    assert_close(geom.y, -50.0);
}

#[test]
fn resize_rotate_handle_is_noop() {
    let element = box_at(10.0, 20.0, 100.0, 50.0);
    let geom = resize_element(&element, HandleKind::Rotate, 50.0, -100.0, 4.0, None, false);
    assert_eq!(geom.x, 10.0);
    assert_eq!(geom.y, 20.0);
    assert_eq!(geom.width, 100.0);
    assert_eq!(geom.height, 50.0);
}

#[test]
fn rotate_handle_north_is_zero_angle() {
    let element = box_at(0.0, 0.0, 100.0, 100.0);
    let angle = rotate_element(&element, 50.0, -50.0);
    assert!(angle.abs() < EPS || (angle - std::f64::consts::PI * 2.0).abs() < EPS);
}

#[test]
fn rotate_handle_east_is_pi_half() {
    let element = box_at(0.0, 0.0, 100.0, 100.0);
    let angle = rotate_element(&element, 150.0, 50.0);
    assert_close(angle, std::f64::consts::PI / 2.0);
}

#[test]
fn hit_handle_detection() {
    let element = box_at(10.0, 10.0, 100.0, 60.0);
    let handles = selection_handle_points(&element, 20.0);
    assert_eq!(handles.len(), 9);

    let nw = &handles[0];
    assert_eq!(hit_handle(&handles, nw.x, nw.y, 5.0), Some(nw.kind));
    assert_eq!(hit_handle(&handles, 500.0, 500.0, 5.0), None);
}

/// A whole drag, not a single call: the anchor must hold still for the entire gesture.
///
/// `resize_element` is given the element's geometry as it was when the drag began. Read
/// from the *live* element instead and the anchor is fine while the element stays the
/// right way round, then walks along with the pointer the moment it turns through — the
/// box stops growing and creeps sideways, one step per pointer move.
#[test]
fn the_anchor_holds_still_across_a_drag_that_turns_the_element_through() {
    let mut element = box_at(400.0, 300.0, 200.0, 150.0);
    element.id = "r".into();
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec!["r".to_string()]);

    // Grab the south-east handle where it is drawn, 8px out from the corner; the anchor
    // is the north-west corner at (400, 300). The pointer stays on the handle, so the
    // corner is at `x`.
    engine.begin_pointer(608.0, 458.0, false, false);

    let visible = |engine: &DrawEngine| {
        let el = engine
            .get_scene()
            .into_iter()
            .find(|e| e.id == "r")
            .unwrap();
        let lo = el.x.min(el.x + el.width);
        (lo, lo + el.width.abs())
    };

    let mut previous = f64::INFINITY;
    let mut x = 600.0;
    while x >= 160.0 {
        engine.move_pointer(x + 8.0, 458.0, false, false);
        let (lo, hi) = visible(&engine);

        assert!(
            (lo - 400.0).abs() < 1.0 || (hi - 400.0).abs() < 1.0,
            "the anchor left x=400 at pointer {x}: box is [{lo}, {hi}]"
        );
        assert!(
            lo <= previous + 1.0,
            "the left edge went backwards at pointer {x}: {lo} after {previous}"
        );
        previous = lo;
        x -= 20.0;
    }
    engine.end_pointer();

    // Well past the anchor, and mirrored.
    let el = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == "r")
        .unwrap();
    assert!(el.width < 0.0, "turned through, so mirrored");
    assert_close(el.x, 400.0);
    assert_close(el.width, -240.0);
}

// -----------------------------------------------------------------------------
// choosing a tool
// -----------------------------------------------------------------------------

/// Picking a tool that draws puts down what is held.
///
/// `setActiveTool` clears `selectedElementIds` for every tool that is not the selection
/// tool (`packages/excalidraw/components/App.tsx@1118751f:6213-6228`), and the reason is the style
/// panel: it offers the union of what the active *tool* can style and what the
/// *selection* can, and a swatch applies to the selection when there is one. So a shape
/// left selected from a moment ago silently captures the colour meant for the next thing
/// drawn — picking the bucket and choosing a green recoloured the rectangle behind it
/// instead of arming the bucket, and the fill still came out the fallback shade.
#[test]
fn choosing_a_drawing_tool_lets_go_of_the_selection() {
    let mut engine = engine_with_scene(vec![box_at(0.0, 0.0, 100.0, 100.0)]);
    engine.select_all();
    assert_eq!(engine.get_selection().len(), 1, "something to let go of");

    engine.set_tool(DrawTool::BucketFill);
    assert_eq!(
        engine.get_selection(),
        Vec::<String>::new(),
        "the bucket paints the next region, not the thing that happened to be selected"
    );
}

#[test]
fn every_tool_but_select_lets_go() {
    for tool in [
        DrawTool::Rectangle,
        DrawTool::Ellipse,
        DrawTool::Diamond,
        DrawTool::Arrow,
        DrawTool::Line,
        DrawTool::Freedraw,
        DrawTool::Text,
        DrawTool::Eraser,
        DrawTool::Hand,
        DrawTool::BucketFill,
    ] {
        let mut engine = engine_with_scene(vec![box_at(0.0, 0.0, 100.0, 100.0)]);
        engine.select_all();
        engine.set_tool(tool);
        assert!(
            engine.get_selection().is_empty(),
            "{tool:?} should have let go of the selection"
        );
    }
}

/// The one exception, and the reason the rule is written as "not the selection tool"
/// rather than "a tool that draws": going back to the select tool is how a person picks
/// up what they just made, so it has to keep it.
#[test]
fn going_back_to_select_keeps_what_is_held() {
    let mut engine = engine_with_scene(vec![box_at(0.0, 0.0, 100.0, 100.0)]);
    engine.select_all();
    let held = engine.get_selection();
    engine.set_tool(DrawTool::Select);
    assert_eq!(engine.get_selection(), held);
}

/// Drawing something selects it, and that must survive the tool reverting to select on
/// its own — otherwise nothing is ever selected after it is drawn.
#[test]
fn what_was_just_drawn_stays_selected() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(10.0, 10.0, false, false);
    engine.move_pointer(110.0, 90.0, false, false);
    engine.end_pointer();

    assert_eq!(engine.get_tool(), DrawTool::Select, "the tool reverts");
    assert_eq!(
        engine.get_selection().len(),
        1,
        "and the new shape is still held"
    );
}

// -----------------------------------------------------------------------------
// A selection that survives its own transform
// -----------------------------------------------------------------------------

/// `design.md:689`, "Selection" — the eighth row of §15's "Transform consequences", the
/// list of what *every* transform has to update. Seven of the eight are about the scene
/// (bindings, text, groups, frames, arrows, containers, attached labels) and this one is
/// about the selection itself: the things you moved are still the things that are held.
///
/// Nothing asserted it. Every resize and rotate test in this file calls `resize_element` or
/// `rotate_element` — the pure functions — and reads the element back **by id**, which
/// passes whether or not the selection survived; and the gesture-level tests in
/// `ci_handles.rs` assert the new geometry for the same reason. The claim is only visible
/// from outside, after the gesture: what `get_selection` says.
///
/// The oracle's is the same shape. `transformElements` takes `selectedElements` as its
/// input and writes the new geometry onto them, returning a boolean and no selection
/// (`packages/element/src/resizeElements.ts@1118751f:94-107`) — the ids in
/// `appState.selectedElementIds` are never rewritten, so the same elements stay held.
///
/// Every test below goes through the engine's own pointer, so what is asserted is the state
/// a person is left in, not a function's return value.
///
/// The layout the engine uses at 1:1 zoom, so a handle can be aimed at where it is drawn.
fn layout() -> HandleLayout {
    HandleLayout::screen(8.0, 26.0, 1.0)
}

fn found(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

/// A drag of the shape itself: the selection is the same before and after, and the shape
/// really did move — so this cannot pass by the drag having done nothing.
#[test]
fn a_selection_survives_its_own_move() {
    let rect = filled(box_at(100.0, 100.0, 200.0, 150.0));
    let id = rect.id.clone();
    let mut engine = engine_with_scene(vec![rect]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);

    engine.begin_pointer(150.0, 150.0, false, false);
    engine.move_pointer(190.0, 210.0, false, false);
    engine.end_pointer();

    let moved = found(&engine, &id);
    assert_close(moved.x, 140.0);
    assert_close(moved.y, 160.0);
    assert_eq!(
        engine.get_selection(),
        vec![id],
        "and it is still the thing being held"
    );
}

/// A resize by its corner, which is the transform that rewrites x, y, width and height at
/// once — the one most likely to leave a selection describing a box the element no longer
/// occupies.
#[test]
fn a_selection_survives_its_own_resize() {
    let rect = filled(box_at(100.0, 100.0, 200.0, 150.0));
    let id = rect.id.clone();
    let mut engine = engine_with_scene(vec![rect]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);

    // The south-east handle sits `handle_offset` beyond the corner.
    let offset = layout().handle_offset;
    engine.begin_pointer(300.0 + offset, 250.0 + offset, false, false);
    engine.move_pointer(360.0 + offset, 300.0 + offset, false, false);
    engine.end_pointer();

    let resized = found(&engine, &id);
    // The corner went where it was dragged.
    assert_close(resized.width, 260.0);
    assert_close(resized.height, 200.0);
    assert_eq!(engine.get_selection(), vec![id], "still held");
}

/// ...and the frame and the handles have followed the new geometry, which is the part that
/// would be wrong if only the element moved and the selection kept describing the old box.
#[test]
fn the_selection_frame_follows_the_element_it_is_a_frame_for() {
    let rect = filled(box_at(100.0, 100.0, 200.0, 150.0));
    let id = rect.id.clone();
    let mut engine = engine_with_scene(vec![rect]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);

    let before = found(&engine, &id);
    let offset = layout().handle_offset;
    engine.begin_pointer(300.0 + offset, 250.0 + offset, false, false);
    engine.move_pointer(360.0 + offset, 300.0 + offset, false, false);
    engine.end_pointer();

    let after = found(&engine, &id);
    assert_ne!(before.width, after.width, "setup: it really was resized");
    let frame = |element: &DrawElement| {
        let corners = selection_corners_padded(element, layout().frame_pad);
        let [min_x, min_y, max_x, max_y] = freehand::points_bounds(&corners.map(|p| [p.x, p.y]));
        (max_x - min_x, max_y - min_y)
    };
    assert_eq!(
        frame(&after),
        (after.width + 8.0, after.height + 8.0),
        "the frame is the resized element's own box, not the one it used to be"
    );
    // The handles are on that same ring, so a person can grab the shape again straight
    // after the resize instead of having to click it once to re-select it.
    let handles = selection_handles(&after, layout());
    let se = handles
        .iter()
        .find(|h| h.kind == HandleKind::Se)
        .expect("a shape this size keeps all eight");
    let corners = selection_corners_padded(&after, layout().handle_offset);
    assert_close(se.x, corners[2].x);
    assert_close(se.y, corners[2].y);
}

/// A rotation rewrites the angle and the position, and it is the transform most likely to
/// take the pointer off the shape it was grabbed from: the selection must not be dropped
/// just because the element moved under the cursor.
#[test]
fn a_selection_survives_its_own_rotation() {
    let rect = filled(box_at(200.0, 200.0, 200.0, 200.0));
    let id = rect.id.clone();
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);

    // Press on the rotation handle **where it is**, rather than at a point worked out from
    // the layout: it sits `rotate_gap` above the element's own top edge
    // (`src/selection/handles.rs:258-263`), not above the frame.
    let rotate = selection_handles(&rect, layout())
        .into_iter()
        .find(|h| h.kind == HandleKind::Rotate)
        .expect("a shape this size offers a rotation handle");
    engine.begin_pointer(rotate.x, rotate.y, false, false);
    engine.move_pointer(rotate.x + 120.0, rotate.y - 40.0, false, false);
    engine.end_pointer();

    let turned = found(&engine, &id);
    assert!(
        turned.angle.abs() > 1e-6,
        "setup: it really was turned, angle is {}",
        turned.angle
    );
    assert_eq!(engine.get_selection(), vec![id], "and still held");
}

/// A multi-selection, which is the case the single-element tests cannot reach: the
/// transform moves every member and the selection has to keep **all** of them — nothing
/// joining and nothing dropped.
///
/// The **order** is not asserted, and deliberately so: `selected_ids` is a `HashSet`
/// (`src/engine/mod.rs:196`) and `get_selection` walks it (`src/engine/mod.rs:741-743`), so
/// what comes back is the hash order rather than the order the ids were selected in. That
/// is a fact about the type, not a claim this test is entitled to make in either direction,
/// and pinning an order here would be pinning an accident. See the Phase 3.4a report.
#[test]
fn a_multi_selection_survives_its_own_transform() {
    let elements: Vec<DrawElement> = [(0.0, 0.0), (300.0, 0.0), (600.0, 0.0), (900.0, 0.0)]
        .into_iter()
        .map(|(x, y)| filled(box_at(x, y, 100.0, 100.0)))
        .collect();
    let ids: Vec<String> = elements.iter().map(|el| el.id.clone()).collect();
    let mut engine = engine_with_scene(elements);
    engine.set_tool(DrawTool::Select);
    engine.select(ids.clone());

    let mut held = engine.get_selection();
    held.sort();
    assert_eq!(held.len(), 4, "setup: all four selected");

    // Drag from the middle of the first one, which carries the whole selection with it.
    engine.begin_pointer(50.0, 50.0, false, false);
    engine.move_pointer(50.0, 250.0, false, false);
    engine.end_pointer();

    let mut after = engine.get_selection();
    after.sort();
    let mut expected = ids.clone();
    expected.sort();
    assert_eq!(
        after, expected,
        "the same four, none joined and none dropped"
    );
    for id in &ids {
        let element = found(&engine, id);
        assert_close(element.y, 200.0);
    }
}

/// The edge of the claim: a transform that changes what is selected would be right if the
/// element *left* the selection — deleted, say. So the selection survives a transform, but
/// not past the element's own removal. Without this, "survives its own transform" could be
/// satisfied by a selection that simply never updates.
#[test]
fn a_selection_does_not_survive_the_element_being_deleted() {
    let rect = filled(box_at(100.0, 100.0, 200.0, 150.0));
    let id = rect.id.clone();
    let mut engine = engine_with_scene(vec![rect]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    assert_eq!(engine.get_selection(), vec![id.clone()]);

    engine.delete_selection();

    assert!(
        engine.get_selection().is_empty(),
        "there is nothing left to hold, so the selection does not pretend otherwise"
    );
}
