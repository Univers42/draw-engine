//! Every gesture that changes the picture asks for a frame.
//!
//! The frame loop only paints when the engine says it is dirty. So a gesture can update
//! its state perfectly and still show nothing — the state is right, and nobody asked for
//! the pixels to catch up. Nothing about that is visible from the state, which is exactly
//! why it survives: every selection test passes, because they all read the state.
//!
//! Found through `editor-inspector`: mid-marquee, `get_pointer_state` reported
//! `kind: "marquee"`, and `region_ink` over the inside of the rectangle read **0**. The
//! `Marquee` arm updated `current` and never called `request_draw`; the `Lasso` arm right
//! above it did. So the lasso drew its trail and the rubber band drew nothing at all.
//!
//! One test per gesture rather than one for the marquee, because this is a class of bug
//! and the next arm someone adds to `advance_interaction` can have it too.

mod common;
use common::*;
use draw_engine::*;

/// Clears the dirty flag, runs `step`, and says whether it asked for a frame.
///
/// `take_dirty` rather than `needs_frame`: the latter also reports motion easing, which
/// would pass a gesture that forgot to ask on the strength of an earlier one.
fn asked_for_a_frame(engine: &mut DrawEngine, step: impl FnOnce(&mut DrawEngine)) -> bool {
    engine.take_dirty();
    step(engine);
    engine.take_dirty()
}

fn board() -> (DrawEngine, String) {
    let shape = filled(box_at(100.0, 100.0, 120.0, 80.0));
    let id = shape.id.clone();
    let mut engine = engine_with_scene(vec![shape]);
    engine.set_tool(DrawTool::Select);
    (engine, id)
}

// ---------------------------------------------------------------------------
// The bug
// ---------------------------------------------------------------------------

#[test]
fn dragging_a_marquee_asks_for_a_frame() {
    let (mut engine, _) = board();
    engine.begin_pointer(400.0, 300.0, false, false);

    assert!(
        asked_for_a_frame(&mut engine, |e| e.move_pointer(500.0, 380.0, false, false)),
        "the rubber band grew and nothing asked for it to be painted"
    );
}

/// And the frame, once painted, shows the rectangle where the pointer now is — not where
/// it was pressed. Asking for a frame is only half of it.
#[test]
fn the_marquee_in_the_paint_view_follows_the_pointer() {
    let (mut engine, _) = board();
    engine.begin_pointer(400.0, 300.0, false, false);
    engine.move_pointer(500.0, 380.0, false, false);

    let rect = engine
        .paint_view()
        .marquee
        .expect("a marquee is in progress");
    assert_close(rect.min_x, 400.0);
    assert_close(rect.min_y, 300.0);
    assert_close(rect.max_x, 500.0);
    assert_close(rect.max_y, 380.0);
}

#[test]
fn releasing_a_marquee_asks_for_a_frame_to_clear_it() {
    let (mut engine, _) = board();
    engine.begin_pointer(400.0, 300.0, false, false);
    engine.move_pointer(500.0, 380.0, false, false);

    assert!(
        asked_for_a_frame(&mut engine, DrawEngine::end_pointer),
        "the rectangle would stay on screen after the gesture ended"
    );
    assert!(engine.paint_view().marquee.is_none());
}

// ---------------------------------------------------------------------------
// The rest of the class
// ---------------------------------------------------------------------------

/// The control — the gesture that already did it, and the one it is compared against.
#[test]
fn dragging_a_lasso_asks_for_a_frame() {
    let (mut engine, _) = board();
    engine.set_tool(DrawTool::Lasso);
    engine.begin_pointer(400.0, 300.0, false, false);

    assert!(asked_for_a_frame(&mut engine, |e| e.move_pointer(480.0, 340.0, false, false)));
}

#[test]
fn moving_a_selection_asks_for_a_frame() {
    let (mut engine, id) = board();
    engine.select(vec![id]);
    engine.begin_pointer(160.0, 140.0, false, false);

    assert!(asked_for_a_frame(&mut engine, |e| e.move_pointer(200.0, 170.0, false, true)));
}

#[test]
fn resizing_asks_for_a_frame() {
    let (mut engine, id) = board();
    engine.select(vec![id]);
    // South-east handle: `handle_offset` outside the corner at (220, 180).
    engine.begin_pointer(228.0, 188.0, false, false);

    assert!(asked_for_a_frame(&mut engine, |e| e.move_pointer(260.0, 220.0, false, true)));
}

#[test]
fn drawing_a_shape_asks_for_a_frame() {
    let (mut engine, _) = board();
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(400.0, 300.0, false, false);

    assert!(asked_for_a_frame(&mut engine, |e| e.move_pointer(480.0, 360.0, false, false)));
}

#[test]
fn drawing_freehand_asks_for_a_frame() {
    let (mut engine, _) = board();
    engine.set_tool(DrawTool::Freedraw);
    engine.begin_pointer(400.0, 300.0, false, false);

    assert!(asked_for_a_frame(&mut engine, |e| e.move_pointer(430.0, 320.0, false, false)));
}

#[test]
fn drawing_a_line_asks_for_a_frame() {
    let (mut engine, _) = board();
    engine.set_tool(DrawTool::Line);
    engine.begin_pointer(400.0, 300.0, false, false);

    assert!(asked_for_a_frame(&mut engine, |e| e.move_pointer(520.0, 360.0, false, false)));
}

#[test]
fn a_path_being_placed_asks_for_a_frame_as_the_cursor_moves() {
    let (mut engine, _) = board();
    engine.set_tool(DrawTool::Line);
    engine.begin_pointer(400.0, 300.0, false, false);
    engine.end_pointer();

    assert!(asked_for_a_frame(&mut engine, |e| e.move_pointer(520.0, 360.0, false, false)));
}

#[test]
fn panning_asks_for_a_frame() {
    let (mut engine, _) = board();
    engine.set_tool(DrawTool::Hand);
    engine.begin_pointer(400.0, 300.0, false, false);

    // `needs_frame` rather than the dirty flag: a pan is driven by camera motion, which
    // keeps the loop awake through `in_motion` rather than by marking the scene dirty.
    engine.move_pointer(440.0, 330.0, false, false);
    assert!(engine.needs_frame());
}
