//! The pencil stays a pencil.
//!
//! Every other drawing tool settles back to Select once its gesture is over, which is
//! right for them: a rectangle is a thing you place, and after placing one you almost
//! always want to move it. Drawing by hand is not like that. A drawn line is made of
//! many strokes — you lift the pen and put it down again constantly — and reverting to
//! Select after each one means the next stroke is a marquee instead, so you have to
//! reach for the tool again between every mark.
//!
//! Excalidraw reverts too, and offers a lock to stop it. The lock is the right escape
//! hatch for a *shape* tool; for the pencil it is the wrong default, because there is no
//! sensible reading of "draw one stroke and stop".
//!
//! The eraser is the same shape of gesture and already behaves this way, as does the
//! laser — so this makes the pencil consistent with the other two continuous tools
//! rather than inventing a rule for it.

mod common;
use common::*;
use draw_engine::*;

fn scribble(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.begin_pointer(x, y, false, false);
    engine.move_pointer(x + 20.0, y + 12.0, false, false);
    engine.move_pointer(x + 45.0, y + 30.0, false, false);
    engine.end_pointer();
}

#[test]
fn the_pencil_is_still_selected_after_a_stroke() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Freedraw);
    scribble(&mut engine, 100.0, 100.0);
    assert_eq!(engine.get_tool(), DrawTool::Freedraw);
}

/// The point of the change: a second stroke has to be a stroke, not a marquee.
#[test]
fn a_second_stroke_draws_rather_than_selecting() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Freedraw);
    scribble(&mut engine, 100.0, 100.0);
    scribble(&mut engine, 300.0, 200.0);

    let strokes = engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == DrawElementType::Freedraw)
        .count();
    assert_eq!(strokes, 2, "both gestures should have left a stroke");
}

/// A stroke too short to keep is discarded — but it must not take the tool with it, or a
/// stray tap mid-drawing drops you back into Select.
#[test]
fn a_tap_that_leaves_nothing_keeps_the_pencil() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Freedraw);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();

    assert!(engine.get_scene().is_empty(), "a tap leaves no stroke");
    assert_eq!(engine.get_tool(), DrawTool::Freedraw);
}

/// A finished stroke is not left selected either. Selecting it puts a frame and eight
/// handles over the drawing you are in the middle of making, and the next stroke starting
/// inside that frame would grab it instead of drawing.
#[test]
fn a_finished_stroke_is_not_left_selected() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Freedraw);
    scribble(&mut engine, 100.0, 100.0);
    assert!(engine.get_selection().is_empty());
}

/// Auto-shape is a different promise: it watches one stroke and turns it into a shape, so
/// the gesture ends with a shape you will want to place. It keeps settling.
#[test]
fn autoshape_still_hands_back_to_select() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::AutoShape);
    scribble(&mut engine, 100.0, 100.0);
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

/// The shape tools are untouched.
#[test]
fn a_rectangle_still_hands_back_to_select() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(200.0, 180.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_tool(), DrawTool::Select);
}
