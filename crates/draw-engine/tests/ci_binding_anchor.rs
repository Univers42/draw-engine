//! Where an arrow end attaches, and that it stays there.
//!
//! An end bound to a shape stores an anchor in the shape's own frame — Excalidraw's
//! `fixedPoint` and bind `mode` (`packages/element/src/binding.ts`) — rather than only the
//! shape's id. Three things were wrong while it stored only the id, and each has a case
//! here:
//!
//! - a turned shape left its arrows at a world offset, because the attachment ignored the
//!   turn;
//! - an arrow always arrived on the line between the two centres, wherever it was let go;
//! - with the arrow tool in hand and nothing drawn yet, hovering a shape suggested nothing.
//!
//! The rest pins the edges: what can be bound through what, shapes a fraction of a unit
//! across deep in a zoomed presentation, nested shapes, and groups turned and scaled with
//! their arrows.

mod common;
use common::*;
use draw_engine::scene::binding::{anchor, binding_gap, focus_point, is_inside, End};
use draw_engine::scene::element::BindMode;
use draw_engine::selection::linear::world_points;
use draw_engine::*;

const B: &str = "b";

fn shape(id: &str, mut el: DrawElement) -> DrawElement {
    el.id = id.into();
    el
}

/// B, filled, 100×80 at (300, 300): centre (350, 340).
fn target() -> DrawElement {
    shape(B, filled(box_at(300.0, 300.0, 100.0, 80.0)))
}

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

fn the_arrow(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .rev()
        .find(|el| el.kind == DrawElementType::Arrow && !el.is_deleted)
        .expect("an arrow was drawn")
}

