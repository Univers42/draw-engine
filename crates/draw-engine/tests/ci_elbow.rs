//! Elbow arrows in the editor: drawn between shapes, following them, dragged by their
//! ends and segments, and converted to and from the other two types.
//!
//! The router itself is held to the oracle by `ci_elbow_oracle.rs`; these are the
//! gestures that drive it (`App.tsx@1118751f`, `LinearElementEditor`, `actionFinalize`).

mod common;
use common::*;
use draw_engine::engine::ArrowType;
use draw_engine::scene::elbow;
use draw_engine::selection::LinearHandle;
use draw_engine::*;

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

fn arrows(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == DrawElementType::Arrow && !el.is_deleted)
        .collect()
}

fn world(arrow: &DrawElement) -> Vec<[f64; 2]> {
    arrow
        .points
        .as_ref()
        .expect("an arrow has points")
        .iter()
        .map(|p| [arrow.x + p[0], arrow.y + p[1]])
        .collect()
}

/// Every run horizontal or vertical, as the router lays them.
fn assert_orthogonal(arrow: &DrawElement) {
    let points = arrow.points.as_deref().expect("points");
    assert!(points.len() >= 2, "{points:?}");
    assert!(elbow::validate_points(points), "not orthogonal: {points:?}");
}

fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) {
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=6 {
        let t = step as f64 / 6.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
}

/// Two boxes, one down and to the right of the other, and an elbow arrow drawn from
/// inside the first to inside the second.
fn drawn() -> (DrawEngine, String, String, String) {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(300.0, 200.0, 100.0, 80.0));
    let (l, r) = (left.id.clone(), right.id.clone());
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_arrow_type(ArrowType::Elbow);
    engine.set_tool(DrawTool::Arrow);
    drag(&mut engine, (70.0, 30.0), (330.0, 250.0));
    let drawn = arrows(&engine);
    assert_eq!(drawn.len(), 1, "setup: one arrow");
    (engine, drawn[0].id.clone(), l, r)
}

fn midpoint_of(engine: &DrawEngine, index: usize) -> (f64, f64) {
    let handle = engine
        .linear_points()
        .into_iter()
        .find(|h| h.handle == LinearHandle::Midpoint(index))
        .expect("the segment offers its midpoint");
    (handle.x, handle.y)
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

#[test]
fn an_arrow_drawn_between_two_shapes_binds_both_and_routes_square() {
    let (engine, id, l, r) = drawn();
    let arrow = get(&engine, &id);
    assert_eq!(arrow.elbowed, Some(true));
    assert_eq!(arrow.roundness, None, "an elbow arrow has no roundness");
    assert_eq!(arrow.angle, 0.0);
    assert_eq!(arrow.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(r.as_str()));
    assert_orthogonal(&arrow);
    let points = world(&arrow);
    assert!(points.len() >= 3, "it turns a corner: {points:?}");
    // Each end leaves its shape square on, just outside the outline.
    let (first, last) = (points[0], points[points.len() - 1]);
    assert!(
        first[0] > 100.0 || first[1] > 80.0,
        "the tail is on the left box's outline: {first:?}"
    );
    assert!(
        last[0] < 300.0 || last[1] < 200.0,
        "the head is on the right box's outline: {last:?}"
    );
    // The route is the router's own: routing it again changes nothing.
    let mut again = arrow.clone();
    let scene = engine.get_scene();
    elbow::reroute(&mut again, &elbow::Board::new(scene.iter()));
    assert_eq!(again.points, arrow.points);
    assert_eq!(engine.get_tool(), DrawTool::Select, "the tool settles");
}

#[test]
fn a_click_then_a_click_places_an_elbow_arrow_by_its_ends() {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(300.0, 200.0, 100.0, 80.0));
    let (l, r) = (left.id.clone(), right.id.clone());
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_arrow_type(ArrowType::Elbow);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(70.0, 30.0, false, false);
    engine.end_pointer();
    engine.move_pointer(200.0, 150.0, false, false);
    engine.move_pointer(330.0, 250.0, false, false);
    engine.begin_pointer(330.0, 250.0, false, false);
    engine.end_pointer();
    assert_eq!(
        engine.linear_in_progress(),
        None,
        "the second click ends it"
    );
    let placed = arrows(&engine);
    assert_eq!(placed.len(), 1);
    let arrow = &placed[0];
    assert_eq!(arrow.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(r.as_str()));
    assert_orthogonal(arrow);
}

