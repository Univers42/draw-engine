//! Selecting more than one thing.
//!
//! Every rule here is Excalidraw's, and they are worth holding down together because
//! they interlock: each one is the escape hatch for another. A marquee that only ever
//! replaced the selection would make building one up impossible, so shift extends it; a
//! shift-click that only ever added would make correcting a mis-click impossible, so
//! shift-click on something already held takes it out again; and a click inside the
//! selection that always moved it would mean the selection could never be let go of, so
//! a click *outside* clears.
//!
//! Get one of those wrong and the others stop being reachable, which is why this is a
//! file rather than a handful of cases scattered through `ci_selection.rs`.

mod common;
use common::*;
use draw_engine::*;

/// Three boxes in a row, well apart, each grabbable anywhere inside it.
///
/// Filled on purpose. A transparent shape is hit on its outline only, which is correct
/// and is tested in `ci_grab_selected.rs` — but it would make every case here about the
/// fill rule rather than about the selection.
fn three_boxes() -> (DrawEngine, String, String, String) {
    let a = filled(box_at(50.0, 50.0, 80.0, 60.0));
    let b = filled(box_at(200.0, 50.0, 80.0, 60.0));
    let c = filled(box_at(350.0, 50.0, 80.0, 60.0));
    let (ida, idb, idc) = (a.id.clone(), b.id.clone(), c.id.clone());
    let mut engine = engine_with_scene(vec![a, b, c]);
    engine.set_tool(DrawTool::Select);
    (engine, ida, idb, idc)
}

/// The middle of the box at `index`, which is where you would aim to grab it.
fn middle_of(index: usize) -> (f64, f64) {
    (90.0 + 150.0 * index as f64, 80.0)
}

fn click_at(engine: &mut DrawEngine, x: f64, y: f64, additive: bool) {
    engine.begin_pointer(x, y, additive, false);
    engine.end_pointer();
}

fn click_box(engine: &mut DrawEngine, index: usize, additive: bool) {
    let (x, y) = middle_of(index);
    click_at(engine, x, y, additive);
}

/// A rubber band from one corner to another, in steps so it reads as a drag.
fn marquee(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64), additive: bool) {
    engine.begin_pointer(from.0, from.1, additive, false);
    for step in 1..=4 {
        let t = step as f64 / 4.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
}

fn selection(engine: &DrawEngine) -> std::collections::HashSet<String> {
    engine.get_selection().into_iter().collect()
}

fn holds(engine: &DrawEngine, ids: &[&String]) -> bool {
    let actual = selection(engine);
    actual.len() == ids.len() && ids.iter().all(|id| actual.contains(*id))
}

// ---------------------------------------------------------------------------
// The rubber band
// ---------------------------------------------------------------------------

#[test]
fn a_marquee_takes_everything_it_encloses() {
    let (mut engine, a, b, _c) = three_boxes();
    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);
    assert!(holds(&engine, &[&a, &b]), "got {:?}", selection(&engine));
}

/// **Containment, not overlap** — Excalidraw's `getElementsWithinSelection`. On overlap a
/// small drag across a big background shape picks the background up too, so a marquee
/// meant to gather a few small things quietly grabs the frame around them.
#[test]
fn a_marquee_that_merely_clips_a_shape_leaves_it() {
    let (mut engine, _a, _b, _c) = three_boxes();
    // Cuts through the first box without containing it.
    marquee(&mut engine, (20.0, 20.0), (90.0, 140.0), false);
    assert!(
        selection(&engine).is_empty(),
        "got {:?}",
        selection(&engine)
    );
}

#[test]
fn a_marquee_replaces_what_was_held() {
    let (mut engine, a, b, c) = three_boxes();
    click_box(&mut engine, 2, false);
    assert!(holds(&engine, &[&c]));

    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);
    assert!(holds(&engine, &[&a, &b]), "got {:?}", selection(&engine));
}

/// Shift makes the band add instead. Without it a selection can only ever be built by
/// one rectangle, so two things at opposite corners of a board cannot be held together.
#[test]
fn a_marquee_with_shift_adds_to_what_was_held() {
    let (mut engine, a, b, c) = three_boxes();
    click_box(&mut engine, 2, false);

    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), true);
    assert!(
        holds(&engine, &[&a, &b, &c]),
        "got {:?}",
        selection(&engine)
    );
}

// ---------------------------------------------------------------------------
// Shift-clicking
// ---------------------------------------------------------------------------