/// Draws an arrow by dragging from `from` to `to`, in several steps.
fn draw_arrow(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) -> DrawElement {
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=5 {
        let t = step as f64 / 5.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
    // The one just drawn is selected; it is not always the topmost arrow, since one drawn
    // in a frame goes below the frame.
    let drawn = engine.get_selection();
    engine
        .get_scene()
        .into_iter()
        .find(|el| drawn.contains(&el.id) && el.kind == DrawElementType::Arrow)
        .expect("an arrow was drawn")
}

fn ends(arrow: &DrawElement) -> (Point, Point) {
    let points = world_points(arrow);
    (points[0], *points.last().unwrap())
}

fn near(a: Point, b: Point, tolerance: f64) -> bool {
    (a.x - b.x).hypot(a.y - b.y) <= tolerance
}

/// Turns the one selected element by dragging its rotation handle to `to`, found the way
/// a person finds it: by where the cursor offers a grab above the selection.
fn turn_selection(engine: &mut DrawEngine, above: (f64, f64), to: (f64, f64)) {
    engine.set_tool(DrawTool::Select);
    let handle_y = (0..120)
        .map(|dy| above.1 - dy as f64)
        .find(|&y| engine.hover_cursor(above.0, y) == HoverCursor::Grab)
        .expect("the selection has a rotation handle");
    engine.begin_pointer(above.0, handle_y, false, false);
    engine.move_pointer(to.0, to.1, false, false);
    engine.end_pointer();
}

/// An arrow into B from straight above, its end anchored at B's left midpoint.
fn arrow_into_b(fixed_point: [f64; 2], mode: BindMode) -> DrawElement {
    let mut arrow = connector(350.0, 0.0, 300.0, 340.0, DrawElementType::Arrow);
    arrow.id = "arrow".into();
    arrow.end_binding = Some(B.into());
    arrow.end_fixed_point = Some(fixed_point);
    arrow.end_bind_mode = Some(mode);
    arrow
}

// ------------------------------------------------------------------------ turning

/// The first bug. A quarter turn carries B's left side to its top, and an end anchored
/// there goes with it: straight above the centre, the gap clear of the turned outline.
/// Aiming at the unturned box put it at y = 294, twenty units inside the turned one.
#[test]
fn an_end_turns_with_its_shape() {
    let mut engine = engine_with_scene(vec![target(), arrow_into_b([0.0, 0.5], BindMode::Orbit)]);
    engine.select(vec![B.into()]);
    turn_selection(&mut engine, (350.0, 300.0), (650.0, 340.0));

    let b = get(&engine, B);
    assert_close(b.angle, std::f64::consts::FRAC_PI_2);
    let (_, tip) = ends(&get(&engine, "arrow"));
    // Turned, B spans y 290..390; its top is 290, the gap above that.
    assert!(
        near(
            tip,
            Point {
                x: 350.0,
                y: 290.0 - binding_gap(&b)
            },
            1e-6
        ),
        "the end stayed with the turned side: {tip:?}"
    );
}

/// An end let go inside a shape is exactly where it was put, and a turn carries it round
/// the centre like any other point of the shape — no offset, at any angle.
#[test]
fn an_inside_end_is_carried_round_exactly() {
    let mut engine =
        engine_with_scene(vec![target(), arrow_into_b([0.25, 0.25], BindMode::Inside)]);
    engine.select(vec![B.into()]);
    turn_selection(&mut engine, (350.0, 300.0), (650.0, 340.0));

    let (_, tip) = ends(&get(&engine, "arrow"));
    // (325, 320) is (-25, -20) from the centre; a quarter turn makes that (20, -25).
    assert!(
        near(tip, Point { x: 370.0, y: 315.0 }, 1e-6),
        "the end kept its place on the shape: {tip:?}"
    );
}

/// At every angle an orbiting end is on the line from its turned anchor toward where the
/// arrow comes from, and outside the turned shape — on whichever side faces the arrow.
#[test]
fn every_angle_keeps_the_end_on_its_line_and_off_the_shape() {
    for degrees in (0..360).step_by(15) {
        let mut b = target();
        b.angle = (degrees as f64).to_radians();
        let scene = refresh_bindings(&[b.clone(), arrow_into_b([0.0, 0.5], BindMode::Orbit)]);
        let arrow = scene.iter().find(|el| el.id == "arrow").unwrap();
        let (tail, tip) = ends(arrow);
        let focus = focus_point(&b, [0.0, 0.5]);
        assert!(
            !is_inside(&b, tip),
            "{degrees}°: the end is inside B at {tip:?}"
        );
        let (along, to_tail) = (
            (tip.x - focus.x, tip.y - focus.y),
            (tail.x - focus.x, tail.y - focus.y),
        );
        let cross = along.0 * to_tail.1 - along.1 * to_tail.0;
        let dot = along.0 * to_tail.0 + along.1 * to_tail.1;
        let reach = to_tail.0.hypot(to_tail.1);
        assert!(
            cross.abs() <= 1e-6 * reach * reach,
            "{degrees}°: off the line, {tip:?}"
        );
        assert!(
            dot >= 0.0 && along.0.hypot(along.1) <= reach,
            "{degrees}°: not between, {tip:?}"
        );
    }
}

// ------------------------------------------------------------ where it is let go

/// The second bug. Let go above B's top edge, away from its middle, the end arrives there
/// — on the line it was drawn along — instead of wherever a line from B's centre crosses.
#[test]
fn the_end_arrives_where_it_was_let_go() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (380.0, 100.0), (380.0, 295.0));

    let a = anchor(&arrow, End::End).expect("bound to B");
    assert_eq!(a.element_id, B);
    assert_eq!(a.mode, BindMode::Orbit);
    let (_, tip) = ends(&arrow);
    assert!(
        near(
            tip,
            Point {
                x: 380.0,
                y: 300.0 - binding_gap(&target())
            },
            1e-6
        ),
        "the end stayed on the line it was drawn along: {tip:?}"
    );
}

/// Near the middle of a side, the end snaps to it — close enough to read as meant, and
/// only that close: a quarter of the shape at most.
#[test]
fn near_a_side_midpoint_the_end_snaps_to_it() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (350.0, 100.0), (356.0, 295.0));
    let a = anchor(&arrow, End::End).unwrap();
    assert_eq!(a.fixed_point, [0.5, 0.0], "the top midpoint");
    assert_eq!(a.mode, BindMode::Orbit);
}

/// Inside a shape the end is exactly where it was let go (`binding.ts@1118751f:847-854`).
#[test]
fn inside_a_shape_the_end_is_exactly_where_let_go() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (320.0, 330.0));
    let a = anchor(&arrow, End::End).unwrap();
    assert_eq!(a.mode, BindMode::Inside);
    assert_point_close(ends(&arrow).1, Point { x: 320.0, y: 330.0 });
}

