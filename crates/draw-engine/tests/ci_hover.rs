//! What the cursor promises.
//!
//! The cursor is the only affordance a canvas has: every pixel looks the same, so the
//! only way to say "this resizes" rather than "this moves" is to change the pointer. That
//! makes the cursor part of the hit-test contract, not decoration — if it disagrees with
//! what a press actually does, it is worse than having no cursor at all.

mod common;
use common::*;
use draw_engine::*;

fn engine_with(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(1200.0, 800.0, 1.0);
    engine.set_scene(Scene::new(elements));
    engine
}

/// Screen coordinates equal world coordinates at the identity camera, so the probes below
/// read directly.
fn probe() -> DrawEngine {
    let mut element = box_at(400.0, 300.0, 300.0, 200.0);
    element.id = "r1".into();
    let mut engine = engine_with(vec![element]);
    engine.select(vec!["r1".to_string()]);
    engine
}

#[test]
fn empty_canvas_says_nothing() {
    let engine = probe();
    assert_eq!(engine.hover_cursor(50.0, 50.0), HoverCursor::Default);
}

/// The element's own body and outline move it. This is the cursor half of the bug where
/// grabbing a shape's border resized it.
#[test]
fn the_element_itself_reads_as_movable() {
    let engine = probe();
    for (x, y) in [
        (550.0, 400.0), // interior
        (550.0, 300.0), // top edge
        (400.0, 400.0), // left edge
        (400.0, 300.0), // corner
    ] {
        assert_eq!(
            engine.hover_cursor(x, y),
            HoverCursor::Move,
            "({x}, {y}) is part of the element and must read as movable"
        );
    }
}

/// Handles sit on the ring 8px outside, and each says which way it scales.
#[test]
fn each_handle_names_its_own_axis() {
    let engine = probe();
    for (x, y, expected) in [
        (550.0, 292.0, HoverCursor::ResizeNs),   // N
        (550.0, 508.0, HoverCursor::ResizeNs),   // S
        (392.0, 400.0, HoverCursor::ResizeEw),   // W
        (708.0, 400.0, HoverCursor::ResizeEw),   // E
        (392.0, 292.0, HoverCursor::ResizeNwse), // NW
        (708.0, 508.0, HoverCursor::ResizeNwse), // SE
        (708.0, 292.0, HoverCursor::ResizeNesw), // NE
        (392.0, 508.0, HoverCursor::ResizeNesw), // SW
    ] {
        assert_eq!(engine.hover_cursor(x, y), expected, "handle at ({x}, {y})");
    }
}

#[test]
fn the_rotation_handle_offers_a_grab() {
    let engine = probe();
    // Frame top is 292; the rotation handle sits ROTATE_GAP above it.
    assert_eq!(engine.hover_cursor(550.0, 266.0), HoverCursor::Grab);
}

/// Excalidraw turns the cursor with the element to the nearest 45 degrees. A shape on its
/// side scales left-right through the handle that was up-down, and the cursor has to say
/// so — otherwise every handle on a rotated shape lies about its direction.
#[test]
fn cursors_turn_with_the_element() {
    let mut element = box_at(400.0, 300.0, 300.0, 200.0);
    element.id = "r1".into();
    element.angle = std::f64::consts::FRAC_PI_2;
    let mut engine = engine_with(vec![element]);
    engine.select(vec!["r1".to_string()]);

    // Centre (550, 400). A quarter turn puts the north handle on the right and the west
    // handle on top.
    assert_eq!(engine.hover_cursor(658.0, 400.0), HoverCursor::ResizeEw);
    assert_eq!(engine.hover_cursor(550.0, 242.0), HoverCursor::ResizeNs);
}

/// Half a turn is two steps through a four-cursor cycle, so it lands back where it
/// started — the diagonal through a corner is unchanged by flipping the shape.
#[test]
fn a_half_turn_returns_the_same_cursors() {
    let plain = probe();
    let upright = plain.hover_cursor(550.0, 292.0);

    let mut element = box_at(400.0, 300.0, 300.0, 200.0);
    element.id = "r1".into();
    element.angle = std::f64::consts::PI;
    let mut engine = engine_with(vec![element]);
    engine.select(vec!["r1".to_string()]);
    assert_eq!(engine.hover_cursor(550.0, 508.0), upright);
}

