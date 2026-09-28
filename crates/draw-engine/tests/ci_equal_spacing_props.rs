//! Properties of equal-spacing guides, over generated boards rather than chosen ones.
//!
//! The properties here are the ones a hand-picked board cannot establish: they must hold
//! for *every* board the sweep produces, and each of them has caught a class of bug that a
//! single example cannot.
//!
//! - **P1 equal spacing is transitive**: a box centred in a gap leaves two gaps of the same
//!   length, and the two guide segments plus the box's own width **tile** the gap exactly.
//!   This is the invariant the brief calls transitivity, stated as arithmetic, because a
//!   "which pair wins" question has no answer until the arithmetic is checked.
//! - **P2 nothing is drawn beyond reach**: every guide in a result came from a candidate
//!   whose offset was within the threshold. This is the invariant a `<` in place of a `<=`
//!   breaks, and it is checked at and just past the boundary rather than at one value.
//! - **P3 a guide is between the boxes it connects**: every gap guide's span lies within
//!   the union of the two references and the dragged box, so a guide can never be the
//!   thing that moved something.
//! - **P4 the axis is independent**: an x decision never changes dy and a y decision never
//!   changes dx, which is `nearestSnapsX[0]` and `nearestSnapsY[0]` taken independently
//!   (`snapping.ts:751-754`).
//! - **P5 a tighter threshold can only remove guides**: sweeping the tolerance down must
//!   never add a guide. A non-monotone threshold is how an off-by-one in the reach test
//!   hides.
//!
//! ## The sweep is guarded, because a sweep that found nothing is not evidence
//!
//! `the_sweep_actually_exercised_the_property` asserts that the boards really did produce
//! equal-spacing guides, at both ends of the threshold boundary, and that some of them
//! were rejected. A sweep that silently degenerated to "no candidates anywhere" would pass
//! P2 and P3 for free. That has happened once already tonight.

mod common;
use common::*;
use draw_engine::*;

fn b(x: f64, y: f64, w: f64, h: f64) -> WorldBounds {
    WorldBounds {
        min_x: x,
        min_y: y,
        max_x: x + w,
        max_y: y + h,
    }
}

/// The guides that run **along x**, as `(y, x_from, x_to)`. `SnapGuide.axis` is the axis
/// `at` is measured on and the line runs across the other one, so a horizontal
/// equal-spacing guide is an `Axis::Y` guide (`wasm/paint.rs:1997-2013`).
fn along_x(res: &SnapResult) -> Vec<(f64, f64, f64)> {
    res.guides
        .iter()
        .filter(|g| g.axis == Axis::Y)
        .map(|g| (g.at, g.from, g.to))
        .collect()
}

/// Where a gap of `length` starting at `start_min` wants a `wide` box, on one axis.
///
/// The oracle's three candidates (`snapping.ts:485-542`). Only the **centre** is gated on
/// the gap being wider than the box (`:483`, `gapIsLargerThanSelection`) — when it is not,
/// the centre branch falls through and the two sides are still tried, so a narrow gap has
/// two landings and not zero. That asymmetry is the kind of thing a helper written from the
/// shape of the code rather than from the code gets wrong, and this test caught it.
fn equal_spacing_landings(start_min: f64, length: f64, wide: f64) -> Vec<f64> {
    let mut out = Vec::new();
    if length > wide {
        out.push(start_min + length / 2.0 - wide / 2.0);
    }
    out.push(start_min + length + length);
    out.push(start_min - length - wide);
    out
}

/// The bounding range every box shares on one axis, given as a picker.
fn union(
    statics: &[WorldBounds],
    moving: &WorldBounds,
    pick: fn(&WorldBounds) -> (f64, f64),
) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in statics.iter().chain(std::iter::once(moving)) {
        let (a, b2) = pick(s);
        lo = lo.min(a);
        hi = hi.max(b2);
    }
    (lo, hi)
}