/// Alt binds exactly where the end is, even from outside the shape.
#[test]
fn alt_binds_exactly_where_the_end_is() {
    let mut engine = engine_with_scene(vec![target()]);
    engine.set_tool(DrawTool::Arrow);
    // As the host reports it: Alt with the press, and again with every move.
    engine.begin_pointer(380.0, 100.0, false, true);
    engine.set_alt_held(true);
    engine.move_pointer(380.0, 200.0, false, false);
    engine.move_pointer(380.0, 295.0, false, false);
    engine.end_pointer();
    let arrow = the_arrow(&engine);
    let a = anchor(&arrow, End::End).expect("still bound");
    assert_eq!(a.mode, BindMode::Inside);
    assert_point_close(ends(&arrow).1, Point { x: 380.0, y: 295.0 });
}

/// Ctrl/Cmd binds nothing, as in Excalidraw.
#[test]
fn ctrl_binds_nothing() {
    let mut engine = engine_with_scene(vec![target()]);
    engine.set_ctrl_held(true);
    let arrow = draw_arrow(&mut engine, (380.0, 100.0), (380.0, 295.0));
    assert_eq!(arrow.end_binding, None);
    assert_point_close(ends(&arrow).1, Point { x: 380.0, y: 295.0 });
}

/// Starting inside a shape and ending beside the same shape puts both ends inside it —
/// orbiting a shape from inside it has no side to arrive on (`binding.ts@1118751f:776-826`).
#[test]
fn both_ends_on_one_shape_sit_where_they_were_put() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (320.0, 320.0), (405.0, 340.0));
    let (start, end) = (
        anchor(&arrow, End::Start).unwrap(),
        anchor(&arrow, End::End).unwrap(),
    );
    assert_eq!((start.mode, end.mode), (BindMode::Inside, BindMode::Inside));
    let (tail, tip) = ends(&arrow);
    assert_point_close(tail, Point { x: 320.0, y: 320.0 });
    assert_point_close(tip, Point { x: 405.0, y: 340.0 });
}

/// Passing over the start's shape on the way elsewhere leaves no trace on the start.
#[test]
fn crossing_the_start_shape_does_not_pin_the_start() {
    let far = shape("far", filled(box_at(700.0, 300.0, 80.0, 80.0)));
    let mut engine = engine_with_scene(vec![target(), far]);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(295.0, 340.0, false, false);
    let before = anchor(&the_arrow(&engine), End::Start).unwrap();
    for x in [320.0, 360.0, 420.0, 600.0, 695.0] {
        engine.move_pointer(x, 340.0, false, false);
    }
    engine.end_pointer();
    let arrow = the_arrow(&engine);
    assert_eq!(
        anchor(&arrow, End::Start),
        Some(before),
        "the start is as pressed"
    );
    assert_eq!(arrow.end_binding.as_deref(), Some("far"));
}

// ------------------------------------------------------------------ hovering

/// The third bug. With the arrow tool in hand and nothing drawn, a shape the pointer is
/// near lights up — and goes out when the pointer leaves, or the tool is put down.
#[test]
fn hovering_with_the_arrow_tool_suggests_the_shape() {
    let mut engine = engine_with_scene(vec![target()]);
    engine.set_tool(DrawTool::Arrow);

    engine.hover_pointer(405.0, 340.0);
    assert_eq!(
        engine
            .paint_view()
            .binding_highlight
            .map(|el| el.id.as_str()),
        Some(B)
    );
    let (mark, snaps) = engine
        .paint_view()
        .binding_midpoint
        .expect("a midpoint is marked");
    assert_point_close(mark, Point { x: 400.0, y: 340.0 });
    assert!(snaps, "five units from the right midpoint snaps to it");

    engine.hover_pointer(600.0, 100.0);
    assert!(
        engine.paint_view().binding_highlight.is_none(),
        "nothing near"
    );

    engine.hover_pointer(405.0, 340.0);
    engine.end_hover();
    assert!(
        engine.paint_view().binding_highlight.is_none(),
        "the pointer left"
    );

    engine.hover_pointer(405.0, 340.0);
    engine.set_tool(DrawTool::Select);
    assert!(
        engine.paint_view().binding_highlight.is_none(),
        "the tool was put down"
    );
}

/// Every other tool ignores the hover; Ctrl suppresses it as it suppresses the bind.
#[test]
fn only_the_arrow_tool_suggests() {
    let mut engine = engine_with_scene(vec![target()]);
    for tool in [DrawTool::Select, DrawTool::Line, DrawTool::Rectangle] {
        engine.set_tool(tool);
        engine.hover_pointer(405.0, 340.0);
        assert!(engine.paint_view().binding_highlight.is_none(), "{tool:?}");
    }
    engine.set_tool(DrawTool::Arrow);
    engine.set_ctrl_held(true);
    engine.hover_pointer(405.0, 340.0);
    assert!(engine.paint_view().binding_highlight.is_none(), "ctrl held");
}

