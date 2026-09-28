//! Snapping to other elements while **resizing** — 6.3, `design.md:163`.
//!
//! The oracle's second entry point, `snapResizingElements`
//! (`snapping.ts@1118751f:1108-1244`), reached from `maybeHandleResize`
//! (`App.tsx@1118751f:13625`, under `resize.isResizing` at `:10725`) and from
//! `maybeHandleCrop` (`:13507`).
//!
//! Three things make it a different question from the move path, and each is asserted
//! here rather than assumed:
//!
//! - **What moves is the edge, not the box.** A side handle offers its edge's two
//!   endpoints; a corner handle offers one point (`snapping.ts@1118751f:1146-1183`). The
//!   box's other three corners are not candidates, so a resize is not a move in disguise.
//! - **A single turned element does not snap at all** (`:1121-1122`,
//!   `!areRoughlyEqual(selectedElements[0].angle, 0)`). A group of several does, because
//!   the clause is about a *single* element's own angle.
//! - **Only the origin's box is offered.** The oracle measures the moving points from
//!   `getCommonBounds(selectedOriginalElements)` plus the drag offset (`:1130-1144`), so
//!   the anchors of the resize are not themselves candidates.
//!
//! ## The numbers are the oracle's, written down before they were run
//!
//! The scene is a box at `(0, 0, 100, 80)` and a reference at `(300, 200, 100, 80)`, whose
//! stops are **x ∈ {300, 350, 400}** and **y ∈ {200, 240, 280}**. The reference is
//! offset vertically on purpose: the resized box's own y-extent is 0…80, and none of the
//! reference's y stops is within reach of it, so the y answer must come out as "unchanged"
//! without the two ranges coinciding and hiding a mistake.

mod common;
use common::*;
use draw_engine::*;

/// A reference at `(300, 200, 100, 80)`. Its stops are known by hand; see the header.
fn reference() -> DrawElement {
    filled(box_at(300.0, 200.0, 100.0, 80.0))
}