#[test]
fn shift_click_adds_one_at_a_time() {
    let (mut engine, a, b, c) = three_boxes();
    click_box(&mut engine, 0, false);
    click_box(&mut engine, 1, true);
    click_box(&mut engine, 2, true);
    assert!(
        holds(&engine, &[&a, &b, &c]),
        "got {:?}",
        selection(&engine)
    );
}

/// The correction. A shift-click that only ever added would make a mis-click
/// unrecoverable except by starting the whole selection again.
#[test]
fn shift_click_takes_out_what_is_already_held() {
    let (mut engine, a, _b, c) = three_boxes();
    click_box(&mut engine, 0, false);
    click_box(&mut engine, 1, true);
    click_box(&mut engine, 2, true);

    click_box(&mut engine, 1, true);
    assert!(holds(&engine, &[&a, &c]), "got {:?}", selection(&engine));
}

/// The take-out happens on the release, and only if the press stayed a click: the oracle
/// changes nothing on a shift-press over something already held (`App.tsx@1118751f:9664-9668`)
/// and removes it on a release with no drag (`:12183-12260`). Taken out on the press, a
/// shift-drag moved the rest of the selection and left the grabbed member behind.
#[test]
fn a_shift_drag_on_a_held_member_moves_everything() {
    let (mut engine, a, b, _c) = three_boxes();
    click_box(&mut engine, 0, false);
    click_box(&mut engine, 1, true);

    let (x, y) = middle_of(0);
    engine.begin_pointer(x, y, true, false);
    assert!(
        holds(&engine, &[&a, &b]),
        "the press took something out: {:?}",
        selection(&engine)
    );
    for step in 1..=4 {
        engine.move_pointer(x + 10.0 * f64::from(step), y, true, false);
    }
    engine.end_pointer();

    assert!(holds(&engine, &[&a, &b]), "got {:?}", selection(&engine));
    let scene = engine.get_scene();
    assert_close(scene.iter().find(|el| el.id == a).unwrap().x, 90.0);
    assert_close(scene.iter().find(|el| el.id == b).unwrap().x, 240.0);
}

/// A shift-press on a held member that is let go without moving still takes it out.
#[test]
fn a_shift_click_on_a_held_member_takes_it_out_on_release() {
    let (mut engine, _a, b, _c) = three_boxes();
    click_box(&mut engine, 0, false);
    click_box(&mut engine, 1, true);

    let (x, y) = middle_of(0);
    engine.begin_pointer(x, y, true, false);
    assert_eq!(selection(&engine).len(), 2, "not on the press");
    engine.end_pointer();

    assert!(holds(&engine, &[&b]), "got {:?}", selection(&engine));
}

/// Shift on empty canvas is not a clear. It is the start of a band that adds, and a band
/// that was never dragged has to leave the selection exactly as it found it.
#[test]
fn shift_click_on_empty_canvas_keeps_the_selection() {
    let (mut engine, a, b, _c) = three_boxes();
    click_box(&mut engine, 0, false);
    click_box(&mut engine, 1, true);

    click_at(&mut engine, 600.0, 400.0, true);
    assert!(holds(&engine, &[&a, &b]), "got {:?}", selection(&engine));
}

/// And a plain click on empty canvas *is* a clear — the way out of a selection.
#[test]
fn a_plain_click_on_empty_canvas_lets_go() {
    let (mut engine, _a, _b, _c) = three_boxes();
    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);
    assert_eq!(engine.get_selection().len(), 2);

    click_at(&mut engine, 600.0, 400.0, false);
    assert!(selection(&engine).is_empty());
}

// ---------------------------------------------------------------------------
// Moving what is held
// ---------------------------------------------------------------------------

/// The point of holding several things at once. Grabbing any one member has to carry the
/// rest, or a multi-selection is only useful for deleting.
#[test]
fn dragging_one_member_carries_the_others() {
    let (mut engine, a, b, _c) = three_boxes();
    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);

    // Snapping to objects off, which is the default. Alignment guides pull a moving
    // selection onto the edges and centres of what is *not* moving, and the third box is
    // close enough that a 25px nudge lands on its bottom edge instead — correct behaviour,
    // and nothing to do with whether the other member came along.
    let (x, y) = middle_of(0);
    engine.begin_pointer(x, y, false, false);
    engine.move_pointer(x + 40.0, y + 25.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    let moved_a = scene.iter().find(|el| el.id == a).unwrap();
    let moved_b = scene.iter().find(|el| el.id == b).unwrap();
    assert_close(moved_a.x, 90.0);
    assert_close(moved_a.y, 75.0);
    assert_close(moved_b.x, 240.0);
    assert_close(moved_b.y, 75.0);
}