/// What the hover promises is what the press does: the marked midpoint is where the
/// arrow's tail is anchored.
#[test]
fn the_press_keeps_the_hovers_promise() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (405.0, 342.0), (600.0, 342.0));
    let a = anchor(&arrow, End::Start).unwrap();
    assert_eq!(a.fixed_point, [1.0, 0.5], "the right midpoint");
}

// ------------------------------------------------------------ what can be bound

/// A filled shape hides whatever is under it from an arrow end, however small that is
/// (`collision.ts@1118751f:356-414`). An empty one does not.
#[test]
fn a_filled_shape_hides_what_is_beneath_it() {
    let small = shape("small", filled(box_at(320.0, 320.0, 20.0, 20.0)));
    let cover = shape("cover", filled(box_at(300.0, 300.0, 100.0, 80.0)));
    let mut engine = engine_with_scene(vec![small.clone(), cover]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (330.0, 330.0));
    assert_eq!(arrow.end_binding.as_deref(), Some("cover"));

    let empty = shape("cover", box_at(300.0, 300.0, 100.0, 80.0));
    let mut engine = engine_with_scene(vec![small, empty]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (330.0, 330.0));
    assert_eq!(
        arrow.end_binding.as_deref(),
        Some("small"),
        "the smallest wins"
    );
}

/// A shape nested inside an empty one is reachable however the two are stacked.
#[test]
fn a_nested_shape_is_reachable_either_way_up() {
    for inner_on_top in [true, false] {
        let outer = shape("outer", box_at(200.0, 200.0, 400.0, 300.0));
        let inner = shape("inner", filled(box_at(300.0, 300.0, 100.0, 80.0)));
        let scene = if inner_on_top {
            vec![outer, inner]
        } else {
            vec![inner, outer]
        };
        let mut engine = engine_with_scene(scene);
        let arrow = draw_arrow(&mut engine, (100.0, 100.0), (330.0, 330.0));
        assert_eq!(
            arrow.end_binding.as_deref(),
            Some("inner"),
            "inner on top: {inner_on_top}"
        );
    }
}

/// A locked shape is not something the pointer acts on, so not a target either.
#[test]
fn a_locked_shape_is_not_a_target() {
    let mut b = target();
    b.locked = Some(true);
    let mut engine = engine_with_scene(vec![b]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (330.0, 330.0));
    assert_eq!(arrow.end_binding, None);
}

/// Free text is a target, as in Excalidraw; a label belongs to its container.
#[test]
fn free_text_is_a_target_and_a_label_is_not() {
    let note = shape("note", text_at(300.0, 300.0, 100.0, 30.0));
    let mut engine = engine_with_scene(vec![note]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (330.0, 310.0));
    assert_eq!(arrow.end_binding.as_deref(), Some("note"));

    let mut label = shape("label", text_at(300.0, 300.0, 100.0, 30.0));
    label.container_id = Some("elsewhere".into());
    let mut engine = engine_with_scene(vec![label]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (330.0, 310.0));
    assert_eq!(arrow.end_binding, None);
}

// ----------------------------------------------------------------- at any depth

/// Deep inside a zoomed presentation a shape can be a unit across. Excalidraw's gap is a
/// fixed six units, which left its arrows floating shapes away from it; here the gap is
/// capped to a quarter of the shape, so a binding looks the same at every depth.
#[test]
fn a_tiny_shape_keeps_its_arrow_close() {
    for side in [0.004, 0.2, 2.0, 24.0, 200.0] {
        let tiny = shape(B, filled(box_at(1000.0, 1000.0, side, side)));
        let y = 1000.0 + side / 2.0;
        let mut arrow = connector(1000.0 - side * 5.0, y, 1000.0, y, DrawElementType::Arrow);
        arrow.id = "arrow".into();
        arrow.end_binding = Some(B.into());
        let scene = refresh_bindings(&[tiny.clone(), arrow]);
        let (_, tip) = ends(scene.iter().find(|el| el.id == "arrow").unwrap());
        let gap = 1000.0 - tip.x;
        assert!(gap > 0.0, "side {side}: the end is outside the shape");
        assert!(
            gap <= side * 0.25 + 1e-9 && gap <= 6.0 + 1e-9,
            "side {side}: gap {gap} dwarfs the shape"
        );
        assert_close(tip.y, 1000.0 + side / 2.0);
    }
}

