//! Properties of the nearest-candidate snap, over a lattice of scenes.
//!
//! The interesting failure is not "it did not snap" but **"it snapped to the wrong
//! candidate"**, because a reordering looks right on a quiet board and is wrong on a busy
//! one. A single hand-computed fixture cannot see that, so the nearest-candidate maths is
//! written a second time here — from the oracle's rule, not from the engine's loop — and
//! the two are compared over a few hundred cases.
//!
//! ## The rule, from `snapping.ts@1118751f:636-690`
//!
//! ```text
//! for each of our own points, for each reference point:
//!     offset = reference − ours, per axis
//!     if |offset| <= threshold:
//!         if |offset| < best: best = offset     // strictly, so a tie keeps the first
//! ```
//!
//! The two invariants:
//!
//! 1. **A snap never moves a point further than the threshold**, on either axis. A larger
//!    move is not a snap, it is a teleport, and it is what a wrong threshold or a wrong
//!    candidate looks like from the outside.
//! 2. **The chosen candidate is the nearest under the oracle's own metric** — the minimum
//!    `|offset|` over every (ours, reference) pair within the threshold. This is what
//!    catches a reordering.
//!
//! ## The inputs, and the guard that keeps them honest
//!
//! A lattice of candidate boxes **on one line and off it**, exactly at the reach and a
//! hundredth either side, at 1× / 2× / 0.5× / 3×, and with fractional pans. Two things
//! keep the sweep from going degenerate silently, and both are asserted rather than
//! assumed:
//!
//! - every case checks that at least one candidate is **on screen**, because the engine
//!   culls candidates to the viewport (`engine/pointer.rs` › `snap_targets`, and the
//!   oracle's `getVisibleAndNonSelectedElements`, `snapping.ts@1118751f:315-326`). A case
//!   whose only candidate is off screen would "pass" with no snapping at all;
//! - the second implementation applies the same cull, so the two answers are comparable —
//!   and a disagreement about which candidates exist is itself a finding, not something
//!   to paper over by dropping the cull from one side.
//!
//! The press is at `(1000, 1000)`, far outside the viewport on purpose: it is then
//! nowhere near any candidate, so the only thing that can move is the corner, and the
//! press-origin snap cannot be confused with the corner snap.

mod common;
use common::*;
use draw_engine::*;

/// The reach in world units at a given scale: `SNAP_DISTANCE / zoom`
/// (`snapping.ts@1118751f:48-50`), and `engine/mod.rs` › `SNAP_PX`.
fn reach(scale: f64) -> f64 {
    6.0 / scale
}

/// The press for every case: nowhere near the viewport, so nothing but the corner snaps.
const PRESS: (f64, f64) = (1000.0, 1000.0);

/// A box's three stops on one axis: its two edges and its centre.
fn axis_stops(min: f64, extent: f64) -> [f64; 3] {
    [min, min + extent / 2.0, min + extent]
}

fn stops_of(element: &DrawElement) -> ([f64; 3], [f64; 3]) {
    (
        axis_stops(element.x, element.width),
        axis_stops(element.y, element.height),
    )
}

fn bounds_of(element: &DrawElement) -> WorldBounds {
    let (xs, ys) = stops_of(element);
    WorldBounds {
        min_x: xs[0],
        min_y: ys[0],
        max_x: xs[2],
        max_y: ys[2],
    }
}

/// Whether a candidate is on screen — the engine's cull, restated so that a disagreement
/// about it shows up as a failing case rather than as a silently empty sweep.
fn on_screen(bounds: WorldBounds, camera: Camera) -> bool {
    let view = visible_world_rect(camera, 800.0, 600.0);
    bounds.min_x <= view.max_x
        && bounds.max_x >= view.min_x
        && bounds.min_y <= view.max_y
        && bounds.max_y >= view.min_y
}

/// **The second implementation of the nearest-candidate maths.** Written from the oracle's
/// rule: the loops run the other way round from the engine's and the comparison is a fold,
/// so a transcription of the engine would have reproduced whatever the engine got wrong.
fn oracle_offset(ours: f64, theirs: &[f64; 3], threshold: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for &target in theirs.iter() {
        let offset = target - ours;
        if offset.abs() <= threshold && best.is_none_or(|b| offset.abs() < b.abs()) {
            best = Some(offset);
        }
    }
    best
}

