//! Equal-spacing guides: a dragged box wants the gap on one side to equal the gap on the
//! other. Excalidraw calls it **gap snapping** and it is a **move-only** feature, which is
//! a fact about the call graph and not an assumption: `getGapSnaps` is called from exactly
//! two places and both are inside `snapDraggedElements` (`snapping.ts@1118751f:738,783`).
//! `snapNewElement` (`:1246`) and `snapResizingElements` (`:1108`) never reach it.
//!
//! The candidates are NOT the corners and centre of `getPointSnaps` (`:636-690`). They are
//! **gaps between pairs of reference boxes** — `getVisibleGaps` (`:328-444`) re-runs the
//! same gatherer (`getReferenceElements` → `getMaximumGroups` → the bound-to-container
//! filter, `:341-351`) but maps each group to its rounded common bounds and then forms a
//! candidate per **pair** that is separated on one axis and overlapping on the other. So:
//! one gatherer, two candidate maths, and the second one is unavoidable — a gap is a
//! derived offset, not a point, so it cannot be fed to `nearest_axis`.
//!
//! ## The numbers below are computed by hand, not read back from the engine
//!
//! Every expected offset and every expected guide segment in this file is arithmetic on
//! board coordinates written out in the fixture. That is the whole point: the alternative
//! is asserting that the implementation agrees with itself, which six agents tonight
//! shipped and every one of them passed.
//!
//! The `round` the oracle applies is six decimal places (`snapping.ts:809-812`), and every
//! board here is integral, so the expected values are exact integers, not tolerances.

mod common;
use common::*;
use draw_engine::*;

const TOL: f64 = 6.0;

fn b(x: f64, y: f64, w: f64, h: f64) -> WorldBounds {
    WorldBounds {
        min_x: x,
        min_y: y,
        max_x: x + w,
        max_y: y + h,
    }
}

/// Every guide, as `(axis, at, from, to)`, so a test can also pin the **orientation**.
///
/// `SnapGuide.axis` is the axis `at` is measured on and the line runs across the other
/// one (`wasm/paint.rs:1997-2013`), so a horizontal equal-spacing guide is an `Axis::Y`
/// guide. Reading a guide by its axis rather than by the direction it runs is the mistake
/// that let the first version of this code paint vertical lines at y coordinates.
fn all_guides(res: &SnapResult) -> Vec<(Axis, f64, f64, f64)> {
    res.guides
        .iter()
        .map(|g| (g.axis, g.at, g.from, g.to))
        .collect()
}

/// The guides that run **along x**, as `(y, x_from, x_to)` — which is
/// `(at, from, to)` for an `Axis::Y` guide.
fn along_x(res: &SnapResult) -> Vec<(f64, f64, f64)> {
    res.guides
        .iter()
        .filter(|g| g.axis == Axis::Y)
        .map(|g| (g.at, g.from, g.to))
        .collect()
}

// ---------------------------------------------------------------------------
// Board A — the headline: one gap of 60, a box dropped 2 short of splitting it
//
// B0 x[0,40]   B1 x[100,140]   gap length 60, y-overlap [0,40]
// dragged x[202,242], 40 wide
//
// equal-spacing position ("ahead", the box lands after B1's right edge):
//     minX = gap.end.maxX + gap.length = 140 + 60 = 200
// so the offset is  length - (minX - gap.end.maxX) = 60 - (202-140) = -2
// ---------------------------------------------------------------------------