#[test]
fn an_end_over_nothing_is_left_free() {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let l = left.id.clone();
    let mut engine = engine_with_scene(vec![left]);
    engine.set_arrow_type(ArrowType::Elbow);
    engine.set_tool(DrawTool::Arrow);
    drag(&mut engine, (70.0, 30.0), (400.0, 300.0));
    let arrow = arrows(&engine).remove(0);
    assert_eq!(arrow.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(arrow.end_binding, None);
    let points = world(&arrow);
    assert_eq!(
        points[points.len() - 1],
        [400.0, 300.0],
        "a free end is where it was let go"
    );
    assert_orthogonal(&arrow);
}

#[test]
fn ctrl_draws_an_elbow_arrow_bound_to_nothing() {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(300.0, 200.0, 100.0, 80.0));
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_arrow_type(ArrowType::Elbow);
    engine.set_tool(DrawTool::Arrow);
    engine.set_ctrl_held(true);
    drag(&mut engine, (70.0, 30.0), (330.0, 250.0));
    let arrow = arrows(&engine).remove(0);
    assert_eq!(arrow.start_binding, None);
    assert_eq!(arrow.end_binding, None);
    let points = world(&arrow);
    assert_eq!(points[0], [70.0, 30.0]);
    assert_eq!(points[points.len() - 1], [330.0, 250.0]);
}

// ---------------------------------------------------------------------------
// Following the shapes
// ---------------------------------------------------------------------------

#[test]
fn moving_a_bound_shape_routes_the_arrow_after_it() {
    let (mut engine, id, _l, r) = drawn();
    let before = get(&engine, &id);
    engine.select(vec![r.clone()]);
    drag(&mut engine, (390.0, 275.0), (390.0, 475.0));
    let moved = get(&engine, &r);
    assert_close(moved.y, 400.0);
    let after = get(&engine, &id);
    assert_eq!(
        after.end_binding.as_deref(),
        Some(r.as_str()),
        "still bound"
    );
    assert_ne!(after.points, before.points, "the route changed");
    assert_orthogonal(&after);
    let last = *world(&after).last().expect("points");
    assert!(
        last[1] >= 400.0 - 20.0 && last[1] <= 480.0 + 20.0,
        "the head went with the box: {last:?}"
    );
    // And it is the route the router gives for where the box is now.
    let mut expected = after.clone();
    let scene = engine.get_scene();
    elbow::reroute(&mut expected, &elbow::Board::new(scene.iter()));
    assert_eq!(expected.points, after.points);
}

#[test]
fn resizing_a_bound_shape_routes_the_arrow_after_it() {
    let (mut engine, id, _l, r) = drawn();
    let before = get(&engine, &id);
    engine.select(vec![r.clone()]);
    // The handle the engine paints, a few pixels outside the corner.
    let corner = {
        let view = engine.paint_view();
        let handles = selection_handles(&get(&engine, &r), view.handle_layout);
        let se = handles
            .iter()
            .find(|handle| handle.kind == HandleKind::Se)
            .expect("no south-east handle");
        Point { x: se.x, y: se.y }
    };
    drag(
        &mut engine,
        (corner.x, corner.y),
        (corner.x + 150.0, corner.y + 120.0),
    );
    let grown = get(&engine, &r);
    assert!(grown.width > 200.0, "setup: the box grew, {grown:?}");
    let after = get(&engine, &id);
    assert_ne!(after.points, before.points);
    assert_orthogonal(&after);
}

#[test]
fn an_elbow_arrow_dragged_with_both_its_shapes_moves_as_it_is() {
    let (mut engine, id, _l, _r) = drawn();
    let before = get(&engine, &id);
    engine.select_all();
    drag(&mut engine, (50.0, 40.0), (90.0, 70.0));
    let after = get(&engine, &id);
    assert_eq!(after.points, before.points, "not routed again");
    assert_close(after.x, before.x + 40.0);
    assert_close(after.y, before.y + 30.0);
}

#[test]
fn a_moved_segment_travels_with_the_arrow_and_both_its_shapes() {
    let (mut engine, id, _l, _r) = drawn();
    engine.select(vec![id.clone()]);
    let points = world(&get(&engine, &id));
    let index = points
        .windows(2)
        .position(|w| (w[0][0] - w[1][0]).abs() < 1e-6)
        .expect("a vertical run");
    let at = midpoint_of(&engine, index);
    drag(&mut engine, at, (at.0 + 40.0, at.1));
    let before = get(&engine, &id);
    assert!(before.fixed_segments.is_some(), "setup: a fixed segment");
    engine.select_all();
    drag(&mut engine, (20.0, 60.0), (60.0, 90.0));
    let after = get(&engine, &id);
    assert_eq!(after.points, before.points);
    assert_eq!(after.fixed_segments, before.fixed_segments);
    assert_close(after.x, before.x + 40.0);
    assert_close(after.y, before.y + 30.0);
}