/// A deterministic pseudo-random board, so a failure is reproducible from the seed alone.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// P1: a box the guides put exactly in the middle of a gap leaves equal gaps on both
/// sides, and the two guide segments plus the box tile the gap with no gap and no overlap.
#[test]
fn equal_spacing_is_transitive_across_a_sweep() {
    let mut rng = Rng(11_243_601);
    let mut checked = 0usize;
    for _ in 0..400 {
        let start_min = rng.between(0.0, 300.0).floor();
        let length = rng.between(20.0, 200.0).floor();
        let wide = rng.between(10.0, 90.0).floor();
        if length <= wide {
            continue;
        }
        let statics = [
            b(start_min - 40.0, 0.0, 40.0, 40.0),
            b(start_min + length, 0.0, 40.0, 40.0),
        ];
        // The equal-spacing landing for the centre candidate, dropped one unit short.
        let wanted = start_min + length / 2.0 - wide / 2.0;
        let res = snap_move(b(wanted, 10.0, wide, 40.0), &statics, 6.0);
        let landed = wanted + res.dx;
        let left = landed - (start_min - 40.0 + 40.0);
        let right = (start_min + length) - (landed + wide);
        assert_close_msg(left, right, "a centred box leaves two equal gaps");
        assert_close_msg(left + right, length - wide, "and they fill the gap exactly");
        checked += 1;
    }
    assert!(
        checked > 150,
        "the sweep degenerated: only {checked} boards"
    );
}

/// P1 on the guides themselves: the pair must tile `start.max .. end.min` with the box in
/// the middle, with nothing outside and nothing doubled.
#[test]
fn the_centre_guides_tile_the_gap_with_the_box() {
    let mut rng = Rng(11_243_602);
    let mut checked = 0usize;
    for _ in 0..400 {
        let start_min = rng.between(0.0, 300.0).floor();
        let length = rng.between(20.0, 200.0).floor();
        let wide = rng.between(10.0, 90.0).floor();
        if length <= wide {
            continue;
        }
        let statics = [
            b(start_min - 40.0, 0.0, 40.0, 40.0),
            b(start_min + length, 0.0, 40.0, 40.0),
        ];
        let wanted = start_min + length / 2.0 - wide / 2.0;
        let res = snap_move(b(wanted, 10.0, wide, 40.0), &statics, 6.0);
        if along_x(&res).is_empty() {
            continue;
        }
        let landed = wanted + res.dx;
        let gap_from = start_min; // the start reference's max_x
        let gap_to = start_min + length; // the end reference's min_x
        let guides = along_x(&res);
        let (a, b_side) = (guides[0], guides[1]);
        assert_close_msg(
            a.1,
            gap_from,
            "the first guide starts at the gap's near edge",
        );
        assert_close_msg(b_side.2, gap_to, "the second ends at its far edge");
        assert_close_msg(a.2, landed, "the first ends where the box starts");
        assert_close_msg(
            b_side.1,
            landed + wide,
            "the second starts where the box ends",
        );
        assert_eq!(a.0, b_side.0, "both segments are drawn on one line");
        assert!(
            a.1 <= a.2 && b_side.1 <= b_side.2,
            "a segment runs forwards"
        );
        checked += 1;
    }
    assert!(
        checked > 150,
        "the sweep degenerated: only {checked} boards"
    );
}

/// P2: at and just past the boundary, a candidate is in reach at exactly the threshold
/// and out of reach a hair beyond it — and nothing is drawn for the second.
///
/// The board is constrained so that **no point snap can be within reach** of the dragged
/// box's stops. A gap of at least 96 is what buys that: a 40-wide box sitting in the
/// middle of it has all three of its stops at least 8 from every reference stop. The
/// first version of this test swept 30..60 and read 3.5, and then −3, where it expected 6
/// — both times a point candidate, answering on the gap's behalf and shrinking the reach
/// before the gap was ever offered.
#[test]
fn no_guide_is_ever_produced_beyond_the_tolerance() {
    let mut rng = Rng(11_243_603);
    let mut at_edge = 0usize;
    let mut just_out = 0usize;
    for _ in 0..300 {
        let start_min = (rng.between(0.0, 200.0) / 2.0).floor() * 2.0;
        let length = (rng.between(96.0, 200.0) / 2.0).floor() * 2.0;
        let wide = 40.0;
        let statics = [
            b(start_min - 40.0, 0.0, 40.0, 40.0),
            b(start_min + length, 0.0, 40.0, 40.0),
        ];
        let wanted = start_min + length / 2.0 - wide / 2.0;
        let on = snap_move(b(wanted - 6.0, 10.0, wide, 40.0), &statics, 6.0);
        let off = snap_move(b(wanted - 6.5, 10.0, wide, 40.0), &statics, 6.0);
        assert_close_msg(on.dx, 6.0, "exactly at the threshold it still snaps");
        assert!(
            !along_x(&on).is_empty(),
            "and the guide that says so is drawn"
        );
        assert_close_msg(off.dx, 0.0, "6.5 is out of reach");
        assert!(
            along_x(&off).is_empty(),
            "a guide implies a move on its own axis"
        );
        at_edge += 1;
        just_out += 1;
    }
    assert!(
        at_edge > 100,
        "the sweep degenerated: {at_edge} at the edge"
    );
    assert!(
        just_out > 100,
        "the sweep degenerated: {just_out} just outside"
    );
}

