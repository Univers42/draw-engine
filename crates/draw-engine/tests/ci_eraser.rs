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

// ---------------------------------------------------------------------------
// Mark, then delete on release — Excalidraw's two stages
//
// The web host used to run its own eraser over this one, and it asked for the element
// under each sample: the topmost. The top copy of a stack, once marked, went on answering
// every sample, and the copies beneath were never reached — "it doesn't delete
// everything". It also faded elements by rewriting their opacity as if a peer had sent
// it, which undo then took for their real state. The two stages live here now.
// ---------------------------------------------------------------------------

/// A sweep across a stack of copies, in several coalesced moves — the reported case.
#[test]
fn a_sweep_across_a_stack_marks_every_copy_and_release_deletes_them_all() {
    let mut engine = engine_with_scene(stack_of(50));
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(20.0, 175.0, false, false);
    engine.move_pointer(150.0, 175.0, false, false);
    engine.move_pointer(380.0, 175.0, false, false);
    assert_eq!(
        engine.marked_for_erasure().len(),
        50,
        "every copy is marked"
    );
    engine.end_pointer();

    assert!(
        alive(&engine).is_empty(),
        "{} survived",
        alive(&engine).len()
    );
}

#[test]
fn nothing_is_deleted_until_release() {
    let mut engine = engine_with_scene(row_of_four());
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);

    assert_eq!(alive(&engine).len(), 4, "marked, not deleted");
    assert_eq!(engine.marked_for_erasure().len(), 4);
    assert_eq!(engine.debug_state().interaction.marked_for_erasure.len(), 4);
    engine.end_pointer();
    assert!(alive(&engine).is_empty());
    assert!(engine.marked_for_erasure().is_empty());
}

/// Marking is not an edit: the elements are drawn faded, but the document is untouched.
/// Faded by rewriting their opacity, undo brought erased elements back see-through.
#[test]
fn marking_does_not_touch_the_document() {
    let mut engine = engine_with_scene(row_of_four());
    let before = engine.get_scene();
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);

    assert_eq!(engine.get_scene(), before);
    engine.end_pointer();
    engine.undo();
    for element in engine.get_scene() {
        assert!(!element.is_deleted);
        assert_close(element.opacity, 100.0);
    }
}

#[test]
fn escape_mid_sweep_deletes_nothing() {
    let mut engine = engine_with_scene(row_of_four());
    engine.set_tool(DrawTool::Eraser);
    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);

    engine.cancel_pointer();
    engine.end_pointer();

    assert_eq!(alive(&engine).len(), 4);
    assert!(
        engine.marked_for_erasure().is_empty(),
        "and nothing stays faded"
    );
    assert!(!engine.debug_state().scene.can_undo, "no step was recorded");
}

/// Excalidraw's restore: sweeping back over marked elements with Alt held un-marks them.
#[test]
fn sweeping_back_with_alt_unmarks() {
    let mut engine = engine_with_scene(row_of_four());
    let ids = alive(&engine);
    engine.set_tool(DrawTool::Eraser);
    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);

    // Round below the row to the gap at x = 400, touching nothing, then back along the
    // row over the first two shapes (x = 100 and 300) with Alt held.
    engine.move_pointer(1000.0, 300.0, false, false);
    engine.move_pointer(400.0, 300.0, false, false);
    engine.set_alt_held(true);
    engine.move_pointer(400.0, 100.0, false, false);
    assert_eq!(
        engine.marked_for_erasure().len(),
        4,
        "setup: the detour touched nothing"
    );
    engine.move_pointer(0.0, 100.0, false, false);
    engine.end_pointer();

    assert_eq!(
        alive(&engine),
        ids[..2].to_vec(),
        "the two swept back over stay"
    );
}

#[test]
fn erasing_one_member_takes_the_whole_group() {
    let mut a = filled(box_at(100.0, 100.0, 60.0, 40.0));
    let mut b = filled(box_at(400.0, 100.0, 60.0, 40.0));
    a.group_ids = vec!["g".into()];
    b.group_ids = vec!["g".into()];
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Eraser);

    // A click on the first only.
    engine.begin_pointer(130.0, 120.0, false, false);
    engine.end_pointer();

    assert!(alive(&engine).is_empty(), "a group is one thing");
}

/// A frame is what its contents live in; erasing it takes them, as Excalidraw's does,
/// even when the sweep never touched them.
#[test]
fn a_marked_frame_takes_what_it_holds() {
    let mut engine = engine_with_scene(vec![filled(box_at(150.0, 150.0, 60.0, 40.0))]);
    engine.set_tool(DrawTool::Frame);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(400.0, 300.0, false, false);
    engine.end_pointer();
    assert!(
        engine.get_scene().iter().any(|el| el.frame_id.is_some()),
        "setup: the box is in the frame"
    );
    engine.set_tool(DrawTool::Eraser);

    // Across the frame's right edge, well away from the box.
    engine.begin_pointer(380.0, 250.0, false, false);
    engine.move_pointer(420.0, 250.0, false, false);
    engine.end_pointer();

    assert!(
        alive(&engine).is_empty(),
        "left behind: {:?}",
        alive(&engine)
    );
}

/// An arrow that stays is let go of the shape erased from under it, as Excalidraw's
/// `eraseElements` does — and in the same step, so undo binds it again.
#[test]
fn an_arrow_is_let_go_of_an_erased_shape_and_undo_binds_it_again() {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(350.0, 0.0, 100.0, 80.0));
    let right_id = right.id.clone();
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(50.0, 40.0, false, false);
    engine.move_pointer(200.0, 40.0, false, false);
    engine.move_pointer(400.0, 40.0, false, false);
    engine.end_pointer();
    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow was drawn");
    assert_eq!(
        arrow.end_binding.as_deref(),
        Some(right_id.as_str()),
        "setup"
    );

    // A click on the right box, clear of the arrow's end.
    engine.set_tool(DrawTool::Eraser);
    engine.begin_pointer(430.0, 70.0, false, false);
    engine.end_pointer();

    let get = |engine: &DrawEngine| {
        engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == arrow.id)
            .expect("the arrow is still there")
    };
    assert!(!get(&engine).is_deleted, "the arrow was not erased");
    assert_eq!(get(&engine).end_binding, None);

    engine.undo();
    assert_eq!(get(&engine).end_binding.as_deref(), Some(right_id.as_str()));
}

#[test]
fn leaving_the_eraser_mid_sweep_lets_the_marks_go() {
    let mut engine = engine_with_scene(row_of_four());
    engine.set_tool(DrawTool::Eraser);
    engine.begin_pointer(0.0, 100.0, false, false);
    engine.move_pointer(1000.0, 100.0, false, false);

    engine.set_tool(DrawTool::Select);
    engine.end_pointer();

    assert_eq!(alive(&engine).len(), 4);
    assert!(engine.marked_for_erasure().is_empty());
}

/// A locked element is left alone, as Excalidraw's eraser leaves it.
#[test]
fn a_locked_element_is_not_marked() {
    let mut locked = filled(box_at(100.0, 80.0, 60.0, 40.0));
    locked.locked = Some(true);
    let mut engine = engine_with_scene(vec![locked]);
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(130.0, 100.0, false, false);
    assert!(engine.marked_for_erasure().is_empty());
    engine.end_pointer();

    assert_eq!(alive(&engine).len(), 1);
}
