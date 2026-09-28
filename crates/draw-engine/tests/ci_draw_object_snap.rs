//! Snapping to other elements while **drawing** — 6.3, `design.md:163`.
//!
//! It used to be a move-only gate. `ci_objects_snap.rs` covers that half, and
//! `registry.ts`'s note on the rule was right about the symptom and wrong about the
//! cause: Excalidraw has four exported entry points into `snapping.ts` and we had one.
//! The two that reach a shape being drawn are `getSnapLinesAtPointer`
//! (`snapping.ts@1118751f:1318-1400`, from `App.tsx@1118751f:7870`) and `snapNewElement`
//! (`:1246-1316`, from `:13383`). Both run the same `getPointSnaps` (`:636-690`) over the
//! same reference points, and both have exactly **one** moving point:
//! `snapNewElement`'s is `origin + dragOffset`, which is the pointer
//! (`App.tsx@1118751f:13387-13398`).
//!
//! **The candidates are boxes, not points.** `getReferenceSnapPoints` (`:616-634`) maps
//! every reference group through `getElementsCorners` (`:198-313`), which for a rectangle
//! is its four corners and its centre. A point on some other shape's outline is never a
//! candidate, and neither is an edge midpoint.
//!
//! The reach is `SNAP_DISTANCE / zoom` in **screen** pixels (`:48-50`, `:717`), so it is
//! 6 world units at 1× here and 12 at 0.5×.
//!
//! ## The numbers are the oracle's, written down before they were run
//!
//! Every scene below is one reference box at the origin, so its stops are known by hand:
//! **x ∈ {0, 50, 100}** and **y ∈ {0, 40, 80}**. Every expected landing point below is
//! one of those stops plus a stated offset, and none of them was read back out of the
//! implementation. That is the whole point of the file: a snapper that snaps *always*
//! passes "a snap happened", and one that snaps at the wrong distance, in the wrong
//! order, or on the wrong candidate passes a self-consistent test of its own arithmetic.

mod common;
use common::*;
use draw_engine::*;

/// One reference box, at the origin, whose stops are therefore known by hand.
fn reference() -> DrawElement {
    filled(box_at(0.0, 0.0, 100.0, 80.0))
}

/// Objects snapped, at 1× with an 800×600 viewport, so every number here is in world
/// units and the reach is 6.
fn snapping() -> DrawEngine {
    let mut engine = engine_with_scene(vec![reference()]);
    engine.set_objects_snap(true);
    engine
}

fn box_of(engine: &DrawEngine) -> (f64, f64, f64, f64) {
    let element = engine.get_scene().pop().expect("nothing was drawn");
    (element.x, element.y, element.width, element.height)
}

fn bounds_of(element: &DrawElement) -> (f64, f64, f64, f64) {
    (element.x, element.y, element.width, element.height)
}

/// Presses at `press` and drags to `corner`, in world units at 1×. The corner is the raw
/// landing point: this engine draws from the pointer, so the drag distance is exactly
/// `corner - press` unless a snap moves one end of it.
fn draw_to(engine: &mut DrawEngine, press: (f64, f64), corner: (f64, f64)) {
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(press.0, press.1, false, false);
    engine.move_pointer(corner.0, corner.1, false, false);
    engine.end_pointer();
}

/// The scene, snap guides and all, at the moment the pointer is where it was left.
fn guides_at(engine: &mut DrawEngine, press: (f64, f64), corner: (f64, f64)) -> Vec<SnapGuide> {
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(press.0, press.1, false, false);
    engine.move_pointer(corner.0, corner.1, false, false);
    let guides = engine.snap_guides().to_vec();
    engine.end_pointer();
    guides
}

// ---------------------------------------------------------------- the corner

/// The pointer corner is pulled onto the nearest reference stop. 297 is 3 from the box's
/// right edge at 100; the press is 400 away from anything, so only the corner snaps.
#[test]
fn the_corner_you_are_dragging_snaps_to_a_nearby_box() {
    let mut engine = snapping();

    draw_to(&mut engine, (500.0, 500.0), (103.0, 240.0));

    // The corner landed on (100, 240), so the box spans x ∈ [100, 500], y ∈ [240, 500].
    assert_eq!(box_of(&engine), (100.0, 240.0, 400.0, 260.0));
}

/// The half that says it was the *distance* and not merely the presence of another
/// element. 106.5 is 6.5 from the same stop — half a unit past the 6-unit reach — and
/// nothing else about the scene changes.
#[test]
fn a_corner_a_hair_outside_the_threshold_does_not_snap() {
    let mut engine = snapping();

    draw_to(&mut engine, (500.0, 500.0), (106.5, 240.0));

    assert_eq!(box_of(&engine), (106.5, 240.0, 393.5, 260.0));
}