/// P3: a guide is drawn **at** a coordinate on its own axis and **across** a span on the
/// other, so the two halves are checked against different unions. Asserting a cross-axis
/// span against x bounds — which the first version of this test did, and it failed on a
/// perfectly good 6.3 point guide — is the mistake this property is here to prevent.
#[test]
fn a_guide_never_reaches_outside_the_boxes_it_connects() {
    let mut rng = Rng(11_243_604);
    let mut checked = 0usize;
    for _ in 0..400 {
        let n = 3 + (rng.next() * 3.0) as usize;
        let statics: Vec<WorldBounds> = (0..n)
            .map(|_| {
                b(
                    rng.between(0.0, 600.0).floor(),
                    rng.between(0.0, 3.0).floor() * 20.0,
                    rng.between(20.0, 90.0).floor(),
                    40.0,
                )
            })
            .collect();
        let moving = b(
            rng.between(0.0, 600.0).floor(),
            rng.between(0.0, 3.0).floor() * 20.0,
            rng.between(10.0, 80.0).floor(),
            40.0,
        );
        let res = snap_move(moving, &statics, 6.0);
        // The union is widened by the threshold on both sides, because a guide is drawn at
        // where the box **landed** and the landing is up to the threshold away from where
        // it was. Without the widening the first board that snapped a guide failed this
        // with a span of 449.5 against a union ending at 444 — a correct guide, measured
        // against the wrong box.
        let (x_lo, x_hi) = union(&statics, &moving, |s| (s.min_x, s.max_x));
        let (y_lo, y_hi) = union(&statics, &moving, |s| (s.min_y, s.max_y));
        let (x_lo, x_hi) = (x_lo - 6.0, x_hi + 6.0);
        let (y_lo, y_hi) = (y_lo - 6.0, y_hi + 6.0);
        for g in res.guides.iter() {
            // `at` is on `axis`; the span is on the other. Checking the other way round is
            // how a perfectly good 6.3 point guide failed this test in its first version.
            let (at_lo, at_hi) = match g.axis {
                Axis::X => (x_lo, x_hi),
                Axis::Y => (y_lo, y_hi),
            };
            let (span_lo, span_hi) = match g.axis {
                Axis::X => (y_lo, y_hi),
                Axis::Y => (x_lo, x_hi),
            };
            assert!(
                g.at >= at_lo - 1e-9 && g.at <= at_hi + 1e-9,
                "guide drawn at {} outside {at_lo}..{at_hi} on {:?}",
                g.at,
                g.axis
            );
            assert!(
                g.from >= span_lo - 1e-9 && g.to <= span_hi + 1e-9,
                "guide spans {}..{} outside {span_lo}..{span_hi} on {:?}",
                g.from,
                g.to,
                g.axis
            );
            assert!(g.from <= g.to, "a segment runs forwards");
            checked += 1;
        }
    }
    assert!(checked > 200, "the sweep degenerated: {checked} guides");
}

/// P4: the two axes are decided independently.
#[test]
fn the_two_axes_are_decided_independently() {
    let mut rng = Rng(11_243_605);
    let mut checked = 0usize;
    for _ in 0..400 {
        let statics = vec![
            b(0.0, 0.0, 40.0, 40.0),
            b(300.0, 0.0, 40.0, 40.0),
            b(600.0, 0.0, 40.0, 40.0),
        ];
        let _ = &mut rng;
        // Move on x only: the y decision must be a no-op and vice versa.
        let a = snap_move(
            b(rng.between(0.0, 400.0).floor(), 500.0, 40.0, 40.0),
            &statics,
            6.0,
        );
        assert_close_msg(a.dy, 0.0, "no y candidate, so no y move");
        let c = snap_move(
            b(1000.0, rng.between(0.0, 200.0).floor(), 40.0, 40.0),
            &statics,
            6.0,
        );
        assert_close_msg(c.dx, 0.0, "no x candidate, so no x move");
        checked += 1;
    }
    assert_eq!(checked, 400);
}