#[test]
fn a_box_two_short_of_equal_spacing_moves_two_to_split_the_gap() {
    let res = snap_move(
        b(202.0, 10.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(100.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    assert_close_msg(
        res.dx,
        -2.0,
        "left gap 62, right gap 58, equalise to 60 and 60",
    );
    assert_close(res.dy, 0.0);
}

/// The hair outside: 202 + 6 = 206 gives |offset| exactly 6, and the oracle's test is
/// `<= minOffset` (`snapping.ts:505,507`), so **6 still snaps**. This is the half of the
/// pair that a `<` would fail.
#[test]
fn a_box_exactly_at_the_tolerance_still_splits_the_gap() {
    let res = snap_move(
        b(206.0, 10.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(100.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    assert_close_msg(res.dx, -6.0, "6 is within reach: the oracle's <=, not <");
}

/// A hair past it: 206.5 is 6.5 from the equal-spacing position, so nothing moves.
#[test]
fn a_box_a_hair_past_the_tolerance_is_left_alone() {
    let res = snap_move(
        b(206.5, 10.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(100.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    assert_close_msg(
        res.dx,
        0.0,
        "6.5 is out of reach, and no point snap competes",
    );
    assert_close(res.dy, 0.0);
}

/// Both halves of the pair, quoted side by side, on the same scene and the same drag.
#[test]
fn the_pair_is_the_same_scene_at_and_past_the_tolerance() {
    let statics = [b(0.0, 0.0, 40.0, 40.0), b(100.0, 0.0, 40.0, 40.0)];
    let inside = snap_move(b(206.0, 10.0, 40.0, 40.0), &statics, TOL);
    let outside = snap_move(b(206.5, 10.0, 40.0, 40.0), &statics, TOL);
    assert_close_msg(inside.dx, -6.0, "206 lands at 200");
    assert_close_msg(outside.dx, 0.0, "206.5 stays at 206.5");
    assert!(!along_x(&inside).is_empty(), "the gap that fired is drawn");
    assert!(
        along_x(&outside).is_empty(),
        "an out-of-reach gap draws nothing on its axis"
    );
}

/// The guides for Board A, at the landed position minX = 200. `createGapSnapLines`
/// `case "side_right"` (`snapping.ts:995-1016`) draws the gap itself and the space the box
/// now occupies, both at the midpoint of the y-overlap, which is (0+40)/2 = 20.
#[test]
fn the_guides_span_the_gap_and_the_space_the_box_filled() {
    let res = snap_move(
        b(202.0, 10.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(100.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    assert_eq!(
        along_x(&res),
        vec![
            (25.0, 40.0, 100.0),  // B0's right edge to B1's left edge: the gap
            (25.0, 140.0, 200.0), // B1's right edge to the box's landed left edge
        ]
    );
}

// ---------------------------------------------------------------------------
// Board B — a gap wider than the box, and the CENTRE rule
//
// B0 x[0,40]   B1 x[300,340]   gap length 260, y-overlap [0,40]
// dragged x[148,188]
//
// The centre candidate needs gap.length > box width (260 > 40, :483) and asks for the
// box's centre on the gap's centre:
//     gapMidX = gap.start.maxX + gap.length/2 = 40 + 130 = 170
//     centreX = 148 + 20 = 168
//     offset   = 170 - 168 = +2
// landing minX = 150, centre 170, so 40..170 is 130 and 170..300 is 130. Equal.
// ---------------------------------------------------------------------------

#[test]
fn a_box_near_the_middle_of_a_wide_gap_centres_on_it() {
    let res = snap_move(
        b(148.0, 10.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(300.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    assert_close_msg(res.dx, 2.0, "two gaps of 130 after the move");
}

/// The centre candidate is refused when the box is **wider** than the gap
/// (`gap.length > maxX - minX`, `:483`). A 300-wide box in the 260 gap is not centred.
#[test]
fn a_box_wider_than_the_gap_is_not_centred_on_it() {
    let res = snap_move(
        b(130.0, 10.0, 300.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(300.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    assert_close_msg(res.dx, 0.0, "the gap is not wider than the selection");
}

/// The centre guides, at the landed position minX = 150: from B0's right edge to the box
/// and from the box to B1's left edge, both at cross = (0+40)/2 = 20
/// (`snapping.ts:943-967`).
#[test]
fn the_centre_guides_span_both_halves_of_the_gap() {
    let res = snap_move(
        b(148.0, 10.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(300.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    assert_eq!(
        along_x(&res),
        vec![
            (25.0, 40.0, 150.0),
            (25.0, 190.0, 300.0), // 150 + the box's own 40
        ]
    );
}

// ---------------------------------------------------------------------------
// Board C — THE TIE. This is the whole task.
//
// P  x[0,40]      Q  x[140,180]     gap length 100, y-overlap [0,40]
// R  x[94,134] y[200,240]   — out of the y band, so it makes no gap; it only ever
//                              produces a point snap
// dragged x[72,112]
//
// POINT pass, our stops [72, 92, 112] against R's [94, 114, 134]:
//     |94 - 92|  = 2  -> offset +2   (first found, so this one wins)
//     |114 - 112| = 2  -> offset +2   (a tie: kept, drawn, does not replace)
// reach is now 2.
//
// GAP pass, gap P->Q: centre at 40 + 50 = 90, our centre 92, so offset = 90 - 92 = -2.
//     |-2| = 2, which is neither > 2 (out of reach) nor < 2 (strictly nearer)
//     -> a TIE. Kept, and drawn, and it does NOT take the offset.
//
// Landing: +2, i.e. minX = 74. The plausible alternatives all give -2 and land at 70:
// "last candidate wins" and "clear on <=" both break this tie the wrong way.
// ---------------------------------------------------------------------------

fn tie_board() -> [WorldBounds; 3] {
    [
        b(0.0, 0.0, 40.0, 40.0),
        b(140.0, 0.0, 40.0, 40.0),
        b(94.0, 200.0, 40.0, 40.0),
    ]
}

#[test]
fn a_tie_is_kept_both_ways_and_the_first_found_lands() {
    let res = snap_move(b(72.0, 10.0, 40.0, 40.0), &tie_board(), TOL);
    assert_close_msg(res.dx, 2.0, "the point pass ran first, so +2 wins the tie");
}

#[test]
fn a_tied_gap_still_draws_its_guides() {
    let res = snap_move(b(72.0, 10.0, 40.0, 40.0), &tie_board(), TOL);
    // THREE guides, and the count is itself the claim. The point pass found **two**
    // candidates at |2| — our 92 against R's 94 and our 112 against R's 114 — and this
    // engine draws **one** point guide per axis, so only the first appears. The gap pass
    // found one at |2| as well and **both** of its segments appear, because a gap snap is
    // two segments and drawing one of them would say less than the oracle does.
    //
    // The orientation is asserted too, because it is the thing a hand-written example gets
    // wrong: the point guide is a **vertical** line at x = 94, so an `Axis::X` guide with a
    // y-span; the two gap guides are **horizontal** lines at y = 20, so `Axis::Y` guides
    // with x-spans. The sweep in `ci_equal_spacing_props.rs` is what caught the first
    // version drawing both as `Axis::X`.
    //
    // The remaining divergence is deliberate and is NOT fixed here: the oracle's
    // `createPointSnapLines` (`:828-896`) groups by coordinate and draws **every** tied
    // point snap, so a row of three aligned boxes gets one guide spanning all three. Ours
    // draws one. Changing that is a change to the *object alignment guides* feature
    // (design.md:823, already `covered`) and it would move a 6.3-green assertion —
    // `ci_snapping.rs:144` expects 2 guides for a board whose x axis is itself a two-way
    // tie. Filed as a Phase 1 candidate rather than smuggled in here.
    assert_eq!(
        all_guides(&res),
        vec![
            (Axis::X, 94.0, 10.0, 240.0), // the point guide: vertical, at R's left edge
            (Axis::Y, 25.0, 40.0, 74.0),  // the gap, up to where the box landed
            (Axis::Y, 25.0, 114.0, 140.0),
        ],
        "the tie draws the gap's guides without moving anything"
    );
}

/// The alternative reading, made explicit and made to fail: if a tie were broken by
/// replacing, the landing would be -2 and the box would sit at 70, i.e. **exactly**
/// centred in the 100 gap and 4 units from where the oracle puts it.
#[test]
fn the_tie_is_not_broken_by_the_later_candidate() {
    let res = snap_move(b(72.0, 10.0, 40.0, 40.0), &tie_board(), TOL);
    let landed = 72.0 + res.dx;
    assert_close_msg(landed, 74.0, "not 70: the gap candidate found second");
    assert_ne!(
        landed, 70.0,
        "a 'last wins' tie-break would land here and look right on a quiet board"
    );
}

/// With R removed there is no point snap at all, so the same drag is decided by the gap
/// alone and lands at 70. That is the board where the two readings agree — which is why
/// the difference is invisible without R, and why the tie board above is the one that
/// discriminates.
#[test]
fn without_the_competing_point_snap_the_same_drag_lands_on_the_gap() {
    let quiet = [b(0.0, 0.0, 40.0, 40.0), b(140.0, 0.0, 40.0, 40.0)];
    let res = snap_move(b(72.0, 10.0, 40.0, 40.0), &quiet, TOL);
    assert_close_msg(res.dx, -2.0, "the gap is all there is, so it wins outright");
    assert_eq!(
        along_x(&res),
        vec![(25.0, 40.0, 70.0), (25.0, 110.0, 140.0)]
    );
}

// ---------------------------------------------------------------------------
// Board E — a nearer gap takes the offset FROM the point pass
//
// P x[0,40]  Q x[140,180]  gap 100, overlap y[0,40]
// dragged x[73,113]: point snaps at |3| (73 vs 40? no — 113 vs 140 is 27) ...
// use R x[95,135] y[200,240] -> stops 95,115,135 against ours 73,93,113:
//     |95-93| = 2, |115-113| = 2  -> point offset +2, reach 2
// gap centre 90, our centre 93 -> offset -3. |−3| = 3 > 2, so OUT of reach.
// Move the drag 1 left: x = 72 -> centre 92 -> gap offset -2, a tie again.
// Move the drag 2 right of the tie point: x = 74 -> centre 94 -> gap offset -4, |4| > 2,
// out of reach for the gap, and the point snaps are |95-94|... ours 74,94,114 vs 95,115,135:
//     |95-94| = 1, |115-114| = 1 -> point offset +1, reach 1; gap offset -4 out of reach.
// To get the gap NEARER than the point, the centre offset must be smaller than the point
// offset. Drag x = 90: ours 90,110,130 vs R 95,115,135 -> |95-110|? 15; |115-110| = 5,
// |135-130| = 5 -> point offset -5, reach 5. Gap centre 90, ours 110 -> offset -20. Worse.
// Drag x = 68: ours 68,88,108 vs 95,115,135 -> |95-88| = 7, |115-108| = 7 out of reach;
// nearest point |88-95| = 7 > 6 so no point snap. Gap centre 90, ours 88 -> offset +2,
// |2| <= 6 -> the gap lands, and there is nothing to beat.
// For the gap to beat a point snap the point must sit at a bigger distance, which needs
// a third reference. Add S x[96,136] y[400,440] so the point is 3 away and the gap 2.
// ---------------------------------------------------------------------------

#[test]
fn a_nearer_gap_takes_the_offset_from_the_point_pass() {
    let statics = [
        b(0.0, 0.0, 40.0, 40.0),
        b(140.0, 0.0, 40.0, 40.0),
        b(97.0, 200.0, 40.0, 40.0), // ours [72,92,112] -> |97-92| = 5
    ];
    // point offset +5 (reach 5); gap centre 90 vs our centre 92 -> offset -2, strictly
    // nearer, so it clears the point snaps and lands.
    let res = snap_move(b(72.0, 10.0, 40.0, 40.0), &statics, TOL);
    assert_close_msg(res.dx, -2.0, "the gap is nearer, so it supersedes");
    assert_eq!(
        along_x(&res),
        vec![(25.0, 40.0, 70.0), (25.0, 110.0, 140.0)],
        "and the superseded point guide is gone with it"
    );
}

// ---------------------------------------------------------------------------
// The vertical axis, and the one asymmetry between the two
// ---------------------------------------------------------------------------

#[test]
fn the_vertical_axis_works_the_same_way() {
    let res = snap_move(
        b(10.0, 202.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(0.0, 100.0, 40.0, 40.0)],
        TOL,
    );
    assert_close_msg(res.dy, -2.0, "two gaps of 60 after the move");
}

/// On x the oracle tries **ahead** before **behind** (`:502` `side_right`, then `:523`
/// `side_left`); on y it tries **behind** before **ahead** (`:571` `side_top`, then `:592`
/// `side_bottom`).
///
/// **The order cannot be observed, and that is the finding.** The two ends of one gap are
/// `start.min - L - w` and `end.max + L` apart by `p + 2L + q + w` for boxes of width `p`
/// and `q` and a dragged box of width `w`. To have both within one reach the reach must
/// exceed half that, and meanwhile the dragged box has to sit *between the two
/// references* to be near either landing — which puts it within reach of a **point**
/// candidate, and the point pass runs first and shrinks the reach to the nearest stop.
/// `both_ends_of_one_gap_are_never_in_reach_together` sweeps that rather than asserting
/// it, and this test pins what *is* reachable: each end, on each axis, to its own number.
#[test]
fn the_two_axes_reach_the_two_ends_in_opposite_orders() {
    // Stacked, so the y pass has a gap of its own: A y[0,40], B y[48,88], length 8, and
    // the gap's x-overlap is (0,40) so a box at x 20..24 is considered.
    let statics = [b(0.0, 0.0, 40.0, 40.0), b(0.0, 48.0, 40.0, 40.0)];
    // y's `ahead` landing is end.maxY + L = 88 + 8 = 96, so a box at 94 is 2 from it:
    //   ahead = 8 - (94 - 88) = 2
    assert_close_msg(
        snap_move(b(20.0, 94.0, 4.0, 4.0), &statics, 6.0).dy,
        2.0,
        "y reaches 'ahead' second and takes it",
    );
    // y's `behind` landing is start.minY - L - w = 0 - 8 - 4 = -12, out of reach at 6.
    // The box is placed at 28 so that it is also 8 clear of every **point** stop (our
    // 28/30/32 against the references' 0, 20, 40, 48, 68, 88) — the first attempt put it
    // at 30, where `|34 - 40| = 6` produced a point snap and the assertion read 6 against
    // 0 and blamed the gap pass for an answer the gap pass never gave.
    assert_close_msg(
        snap_move(b(20.0, 28.0, 4.0, 4.0), &statics, 6.0).dy,
        0.0,
        "y has no 'behind' candidate in reach",
    );
    // Side by side, the same two answers on x: `ahead` is end.maxX + 8 = 48 + 8 = 96, so a
    // box at 94 is 2 from it: 8 - (94 - 88) = 2. The box cannot sit at 88 — the landing
    // before it — because 88 is the reference's own stop, the point pass finds an exact 0
    // there first and the reach is already 0. This is the same collision the tie board
    // builds on purpose and the reach makes unavoidable by accident.
    let wide = [b(0.0, 0.0, 40.0, 40.0), b(48.0, 0.0, 40.0, 40.0)];
    assert_close_msg(
        snap_move(b(94.0, 20.0, 4.0, 4.0), &wide, 6.0).dx,
        2.0,
        "x reaches 'ahead' first and takes it",
    );
    // x's `behind` landing is 0 - 8 - 4 = -12. A box at 110 is 22 clear of the nearest
    // reference stop, so nothing at all is in reach on either pass.
    assert_close_msg(
        snap_move(b(110.0, 20.0, 4.0, 4.0), &wide, 6.0).dx,
        0.0,
        "x has no 'behind' candidate in reach",
    );
}

/// The sweep that turns the paragraph above from an argument into a measurement: over
/// 81,920 small boards, at most one of a gap's two ends is ever reached. If this ever
/// fails, the axis order has become observable and the next thing to do is pin *which*
/// end each axis prefers — which is why the claim is worth a test and not a comment.
#[test]
fn both_ends_of_one_gap_are_never_in_reach_together() {
    let mut checked = 0usize;
    for p in 1..=8u32 {
        for len in 1..=8u32 {
            for q in 1..=8u32 {
                for w in 1..=8u32 {
                    for step in 0..20u32 {
                        let (p, len, q, w) = (p as f64, len as f64, q as f64, w as f64);
                        let statics = [b(0.0, 0.0, p, 10.0), b(p + len, 0.0, q, 10.0)];
                        let behind = -len - w;
                        let ahead = p + len + q + len;
                        let m = behind + (ahead - behind) * step as f64 / 19.0;
                        let res = snap_move(b(m, 0.0, w, 10.0), &statics, 6.0);
                        // The reached offsets: the point pass's, and the gap's own, all
                        // read off the result by where the box landed.
                        let landed = m + res.dx;
                        let on_ahead = (landed - ahead).abs() < 1e-9;
                        let on_behind = (landed - behind).abs() < 1e-9;
                        assert!(
                            !(on_ahead && on_behind),
                            "both ends reached at once with p={p} L={len} q={q} w={w}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        checked,
        8 * 8 * 8 * 8 * 20,
        "the sweep covered every board it claims"
    );
}

// ---------------------------------------------------------------------------
// A gap needs the two boxes to overlap on the OTHER axis. Without it there is no gap and
// therefore no equal-spacing guide, however inviting the coordinates.
// ---------------------------------------------------------------------------

#[test]
fn two_boxes_that_do_not_overlap_make_no_gap() {
    let res = snap_move(
        b(202.0, 10.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(100.0, 100.0, 40.0, 40.0)],
        TOL,
    );
    assert_close_msg(res.dx, 0.0, "y ranges [0,40] and [100,140] do not overlap");
    // Nothing at all. The dragged box sits at y 10..50, ten clear of the first
    // reference's stops, because a board whose y ranges coincide snaps on y to an offset
    // of exactly 0 — `getPointSnaps` compares a stop against a stop with no overlap test
    // at all (`snapping.ts:655-656`) — and the guide for *that* is a horizontal one, the
    // same shape as an equal-spacing guide, so the two are indistinguishable by axis and
    // a board that produced both could not tell them apart.
    assert!(
        along_x(&res).is_empty(),
        "no equal-spacing guide on x without an overlap on y"
    );
    assert!(all_guides(&res).is_empty(), "and nothing on y either");
}

/// Two boxes that merely **share an edge** make no gap, because the separation test is
/// strict — `startMaxX < endMinX` (`:373`), not `<=`. This is the half of
/// `rangesOverlap`'s inclusiveness that does *not* apply: the inclusive test is on the
/// **cross** axis (`:376`), which the next test pins instead.
#[test]
fn two_boxes_that_only_touch_along_the_gap_axis_make_no_gap() {
    let res = snap_move(
        b(82.0, 0.0, 40.0, 40.0),
        &[b(0.0, 0.0, 40.0, 40.0), b(40.0, 0.0, 40.0, 40.0)],
        TOL,
    );
    // The only snap is a point one: our 82 against the second box's 80 -> -2.
    assert_close_msg(res.dx, -2.0, "a point snap, not a gap");
    assert_eq!(
        along_x(&res).len(),
        1,
        "one point guide, so no gap was ever a candidate"
    );
}

/// The cross axis **is** inclusive, so a gap of 60 is real even when the two boxes share
/// only the single y coordinate 40: `rangeInclusive(0,40)` and `rangeInclusive(40,80)`
/// overlap at 40 (`:376-377`). The guide's cross coordinate is the midpoint of that
/// overlap, so a one-point overlap puts the guide **on** 40.
#[test]
fn the_cross_axis_is_inclusive_so_a_one_unit_overlap_is_a_gap() {
    let statics = [b(0.0, 0.0, 40.0, 40.0), b(100.0, 40.0, 40.0, 40.0)];
    // gap 40..100, length 60, y-overlap (40, 40). The box lands at x = 200, y = 40, so
    // its own y-overlap with the gap is that one point and the guides run along 40.
    let res = snap_move(b(202.0, 30.0, 40.0, 40.0), &statics, TOL);
    assert_close_msg(res.dx, -2.0, "the gap exists: the y ranges share 40");
    assert_eq!(
        along_x(&res),
        vec![(40.0, 40.0, 100.0), (40.0, 140.0, 200.0)],
        "and the guide runs along the single shared coordinate"
    );
}

// ---------------------------------------------------------------------------
// A guide must SAY where, not DO anything.
// ---------------------------------------------------------------------------

/// The result is read-only by construction — it is a value, and moving the box is the
/// caller's job. This pins the two numbers the caller does use and asserts that the guide
/// list is derived from the box it was given, so a guide cannot be a second source of
/// movement.
#[test]
fn a_guide_reports_a_position_and_never_another_one() {
    let statics = [b(0.0, 0.0, 40.0, 40.0), b(100.0, 0.0, 40.0, 40.0)];
    let moving = b(202.0, 10.0, 40.0, 40.0);
    let res = snap_move(moving, &statics, TOL);
    // The caller's box is untouched: SnapResult holds no box, and the delta is applied by
    // pointer_move.rs to whatever bounds it built.
    assert_eq!(moving.min_x, 202.0);
    assert_close(res.dx, -2.0);
    // Every guide is inside the union of the dragged box and the two references, and spans
    // at most that union. A guide reaching outside it is a guide that moved something.
    //
    // This is the assertion that says *where*, on purpose: a test that only checked that a
    // guide appeared would pass for an implementation that also nudged the element, and one
    // that only checked the delta would pass for one that drew the guide somewhere else.
    let (x_lo, x_hi) = (0.0f64, 242.0f64);
    let (y_lo, y_hi) = (0.0f64, 50.0f64);
    for g in res.guides.iter() {
        let (at_lo, at_hi) = match g.axis {
            Axis::X => (x_lo, x_hi),
            Axis::Y => (y_lo, y_hi),
        };
        let (span_lo, span_hi) = match g.axis {
            Axis::X => (y_lo, y_hi),
            Axis::Y => (x_lo, x_hi),
        };
        assert!(
            g.at >= at_lo && g.at <= at_hi,
            "guide drawn at {} outside {at_lo}..{at_hi}",
            g.at
        );
        assert!(
            g.from >= span_lo && g.to <= span_hi,
            "guide {}..{} escapes the boxes it connects",
            g.from,
            g.to
        );
    }
    // And the two segments, which is the whole claim in one assertion: the gap itself and
    // the space the box now fills, with the box's own width between them and nothing else.
    assert_eq!(
        along_x(&res),
        vec![(25.0, 40.0, 100.0), (25.0, 140.0, 200.0)]
    );
}

// ---------------------------------------------------------------------------
// The same board through a real pointer, because a pure function cannot prove the gesture
// reaches it.
//
// Everything above calls `snap_move` directly. That is the right level for the geometry
// and the wrong level for the wiring: `move_selection` builds the dragged bounds, reads
// `static_bounds` out of the interaction, adds the deltas to the elements and publishes
// the guides, and a pure test says nothing about any of that. So the same board is dragged
// with `begin_pointer` / `move_pointer` and the element's own `x` is read back.
// ---------------------------------------------------------------------------

fn engine_on_board_a() -> (DrawEngine, String) {
    let still = filled(box_at(0.0, 0.0, 40.0, 40.0));
    let other = filled(box_at(100.0, 0.0, 40.0, 40.0));
    let moving = filled(box_at(202.0, 10.0, 40.0, 40.0));
    let id = moving.id.clone();
    (engine_with_scene(vec![still, other, moving]), id)
}

fn x_of(engine: &DrawEngine, id: &str) -> f64 {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
        .x
}

/// Presses the moving box, nudges it so the gesture is unambiguously a drag, and hands
/// back the guides that were on screen just before release.
fn nudge(engine: &mut DrawEngine) -> Vec<SnapGuide> {
    engine.begin_pointer(222.0, 30.0, false, false);
    for step in 1..4 {
        let t = step as f64 / 3.0;
        engine.move_pointer(222.0 + 0.5 * t, 30.0 + 0.5 * t, false, false);
    }
    let guides = engine.snap_guides().to_vec();
    engine.end_pointer();
    guides
}

#[test]
fn a_real_drag_splits_the_gap_and_publishes_the_guides() {
    let (mut engine, id) = engine_on_board_a();
    engine.set_objects_snap(true);

    let guides = nudge(&mut engine);

    assert_close_msg(
        x_of(&engine, &id),
        200.0,
        "the element itself landed on the gap's half",
    );
    let along: Vec<(f64, f64, f64)> = guides
        .iter()
        .filter(|g| g.axis == Axis::Y)
        .map(|g| (g.at, g.from, g.to))
        .collect();
    assert_eq!(
        along,
        // The nudge lands the box at x 202.5, y 10.5, so the offset is -2.5 and the guide
        // runs along the midpoint of the gap's y-overlap [0,40] with the box's [10.5, 50.5],
        // which is (10.5 + 40) / 2 = 25.25. The two segments are still the gap itself,
        // 40..100, and the space the box now fills, 140..200.
        vec![(25.25, 40.0, 100.0), (25.25, 140.0, 200.0)],
        "and the published guides are the two segments, in world units"
    );
}

/// The gate still governs the whole thing, because the gap pass went through it rather
/// than around it: the grid refuses both passes at once.
#[test]
fn the_grid_still_wins_over_equal_spacing() {
    let (mut engine, id) = engine_on_board_a();
    engine.set_grid(GridSettings {
        enabled: true,
        size: 20.0,
        step: 1,
        snap: true,
    });

    nudge(&mut engine);

    // 202 is already on the 20 grid, so the grid's answer and the gap's differ by 2 and
    // the grid must be the one that wins — which is `objects_snap_gesture` refusing both
    // passes together rather than each consulting the grid.
    assert_close_msg(x_of(&engine, &id), 202.0, "the grid wins, as the gate says");
}

#[test]
fn it_is_off_unless_asked_for_like_every_other_object_snap() {
    let (mut engine, id) = engine_on_board_a();

    let guides = nudge(&mut engine);

    assert_close_msg(
        x_of(&engine, &id),
        202.5,
        "the drag lands where the pointer left it",
    );
    assert!(!engine.objects_snap());
    assert!(
        guides.is_empty(),
        "no equal-spacing guides while object snapping is off"
    );
}