/// The same drawn by hand at the deepest zoom. The reach is Excalidraw's, in world units
/// (`maxBindingDistance_simple`, never below 15), so at 30× it spans hundreds of screen
/// pixels and a shape two units across is easy to hit; the arrow starts 40 units away so
/// that only its end binds.
#[test]
fn binding_at_the_deepest_zoom() {
    let tiny = shape(B, filled(box_at(10.0, 10.0, 2.0, 2.0)));
    let mut engine = engine_with_scene(vec![tiny]);
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale: MAX_ZOOM,
    });
    let scale = MAX_ZOOM;
    // A screen point 10px left of the shape's left midpoint.
    let (sx, sy) = (10.0 * scale - 10.0, 11.0 * scale);
    let arrow = draw_arrow(&mut engine, (sx - 40.0 * scale, sy), (sx, sy));
    assert_eq!(arrow.start_binding, None, "setup");
    let a = anchor(&arrow, End::End).expect("bound at the deepest zoom");
    assert_eq!(a.fixed_point, [0.0, 0.5]);
    let (_, tip) = ends(&arrow);
    assert!(
        !is_inside(&get(&engine, B), tip) && 10.0 - tip.x <= 0.5 + 1e-9,
        "{tip:?}"
    );
}

// ------------------------------------------------------------------- groups

/// Turning a group turns its arrow with it: the ends stay bound to the shapes inside the
/// group, and are drawn on them — not at the angle-less positions the arrow had.
#[test]
fn turning_a_group_keeps_its_arrows_attached() {
    let a = shape("a", filled(box_at(100.0, 300.0, 100.0, 80.0)));
    let mut engine = engine_with_scene(vec![a, target()]);
    let arrow = draw_arrow(&mut engine, (205.0, 340.0), (295.0, 340.0));
    assert_eq!(arrow.start_binding.as_deref(), Some("a"), "setup");
    assert_eq!(arrow.end_binding.as_deref(), Some(B), "setup");

    engine.select(vec!["a".into(), B.into(), arrow.id.clone()]);
    // The group spans x 100..400, y 300..380: centre (250, 340). A quarter turn.
    turn_selection(&mut engine, (250.0, 300.0), (550.0, 340.0));

    let arrow = get(&engine, &arrow.id);
    assert_eq!(arrow.start_binding.as_deref(), Some("a"));
    assert_eq!(arrow.end_binding.as_deref(), Some(B));
    assert_close(arrow.angle, 0.0);
    let (tail, tip) = ends(&arrow);
    // A now sits above B: the arrow runs straight down between them.
    assert!(
        (tail.x - 250.0).abs() < 1e-6 && (tip.x - 250.0).abs() < 1e-6,
        "{tail:?} {tip:?}"
    );
    assert!(tail.y < tip.y, "still from A to B");
    assert!(!is_inside(&get(&engine, "a"), tail) && !is_inside(&get(&engine, B), tip));
}

/// A group scaled through its corner scales a leftward line through its points: it stays
/// where the scale puts it rather than jumping a whole width.
#[test]
fn scaling_a_group_scales_a_leftward_line_in_place() {
    let mut line = connector(200.0, 100.0, 0.0, 100.0, DrawElementType::Line);
    line.id = "line".into();
    let b = shape(B, box_at(0.0, 0.0, 200.0, 200.0));
    let frame = selection::group_transform::GroupFrame::capture([&line, &b]).unwrap();
    let out = selection::group_transform::resize_group(
        &[line, b],
        &frame,
        HandleKind::Se,
        Point { x: 400.0, y: 400.0 },
        false,
        false,
    );
    let line = out.iter().find(|el| el.id == "line").unwrap();
    let points = world_points(line);
    assert_point_close(points[0], Point { x: 400.0, y: 200.0 });
    assert_point_close(points[1], Point { x: 0.0, y: 200.0 });
}

// ------------------------------------------------------------------ documents