/// P5: shrinking the threshold never adds a guide.
#[test]
fn a_tighter_threshold_only_removes_guides() {
    let mut rng = Rng(11_243_606);
    let mut checked = 0usize;
    for _ in 0..200 {
        let statics: Vec<WorldBounds> = (0..4)
            .map(|_| {
                b(
                    rng.between(0.0, 400.0).floor(),
                    rng.between(0.0, 2.0).floor() * 40.0,
                    rng.between(20.0, 60.0).floor(),
                    40.0,
                )
            })
            .collect();
        let moving = b(
            rng.between(0.0, 400.0).floor(),
            rng.between(0.0, 2.0).floor() * 40.0,
            rng.between(10.0, 50.0).floor(),
            40.0,
        );
        let loose = snap_move(moving, &statics, 12.0);
        let tight = snap_move(moving, &statics, 3.0);
        if loose.guides.len() < tight.guides.len() {
            panic!(
                "a tighter threshold added guides: {} loose vs {} tight",
                loose.guides.len(),
                tight.guides.len()
            );
        }
        if tight.dx.abs() > 3.0 + 1e-9 || tight.dy.abs() > 3.0 + 1e-9 {
            panic!(
                "a tighter threshold still moved {} by {}",
                tight.dx, tight.dy
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 200);
}

/// The guard: without it, every property above also passes on a function that returns
/// nothing at all. This asserts the sweep reached the boundary from both sides and that
/// real equal-spacing decisions — both the centre candidate and a side candidate — were
/// among them.
#[test]
fn the_sweep_actually_exercised_the_property() {
    // A row of three with 60-unit gaps; a 40-wide box belongs at 50, 200 and -100.
    let statics = [
        b(0.0, 0.0, 40.0, 40.0),
        b(100.0, 0.0, 40.0, 40.0),
        b(200.0, 0.0, 40.0, 40.0),
    ];
    let centre = snap_move(b(47.0, 10.0, 40.0, 40.0), &statics, 6.0);
    assert_close_msg(
        centre.dx,
        3.0,
        "47 belongs at 50, the centre of the 0..100 gap",
    );
    assert_eq!(
        along_x(&centre),
        vec![(25.0, 40.0, 50.0), (25.0, 90.0, 100.0)]
    );
    // The "ahead" landing after B1 is end.maxX + L = 140 + 60 = 200, which the third box
    // occupies, so use a two-box board: 140 + 60 = 200 is still free there, and a box at
    // 198 is 2 from it — 60 - (198 - 140) = 2.
    let pair = [b(0.0, 0.0, 40.0, 40.0), b(100.0, 0.0, 40.0, 40.0)];
    let ahead = snap_move(b(198.0, 10.0, 40.0, 40.0), &pair, 6.0);
    assert_close_msg(ahead.dx, 2.0, "198 belongs at 200, a side candidate");
    // And one rejection, so "no guides" is known to be reachable rather than universal.
    let nowhere = snap_move(b(1000.0, 1010.0, 40.0, 40.0), &pair, 6.0);
    assert!(nowhere.guides.is_empty(), "an empty result is reachable");
    assert_close(nowhere.dx, 0.0);
    assert_close(nowhere.dy, 0.0);
}

/// The `equal_spacing_landings` helper the other properties reason about is itself the
/// oracle's three candidates, so it gets its own check against hand arithmetic.
#[test]
fn the_three_landings_are_the_ones_written_down() {
    // A gap of 100 starting at 40, a 40-wide box.
    //   centre  40 + 50 - 20 = 70   (gated on 100 > 40, which holds)
    //   ahead   40 + 100 + 100 = 240
    //   behind  40 - 100 - 40 = -100
    assert_eq!(
        equal_spacing_landings(40.0, 100.0, 40.0),
        vec![70.0, 240.0, -100.0]
    );
    // A gap of 30 with a 40-wide box: the centre is refused, the two sides are not.
    //   ahead   40 + 30 + 30 = 100
    //   behind  40 - 30 - 40 = -30
    assert_eq!(equal_spacing_landings(40.0, 30.0, 40.0), vec![100.0, -30.0]);
}
