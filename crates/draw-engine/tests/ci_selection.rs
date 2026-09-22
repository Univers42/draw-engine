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

    let geom = resize_element(&element, handle, target.x, target.y, 4.0, aspect);
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
    let geom = resize_element(&element, HandleKind::N, 60.0, -10.0, 4.0, None);
    assert_close(geom.width, 80.0);
    assert_close(geom.height, 60.0);
}

#[test]
fn resize_s_handle_preserves_width() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::S, 60.0, 90.0, 4.0, None);
    assert_close(geom.width, 80.0);
    assert_close(geom.height, 80.0);
}

#[test]
fn resize_e_handle_preserves_height() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::E, 140.0, 30.0, 4.0, None);
    assert_close(geom.height, 40.0);
    assert_close(geom.width, 120.0);
}

#[test]
fn resize_w_handle_preserves_height() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::W, -20.0, 30.0, 4.0, None);
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
    let near = resize_element(&element, HandleKind::Se, 54.0, 54.0, 1.0, None);
    assert!(near.width > 0.0 && near.width < 6.0);

    // Well past it: the same size, on the other side, and mirrored.
    let through = resize_element(&element, HandleKind::Se, 10.0, 10.0, 1.0, None);
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
    let once = resize_element(&element, HandleKind::Se, 10.0, 10.0, 1.0, None);

    let mut mirrored = element.clone();
    mirrored.x = once.x;
    mirrored.y = once.y;
    mirrored.width = once.width;
    mirrored.height = once.height;
    assert!(mirrored.width < 0.0 && mirrored.height < 0.0);

    // It now occupies [10, 50]; its south-east handle is at (50, 50) and the anchor
    // opposite that is (10, 10). Drag the handle out through *that* anchor.
    let twice = resize_element(&mirrored, HandleKind::Se, -30.0, -30.0, 1.0, None);
    assert!(twice.width > 0.0, "back the right way round");
    assert!(twice.height > 0.0);
}

/// Growing a mirrored element does not quietly un-mirror it. Only turning it back
/// through its anchor does that.
#[test]
fn growing_a_mirrored_element_keeps_it_mirrored() {
    let element = box_at(50.0, 50.0, 60.0, 60.0);
    let once = resize_element(&element, HandleKind::Se, 10.0, 10.0, 1.0, None);
    let mut mirrored = element.clone();
    mirrored.x = once.x;
    mirrored.y = once.y;
    mirrored.width = once.width;
    mirrored.height = once.height;

    // Away from the anchor at (10, 10), so no flip.
    let bigger = resize_element(&mirrored, HandleKind::Se, 110.0, 110.0, 1.0, None);
    assert!(bigger.width < 0.0, "still mirrored");
    assert_close(bigger.width.abs(), 100.0);
}

/// The minimum is a floor on size, not a wall the pointer collides with.
#[test]
fn the_minimum_size_applies_to_either_side() {
    let element = box_at(50.0, 50.0, 60.0, 60.0);
    let geom = resize_element(&element, HandleKind::Se, 49.0, 49.0, 4.0, None);
    assert_close(geom.width.abs(), 4.0);
    assert!(geom.width < 0.0, "and on the far side of the anchor");
}

#[test]
fn resize_aspect_ratio_lock_se() {
    let element = box_at(0.0, 0.0, 100.0, 50.0);
    let ratio = 100.0 / 50.0;
    let geom = resize_element(&element, HandleKind::Se, 200.0, 80.0, 4.0, Some(ratio));
    assert_close(geom.width / geom.height, ratio);
}

#[test]
fn resize_aspect_ratio_lock_nw() {
    let element = box_at(50.0, 50.0, 80.0, 40.0);
    let ratio = 80.0 / 40.0;
    let geom = resize_element(&element, HandleKind::Nw, 10.0, 20.0, 4.0, Some(ratio));
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

#[test]
fn resize_rotate_handle_is_noop() {
    let element = box_at(10.0, 20.0, 100.0, 50.0);
    let geom = resize_element(&element, HandleKind::Rotate, 50.0, -100.0, 4.0, None);
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

    // Grab the south-east handle; the anchor is the north-west corner at (400, 300).
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
        engine.move_pointer(x, 458.0, false, false);
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
/// tool (`packages/excalidraw/components/App.tsx:6211-6226`), and the reason is the style
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