/// **The pair as one fact.** Same scene, same press, same gesture, 3.5 apart on the
/// pointer. The one that snapped moved 3.5 less than the one that did not, and the
/// vertical is bit-identical across the two — the strongest available statement that a
/// snap on one axis cannot leak into the other.
#[test]
fn the_pair_differs_by_exactly_the_snap_and_by_nothing_else() {
    let mut inside = snapping();
    let mut outside = snapping();

    draw_to(&mut inside, (500.0, 500.0), (103.0, 240.0));
    draw_to(&mut outside, (500.0, 500.0), (106.5, 240.0));

    let (ix, _, _, ih) = box_of(&inside);
    let (ox, oy, _, oh) = box_of(&outside);
    assert_close_msg(
        ox - ix,
        6.5,
        "the corners are 3.5 apart and the inside one snapped a further 3",
    );
    assert_close(oy, 240.0);
    assert_close(ih, oh);
    assert_close(ih, 260.0);
}

/// A snap decides **where the shape lands**. It must not move the reference, and it must
/// not resize the shape by an amount of its own: the height comes from the raw vertical
/// drag and the width from the snapped corner, with nothing left over.
#[test]
fn a_snap_moves_the_new_shape_and_nothing_else() {
    let mut engine = snapping();
    let before = bounds_of(&reference());
    let count = engine.get_scene().len();

    draw_to(&mut engine, (500.0, 500.0), (103.0, 240.0));

    let (x, y, width, height) = box_of(&engine);
    assert_close(height, 500.0 - 240.0);
    assert_close(width, 500.0 - x);
    assert_close(y, 240.0);
    assert_eq!(
        bounds_of(&reference()),
        before,
        "the reference did not move"
    );
    assert_eq!(
        engine.get_scene().len(),
        count + 1,
        "one new shape, no more"
    );
}

// ------------------------------------------------------------------ the press

/// The press itself snaps, so a shape is **born** aligned rather than only becoming
/// aligned once the pointer has moved. 103 is 3 from the same right edge at 100.
#[test]
fn the_press_snaps_so_a_shape_is_born_aligned() {
    let mut engine = snapping();

    draw_to(&mut engine, (103.0, 300.0), (143.0, 340.0));

    // The origin snapped to (100, 300); the corner is exactly where it was left, at 143.
    assert_eq!(box_of(&engine), (100.0, 300.0, 43.0, 40.0));
}

/// **The press half of the pair, and the "does not alter" claim in the only form it is true
/// in.** The same corner in both halves, and the press 3.5 either side of the same stop.
/// The **corner lands exactly where the pointer left it, bit-identical across the two** —
/// that is what a press-origin snap may not touch. The *width* does differ, by 6.5, and it
/// must: a shape is the distance from its press to its corner
/// (`dragElements.ts@1118751f:368-377`), so moving the press 3.5 nearer the corner makes
/// the shape 6.5 shorter. A test demanding the width be unchanged would be demanding the
/// press not move at all.
#[test]
fn the_press_pair_moves_the_press_and_not_the_corner() {
    let mut inside = snapping();
    let mut outside = snapping();

    draw_to(&mut inside, (103.0, 300.0), (143.0, 340.0));
    draw_to(&mut outside, (106.5, 300.0), (143.0, 340.0));

    let (ix, iy, iw, ih) = box_of(&inside);
    let (ox, oy, ow, oh) = box_of(&outside);
    assert_close_msg(ix, 100.0, "the press snapped to the stop");
    assert_close_msg(ox, 106.5, "and this one did not");
    assert_close_msg(
        ox - ix,
        6.5,
        "the presses are 3.5 apart and the inside one snapped a further 3",
    );
    assert_close_msg(ix + iw, ox + ow, "the corner is the same in both");
    assert_close_msg(iy, oy, "and so is its y");
    assert_close_msg(ih, oh, "and so is the height");
    assert_close_msg(ix + iw, 143.0, "where the pointer left it");
    assert_close_msg(iw, 43.0, "the press-to-corner distance");
}

// ------------------------------------------------------------- the candidates

/// **Corners and centres, not points and midpoints.** The pointer is 3 from the box's
/// **centre** on x and over 400 from every edge, so this is the centre stop or nothing: no
/// corner is within reach, and the nearest outline point — the left edge at 0 — is 53 away.
#[test]
fn a_box_snaps_by_its_centre_as_well_as_by_its_edges() {
    let mut engine = snapping();

    draw_to(&mut engine, (500.0, 500.0), (53.0, 540.0));

    // The centre stop is 50 and 53 − 3 = 50. The other two stops are 53 and 47 away.
    assert_eq!(box_of(&engine), (50.0, 500.0, 450.0, 40.0));
}

/// The centre obeys the same threshold as every other candidate: half a unit further out
/// and it is gone. Same scene, same gesture.
#[test]
fn the_centre_stops_at_the_threshold_like_every_other_candidate() {
    let mut engine = snapping();

    draw_to(&mut engine, (500.0, 500.0), (56.5, 540.0));

    assert_eq!(box_of(&engine), (56.5, 500.0, 443.5, 40.0));
}