/// A duplicate of an arrow with its shapes keeps its anchors; one without them lets go.
#[test]
fn a_copy_keeps_its_anchors_only_with_its_shapes() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (380.0, 100.0), (380.0, 295.0));
    let original = anchor(&arrow, End::End).unwrap();

    engine.select(vec![arrow.id.clone(), B.into()]);
    engine.duplicate_selection(0.0, 500.0);
    let copies = engine.get_scene();
    let copy = copies
        .iter()
        .find(|el| el.kind == DrawElementType::Arrow && el.id != arrow.id)
        .unwrap();
    let copied = anchor(copy, End::End).unwrap();
    assert_ne!(copied.element_id, B, "points at the copied shape");
    assert_eq!(
        (copied.fixed_point, copied.mode),
        (original.fixed_point, original.mode)
    );

    let arrows = |engine: &DrawEngine| -> Vec<String> {
        engine
            .get_scene()
            .into_iter()
            .filter(|el| el.kind == DrawElementType::Arrow)
            .map(|el| el.id)
            .collect()
    };
    let before = arrows(&engine);
    engine.select(vec![arrow.id.clone()]);
    engine.duplicate_selection(0.0, 500.0);
    let fresh = arrows(&engine)
        .into_iter()
        .find(|id| !before.contains(id))
        .expect("a copy");
    let alone = get(&engine, &fresh);
    assert_eq!(alone.end_binding, None);
    assert_eq!((alone.end_fixed_point, alone.end_bind_mode), (None, None));
}

/// Anchors survive the file format, and a document without them — every board saved
/// before they existed — reads as the centre, in orbit.
#[test]
fn anchors_round_trip_and_old_documents_read_as_before() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (380.0, 100.0), (380.0, 295.0));
    let json = scene_to_json(&engine.get_scene());
    assert!(json.contains("endFixedPoint") && json.contains("endBindMode"));
    let back = elements_from_json(&json).unwrap();
    let again = back.iter().find(|el| el.id == arrow.id).unwrap();
    assert_eq!(anchor(again, End::End), anchor(&arrow, End::End));

    let mut legacy = arrow.clone();
    legacy.end_fixed_point = None;
    legacy.end_bind_mode = None;
    let a = anchor(&legacy, End::End).unwrap();
    assert_eq!((a.fixed_point, a.mode), ([0.5, 0.5], BindMode::Orbit));
}

/// A document is data from anywhere: a non-finite or absurd anchor is made safe rather
/// than putting the end nowhere.
#[test]
fn a_hostile_anchor_is_made_safe() {
    let mut arrow = arrow_into_b([f64::NAN, 1e12], BindMode::Orbit);
    arrow.end_fixed_point = Some([f64::NAN, 1e12]);
    let a = anchor(&arrow, End::End).unwrap();
    assert_eq!(a.fixed_point, [0.5, 10.0]);
    let scene = refresh_bindings(&[target(), arrow]);
    let (_, tip) = ends(scene.iter().find(|el| el.id == "arrow").unwrap());
    assert!(tip.x.is_finite() && tip.y.is_finite());
}

// ------------------------------------------------------------------- frames

/// A frame — a slide, in a presentation — is bound from outside, near its border. Inside
/// it, an arrow is aimed at what the frame holds, never at the frame
/// (`collision.ts@1118751f:301-334`).
#[test]
fn a_frame_is_bound_from_outside_only() {
    let slide = shape(
        "slide",
        create_element_default(
            DrawElementType::Frame,
            Geometry {
                x: 300.0,
                y: 300.0,
                width: 300.0,
                height: 200.0,
            },
        ),
    );
    let mut engine = engine_with_scene(vec![slide.clone()]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (295.0, 420.0));
    let a = anchor(&arrow, End::End).expect("bound to the frame from outside");
    assert_eq!((a.element_id.as_str(), a.mode), ("slide", BindMode::Orbit));

    let mut engine = engine_with_scene(vec![slide]);
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (450.0, 420.0));
    assert_eq!(
        arrow.end_binding, None,
        "inside the frame, nothing to bind to"
    );
}

/// A shape inside a frame is clipped by it, and cannot be bound where it is not shown.
#[test]
fn a_shape_is_not_bound_where_its_frame_clips_it() {
    let slide = shape(
        "slide",
        create_element_default(
            DrawElementType::Frame,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 350.0,
                height: 600.0,
            },
        ),
    );
    let mut b = target();
    b.frame_id = Some("slide".into());
    let mut engine = engine_with_scene(vec![slide, b]);
    // B spans x 300..400; the frame clips it at 350.
    let arrow = draw_arrow(&mut engine, (700.0, 100.0), (380.0, 340.0));
    assert_ne!(
        arrow.end_binding.as_deref(),
        Some(B),
        "B is not drawn there"
    );
    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (320.0, 340.0));
    assert_eq!(
        arrow.end_binding.as_deref(),
        Some(B),
        "where B is shown, it binds"
    );
}

