//! A bound arrow can be picked up and moved.
//!
//! It could not. Reproduced through `editor-inspector`: an arrow bound to a box at each
//! end, selected, dragged 150px down by its shaft — and afterwards it was exactly where it
//! started, both bindings intact. `move_selection` moves the arrow and then calls
//! `apply_bindings`, which re-resolves every bound arrow's ends onto its shapes. So each
//! frame of the drag put the arrow back. From the outside that reads as "blocked".
//!
//! Excalidraw's rule, `packages/element/src/dragElements.ts:110-167`:
//!
//! - Moving an arrow **unbinds each end whose shape is not moving with it** — "otherwise
//!   we would have weird situations, like 0 length arrow when the user moves the arrow
//!   outside a filled shape".
//! - An end whose shape *is* in the drag stays bound; the whole thing moves together.
//! - A lone bound arrow has to travel `DRAGGING_THRESHOLD` (10px) first, so the click that
//!   selects it cannot unbind it by accident.

mod common;
use common::*;
use draw_engine::*;

/// Two filled boxes with an arrow drawn between them, bound at both ends.
///
/// The arrow runs along y = 40 from where it was pressed inside the left box (x = 50) to
/// where it was let go inside the right one (x = 400): an end drawn inside a shape binds
/// inside it, exactly there (`packages/element/src/binding.ts:838-845`). Its midpoint
/// handle sits at x = 225 — a press there bends the arrow, as Excalidraw's does, rather
/// than moving it — so every grab below is on the shaft well clear of it and of both ends.
fn bound_arrow() -> (DrawEngine, String, String, String) {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(350.0, 0.0, 100.0, 80.0));
    let (l, r) = (left.id.clone(), right.id.clone());
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
    assert_eq!(arrow.start_binding.as_deref(), Some(l.as_str()), "setup");
    assert_eq!(arrow.end_binding.as_deref(), Some(r.as_str()), "setup");
    (engine, arrow.id, l, r)
}

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

/// Drags from `from` by `(dx, dy)` in several steps, with object snapping at its default: off.
fn drag(engine: &mut DrawEngine, from: (f64, f64), dx: f64, dy: f64) {
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=5 {
        let t = step as f64 / 5.0;
        engine.move_pointer(from.0 + dx * t, from.1 + dy * t, false, false);
    }
    engine.end_pointer();
}

/// On the shaft, clear of the midpoint handle and of both ends.
const SHAFT: (f64, f64) = (160.0, 40.0);

// ---------------------------------------------------------------------------
// The bug
// ---------------------------------------------------------------------------

/// The gesture has to be a move in the first place, or everything below is testing the
/// wrong thing. Checked mid-drag, because the bug is invisible once the pointer is up.
#[test]
fn grabbing_the_shaft_starts_a_move() {
    let (mut engine, _arrow, _l, _r) = bound_arrow();
    engine.begin_pointer(SHAFT.0, SHAFT.1, false, false);
    engine.move_pointer(SHAFT.0, SHAFT.1 + 30.0, false, false);
    assert_eq!(engine.debug_state().interaction.kind, Some("move"));
    engine.end_pointer();
}

#[test]
fn a_bound_arrow_dragged_on_its_own_moves() {
    let (mut engine, arrow, _l, _r) = bound_arrow();
    let before = get(&engine, &arrow);

    drag(&mut engine, SHAFT, 0.0, 150.0);

    let after = get(&engine, &arrow);
    assert_close(after.y - before.y, 150.0);
    assert_close(after.x - before.x, 0.0);
}

#[test]
fn and_lets_go_of_both_shapes() {
    let (mut engine, arrow, _l, _r) = bound_arrow();

    drag(&mut engine, SHAFT, 0.0, 150.0);

    let after = get(&engine, &arrow);
    assert_eq!(after.start_binding, None);
    assert_eq!(after.end_binding, None);
}

/// Once free, it is free: moving a shape it used to be bound to must not reach across
/// and drag it back.
#[test]
fn a_shape_it_was_unbound_from_no_longer_moves_it() {
    let (mut engine, arrow, left, _r) = bound_arrow();
    drag(&mut engine, SHAFT, 0.0, 150.0);
    let parked = get(&engine, &arrow);

    engine.select(vec![left]);
    drag(&mut engine, (50.0, 40.0), 0.0, -60.0);

    let after = get(&engine, &arrow);
    assert_close(after.x, parked.x);
    assert_close(after.y, parked.y);
}

#[test]
fn one_undo_puts_it_back_bound() {
    let (mut engine, arrow, l, r) = bound_arrow();
    let before = get(&engine, &arrow);
    drag(&mut engine, SHAFT, 0.0, 150.0);

    engine.undo();

    let after = get(&engine, &arrow);
    assert_close(after.y, before.y);
    assert_eq!(after.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(after.end_binding.as_deref(), Some(r.as_str()));
}

// ---------------------------------------------------------------------------
// When it must NOT unbind
// ---------------------------------------------------------------------------

/// The click that selects an arrow is never perfectly still. Under the threshold nothing
/// happens to the bindings — otherwise selecting an arrow would quietly detach it.
#[test]
fn a_wobble_below_the_threshold_keeps_both_bindings() {
    let (mut engine, arrow, l, r) = bound_arrow();

    drag(&mut engine, SHAFT, 4.0, 5.0);

    let after = get(&engine, &arrow);
    assert_eq!(after.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(after.end_binding.as_deref(), Some(r.as_str()));
}

/// Dragging the whole assembly keeps it assembled.
#[test]
fn moving_the_arrow_with_both_shapes_keeps_it_bound() {
    let (mut engine, arrow, l, r) = bound_arrow();
    let before = get(&engine, &arrow);
    engine.select(vec![arrow.clone(), l.clone(), r.clone()]);

    drag(&mut engine, (50.0, 40.0), 0.0, 100.0);

    let after = get(&engine, &arrow);
    assert_eq!(after.start_binding.as_deref(), Some(l.as_str()));
    assert_eq!(after.end_binding.as_deref(), Some(r.as_str()));
    assert_close(after.y - before.y, 100.0);
}

/// Only the end whose shape stays behind lets go.
#[test]
fn moving_the_arrow_with_one_shape_releases_only_the_other_end() {
    let (mut engine, arrow, l, _r) = bound_arrow();
    engine.select(vec![arrow.clone(), l.clone()]);

    drag(&mut engine, (50.0, 40.0), 0.0, 100.0);

    let after = get(&engine, &arrow);
    assert_eq!(
        after.start_binding.as_deref(),
        Some(l.as_str()),
        "its shape moved with it"
    );
    assert_eq!(after.end_binding, None, "its shape stayed behind");
}

/// The control: binding still does its job when the arrow is *not* what is being moved.
#[test]
fn moving_a_shape_still_drags_a_bound_arrow_end() {
    let (mut engine, arrow, _l, right) = bound_arrow();
    let before = get(&engine, &arrow);
    engine.select(vec![right]);

    // Grabbed clear of the arrow, which is drawn across the box.
    drag(&mut engine, (430.0, 70.0), 0.0, 120.0);

    let after = get(&engine, &arrow);
    assert!(
        after.height.abs() > 60.0,
        "the arrow's far end should have followed the shape down: {} -> {}",
        before.height,
        after.height
    );
}