/// The threshold is inclusive, which is the oracle's `<=`
/// (`snapping.ts@1118751f:660`). Exactly on it snaps; a tenth of a unit past does not.
#[test]
fn exactly_on_the_threshold_snaps_and_a_tenth_past_does_not() {
    let mut on = snapping();
    let mut past = snapping();

    draw_to(&mut on, (500.0, 500.0), (106.0, 540.0));
    draw_to(&mut past, (500.0, 500.0), (106.1, 540.0));

    assert_close(box_of(&on).0, 100.0);
    assert_close(box_of(&past).0, 106.1);
}

// -------------------------------------------------------------------- the gate

/// Off unless asked for, like the move path — and the preference stays a preference.
#[test]
fn it_is_off_unless_the_preference_is_on() {
    let mut engine = engine_with_scene(vec![reference()]);

    draw_to(&mut engine, (500.0, 500.0), (103.0, 240.0));

    assert_eq!(box_of(&engine), (103.0, 240.0, 397.0, 260.0));
}

/// Ctrl/Cmd inverts the preference for the gesture, as it does for a move
/// (`snapping.ts@1118751f:178-184`).
#[test]
fn holding_ctrl_inverts_it_for_the_drawing() {
    let mut engine = snapping();

    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(500.0, 500.0, false, false);
    engine.move_pointer(103.0, 240.0, false, true);
    engine.end_pointer();

    assert_eq!(box_of(&engine), (103.0, 240.0, 397.0, 260.0));
    assert!(engine.objects_snap(), "and the preference is untouched");
}

/// A snapping grid and object snapping give different answers, so the grid wins — the
/// rule the move path already follows, now from one function all three call.
///
/// The corner is at 43, which is **3 from the box's centre stop at 50** and **3 from the
/// 20-grid's 40**. The two answers are 50 apart from each other by 10, so this cannot pass
/// by the two agreeing.
#[test]
fn a_snapping_grid_wins_over_objects() {
    let mut engine = snapping();
    engine.set_grid(GridSettings {
        enabled: true,
        snap: true,
        ..engine.grid()
    });

    draw_to(&mut engine, (500.0, 500.0), (43.0, 240.0));

    // The grid's 40 on both axes, not the box's 50.
    assert_eq!(box_of(&engine), (40.0, 240.0, 460.0, 260.0));
    assert!(engine.snap_guides().is_empty(), "no object guides");
}

/// The reach is in **screen** pixels, so it is 12 world units at 0.5×. The same 10-unit
/// miss that is out of reach at 1× is well inside it at half scale.
#[test]
fn the_reach_is_in_screen_pixels_so_it_grows_as_you_zoom_out() {
    for (scale, want_x, want_w) in [(1.0, 90.0, 30.0), (0.5, 100.0, 20.0)] {
        let mut engine = snapping();
        engine.set_camera(Camera {
            x: 0.0,
            y: 0.0,
            scale,
        });
        let press = world_to_screen(engine.camera, 90.0, 300.0);
        let corner = world_to_screen(engine.camera, 120.0, 340.0);

        draw_to(&mut engine, (press.x, press.y), (corner.x, corner.y));

        assert_close_msg(box_of(&engine).0, want_x, format!("x at {scale}×"));
        assert_close_msg(box_of(&engine).2, want_w, format!("width at {scale}×"));
    }
}

// ------------------------------------------------------------------ the guides

#[test]
fn a_snap_shows_a_guide_to_the_stop_it_took() {
    let mut engine = snapping();

    let guides = guides_at(&mut engine, (500.0, 500.0), (103.0, 240.0));

    assert_eq!(guides.len(), 1, "one axis snapped, so one guide");
    assert_eq!(guides[0].axis, Axis::X);
    assert_close(guides[0].at, 100.0);
    assert_close_msg(guides[0].from, 0.0, "the target box's own top edge");
    assert_close_msg(guides[0].to, 240.0, "and our point");
}

#[test]
fn a_gesture_that_did_not_snap_shows_no_guides() {
    let mut engine = snapping();

    let guides = guides_at(&mut engine, (500.0, 500.0), (106.5, 240.0));

    assert!(guides.is_empty());
    assert!(
        engine.snap_guides().is_empty(),
        "and they stay out afterwards"
    );
}

// -------------------------------------------------------------- a turned box

/// **A known divergence, pinned so that it stays deliberate.** The oracle rotates a
/// reference element's corners by its own angle (`snapping.ts@1118751f:241-291`); this
/// engine reads the unrotated box. A turned reference therefore offers *its box's*
/// corners, not the turned shape's. The reference here is a box at the origin turned a
/// quarter of a way to upright, and the stop this test aims at — the left edge at 0 — is
/// on the rotation's axis, so the test proves the turn did not throw the candidate away
/// and says plainly that it cannot see the difference. `docs/reference/drawing.md` records
/// what the difference is.
#[test]
fn a_turned_reference_still_offers_its_box() {
    let mut turned = reference();
    turned.angle = EIGHTH_TURN;
    let mut engine = engine_with_scene(vec![turned]);
    engine.set_objects_snap(true);

    draw_to(&mut engine, (500.0, 500.0), (3.0, 500.0));

    assert_eq!(box_of(&engine), (0.0, 500.0, 500.0, 0.0));
}