/// What the oracle's rule says, over the candidates the engine could see.
fn oracle_landing(
    targets: &[DrawElement],
    camera: Camera,
    corner: (f64, f64),
    threshold: f64,
) -> (f64, f64) {
    let visible: Vec<DrawElement> = targets
        .iter()
        .filter(|t| on_screen(bounds_of(t), camera))
        .cloned()
        .collect();
    assert!(
        !visible.is_empty(),
        "every candidate is off screen: the case is degenerate"
    );
    let pick = |corner: f64, x: bool| -> f64 {
        visible
            .iter()
            .filter_map(|t| {
                let (xs, ys) = stops_of(t);
                oracle_offset(corner, if x { &xs } else { &ys }, threshold)
            })
            .min_by(|a, b| a.abs().partial_cmp(&b.abs()).expect("finite offsets"))
            .unwrap_or(0.0)
    };
    (pick(corner.0, true), pick(corner.1, false))
}

/// Where the engine actually put the corner, and by how much it moved it.
fn drawn_offset(targets: &[DrawElement], corner: (f64, f64), scale: f64) -> (f64, f64, Camera) {
    let camera = Camera {
        x: 0.0,
        y: 0.0,
        scale,
    };
    let mut engine = engine_with_scene(targets.to_vec());
    engine.set_viewport(800.0, 600.0, scale);
    engine.set_camera(camera);
    engine.set_objects_snap(true);
    let press = world_to_screen(camera, PRESS.0, PRESS.1);
    let at = world_to_screen(camera, corner.0, corner.1);
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(press.x, press.y, false, false);
    engine.move_pointer(at.x, at.y, false, false);
    engine.end_pointer();
    let element = engine.get_scene().pop().expect("no shape was drawn");
    // Whichever edge the corner ended up on: the box is normalised, so the corner is the
    // far edge when it is **beyond** the press and the near edge when it is short of it.
    let landed_x = if corner.0 > PRESS.0 {
        element.x + element.width
    } else {
        element.x
    };
    let landed_y = if corner.1 > PRESS.1 {
        element.y + element.height
    } else {
        element.y
    };
    (landed_x - corner.0, landed_y - corner.1, camera)
}

/// Every case: (targets, corner, scale).
fn lattice() -> Vec<(Vec<DrawElement>, (f64, f64), f64)> {
    let scales = [1.0, 2.0, 0.5, 3.0];
    let mut cases = Vec::new();
    // One candidate, walked across each of its three stops on x and on y, on both sides,
    // at exactly the reach and a hundredth either side of it.
    for scale in scales {
        for (stop_x, stop_y) in [(0.0, 0.0), (50.0, 40.0), (100.0, 80.0)] {
            for sign in [-1.0, 1.0] {
                for delta in [0.0, 0.01, -0.01, 1.0, -1.0, 5.99, 6.01] {
                    let one = vec![filled(box_at(0.0, 0.0, 100.0, 80.0))];
                    cases.push((one.clone(), (stop_x + sign * delta, 300.0), scale));
                    cases.push((one, (300.0, stop_y + sign * delta), scale));
                }
            }
        }
    }
    // A busier board: candidates on one row and a second row below it, so "nearest" has
    // to choose rather than merely find, and a candidate 400 away must never win.
    let busy = vec![
        filled(box_at(0.0, 0.0, 100.0, 80.0)),
        filled(box_at(150.0, 0.0, 100.0, 80.0)),
        filled(box_at(0.0, 150.0, 100.0, 80.0)),
    ];
    for scale in scales {
        for corner in [
            (3.0, 40.0),
            (53.0, 40.0),
            (147.0, 40.0),
            (203.0, 40.0),
            (40.0, 3.0),
            (40.0, 153.0),
            (3.0, 153.0),
        ] {
            cases.push((busy.clone(), corner, scale));
        }
    }
    cases
}

/// **Invariant 1: a snap never moves a point further than the reach.** The strongest form
/// available here — a landing point is either exactly the raw pointer or exactly a
/// candidate's stop, and the distance between the two is the whole of the snap.
#[test]
fn a_snap_never_moves_a_point_further_than_the_threshold() {
    let mut checked = 0;
    for (targets, corner, scale) in lattice() {
        let (dx, dy, camera) = drawn_offset(&targets, corner, scale);
        let threshold = reach(scale);
        assert!(
            dx.abs() <= threshold + 1e-9 && dy.abs() <= threshold + 1e-9,
            "moved ({dx}, {dy}) at {scale}× with a {threshold} reach, corner {corner:?}"
        );
        assert!(
            targets.iter().any(|t| on_screen(bounds_of(t), camera)),
            "the case is degenerate: nothing on screen at {camera:?}"
        );
        checked += 1;
    }
    assert!(
        checked > 350,
        "the sweep is too small to be evidence: {checked}"
    );
}