#[test]
fn a_bound_elbow_arrow_dragged_alone_stays_put() {
    let (mut engine, id, _l, _r) = drawn();
    let before = get(&engine, &id);
    engine.select(vec![id.clone()]);
    // On the middle of the first run, clear of its handles.
    let points = world(&before);
    let grab = (
        points[0][0] + (points[1][0] - points[0][0]) * 0.3,
        points[0][1] + (points[1][1] - points[0][1]) * 0.3,
    );
    drag(&mut engine, grab, (grab.0 + 120.0, grab.1 + 90.0));
    let after = get(&engine, &id);
    assert_eq!((after.x, after.y), (before.x, before.y));
    assert_eq!(after.points, before.points);
}

// ---------------------------------------------------------------------------
// Ends and segments
// ---------------------------------------------------------------------------

#[test]
fn a_selected_elbow_arrow_offers_its_ends_and_segments_but_no_box() {
    let (mut engine, id, _l, _r) = drawn();
    engine.select(vec![id.clone()]);
    let arrow = get(&engine, &id);
    let count = arrow.points.as_ref().map_or(0, Vec::len);
    let handles = engine.linear_points();
    let ends: Vec<LinearHandle> = handles
        .iter()
        .map(|h| h.handle)
        .filter(|h| matches!(h, LinearHandle::Point(_)))
        .collect();
    assert_eq!(
        ends,
        vec![LinearHandle::Point(0), LinearHandle::Point(count - 1)]
    );
    let segments = handles
        .iter()
        .filter(|h| matches!(h.handle, LinearHandle::Midpoint(_)))
        .count();
    assert_eq!(segments, count - 1, "every run is long enough to take");
}

#[test]
fn dragging_an_end_off_its_shape_frees_it_and_back_on_binds_it() {
    let (mut engine, id, _l, r) = drawn();
    engine.select(vec![id.clone()]);
    let last = *world(&get(&engine, &id)).last().expect("points");
    drag(&mut engine, (last[0], last[1]), (600.0, 500.0));
    let freed = get(&engine, &id);
    assert_eq!(freed.end_binding, None);
    assert_eq!(*world(&freed).last().expect("points"), [600.0, 500.0]);
    assert_orthogonal(&freed);
    drag(&mut engine, (600.0, 500.0), (360.0, 250.0));
    let bound = get(&engine, &id);
    assert_eq!(bound.end_binding.as_deref(), Some(r.as_str()));
    assert_orthogonal(&bound);
}

#[test]
fn a_segment_dragged_across_stays_there_and_a_double_click_lets_it_go() {
    let (mut engine, id, _l, _r) = drawn();
    engine.select(vec![id.clone()]);
    let arrow = get(&engine, &id);
    let points = world(&arrow);
    // The first run that is vertical, dragged 40 to the right.
    let index = points
        .windows(2)
        .position(|w| (w[0][0] - w[1][0]).abs() < 1e-6)
        .expect("a vertical run");
    let x = points[index][0];
    let at = midpoint_of(&engine, index);
    drag(&mut engine, at, (at.0 + 40.0, at.1));
    let moved = get(&engine, &id);
    let fixed = moved.fixed_segments.clone().expect("a fixed segment");
    assert_eq!(fixed.len(), 1);
    let kept = &fixed[0];
    assert_close(moved.x + kept.start[0], x + 40.0);
    assert_close(moved.x + kept.end[0], x + 40.0);
    assert_orthogonal(&moved);
    let on = world(&moved);
    assert!(
        on.windows(2)
            .any(|w| (w[0][0] - (x + 40.0)).abs() < 1e-6 && (w[1][0] - (x + 40.0)).abs() < 1e-6),
        "the route runs through the moved segment: {on:?}"
    );

    let at = midpoint_of(&engine, kept.index - 1);
    engine.handle_double_click(at.0, at.1);
    let released = get(&engine, &id);
    assert_eq!(released.fixed_segments, None, "let go of");
    assert_eq!(released.points, arrow.points, "routed as before the drag");
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A sharp arrow bound between two boxes, selected.
fn sharp_between() -> (DrawEngine, String, String, String) {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(300.0, 200.0, 100.0, 80.0));
    let (l, r) = (left.id.clone(), right.id.clone());
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_arrow_type(ArrowType::Sharp);
    engine.set_tool(DrawTool::Arrow);
    drag(&mut engine, (70.0, 30.0), (330.0, 250.0));
    let id = arrows(&engine).remove(0).id;
    engine.select(vec![id.clone()]);
    (engine, id, l, r)
}