/// The box at `(0, 0, 100, 80)`, turned by `turn`, with a reference beside it, selected.
fn scene(turn: f64) -> (DrawEngine, String) {
    let mut resized = filled(box_at(0.0, 0.0, 100.0, 80.0));
    resized.angle = turn;
    let id = resized.id.clone();
    let mut engine = engine_with_scene(vec![resized, reference()]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    (engine, id)
}

/// That box, alone in the scene, selected, with objects snapped or not.
fn selected(snapping: bool) -> (DrawEngine, String) {
    let element = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.set_objects_snap(snapping);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    (engine, id)
}

fn element_of(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the element is gone")
}

fn box_of(engine: &DrawEngine, id: &str) -> (f64, f64, f64, f64) {
    let element = element_of(engine, id);
    (element.x, element.y, element.width, element.height)
}

/// The engine's own handle size, from `engine/mod.rs`'s private `HANDLE_PX` and
/// `ROTATE_GAP_PX`. A handle's centre sits `FRAME_MARGIN_PX + HANDLE_PX / 2` outside the
/// outline (`selection/handles.rs` › `HandleLayout::screen`), which is why a press on the
/// box's own right edge is a **move** and not a resize.
const HANDLE_PX: f64 = 8.0;
const ROTATE_GAP_PX: f64 = 26.0;

/// The `E` handle of `id`, in world units, turned with the element.
fn east_handle(engine: &DrawEngine, id: &str) -> Point {
    let layout = HandleLayout::screen(HANDLE_PX, ROTATE_GAP_PX, engine.camera.scale);
    selection_handles(&element_of(engine, id), layout)
        .into_iter()
        .find(|handle| handle.kind == HandleKind::E)
        .map(|handle| Point {
            x: handle.x,
            y: handle.y,
        })
        .expect("the east handle is drawn")
}

/// Presses the `E` handle and drags so that the **moving point** lands on world x `to`.
///
/// The grab is measured from the pointer to the box's own east edge, not to the handle:
/// the engine does that (`engine/pointer.rs`) and so does the oracle, from the raw
/// pointer-down against the element's bounds (`getResizeOffsetXY`,
/// `App.tsx@1118751f:9406-9416`). The handle is drawn 8 units further out than the edge,
/// so the pointer travels 8 further than the edge does — and a test that forgot that
/// would be testing a resize of 289 when it said 297.
fn drag_east(engine: &mut DrawEngine, id: &str, to: f64, invert: bool) {
    let handle = east_handle(engine, id);
    let edge = element_of(engine, id).x + element_of(engine, id).width;
    engine.begin_pointer(handle.x, handle.y, false, false);
    engine.move_pointer(handle.x + (to - edge), handle.y, false, invert);
    engine.end_pointer();
}

// ------------------------------------------------------------------ the edge

/// 297 is 3 from the reference's left edge at 300, so the edge the handle holds lands
/// there and the box grows to it.
#[test]
fn the_edge_you_are_dragging_snaps_to_a_nearby_box() {
    let (mut engine, id) = scene(0.0);
    engine.set_objects_snap(true);

    drag_east(&mut engine, &id, 297.0, false);

    assert_eq!(box_of(&engine, &id), (0.0, 0.0, 300.0, 80.0));
}

/// The other half of the pair: 6.5 from the same stop — half a unit past the reach — with
/// nothing else about the scene changed.
#[test]
fn an_edge_a_hair_outside_the_threshold_does_not_snap() {
    let (mut engine, id) = scene(0.0);
    engine.set_objects_snap(true);

    drag_east(&mut engine, &id, 306.5, false);

    assert_eq!(box_of(&engine, &id), (0.0, 0.0, 306.5, 80.0));
}

/// **The pair as one fact.** Same scene, same press, same gesture, 6.5 apart on the
/// pointer. The snapped one is 6.5 narrower; the anchor and the height are bit-identical
/// across the two, which is the statement that a snap on the dragged edge moved **nothing
/// else** — not the opposite edge, not the height, and not by a leftover amount.
#[test]
fn the_pair_differs_by_exactly_the_snap_and_by_nothing_else() {
    let (mut inside, inside_id) = scene(0.0);
    let (mut outside, outside_id) = scene(0.0);
    inside.set_objects_snap(true);
    outside.set_objects_snap(true);

    drag_east(&mut inside, &inside_id, 297.0, false);
    drag_east(&mut outside, &outside_id, 306.5, false);

    let (ix, iy, iw, ih) = box_of(&inside, &inside_id);
    let (ox, oy, ow, oh) = box_of(&outside, &outside_id);
    assert_close_msg(
        ow - iw,
        9.5 - 3.0,
        "the pointers are 9.5 apart and the inside one snapped a further 3",
    );
    assert_close(ix, ox);
    assert_close(iy, oy);
    assert_close(ih, oh);
    assert_close(ih, 80.0);
    assert_close(ix, 0.0);
}

/// The element being resized is not its own target. With the reference taken away and
/// nothing else in the scene, the edge still goes exactly where the pointer put it — a
/// resize that snapped to its own left edge would be stuck at width 100 forever.
#[test]
fn the_element_being_resized_is_not_its_own_target() {
    let (mut engine, id) = selected(true);

    drag_east(&mut engine, &id, 250.0, false);

    assert_eq!(box_of(&engine, &id), (0.0, 0.0, 250.0, 80.0));
}

/// **The candidate is a box**, and the box offers its edges and its centre alike. Two
/// pairs: 3 units from a stop snaps, 6.5 does not — once for the left edge at 300 and once
/// for the centre at 350. No point of the reference's outline is a candidate, and the
/// **inside** half of each pair says so: at 297 the outline's left edge is 3 away on x and
/// the outline's top-left *corner* is 3 away on x and 37 on y, so a candidate set of
/// outline points could answer on x with the same number and the **outside** half is where
/// the two would part company — at 306.5 an outline point 6.5 away on x and 0 on y (the
/// left edge is 6.5 away on y too, but a point on the top edge at (300, 200) is 6.5 away
/// on x and 160 on y) never reaches the reach on both axes at once, while the box's own
/// edge answers on x alone.
#[test]
fn a_reference_offers_its_edges_and_its_centre_and_nothing_else() {
    for (pointer, want) in [
        (297.0, 300.0),
        (306.5, 306.5),
        (347.0, 350.0),
        (356.5, 356.5),
    ] {
        let (mut engine, id) = scene(0.0);
        engine.set_objects_snap(true);

        drag_east(&mut engine, &id, pointer, false);

        assert_close_msg(
            box_of(&engine, &id).2,
            want,
            format!("pointer at {pointer}"),
        );
    }
}

// -------------------------------------------------------------- the angle gate

/// **A single turned element does not snap while resizing** — the oracle's
/// `selectedElements.length === 1 && !areRoughlyEqual(angle, 0)`
/// (`snapping.ts@1118751f:1121-1122`). The pointer is 3 from the reference's left edge and
/// so well inside the reach: it is the gate that refuses, not the distance. The upright
/// element in the very same test is the control, and it does snap.
#[test]
fn one_turned_element_does_not_snap_while_resizing() {
    let (mut turned, turned_id) = scene(EIGHTH_TURN);
    let (mut plain, plain_id) = scene(0.0);
    turned.set_objects_snap(true);
    plain.set_objects_snap(true);

    drag_east(&mut turned, &turned_id, 297.0, false);
    drag_east(&mut plain, &plain_id, 297.0, false);

    let width = box_of(&turned, &turned_id).2;
    assert_ne!(
        width, 300.0,
        "the turned element landed on the reference, so this proves nothing"
    );
    assert_close_msg(
        box_of(&plain, &plain_id).2,
        300.0,
        "and the upright one did",
    );
}

/// The same turned element, same gesture, preference off: **bit-identical**. Turning
/// snapping on must not move a turned element's resize by so much as a bit, which says
/// more than "it did not reach 300".
#[test]
fn turning_snapping_on_does_not_touch_a_turned_resize_at_all() {
    let (mut on, on_id) = scene(EIGHTH_TURN);
    let (mut off, off_id) = scene(EIGHTH_TURN);
    on.set_objects_snap(true);
    off.set_objects_snap(false);

    drag_east(&mut on, &on_id, 297.0, false);
    drag_east(&mut off, &off_id, 297.0, false);

    assert_eq!(box_of(&on, &on_id), box_of(&off, &off_id));
}

// --------------------------------------------------------------------- the gate

#[test]
fn it_is_off_unless_the_preference_is_on() {
    let (mut engine, id) = scene(0.0);

    drag_east(&mut engine, &id, 297.0, false);

    assert_eq!(box_of(&engine, &id), (0.0, 0.0, 297.0, 80.0));
}

#[test]
fn holding_ctrl_inverts_it_for_the_resizing() {
    let (mut engine, id) = scene(0.0);
    engine.set_objects_snap(true);

    drag_east(&mut engine, &id, 297.0, true);

    assert_eq!(box_of(&engine, &id), (0.0, 0.0, 297.0, 80.0));
    assert!(engine.objects_snap(), "and the preference is untouched");
}

/// A snapping grid and object snapping give different answers, so the grid wins. A 40-grid
/// puts the pointer on 280 where the reference's edge is at 300, so the two cannot be
/// mistaken for one another.
#[test]
fn a_snapping_grid_wins_over_objects() {
    let (mut engine, id) = scene(0.0);
    engine.set_objects_snap(true);
    engine.set_grid(GridSettings {
        enabled: true,
        snap: true,
        size: 40.0,
        ..engine.grid()
    });

    drag_east(&mut engine, &id, 297.0, false);

    assert_eq!(box_of(&engine, &id).2, 280.0);
    assert!(engine.snap_guides().is_empty(), "no object guides");
}

// ------------------------------------------------------------------- the guides

#[test]
fn a_snap_shows_a_guide_and_a_missed_one_shows_none() {
    for (pointer, want_guides) in [(297.0, true), (306.5, false)] {
        let (mut engine, id) = scene(0.0);
        engine.set_objects_snap(true);
        let handle = east_handle(&engine, &id);
        let edge = element_of(&engine, &id).x + element_of(&engine, &id).width;

        engine.begin_pointer(handle.x, handle.y, false, false);
        engine.move_pointer(handle.x + (pointer - edge), handle.y, false, false);
        let guides = engine.snap_guides().to_vec();
        engine.end_pointer();

        assert_eq!(!guides.is_empty(), want_guides, "at {pointer}");
        if want_guides {
            assert_eq!(guides[0].axis, Axis::X);
            assert_close(guides[0].at, 300.0);
        }
    }
}

// --------------------------------------------------------------------- a group

/// A group is not a single element, so the angle clause does not apply and a group resize
/// snaps like anything else (`snapping.ts@1118751f:1121-1122` says `length === 1`).
/// 303 is 3 from the reference's left edge, so the frame's east edge lands on 300.
#[test]
fn a_group_resize_snaps_too() {
    let a = filled(box_at(0.0, 0.0, 40.0, 80.0));
    let b = filled(box_at(50.0, 0.0, 40.0, 80.0));
    let ids = vec![a.id.clone(), b.id.clone()];
    let mut engine = engine_with_scene(vec![a, b, reference()]);
    engine.set_objects_snap(true);
    engine.set_tool(DrawTool::Select);
    engine.select(ids.clone());

    // The group's frame is x ∈ [0, 90] and its east handle sits 8 further out.
    engine.begin_pointer(90.0 + 8.0, 40.0, false, false);
    engine.move_pointer(303.0 + 8.0, 40.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    let left = scene
        .iter()
        .find(|el| el.id == ids[0])
        .expect("a member is gone");
    let right = scene
        .iter()
        .find(|el| el.id == ids[1])
        .expect("a member is gone");
    assert_close_msg(
        right.x + right.width,
        300.0,
        "the dragged edge landed on 300",
    );
    assert_close_msg(left.x, 0.0, "and the anchor did not move");
}