/// **Invariant 2: the candidate is the nearest under the oracle's own metric.** The second
/// implementation says where the point should have gone; the engine says where it did.
#[test]
fn the_chosen_candidate_is_the_nearest_one() {
    let mut checked = 0;
    for (targets, corner, scale) in lattice() {
        let (dx, dy, camera) = drawn_offset(&targets, corner, scale);
        let (want_x, want_y) = oracle_landing(&targets, camera, corner, reach(scale));
        assert_close_msg(dx, want_x, format!("x at {scale}×, corner {corner:?}"));
        assert_close_msg(dy, want_y, format!("y at {scale}×, corner {corner:?}"));
        checked += 1;
    }
    assert!(
        checked > 350,
        "the sweep is too small to be evidence: {checked}"
    );
}

/// **The reach is in screen pixels**, so the same world-unit miss is in reach at one scale
/// and out of reach at another. 10 units short of the right edge at 100: out of reach at
/// 1×, 2× and 3×, and well inside it at 0.5× — where the reach is 12 world units.
#[test]
fn the_reach_grows_and_shrinks_with_the_zoom() {
    let targets = vec![filled(box_at(0.0, 0.0, 100.0, 80.0))];
    for (scale, want_dx) in [(1.0, 0.0), (2.0, 0.0), (0.5, 10.0), (3.0, 0.0)] {
        let (dx, _, _) = drawn_offset(&targets, (90.0, 40.0), scale);
        assert_close_msg(
            dx,
            want_dx,
            format!("at {scale}×, a reach of {}", reach(scale)),
        );
    }
}

/// **A fractional pan does not change what snaps**, only where it is on screen. The same
/// corner, the same candidate and the same answer under four pans with a fractional scale.
#[test]
fn a_fractional_pan_changes_nothing_about_the_answer() {
    let targets = vec![filled(box_at(0.0, 0.0, 100.0, 80.0))];
    for pan in [(0.0, 0.0), (17.5, -9.25), (-0.5, 0.5), (211.125, 87.875)] {
        let landed = near_corner_x(
            &targets,
            Camera {
                x: pan.0,
                y: pan.1,
                scale: 1.0,
            },
            103.0,
        );
        assert_close_msg(landed, 100.0, format!("panned by {pan:?}"));
    }
}

/// Draws from a press in the **lower right** to a corner in the upper left, so the corner
/// is the box's near edge and its landing point is the element's own `x`. The two tests
/// that set their own camera use this rather than repeating the gesture five times.
fn near_corner_x(targets: &[DrawElement], camera: Camera, corner_x: f64) -> f64 {
    let mut engine = engine_with_scene(targets.to_vec());
    engine.set_viewport(800.0, 600.0, camera.scale);
    engine.set_camera(camera);
    engine.set_objects_snap(true);
    let press = world_to_screen(camera, 600.0, 600.0);
    let at = world_to_screen(camera, corner_x, 40.0);
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(press.x, press.y, false, false);
    engine.move_pointer(at.x, at.y, false, false);
    engine.end_pointer();
    engine.get_scene().pop().expect("no shape was drawn").x
}

/// **The tie.** Two candidates offering the same offset must give that offset — not a
/// blend, and not a wobble. And the reach is inclusive: 6 away is in, 6.01 is out.
#[test]
fn a_tie_is_a_tie_and_the_threshold_is_inclusive() {
    let two = vec![
        filled(box_at(0.0, 0.0, 100.0, 80.0)),
        filled(box_at(200.0, 0.0, 100.0, 80.0)),
    ];
    for corner in [(3.0, 40.0), (203.0, 40.0)] {
        let (dx, _, _) = drawn_offset(&two, corner, 1.0);
        assert_close_msg(dx, -3.0, format!("a tie at {corner:?}"));
    }
    for (corner, want) in [
        (93.99, 93.99),
        (94.0, 100.0),
        (105.99, 100.0),
        (106.01, 106.01),
    ] {
        let (dx, _, _) = drawn_offset(&two, (corner, 40.0), 1.0);
        assert_close_msg(corner + dx, want, format!("the reach boundary at {corner}"));
    }
}

/// **A reference off screen is not a candidate.** The oracle culls to the visible
/// elements for two reasons — a guide to something nobody can see is worse than no guide,
/// and gathering from the whole document costs the size of the board on every frame of
/// every drag. Both are the same rule, and this is it: 3 units from the left edge at 0,
/// with the whole box 900 units off the left of the viewport.
#[test]
fn a_reference_off_screen_is_not_a_candidate() {
    let far_left = vec![filled(box_at(-900.0, 0.0, 100.0, 80.0))];
    let camera = Camera {
        x: 0.0,
        y: 0.0,
        scale: 1.0,
    };
    assert!(
        !on_screen(bounds_of(&far_left[0]), camera),
        "setup: off screen"
    );
    let landed = near_corner_x(&far_left, camera, 3.0);
    assert_close_msg(landed, 3.0, "nothing on screen, so nothing to snap to");
}
