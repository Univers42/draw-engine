//! The eraser: what one sweep is supposed to take with it.
//!
//! Two rules, both of which the eraser broke, and both of which make it feel like the
//! tool is fighting you rather than like it is slow:
//!
//! 1. **A sweep is a line, not a series of points.** The host coalesces pointer moves to
//!    one per animation frame, so a quick drag across the board arrives as a handful of
//!    samples tens of pixels apart. An eraser that tests only the sample points steps
//!    straight over everything in between — which reads exactly as "it jumps through
//!    elements and you have to pass again and again".
//!
//! 2. **A point can have more than one element under it.** The eraser took the topmost
//!    and stopped. A board made by holding Ctrl+D is a stack of identical shapes in one
//!    place, so erasing it took one pass per copy: three hundred passes for three hundred
//!    shapes, each of which looked like it had done nothing.

mod common;
use common::*;
use draw_engine::*;

/// Ids still on the board.
fn alive(engine: &DrawEngine) -> Vec<String> {
    engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .map(|e| e.id)
        .collect()
}

/// A row of filled shapes along y = 100, at x = 100, 300, 500, 700.
fn row_of_four() -> Vec<DrawElement> {
    (0..4)
        .map(|i| filled(box_at(100.0 + f64::from(i) * 200.0, 80.0, 60.0, 40.0)))
        .collect()
}

/// `count` identical shapes in one place — what a run of Ctrl+D leaves behind.
fn stack_of(count: usize) -> Vec<DrawElement> {
    (0..count)
        .map(|_| filled(box_at(100.0, 100.0, 200.0, 150.0)))
        .collect()
}

#[test]
fn one_sweep_takes_everything_it_crosses() {
    // The headline bug. Four shapes spread across the board, and a drag that passes
    // through all of them in three samples — which is what a quick sweep looks like once
    // the host has coalesced the moves to one per frame.
    let mut engine = engine_with_scene(row_of_four());
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(400.0, 100.0, false, false);
    engine.move_pointer(900.0, 100.0, false, false);
    engine.end_pointer();

    assert_eq!(
        alive(&engine),
        Vec::<String>::new(),
        "a sweep across all four left some behind"
    );
}

#[test]
fn a_single_long_jump_does_not_step_over_anything() {
    // The worst case: one sample at each end of the board, nothing in between. Sampling
    // the pointer positions cannot see any of these; only testing the segment can.
    let mut engine = engine_with_scene(row_of_four());
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);
    engine.end_pointer();

    assert!(alive(&engine).is_empty(), "a long jump stepped over shapes");
}

#[test]
fn one_click_takes_the_whole_stack_under_it() {
    // A board made by holding Ctrl+D. Every copy is in the same place, so "the element
    // under the cursor" is three hundred of them — and taking one per click meant three
    // hundred clicks that each looked like they had done nothing.
    let mut engine = engine_with_scene(stack_of(300));
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(200.0, 175.0, false, false);
    engine.end_pointer();

    assert!(
        alive(&engine).is_empty(),
        "{} of 300 stacked shapes survived one pass",
        alive(&engine).len()
    );
}

#[test]
fn the_eraser_leaves_what_it_did_not_touch() {
    // The counterpart, so "erase everything" cannot be the way the tests above pass.
    let mut engine = engine_with_scene(row_of_four());
    let ids = alive(&engine);
    engine.set_tool(DrawTool::Eraser);

    // Along y = 300, well below the row.
    engine.begin_pointer(0.0, 300.0, false, false);
    engine.move_pointer(1000.0, 300.0, false, false);
    engine.end_pointer();

    assert_eq!(alive(&engine), ids, "the eraser took something it missed");
}

#[test]
fn a_sweep_that_passes_between_two_shapes_takes_neither() {
    let mut engine = engine_with_scene(vec![
        filled(box_at(100.0, 100.0, 60.0, 40.0)),
        filled(box_at(100.0, 400.0, 60.0, 40.0)),
    ]);
    let ids = alive(&engine);
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 260.0, false, false);
    engine.move_pointer(500.0, 260.0, false, false);
    engine.end_pointer();

    assert_eq!(alive(&engine), ids);
}

#[test]
fn erasing_a_whole_sweep_is_one_undo() {
    // A sweep is one gesture, so it has to be one history entry. Erasing forty shapes and
    // then pressing undo forty times to get them back is not undo, it is punishment.
    let mut engine = engine_with_scene(row_of_four());
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);
    engine.end_pointer();
    assert!(alive(&engine).is_empty());

    engine.undo();

    assert_eq!(alive(&engine).len(), 4, "one sweep should be one undo");
}

#[test]
fn a_transparent_shape_is_erased_by_its_outline_not_its_middle() {
    // The same fill rule the rest of hit-testing uses. A hollow rectangle is a frame
    // around empty canvas, and sweeping through the hole should no more erase it than
    // clicking the hole should select it.
    let hollow = box_at(100.0, 100.0, 400.0, 300.0);
    let mut engine = engine_with_scene(vec![hollow]);
    engine.set_tool(DrawTool::Eraser);

    // Entirely inside the hole, touching no edge.
    engine.begin_pointer(200.0, 200.0, false, false);
    engine.move_pointer(400.0, 300.0, false, false);
    engine.end_pointer();
    assert_eq!(alive(&engine).len(), 1, "the hole is not the shape");

    // Across the left edge.
    engine.begin_pointer(50.0, 200.0, false, false);
    engine.move_pointer(200.0, 200.0, false, false);
    engine.end_pointer();
    assert!(
        alive(&engine).is_empty(),
        "crossing the outline should erase"
    );
}

#[test]
fn erasing_takes_a_bound_label_with_its_container() {
    // A label belongs to its shape. Leaving it behind as an orphan is the kind of debris
    // that only shows up later, when something tries to lay it out against a container
    // that is no longer there.
    let mut engine = engine_with_measure(vec![filled(box_at(100.0, 100.0, 200.0, 120.0))]);
    // A double-click inside a shape is how a bound label comes into being.
    engine.handle_double_click(200.0, 160.0);
    assert_eq!(engine.get_scene().len(), 2);

    engine.set_tool(DrawTool::Eraser);
    engine.begin_pointer(200.0, 160.0, false, false);
    engine.end_pointer();

    assert!(
        alive(&engine).is_empty(),
        "the label outlived its container: {:?}",
        alive(&engine)
    );
}

#[test]
fn a_sweep_erases_each_element_once() {
    // Passing back and forth over the same shape must not push it onto the history or the
    // delta more than once — the second removal is a no-op, and treating it as a change
    // republishes the scene for nothing.
    let mut engine = engine_with_scene(row_of_four());
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);
    engine.move_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);
    engine.end_pointer();
    assert!(alive(&engine).is_empty());

    engine.undo();
    assert_eq!(
        alive(&engine).len(),
        4,
        "the sweep became several undo steps"
    );
}