#[test]
fn a_bound_arrow_made_elbow_is_routed_between_its_shapes() {
    let (mut engine, id, l, r) = sharp_between();
    engine.set_arrow_type(ArrowType::Elbow);
    let arrow = get(&engine, &id);
    assert_eq!(arrow.elbowed, Some(true));
    assert_eq!(arrow.roundness, None);
    assert_eq!(arrow.fixed_segments, None);
    assert_eq!(arrow.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(r.as_str()));
    assert_orthogonal(&arrow);
    assert!(arrow.points.as_ref().is_some_and(|p| p.len() >= 3));
    assert_eq!(engine.selection_style().arrow_type, Some(ArrowType::Elbow));
}

#[test]
fn an_elbow_arrow_made_sharp_or_curved_is_laid_straight_between_its_ends() {
    let (mut engine, id, l, r) = sharp_between();
    engine.set_arrow_type(ArrowType::Elbow);
    let routed = world(&get(&engine, &id));
    engine.set_arrow_type(ArrowType::Sharp);
    let sharp = get(&engine, &id);
    assert_ne!(sharp.elbowed, Some(true));
    assert_eq!(sharp.roundness, None);
    assert_eq!(sharp.points.as_ref().map(Vec::len), Some(2), "straight");
    assert_eq!(sharp.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(sharp.end_binding.as_deref(), Some(r.as_str()));
    let straight = world(&sharp);
    assert!(
        (straight[0][0] - routed[0][0]).hypot(straight[0][1] - routed[0][1]) < 20.0,
        "from about where the route began: {straight:?} {routed:?}"
    );
    engine.set_arrow_type(ArrowType::Round);
    let round = get(&engine, &id);
    assert!(round.roundness.is_some());
    assert_ne!(round.elbowed, Some(true));
}

#[test]
fn the_arrow_key_again_cycles_sharp_round_elbow() {
    let mut engine = engine_with_scene(Vec::new());
    engine.activate_tool(DrawTool::Arrow);
    let next = |engine: &DrawEngine| engine.selection_style().arrow_type;
    assert_eq!(
        next(&engine),
        Some(ArrowType::Round),
        "the oracle's default"
    );
    engine.drain_events();
    engine.activate_tool(DrawTool::Arrow);
    assert_eq!(next(&engine), Some(ArrowType::Elbow));
    assert_eq!(
        engine.drain_events().tool,
        Some(DrawTool::Arrow),
        "announced, so the host re-reads its panel"
    );
    engine.activate_tool(DrawTool::Arrow);
    assert_eq!(next(&engine), Some(ArrowType::Sharp));
    engine.activate_tool(DrawTool::Arrow);
    assert_eq!(next(&engine), Some(ArrowType::Round));
    assert_eq!(engine.get_tool(), DrawTool::Arrow, "the tool stays out");
}

// ---------------------------------------------------------------------------
// Hit test and bounds
// ---------------------------------------------------------------------------

fn corner_arrow() -> DrawElement {
    let mut arrow = connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow);
    arrow.elbowed = Some(true);
    arrow.roundness = None;
    arrow.stroke_width = 2.0;
    arrow.points = Some(vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]]);
    arrow.width = 100.0;
    arrow.height = 100.0;
    arrow
}

#[test]
fn an_elbow_arrow_is_hit_on_its_rounded_corner_not_the_sharp_one() {
    let arrow = corner_arrow();
    // The quadratic from (84, 0) through (100, 0) to (100, 16) passes (96, 4) at its middle.
    assert!(hit_test_element(&arrow, 96.0, 4.0, 0.5));
    assert!(
        !hit_test_element(&arrow, 100.0, 0.0, 0.5),
        "the sharp corner is cut"
    );
    assert!(hit_test_element(&arrow, 50.0, 0.0, 0.5));
    assert!(hit_test_element(&arrow, 100.0, 50.0, 0.5));
}

#[test]
fn an_elbow_arrow_is_bounded_by_its_corners() {
    let arrow = corner_arrow();
    let b = element_bounds(&arrow);
    assert_eq!(
        (b.min_x, b.min_y, b.max_x, b.max_y),
        (0.0, 0.0, 100.0, 100.0)
    );
}

// ---------------------------------------------------------------------------
// For the flowchart
// ---------------------------------------------------------------------------

#[test]
fn bind_and_route_connects_two_shapes() {
    let a = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let b = filled(box_at(300.0, 200.0, 100.0, 80.0));
    let mut arrow = connector(100.0, 40.0, 300.0, 240.0, DrawElementType::Arrow);
    arrow.elbowed = Some(true);
    arrow.roundness = None;
    let elements = [a.clone(), b.clone()];
    elbow::bind_and_route(&mut arrow, &a, &b, &elbow::Board::new(elements.iter()), 1.0);
    assert_eq!(arrow.start_binding.as_deref(), Some(a.id.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(b.id.as_str()));
    assert_orthogonal(&arrow);
}