/// Grabbing a member must not narrow the selection to it. This is the rule that makes
/// "select three, drag one, all three move" work at all, and it is easy to lose: the
/// obvious reading of a click is *select what was clicked*.
///
/// That reading is right once the press is let go without moving anything — then it was
/// a click, and it narrows (`App.tsx@1118751f:12191-12198`, `:12330-12353`). This used to assert
/// the release kept everything too, which is not the oracle's click: it left no way to
/// narrow a multi-selection short of clicking off it first.
#[test]
fn pressing_a_member_keeps_the_rest_until_a_click_narrows() {
    let (mut engine, a, b, _c) = three_boxes();
    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);

    let (x, y) = middle_of(0);
    engine.begin_pointer(x, y, false, false);
    assert!(holds(&engine, &[&a, &b]), "got {:?}", selection(&engine));

    engine.end_pointer();
    assert!(holds(&engine, &[&a]), "got {:?}", selection(&engine));
}

/// Clicking a *non*-member with no modifier does narrow it, which is the other half.
#[test]
fn clicking_outside_the_selection_replaces_it() {
    let (mut engine, _a, _b, c) = three_boxes();
    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);

    click_box(&mut engine, 2, false);
    assert!(holds(&engine, &[&c]), "got {:?}", selection(&engine));
}

// ---------------------------------------------------------------------------
// The group frame
// ---------------------------------------------------------------------------

/// Two or more elements select to one shared frame with corner handles, not to three
/// separate boxes. Without it a multi-selection could only be moved: dragging its corner
/// fell through to the hit test and started a marquee instead.
#[test]
fn a_multi_selection_can_be_scaled_by_its_frame() {
    let (mut engine, a, b, _c) = three_boxes();
    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);
    let before = engine.get_scene();
    let width_before: f64 = before.iter().find(|el| el.id == a).unwrap().width;

    // The frame's south-east handle. Its *centre* sits `handle_offset` outside the
    // union's corner — 4px of frame margin plus half an 8px handle — and its reach is
    // half the handle's diagonal, 5.66px. Aiming at the corner itself is 11.3px away,
    // which misses, falls through to the hit test and moves the group instead: a drag
    // that looks like a resize and changes no widths at all.
    let corner = 280.0 + 8.0;
    engine.begin_pointer(corner, 110.0 + 8.0, false, false);
    engine.move_pointer(560.0 + 8.0, 220.0 + 8.0, false, false);
    engine.end_pointer();

    let after = engine.get_scene();
    let width_after = after.iter().find(|el| el.id == a).unwrap().width;
    assert!(
        width_after > width_before * 1.5,
        "the group should have grown: {width_before} -> {width_after}"
    );
    assert!(
        holds(&engine, &[&a, &b]),
        "and should still be held afterwards"
    );
}

// ---------------------------------------------------------------------------
// The lasso
// ---------------------------------------------------------------------------

/// The other multi-select gesture: a free-form loop, for a cluster no rectangle fits.
/// `ci_lasso.rs` pins the geometry; this is about the tool reaching it.
#[test]
fn the_lasso_takes_what_its_loop_encloses() {
    let (mut engine, a, _b, _c) = three_boxes();
    engine.set_tool(DrawTool::Lasso);

    engine.begin_pointer(20.0, 20.0, false, false);
    for &(x, y) in &[(160.0, 20.0), (160.0, 140.0), (20.0, 140.0), (20.0, 20.0)] {
        engine.move_pointer(x, y, false, false);
    }
    engine.end_pointer();

    assert!(holds(&engine, &[&a]), "got {:?}", selection(&engine));
}

// ---------------------------------------------------------------------------
// Acting on the whole of it
// ---------------------------------------------------------------------------

#[test]
fn select_all_takes_the_board() {
    let (mut engine, a, b, c) = three_boxes();
    engine.select_all();
    assert!(holds(&engine, &[&a, &b, &c]));
}

#[test]
fn deleting_a_multi_selection_takes_all_of_it() {
    let (mut engine, _a, _b, c) = three_boxes();
    marquee(&mut engine, (20.0, 20.0), (300.0, 140.0), false);
    engine.delete_selection();

    // Tombstones are kept in the scene — a deletion has to be *sent* to the other
    // editors, so it is a live element with `is_deleted` rather than an absence.
    let left: Vec<String> = engine
        .get_scene()
        .into_iter()
        .filter(|el| !el.is_deleted)
        .map(|el| el.id)
        .collect();
    assert_eq!(left, vec![c], "only the box outside the band should remain");
}