// ------------------------------------------------------------------- found in review

fn length(arrow: &DrawElement) -> f64 {
    let (a, b) = ends(arrow);
    (b.x - a.x).hypot(b.y - a.y)
}

/// Pressed and let go beside two shapes, off their midpoints, each anchor is where the
/// end was put — outside its shape. Read as trapped inside, the two collapsed the arrow
/// to nothing, the release took it for a click, and Escape then lost it. (Ten units
/// beside them: inside Excalidraw's reach of 15.)
#[test]
fn an_arrow_drawn_beside_two_shapes_keeps_its_length() {
    let a = shape("a", filled(box_at(0.0, 0.0, 100.0, 80.0)));
    let b = shape(B, filled(box_at(0.0, 300.0, 100.0, 80.0)));
    let mut engine = engine_with_scene(vec![a, b]);

    let arrow = draw_arrow(&mut engine, (-10.0, 20.0), (-10.0, 320.0));

    assert_eq!(engine.linear_in_progress(), None, "a drag, not a click");
    assert_eq!(arrow.start_binding.as_deref(), Some("a"), "setup");
    assert_eq!(arrow.end_binding.as_deref(), Some(B), "setup");
    assert!((length(&arrow) - 300.0).abs() < 1e-6, "{}", length(&arrow));
}

/// Click or drag is the hand's travel, not the arrow's: bound between two shapes a few
/// pixels apart, a 240px drag left a 16px arrow and was taken for a click.
#[test]
fn a_drag_between_close_shapes_is_a_drag() {
    let a = shape("a", filled(box_at(0.0, 0.0, 100.0, 80.0)));
    let b = shape(B, filled(box_at(120.0, 0.0, 100.0, 80.0)));
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale: 2.0,
    });

    let arrow = draw_arrow(&mut engine, (204.0, 80.0), (444.0, 80.0));

    assert_eq!(engine.linear_in_progress(), None);
    assert_eq!(arrow.start_binding.as_deref(), Some("a"));
    assert_eq!(arrow.end_binding.as_deref(), Some(B));
}

/// Deep in a zoomed presentation a real drag is a fraction of a unit long, and was thrown
/// away as too small.
#[test]
fn a_short_drag_deep_in_a_zoom_is_kept() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale: 50.0,
    });

    let arrow = draw_arrow(&mut engine, (100.0, 100.0), (160.0, 100.0));

    assert_eq!(engine.linear_in_progress(), None);
    assert!((length(&arrow) - 1.2).abs() < 1e-9, "{}", length(&arrow));
}

/// A press inside a shape is aimed at that shape. With the walk stopping at any filled
/// shape merely near the point, a neighbour on top — or a smaller one below — took it.
#[test]
fn a_press_inside_a_shape_binds_to_it_not_its_neighbour() {
    for (a, b, press) in [
        // Equal sizes, the neighbour on top.
        (
            box_at(0.0, 0.0, 100.0, 80.0),
            box_at(120.0, 0.0, 100.0, 80.0),
            (95.0, 40.0),
        ),
        // The pressed shape smaller and underneath.
        (
            box_at(0.0, 0.0, 60.0, 50.0),
            box_at(80.0, 0.0, 100.0, 80.0),
            (55.0, 25.0),
        ),
    ] {
        let mut engine = engine_with_scene(vec![shape("a", filled(a)), shape(B, filled(b))]);
        let arrow = draw_arrow(&mut engine, press, (press.0, 400.0));
        let start = anchor(&arrow, End::Start).expect("bound");
        assert_eq!(start.element_id, "a");
        assert_eq!(start.mode, BindMode::Inside);
    }
}

/// And a shape nested in the one pressed is still reached from just outside its border.
#[test]
fn a_nested_shape_is_still_reached_from_inside_its_container() {
    let outer = shape("outer", filled(box_at(0.0, 0.0, 400.0, 300.0)));
    let inner = shape(B, filled(box_at(100.0, 100.0, 100.0, 80.0)));
    let mut engine = engine_with_scene(vec![outer, inner]);

    let arrow = draw_arrow(&mut engine, (20.0, 20.0), (95.0, 140.0));

    let end = anchor(&arrow, End::End).expect("bound");
    assert_eq!(end.element_id, B);
    assert_eq!(end.mode, BindMode::Orbit);
}