/// A negative angle must wrap forwards, not index off the end of the cursor table.
/// Excalidraw's own version uses a plain JS remainder here and yields `undefined`.
#[test]
fn a_counter_clockwise_angle_still_names_a_cursor() {
    let mut element = box_at(400.0, 300.0, 300.0, 200.0);
    element.id = "r1".into();
    element.angle = -std::f64::consts::FRAC_PI_2;
    let mut engine = engine_with(vec![element]);
    engine.select(vec!["r1".to_string()]);

    // A quarter turn the other way: the north handle is now on the left.
    assert_eq!(engine.hover_cursor(442.0, 400.0), HoverCursor::ResizeEw);
}

/// A line or arrow is edited by its points, so its handles read as targets to grab, not
/// as box corners to scale.
#[test]
fn a_linear_element_offers_point_handles() {
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 200.0,
            y: 200.0,
            width: 300.0,
            height: 0.0,
        },
    );
    arrow.id = "a1".into();
    arrow.points = Some(vec![[0.0, 0.0], [300.0, 0.0]]);
    let mut engine = engine_with(vec![arrow]);
    engine.select(vec!["a1".to_string()]);

    assert_eq!(engine.hover_cursor(200.0, 200.0), HoverCursor::PointHandle);
    assert_eq!(engine.hover_cursor(350.0, 200.0), HoverCursor::PointHandle);
    assert_eq!(engine.hover_cursor(350.0, 280.0), HoverCursor::Default);
}

/// A locked element cannot be dragged, so promising a move would be a lie.
#[test]
fn a_locked_element_does_not_offer_a_move() {
    let mut element = box_at(400.0, 300.0, 300.0, 200.0);
    element.id = "r1".into();
    element.locked = Some(true);
    let engine = engine_with(vec![element]);
    assert_eq!(engine.hover_cursor(550.0, 400.0), HoverCursor::Default);
}

/// A drawing tool is about to create something, wherever the pointer is.
#[test]
fn a_drawing_tool_overrides_whatever_is_underneath() {
    let mut engine = probe();
    for tool in [
        DrawTool::Rectangle,
        DrawTool::Ellipse,
        DrawTool::Diamond,
        DrawTool::Arrow,
        DrawTool::Line,
        DrawTool::Freedraw,
        DrawTool::Eraser,
    ] {
        engine.set_tool(tool);
        assert_eq!(
            engine.hover_cursor(550.0, 400.0),
            HoverCursor::Crosshair,
            "{tool:?} should read as a drawing tool even over an element"
        );
    }
    engine.set_tool(DrawTool::Text);
    assert_eq!(engine.hover_cursor(550.0, 400.0), HoverCursor::Text);
    engine.set_tool(DrawTool::Hand);
    assert_eq!(engine.hover_cursor(550.0, 400.0), HoverCursor::Grab);
}

/// While a drag is running the cursor must not flicker as the shape passes over other
/// things. The interaction outranks whatever is under the pointer.
#[test]
fn an_active_drag_holds_its_cursor() {
    let mut moving = probe();
    moving.begin_pointer(550.0, 400.0, false, false);
    moving.move_pointer(600.0, 450.0, false, false);
    assert_eq!(moving.hover_cursor(50.0, 50.0), HoverCursor::Grabbing);
    moving.end_pointer();

    // A fresh engine: the drag above left the element 50px along, which would put its
    // south-east handle somewhere other than where this grabs.
    let mut resizing = probe();
    resizing.begin_pointer(708.0, 508.0, false, false);
    resizing.move_pointer(750.0, 550.0, false, false);
    assert_eq!(
        resizing.hover_cursor(50.0, 50.0),
        HoverCursor::ResizeNwse,
        "a resize in progress keeps naming its own direction"
    );
    resizing.end_pointer();
}

/// The codes are the contract with the host's cursor table, which indexes into an array.
/// Renumbering silently reassigns every cursor after the change.
#[test]
fn the_codes_are_stable() {
    assert_eq!(HoverCursor::Default.code(), 0);
    assert_eq!(HoverCursor::Move.code(), 1);
    assert_eq!(HoverCursor::ResizeNs.code(), 2);
    assert_eq!(HoverCursor::ResizeEw.code(), 3);
    assert_eq!(HoverCursor::ResizeNesw.code(), 4);
    assert_eq!(HoverCursor::ResizeNwse.code(), 5);
    assert_eq!(HoverCursor::Grab.code(), 6);
    assert_eq!(HoverCursor::Grabbing.code(), 7);
    assert_eq!(HoverCursor::PointHandle.code(), 8);
    assert_eq!(HoverCursor::Crosshair.code(), 9);
    assert_eq!(HoverCursor::Text.code(), 10);
}