/// Undo in the middle of an end drag: the drag carries on over the undone scene, and what
/// it remembered of the far end — from before the undo — must not be written back.
#[test]
fn undo_during_an_end_drag_is_not_written_back() {
    let a = shape("a", filled(box_at(0.0, 0.0, 100.0, 80.0)));
    let c = shape("c", filled(box_at(0.0, 300.0, 100.0, 80.0)));
    let mut engine = engine_with_scene(vec![a, target(), c]);
    let arrow = draw_arrow(&mut engine, (105.0, 40.0), (295.0, 340.0));
    assert_eq!(arrow.start_binding.as_deref(), Some("a"), "setup");
    engine.set_tool(DrawTool::Select);
    engine.select(vec![arrow.id.clone()]);
    let (start, _) = ends(&arrow);
    engine.begin_pointer(start.x, start.y, false, false);
    engine.move_pointer(50.0, 340.0, false, false);
    engine.end_pointer();
    assert_eq!(
        get(&engine, &arrow.id).start_binding.as_deref(),
        Some("c"),
        "setup"
    );

    let (_, tip) = ends(&get(&engine, &arrow.id));
    engine.begin_pointer(tip.x, tip.y, false, false);
    engine.move_pointer(tip.x + 5.0, tip.y + 60.0, false, false);
    engine.undo();
    assert_eq!(
        get(&engine, &arrow.id).start_binding.as_deref(),
        Some("a"),
        "setup"
    );
    engine.move_pointer(tip.x + 10.0, tip.y + 70.0, false, false);
    engine.end_pointer();

    assert_eq!(get(&engine, &arrow.id).start_binding.as_deref(), Some("a"));
}

/// The dot is filled in only where a drop would snap to it. Held to an angle, the end
/// does not snap, and a dot claiming it would is a promise the drop breaks.
#[test]
fn the_midpoint_dot_promises_only_the_snap_a_drop_makes() {
    let mut engine = engine_with_scene(vec![target()]);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(200.0, 230.0, false, false);
    engine.move_pointer(250.0, 280.0, true, false);
    engine.move_pointer(297.0, 327.0, true, false);

    let (_, snaps) = engine.paint_view().binding_midpoint.expect("a dot");
    assert!(!snaps, "angle-locked: no snap");
    engine.end_pointer();
}

/// Grid snapping holds the end to the grid, so the midpoint snap is off
/// (`binding.ts@1118751f:885-887`): pulled onto B's midpoint, the arrow bent off its row.
#[test]
fn on_the_grid_an_end_is_not_pulled_onto_a_midpoint() {
    let b = shape(B, filled(box_at(300.0, 300.0, 100.0, 90.0)));
    let mut engine = engine_with_scene(vec![b]);
    engine.set_grid(GridSettings {
        enabled: true,
        size: 10.0,
        step: 5,
        snap: true,
    });

    let arrow = draw_arrow(&mut engine, (100.0, 340.0), (291.0, 341.0));

    assert_eq!(arrow.end_binding.as_deref(), Some(B), "setup");
    let (start, tip) = ends(&arrow);
    assert_close(start.y, 340.0);
    assert_close(tip.y, 340.0);
}

/// Escape ends an end drag, and the suggestion it showed goes with it.
#[test]
fn escape_puts_the_suggestion_out() {
    let mut engine = engine_with_scene(vec![target()]);
    let arrow = draw_arrow(&mut engine, (100.0, 340.0), (200.0, 340.0));
    engine.set_tool(DrawTool::Select);
    engine.select(vec![arrow.id.clone()]);
    engine.begin_pointer(200.0, 340.0, false, false);
    engine.move_pointer(250.0, 340.0, false, false);
    engine.move_pointer(296.0, 340.0, false, false);
    assert!(engine.paint_view().binding_highlight.is_some(), "setup");

    engine.cancel_pointer();

    let view = engine.paint_view();
    assert!(view.binding_highlight.is_none());
    assert!(view.binding_midpoint.is_none());
}

/// A shape undone from under the hovering pointer takes its suggestion with it.
#[test]
fn a_deleted_shape_is_not_suggested() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(300.0, 300.0, false, false);
    engine.move_pointer(400.0, 380.0, false, false);
    engine.end_pointer();
    engine.set_tool(DrawTool::Arrow);
    engine.hover_pointer(296.0, 340.0);
    assert!(engine.paint_view().binding_highlight.is_some(), "setup");

    engine.undo();

    let view = engine.paint_view();
    assert!(view.binding_highlight.is_none());
    assert!(view.binding_midpoint.is_none());
}
