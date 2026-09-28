//! Where a line's **own endpoints** land, while the line is being placed and while an
//! existing line's endpoints are being dragged.
//!
//! # What the oracle actually snaps an endpoint to
//!
//! Two gestures, one helper. Placing a line point by point drags the *preview* point
//! (`App.tsx@1118751f:11253`) and dragging an existing line's endpoint drags a *committed*
//! one (`App.tsx@1118751f:10853`); both call
//! `LinearElementEditor.handlePointDragging`. Read together with its call graph — not with
//! a `file:line`, which is a claim — the candidate list is **two entries long**:
//!
//! 1. **the grid**, via `createPointAt` -> `getGridPoint(scenePointerX, scenePointerY,
//!    gridSize)` (`linearElementEditor.ts@1118751f:1468`), which is a **per-axis**
//!    `round(v / size) * size` (`packages/common/src/points.ts@1118751f:69-81`) — a
//!    threshold of `size / 2` on each axis, never a distance;
//! 2. **the angle lock**, via `_getShiftLockedDelta` -> `getLockedLinearCursorAlignSize`
//!    (`linearElementEditor.ts@1118751f:1895-1935`), gated on
//!    `shouldRotateWithDiscreteAngle(event) && singlePointDragged` (`:542`). It *replaces*
//!    the free branch rather than composing with it — but it grid-snaps the pointer first
//!    (`:1916`), so the two are ordered: grid, then angle.
//!
//! # The angle lock: the step, and the geometry that goes with it
//!
//! The step is `SHIFT_LOCKING_ANGLE = Math.PI / 12` — **15 degrees**
//! (`packages/common/src/constants.ts@1118751f:31`), used by both
//! `getLockedLinearCursorAlignSize` (`sizeHelpers.ts@1118751f:196-197`) and the creation
//! path's `getPerfectElementSize` (`:171-172`). It reaches this engine through
//! `constrain_to_angle`, which has **four** call sites: a dragged endpoint
//! (`engine/pointer_move.rs:399`), a preview point (`engine/multi_linear.rs:279`), a
//! drag-drawn line (`interaction/linear_drag.rs:32`) and an elbow end
//! (`engine/elbow.rs:87`). All four go through the one function, so the step and the
//! geometry are decided once, in `math::shift_locked_delta`.
//!
//! Both were wrong here until Phase 6.2, and the second one is the half that a
//! "just change the constant" fix would have left behind. This engine stepped by `PI / 4`
//! and **rotated the delta** onto the locked angle. The oracle keeps the drag's component
//! **along** the locked ray and discards the one across it — the locked ray intersected
//! with the line through the cursor perpendicular to it
//! (`sizeHelpers.ts@1118751f:236-250`) — with the flat and square cases written out as
//! branches of their own (`:229-234`). The two agree about the *angle* and disagree about
//! the *length*, so an assertion on the angle alone cannot tell them apart: the projection
//! stands `|drag| * cos(delta)` from the anchor where a rotation stands `|drag|`.
//! `the_lock_keeps_the_component_along_the_locked_ray` below is the assertion that can.
//!
//! **The rotation handle reaches the same step by a different route.** `selection/transform.rs`
//! calls none of the four — it is the rotation handle, and the checklist's "rotation" is
//! that path, not this one. It quantises over the same
//! [`SHIFT_LOCKING_ANGLE`](math::SHIFT_LOCKING_ANGLE) and by the same rounding once the
//! angle is folded into `[0, 2PI)`, so the two locks agree step for step; what differs is
//! that the rotation normalises and the drag does not, and that the drag's *geometry* is a
//! projection rather than a rotation. See `ci_rotate_lock.rs`.
//!
//! **What is not in the list**, each with the line that keeps it out:
//!
//! - another element's points, midpoints or edges: `maybeCacheReferenceSnapPoints` — the
//!   only writer of the snap cache — is called from four places
//!   (`App.tsx@1118751f:11095`, `13381`, `13505`, `13623`) and **none of them is on this
//!   path**, so `getPointSnaps` would read a cache nobody filled;
//! - `snapResizingElements` (`:13507`, `:13625`): reached only from `maybeHandleResize`,
//!   which runs under `pointerDownState.resize.isResizing` (`:10725`), and a press on a
//!   linear point sets `resize.handleType`/`isResizing` only through the transform-handle
//!   branch at `:9405-9407`;
//! - `snapDraggedElements` (`:11097`): the endpoint path `return`s at `:10886`, before the
//!   `hasHitASelectedElement` branch that reaches it;
//! - `getSnapLinesAtPointer` (`:7870`): gated on `isActiveToolNonLinearSnappable`
//!   (`snapping.ts@1118751f:1402-1414`), whose list is rectangle, ellipse, diamond, frame,
//!   magicframe, image, text — **not** `line`, and not `selection`;
//! - the same line's other points, and arrow binding: `pointDraggingUpdates` returns the
//!   naive drag for anything that is not an arrow (`linearElementEditor.ts@1118751f:
//!   2410-2415`).
//!
//! So the first point of a placed line is `getGridPoint(origin.x, origin.y, ctrl ? null :
//! gridSize)` (`App.tsx@1118751f:10235-10239`) and nothing else.
//!
//! # The Q/R pairs
//!
//! A test that asserts *a snap happened* also passes when a snap happens **always**, so
//! every case here has its other half: the same gesture, the same pointer, the grid on,
//! with Ctrl/Cmd held — which is the oracle's `null` grid at all fifteen of its pointer
//! call sites — landing on the **raw pointer** to `1e-9`. The failure names which side
//! answered.
//!
//! # The tie
//!
//! `getGridPoint` uses `Math.round`, and `Math.round` breaks a tie **toward +infinity**
//! (`Math.round(-0.5) === -0`). `f64::round` breaks it **away from zero**
//! (`(-0.5f64).round() == -1.0`). On a 20-unit grid the two disagree at every negative
//! half-cell, by a whole cell: the oracle puts an endpoint at world x = -10 on **0**,
//! `f64::round` puts it on **-20**. Pan a drawing left and every point you place on a
//! negative tie lands a cell off.

mod common;
use common::*;
use draw_engine::*;

/// The grid these tests snap to. Excalidraw's default is 20 (see
/// `ci_grid.rs::the_grid_is_off_until_asked_for`), and 20 has the useful property that
/// half a cell is 10 — a whole number, so a tie is exactly representable.
const GRID: f64 = 20.0;
/// What a world coordinate must match to be "the raw pointer". Both numbers come out of
/// the same arithmetic, so exact is the right bar; 1e-9 only absorbs the camera's round
/// trip.
const RAW: f64 = 1e-9;

fn grid_on() -> GridSettings {
    GridSettings {
        enabled: true,
        size: GRID,
        step: 5,
        snap: true,
    }
}

fn engine() -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine
}

/// A board with the grid snapping and the line tool up. The camera is 1:1 and
/// untranslated, so screen coordinates and world coordinates are the same numbers.
fn grid_engine() -> DrawEngine {
    let mut engine = engine();
    engine.set_grid(grid_on());
    engine.set_tool(DrawTool::Line);
    engine
}

fn click(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.begin_pointer(x, y, false, false);
    engine.end_pointer();
}

/// A move with no button down — how a path being placed follows the cursor between its
/// clicks, and the only such gesture in the engine.
fn hover(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.move_pointer(x, y, false, false);
}

fn the_line(engine: &DrawEngine) -> DrawElement {
    let lines: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == DrawElementType::Line)
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one line on the board");
    lines.into_iter().next().unwrap()
}

/// The line's points in world space, unturned — the positions a gesture moves.
fn points(engine: &DrawEngine) -> Vec<(f64, f64)> {
    let line = the_line(engine);
    line.points
        .unwrap_or_default()
        .iter()
        .map(|p| (line.x + p[0], line.y + p[1]))
        .collect()
}

fn first_point(engine: &DrawEngine) -> (f64, f64) {
    points(engine)[0]
}

fn last_point(engine: &DrawEngine) -> (f64, f64) {
    *points(engine).last().unwrap()
}

/// Grabs the line's **end** point and drags it to the world position `to`, releasing
/// there. The press lands on the handle, so this is the endpoint gesture and not a move of
/// the whole line.
fn drag_end_to(engine: &mut DrawEngine, to: (f64, f64)) {
    drag_end_to_shifted(engine, to, false);
}

/// As [`drag_end_to`], with `shifted` passed on as the engine's angle lock — the `square`
/// flag, which is the same modifier for a linear point as for a shape.
///
/// The pointer takes **screen** coordinates (`begin_pointer`), so both the press on the
/// handle and the release go through the camera. At the identity camera world and screen
/// are the same numbers, which is why the unswept cases would pass with this step
/// missing and the swept ones would not.
fn drag_end_to_shifted(engine: &mut DrawEngine, to: (f64, f64), shifted: bool) {
    let end = last_point(engine);
    let press = world_to_screen(engine.camera, end.0, end.1);
    let target = world_to_screen(engine.camera, to.0, to.1);
    engine.begin_pointer(press.x, press.y, false, false);
    engine.move_pointer(target.x, target.y, shifted, false);
    engine.end_pointer();
}

fn assert_at(where_: &str, got: (f64, f64), want: (f64, f64)) {
    assert!(
        (got.0 - want.0).abs() < RAW && (got.1 - want.1).abs() < RAW,
        "{where_}: expected {want:?}, got {got:?} — the snap side answered, not the raw one"
    );
}

/// A point is placed by a *release*, and the release places the preview point the hover
/// left behind — so a path needs a hover before each click, as `ci_line_multipoint`'s own
/// `place` does. A click with no preceding move places nothing.
fn place(engine: &mut DrawEngine, path: &[(f64, f64)]) {
    for &(x, y) in path {
        hover(engine, x, y);
        click(engine, x, y);
    }
}

/// A finished two-point line, both ends on grid intersections, left selected.
fn placed_line() -> DrawEngine {
    let mut engine = grid_engine();
    place(&mut engine, &[(100.0, 100.0), (300.0, 100.0)]);
    engine.finish_linear();
    engine
}

// ---------------------------------------------------------------------------
// Placing: the press that starts a line
// ---------------------------------------------------------------------------

/// Q. With the grid on, a press lands on the nearest intersection — not under the cursor.
#[test]
fn placing_a_line_lands_its_first_point_on_the_grid() {
    let mut engine = grid_engine();
    click(&mut engine, 37.0, 23.0);
    assert_at(
        "Q: the grid snap did not happen",
        first_point(&engine),
        (40.0, 20.0),
    );
}

/// R. The same press, same grid, Ctrl held: the oracle passes a `null` grid
/// (`App.tsx@1118751f:10238`), and the point lands where the pointer is.
///
/// This is the half that can fail. A test which only asserted Q would also pass if the
/// endpoint snapped to something, to everything, always.
#[test]
fn holding_ctrl_takes_the_first_point_off_the_grid() {
    let mut engine = grid_engine();
    engine.set_ctrl_held(true);
    click(&mut engine, 37.0, 23.0);
    assert_at(
        "R: the grid still snapped under Ctrl",
        first_point(&engine),
        (37.0, 23.0),
    );
}

/// Both halves in one test, so a regression cannot satisfy one and break the other, and
/// so the pair is shown to actually differ — a Q and an R that agree prove nothing.
#[test]
fn the_press_snaps_to_the_grid_unless_ctrl_is_held() {
    let mut snapped = grid_engine();
    click(&mut snapped, 37.0, 23.0);
    let mut free = grid_engine();
    free.set_ctrl_held(true);
    click(&mut free, 37.0, 23.0);

    assert_at("snapped", first_point(&snapped), (40.0, 20.0));
    assert_at("ctrl held", first_point(&free), (37.0, 23.0));
    assert_ne!(
        first_point(&snapped),
        first_point(&free),
        "the pair must differ, or one of the two sides is not answering"
    );
}

// ---------------------------------------------------------------------------
// Placing: the preview point that follows the cursor
// ---------------------------------------------------------------------------

/// Q. The uncommitted point is tracked live (`App.tsx@1118751f:11253`), and it is
/// tracked onto the grid.
#[test]
fn the_preview_point_of_a_path_is_snapped_to_the_grid() {
    let mut engine = grid_engine();
    click(&mut engine, 100.0, 100.0);
    hover(&mut engine, 137.0, 123.0);
    assert_eq!(
        points(&engine).len(),
        2,
        "the preview point should be there"
    );
    assert_at(
        "Q: the preview did not snap",
        points(&engine)[1],
        (140.0, 120.0),
    );
}

/// R. Same preview, same grid, Ctrl held: the raw pointer, to `1e-9`.
#[test]
fn holding_ctrl_takes_the_preview_point_off_the_grid() {
    let mut engine = grid_engine();
    click(&mut engine, 100.0, 100.0);
    engine.set_ctrl_held(true);
    hover(&mut engine, 137.0, 123.0);
    assert_eq!(
        points(&engine).len(),
        2,
        "the preview point should be there"
    );
    assert_at(
        "R: the preview still snapped",
        points(&engine)[1],
        (137.0, 123.0),
    );
}

// ---------------------------------------------------------------------------
// Dragging: an existing line's endpoint
// ---------------------------------------------------------------------------

/// Q. A dragged endpoint snaps to the grid: `App.tsx@1118751f:10719-10723` grid-snaps
/// the pointer before `handlePointDragging` ever sees it.
#[test]
fn a_dragged_endpoint_snaps_to_the_grid() {
    let mut engine = placed_line();
    drag_end_to(&mut engine, (337.0, 103.0));
    assert_at(
        "Q: the dragged endpoint did not snap",
        last_point(&engine),
        (340.0, 100.0),
    );
}

/// R. Same drag, same grid, Ctrl held: the raw pointer.
#[test]
fn holding_ctrl_takes_a_dragged_endpoint_off_the_grid() {
    let mut engine = placed_line();
    engine.set_ctrl_held(true);
    drag_end_to(&mut engine, (337.0, 103.0));
    assert_at(
        "R: the dragged endpoint still snapped",
        last_point(&engine),
        (337.0, 103.0),
    );
}

// ---------------------------------------------------------------------------
// The angle lock, and the order grid-then-angle
// ---------------------------------------------------------------------------

/// Shift replaces the free branch rather than composing with it
/// (`linearElementEditor.ts@1118751f:542-557`) — this is the second entry in the ordered
/// list, and it is a Q/R pair of its own.
///
/// Asserted as **a multiple of 15 degrees**, the oracle's own floor. That is deliberately
/// the weaker claim: every 45 degrees is a multiple of 15, so while the step was `PI / 4`
/// this assertion was true and the engine was still wrong. It is kept because it is the
/// one statement that holds for *every* endpoint, and the exact-angle cases in
/// `the_lock_lands_on_exactly_15_30_165_and_345_degrees` are what pin the step itself.
///
/// The length is asserted separately, in
/// `a_locked_endpoint_sits_on_the_locked_ray_at_the_projected_distance`, and against the
/// oracle's geometry rather than left to whichever of the two this engine happened to use.
#[test]
fn shift_locks_a_dragged_endpoint_to_a_multiple_of_15_degrees() {
    let mut engine = placed_line();
    drag_end_to_shifted(&mut engine, (337.0, 173.0), true);
    let (x, y) = last_point(&engine);
    let (dx, dy) = (x - 100.0, y - 100.0);
    assert!(
        dx.hypot(dy) > 0.0,
        "the drag was degenerate, nothing was measured"
    );
    let angle = dy.atan2(dx).to_degrees();
    for step in -12..=12 {
        if (angle - (step * 15) as f64).abs() < 1e-6 {
            return;
        }
    }
    panic!("expected a multiple of 15 degrees, got {angle}");
}

/// The other half of that pair, and the one that can fail: **without** Shift the endpoint
/// is the pointer, and its direction is whatever the pointer's is — 18.4 degrees off the
/// pivot here, which is not a multiple of 15. A test asserting only that Shift locks would
/// also pass on a build that locked unconditionally.
#[test]
fn without_shift_a_dragged_endpoint_is_just_the_pointer() {
    let mut engine = placed_line();
    drag_end_to(&mut engine, (337.0, 173.0));
    // The grid snapped the pointer, so that is where the endpoint is.
    assert_at("no shift", last_point(&engine), (340.0, 180.0));
    let (x, y) = last_point(&engine);
    let angle = (y - 100.0).atan2(x - 100.0).to_degrees();
    for step in -12..=12 {
        assert!(
            (angle - (step * 15) as f64).abs() > 1e-6,
            "the endpoint was locked to {angle} degrees with no modifier held"
        );
    }
}

/// The order of the two entries, as a difference rather than as a comment. The grid runs
/// **first**, and the angle is measured from the pointer the grid left behind
/// (`linearElementEditor.ts@1118751f:1916`, which grid-snaps before calling
/// `getLockedLinearCursorAlignSize`), so switching the grid off changes the segment that
/// comes out.
#[test]
fn the_angle_lock_is_measured_from_the_pointer_the_grid_left_behind() {
    let mut on_grid = placed_line();
    drag_end_to_shifted(&mut on_grid, (337.0, 173.0), true);
    let mut off_grid = placed_line();
    off_grid.set_ctrl_held(true);
    drag_end_to_shifted(&mut off_grid, (337.0, 173.0), true);

    assert_ne!(
        last_point(&on_grid),
        last_point(&off_grid),
        "the grid and the angle lock must be ordered, or one of them is not running"
    );
    // Both are locked; only the point they were locked from differs.
    // The grid half was measured from (340, 180); the Ctrl half from (337, 173) itself.
    assert_ne!(
        last_point(&on_grid),
        (340.0, 180.0),
        "with the grid on the press should have been rounded first"
    );
    assert_ne!(
        last_point(&off_grid),
        (337.0, 173.0),
        "with Ctrl held the raw pointer should have been the one used"
    );
}

// ---------------------------------------------------------------------------
// The lock, at the oracle's own resolution
// ---------------------------------------------------------------------------

/// `SHIFT_LOCKING_ANGLE` as the oracle spells it: `Math.PI / 12`
/// (`packages/common/src/constants.ts@1118751f:31`).
const STEP_DEGREES: f64 = 15.0;
const STEP_RADIANS: f64 = std::f64::consts::PI / 12.0;

/// How long the dragged segment is before the lock, in world units. Long enough that the
/// oracle's projection and a rotated delta are far apart: the gap is
/// `DRAG * (1 - cos(delta))`, which is 12 world units on the gentlest case below.
const DRAG: f64 = 200.0;

/// `Math.round`, which breaks a tie **toward +infinity** (`Math.round(-0.5) === -0`) where
/// `f64::round` breaks it away from zero. `Math.round` is `floor(x + 0.5)` for every
/// input, and a port that uses `.round()` is off by a whole step on every negative tie.
fn math_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// The angle of `(x, y)` in degrees, folded into `[0, 360)` so that a locked angle can be
/// quoted the way the oracle's `normalizeRadians` would leave it.
fn bearing(x: f64, y: f64) -> f64 {
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

/// A drag of [`DRAG`] world units at `degrees` from the anchor.
fn drag_at(degrees: f64) -> (f64, f64) {
    let r = degrees.to_radians();
    (DRAG * r.cos(), DRAG * r.sin())
}

/// The angle the oracle's own rounding picks for a raw angle, in degrees.
fn oracle_step(degrees: f64) -> f64 {
    math_round(degrees / STEP_DEGREES) * STEP_DEGREES
}

/// The oracle's `getLockedLinearCursorAlignSize` (`sizeHelpers.ts@1118751f:187-254`),
/// transcribed whole: the rounded step, the two axis branches, and the two lines solved
/// against each other. Written as the **intersection** rather than as a projection, so
/// that it is a second reading of the oracle and not the engine's own arithmetic restated.
fn oracle_locked(dx: f64, dy: f64) -> (f64, f64) {
    let locked = math_round(dy.atan2(dx) / STEP_RADIANS) * STEP_RADIANS;
    if locked == 0.0 {
        return (dx, 0.0);
    }
    if locked == std::f64::consts::FRAC_PI_2 {
        return (0.0, dy);
    }
    // locked angle line y = mx + b, then the line through the cursor across it
    let (a1, b1, c1) = (locked.tan(), -1.0, 0.0);
    let (a2, b2, c2) = (-1.0 / a1, -1.0, dy - (-1.0 / a1) * dx);
    let den = a1 * b2 - a2 * b1;
    ((b1 * c2 - b2 * c1) / den, (c1 * a2 - c2 * a1) / den)
}

/// A placed line, the grid released so the pointer arrives raw, and **the point its end
/// hangs from**.
///
/// Two things here are not obvious and both would make an angle assertion quietly wrong.
/// The grid comes off first, because on it the pointer is rounded to a cell before the
/// lock ever sees it, so the drag below would be a different drag. And the anchor is the
/// line's *other* end, not the point under the pointer: dragging an end holds the segment
/// to a ray drawn from the far end of it (`engine/pointer_move.rs:392`, where a handle
/// takes `points[i - 1]`), which is the oracle's `originX`/`originY`. Measuring from the
/// dragged point instead would report every angle below as wrong by the segment's own.
fn raw_pointer_line() -> (DrawEngine, (f64, f64)) {
    let mut engine = placed_line();
    engine.set_ctrl_held(true);
    let anchor = first_point(&engine);
    (engine, anchor)
}

/// **Q** — the half that can fail. The locked angle is **exactly** the oracle's step, not
/// merely a multiple of it. A 45-degree lock gives 0, 45, 180, 315 for these four raw
/// angles and a lock that does nothing gives 20, 35, 160, 340 — so this is the assertion
/// that separates 15 degrees from 45, and "is a multiple of 15" is not: every 45 is a
/// multiple of 15, which is how the 45-degree build passed the pair above.
#[test]
fn the_lock_lands_on_exactly_15_30_165_and_345_degrees() {
    for (raw, want) in [(20.0, 15.0), (35.0, 30.0), (160.0, 165.0), (-20.0, 345.0)] {
        let (dx, dy) = drag_at(raw);
        let (x, y) = constrain_to_angle(dx, dy);
        let got = bearing(x, y);
        assert!(
            (got - want).abs() < 1e-9,
            "a drag at {raw} degrees locked to {got} degrees, not {want}"
        );
    }
}

/// **R** — the other half of the pair, and it belongs to the gesture rather than to
/// `constrain_to_angle`, which always locks. With nothing held the endpoint is the
/// pointer, so the angle is the raw one to the last decimal; a lock applied
/// unconditionally would fail the gesture pair below. The four raw angles are the ones
/// the Q above uses, so the two halves are the same four drags read two ways.
#[test]
fn the_raw_angles_are_the_ones_the_lock_is_measured_from() {
    for raw in [20.0, 35.0, 160.0, -20.0] {
        let (dx, dy) = drag_at(raw);
        let want = raw.rem_euclid(360.0);
        assert!(
            (bearing(dx, dy) - want).abs() < 1e-9,
            "a drag built at {raw} degrees reported {}, not {want}",
            bearing(dx, dy)
        );
        // Each raw angle sits in its own 15-degree cell, off the step by more than the
        // tolerance: a Q/R pair is only a pair if the Q actually moved the number.
        assert!(
            (raw - oracle_step(raw)).abs() > 2.0,
            "{raw} degrees is too near {want} to tell a lock from no lock"
        );
    }
}

/// The length **through** the lock, which is the assertion the constant alone does not
/// buy. The oracle keeps the component of the drag *along* the locked ray and drops the
/// one across it (`sizeHelpers.ts@1118751f:236-250`), so the endpoint is the pointer's
/// orthogonal projection onto that ray: `DRAG * cos(delta)` from the anchor, not `DRAG`.
///
/// A rotated delta keeps `DRAG` exactly, so it is wrong here by
/// `DRAG * (1 - cos(delta))` — 12 world units on the first case below and 0.76 on the
/// gentlest, both far outside the tolerance. **This is what a 45-degree build cannot
/// pass either**: at 45 a raw 20-degree drag locks flat, and a rotated delta puts its
/// width at 200 where the oracle's flat lock keeps `200 * cos(20 degrees)`.
#[test]
fn the_lock_keeps_the_component_along_the_locked_ray() {
    for raw in [20.0, 35.0, 160.0, -20.0, -170.0, 85.0, 95.0, 4.0] {
        let (dx, dy) = drag_at(raw);
        let (x, y) = constrain_to_angle(dx, dy);
        let delta = (raw - oracle_step(raw)).to_radians();
        let want = DRAG * delta.cos();
        let got = x.hypot(y);
        assert!(
            (got - want).abs() < 1e-9,
            "a drag at {raw} degrees came out {got} from the anchor, not the \
             projection's {want} — the delta was rotated rather than intersected"
        );
    }
}

/// The two cases the oracle writes out as separate branches
/// (`sizeHelpers.ts@1118751f:229-234`): a flat lock **zeroes the height** and keeps the
/// width, a square lock **zeroes the width** and keeps the height. Asserted as exact
/// zeros, because that is what the branches do and what a projection left to `sin_cos`
/// answers with a unit of noise instead.
#[test]
fn a_flat_lock_zeroes_the_height_and_a_square_lock_the_width() {
    for raw in [4.0, -6.0, 174.0] {
        let (dx, dy) = drag_at(raw);
        let (x, y) = constrain_to_angle(dx, dy);
        assert_eq!(
            y, 0.0,
            "a drag at {raw} degrees should lock flat, got ({x}, {y})"
        );
        assert!(
            (x - DRAG * raw.to_radians().cos()).abs() < 1e-9,
            "a flat lock should keep the width, got {x}"
        );
    }
    for raw in [88.0, 95.0] {
        let (dx, dy) = drag_at(raw);
        let (x, y) = constrain_to_angle(dx, dy);
        assert_eq!(
            x, 0.0,
            "a drag at {raw} degrees should lock square, got ({x}, {y})"
        );
        assert!(
            (y - DRAG * raw.to_radians().sin()).abs() < 1e-9,
            "a square lock should keep the height, got {y}"
        );
    }
}

/// **Swept, not sampled.** For every raw angle on a one-degree grid, both signs, all four
/// call sites' input shape — a displacement from an anchor, which is the only thing any
/// of the four hands over — the locked endpoint is the oracle's ray-intersection, to the
/// last bit the two are comparable at.
#[test]
fn every_raw_angle_locks_onto_the_oracles_ray_intersection() {
    for tenth in -3600..3600 {
        let raw = f64::from(tenth) / 10.0;
        let (dx, dy) = drag_at(raw);
        let got = constrain_to_angle(dx, dy);
        let want = oracle_locked(dx, dy);
        assert!(
            (got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9,
            "a drag at {raw} degrees locked to {:?}, the oracle says {:?}",
            got,
            want
        );
    }
}

/// The lock is **idempotent**: locking an already-locked delta must not move it. A drag
/// re-reads its own output on every pointer move, so a lock that kept re-rounding would
/// walk the endpoint along the ray instead of holding it.
#[test]
fn locking_an_already_locked_delta_changes_nothing() {
    for raw in [0.0, 7.0, 20.0, 35.0, 88.0, 160.0, -20.0, -95.0, 179.0] {
        let (dx, dy) = drag_at(raw);
        let once = constrain_to_angle(dx, dy);
        let twice = constrain_to_angle(once.0, once.1);
        assert!(
            (once.0 - twice.0).abs() < 1e-12 && (once.1 - twice.1).abs() < 1e-12,
            "locking {raw} degrees twice moved it: {:?} then {:?}",
            once,
            twice
        );
    }
}

// ---------------------------------------------------------------------------
// The same lock, through a real gesture
// ---------------------------------------------------------------------------

/// **Q**, the wiring: the same four drags as the pair above, taken through a press on the
/// end handle, land on exactly 15, 30, 165 and 345 degrees from the anchor.
#[test]
fn shift_locks_a_dragged_endpoint_to_exactly_the_oracles_step() {
    for (raw, want) in [(20.0, 15.0), (35.0, 30.0), (160.0, 165.0), (-20.0, 345.0)] {
        let (mut engine, anchor) = raw_pointer_line();
        let (dx, dy) = drag_at(raw);
        drag_end_to_shifted(&mut engine, (anchor.0 + dx, anchor.1 + dy), true);
        let (x, y) = last_point(&engine);
        let got = bearing(x - anchor.0, y - anchor.1);
        assert!(
            (got - want).abs() < 1e-9,
            "a drag at {raw} degrees locked to {got} degrees, not {want}"
        );
    }
}

/// **R**, the wiring: the identical gesture with nothing held puts the endpoint on the
/// raw pointer, angle and all. This is the half that fails on a build which locks
/// unconditionally.
#[test]
fn without_shift_a_dragged_endpoint_keeps_the_pointer_angle_exactly() {
    for raw in [20.0, 35.0, 160.0, -20.0] {
        let (mut engine, anchor) = raw_pointer_line();
        let (dx, dy) = drag_at(raw);
        let target = (anchor.0 + dx, anchor.1 + dy);
        drag_end_to_shifted(&mut engine, target, false);
        assert_at("no shift", last_point(&engine), target);
        let (x, y) = last_point(&engine);
        let want = raw.rem_euclid(360.0);
        let got = bearing(x - anchor.0, y - anchor.1);
        assert!(
            (got - want).abs() < 1e-9,
            "an unlocked drag reported {got} degrees, not {want}"
        );
    }
}

/// The length assertion again, this time as a person would meet it: the endpoint is the
/// anchor plus the drag's component along the locked ray, so it sits on that ray and at
/// the projected distance — not at the dragged distance.
#[test]
fn a_locked_endpoint_sits_on_the_locked_ray_at_the_projected_distance() {
    let (mut engine, anchor) = raw_pointer_line();
    let (dx, dy) = drag_at(20.0);
    drag_end_to_shifted(&mut engine, (anchor.0 + dx, anchor.1 + dy), true);
    let (x, y) = last_point(&engine);
    let (rx, ry) = (x - anchor.0, y - anchor.1);
    let want = DRAG * (20.0f64 - 15.0).to_radians().cos();
    assert!(
        (rx.hypot(ry) - want).abs() < 1e-9,
        "the endpoint landed {} from the anchor, not the projection's {want}",
        rx.hypot(ry)
    );
    // On the ray: nothing of the drag is left across it, measured across the **locked**
    // angle rather than the raw one — the whole claim is that the endpoint left the raw
    // direction and joined the locked one.
    let locked = oracle_step(20.0).to_radians();
    let across = rx * locked.sin() - ry * locked.cos();
    assert!(
        across.abs() < 1e-9,
        "the endpoint is {across} off the locked ray"
    );
}

// ---------------------------------------------------------------------------
// What an endpoint does NOT snap to
// ---------------------------------------------------------------------------

/// A board with a solid rectangle whose top-left corner is (200, 200), object snapping
/// **on**, and the grid **off** — so the only thing that could move an endpoint is
/// another element.
fn board_with_a_shape() -> DrawEngine {
    let mut engine = two_shapes();
    engine.set_objects_snap(true);
    engine.set_tool(DrawTool::Line);
    engine
}

/// The same two shapes without the line tool up. A press that is to *move* an element is
/// the selection tool's gesture; with the line tool up it starts a new line instead, which
/// is why the control below needs its own board.
fn two_shapes() -> DrawEngine {
    let shapes = vec![
        filled(box_at(200.0, 200.0, 100.0, 100.0)),
        filled(box_at(600.0, 600.0, 100.0, 100.0)),
    ];
    engine_with_scene(shapes)
}

/// The headline. A line's own endpoint does **not** snap to a nearby element — not to its
/// corner, not to its edge, not to its centre, and not at any distance.
///
/// The corner is 3 units away, well inside the 6px reach the move-selection guides use
/// (`SNAP_PX`, `engine/mod.rs:92`), so a snapper wired into the wrong place would fire
/// here and be caught. It does not, because the oracle has none: see the module header.
#[test]
fn a_line_endpoint_does_not_snap_to_a_nearby_element() {
    let mut engine = board_with_a_shape();
    engine.begin_pointer(60.0, 60.0, false, false);
    engine.move_pointer(197.0, 197.0, false, false);
    engine.end_pointer();
    let line = the_line(&engine);
    let end = *line.points.as_ref().unwrap().last().unwrap();

    assert!(
        (line.x + end[0] - 197.0).hypot(line.y + end[1] - 197.0) < RAW,
        "the endpoint moved toward the rectangle's corner, which the oracle never does"
    );
}

/// The control the last test needs: the same board, the same objects-snap preference and
/// the same 6px reach, dragging a **whole element** *does* snap. Without it the test
/// above would also pass on a board where snapping had been switched off entirely, and
/// would say nothing at all.
#[test]
fn element_snapping_is_live_on_the_same_board() {
    let mut engine = two_shapes();
    engine.set_objects_snap(true);
    let id = engine.get_scene()[0].id.clone();
    // Grab the first rectangle by its own body and drag it until its top-left corner is
    // 3 units off the second one's — inside the reach, on the axis the guides snap on.
    engine.begin_pointer(250.0, 250.0, false, false);
    engine.move_pointer(753.0, 753.0, false, false);
    let guides = engine.snap_guides().len();
    let moved = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the dragged rectangle is gone");
    engine.end_pointer();

    assert!(
        guides > 0,
        "a whole element dragged onto another element's edge must snap: the guide count \
         is zero, so this control is not controlling anything"
    );
    assert!(
        (moved.x - 700.0).abs() < 1e-6 && (moved.y - 700.0).abs() < 1e-6,
        "the dragged rectangle should have been pulled onto the other's left/top edge \
         at (700, 700), got ({}, {})",
        moved.x,
        moved.y
    );
}

// ---------------------------------------------------------------------------
// The tie: Math.round toward +infinity, f64::round away from zero
// ---------------------------------------------------------------------------

/// The oracle's own arithmetic, measured in node against the pinned image rather than
/// reasoned about: `getGridPoint` is `Math.round(v / size) * size`
/// (`points.ts@1118751f:76-77`) and `Math.round` puts a tie toward +infinity — so a
/// **negative** half-cell rounds *up*, to the intersection nearer zero.
#[test]
fn a_negative_half_cell_rounds_towards_zero_like_the_oracle() {
    let grid = grid_on();
    // Math.round(-10/20)*20 == 0 and Math.round(-30/20)*20 == -20.
    assert_eq!(grid.snap_point(-10.0, -30.0), (0.0, -20.0));
    assert_eq!(grid.snap_point(-50.0, -90.0), (-40.0, -80.0));
}

/// The positive side is the control: both languages agree there, so a failure on the
/// negative side cannot be blamed on the test's own arithmetic.
#[test]
fn a_positive_half_cell_rounds_up_as_well() {
    let grid = grid_on();
    assert_eq!(grid.snap_point(10.0, 30.0), (20.0, 40.0));
    assert_eq!(grid.snap_point(50.0, 90.0), (60.0, 100.0));
}

/// A hair either side of a negative tie, pinning which side the tie belongs to. The tie
/// groups with **above** it, not with below — that is the whole content of "rounds toward
/// +infinity", and it is invisible to a test that only probes whole numbers.
#[test]
fn the_tie_belongs_to_the_side_towards_zero() {
    let grid = grid_on();
    let below = grid.snap_point(-10.000001, 0.0).0;
    let tie = grid.snap_point(-10.0, 0.0).0;
    let above = grid.snap_point(-9.999999, 0.0).0;
    assert_eq!(below, -20.0, "below the tie rounds away from zero");
    assert_eq!(above, 0.0, "above the tie rounds towards zero");
    assert_eq!(
        tie, above,
        "the tie takes the +infinity side, as Math.round does"
    );
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

/// A lattice of pointers: every interesting place in and around a cell, at both signs.
/// The two half-cell entries a hair apart are the tie and its neighbourhood, which is
/// where the oracle and `f64::round` part company.
fn lattice() -> Vec<f64> {
    let mut out = Vec::new();
    for cell in -3i64..=3 {
        let base = (cell * GRID as i64) as f64;
        for delta in [
            -0.000001,
            0.0,
            0.000001,
            GRID / 4.0,
            GRID / 2.0 - 0.000001,
            GRID / 2.0,
            GRID / 2.0 + 0.000001,
            GRID - 0.000001,
        ] {
            out.push(base + delta);
            out.push(-(base + delta));
        }
    }
    out
}

/// The world positions the sweep works in: a line from `HOME` to `TIP`, and pointers
/// around it. The camera has to keep both on an 800x600 screen — a press that lands
/// off-canvas starts no gesture, which would make the swept cases pass or fail for a
/// reason that has nothing to do with snapping.
const HOME: (f64, f64) = (200.0, 200.0);
const TIP: (f64, f64) = (400.0, 100.0);

/// Cameras to sweep. The snap is in world units, so scaling and panning must not change
/// the answer — only where the pointer has to be put to name a given world position.
/// Every translation is a multiple of 0.5, so `world_to_screen` and `screen_to_world` are
/// exact inverses here and the sweep is not measuring floating-point noise.
fn cameras() -> Vec<Camera> {
    vec![
        Camera {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        },
        Camera {
            x: -200.0,
            y: -200.0,
            scale: 2.0,
        },
        Camera {
            x: 50.0,
            y: 100.0,
            scale: 0.5,
        },
        Camera {
            x: -500.0,
            y: -300.0,
            scale: 3.0,
        },
    ]
}

/// Asserts what the camera list exists to guarantee, so a future camera added to
/// [`cameras`] cannot quietly take the swept cases off screen.
fn the_line_is_on_screen(camera: Camera) {
    for world in [HOME, TIP] {
        let screen = world_to_screen(camera, world.0, world.1);
        assert!(
            screen.x >= 0.0 && screen.x <= 800.0 && screen.y >= 0.0 && screen.y <= 600.0,
            "camera {camera:?} puts {world:?} at {screen:?}, off the 800x600 screen, so \
             the sweep would be measuring a press that never happened"
        );
    }
}

/// How far a per-axis round can actually reach: the corner of the cell, `size/2 * 2`.
/// Not half a cell — that is the axis-aligned reach, and it is the wrong bound for a
/// snap that rounds each axis on its own.
fn diagonal_reach() -> f64 {
    GRID / 2.0 * 2.0_f64.sqrt()
}

/// P1. A snap never moves a line's endpoint further than the grid's own reach.
///
/// The bound is the **diagonal** of half a cell, because the oracle rounds each axis
/// separately. An implementation that treated the grid as a distance from a candidate —
/// the mistake the plan's wording invites — passes a `size/2` bound on every
/// axis-aligned case and fails exactly here, at a cell's corner.
#[test]
fn a_grid_snap_never_moves_a_point_past_the_corner_of_its_cell() {
    let grid = grid_on();
    let reach = diagonal_reach();
    for x in lattice() {
        for y in lattice() {
            let (sx, sy) = grid.snap_point(x, y);
            let moved = (sx - x).hypot(sy - y);
            assert!(
                moved <= reach + 1e-9,
                "({x}, {y}) snapped to ({sx}, {sy}), {moved} away, past the reach {reach}"
            );
        }
    }
}

/// P1's tightness. A bound nothing can reach is not a bound, and a snapper that never
/// moved anything would satisfy the test above. Something must land on the corner.
#[test]
fn the_corner_of_a_cell_is_reachable_so_the_bound_bites() {
    let grid = grid_on();
    let reach = diagonal_reach();
    let corner = (GRID / 2.0, GRID / 2.0);
    let (sx, sy) = grid.snap_point(corner.0, corner.1);
    let moved = (sx - corner.0).hypot(sy - corner.1);
    assert!(
        (moved - reach).abs() < 1e-9,
        "the corner should attain the reach {reach}, got {moved} at ({sx}, {sy})"
    );
}

/// P2. The chosen intersection is the **nearest** one under the oracle's own metric.
///
/// The metric is per axis: on each axis, the multiple of `size` whose distance from the
/// pointer is least, ties to the larger value. The assertion is deliberately not "within
/// half a cell", which any of three candidates satisfies. It is that no other multiple is
/// nearer, and that among equally near ones the larger was taken. This is the property
/// that catches a reordering, and it is the reason the tie tests above exist.
#[test]
fn the_chosen_intersection_is_the_nearest_under_the_oracles_own_metric() {
    let grid = grid_on();
    for x in lattice() {
        for y in lattice() {
            let (sx, sy) = grid.snap_point(x, y);
            assert_axis_is_nearest(x, sx, "x");
            assert_axis_is_nearest(y, sy, "y");
        }
    }
}

fn assert_axis_is_nearest(pointer: f64, chosen: f64, axis: &str) {
    assert!(
        (chosen / GRID - (chosen / GRID).round()).abs() < 1e-9,
        "{axis}: {chosen} is not a multiple of the grid size"
    );
    let best = (chosen - pointer).abs();
    for step in 1..4 {
        for other in [chosen + step as f64 * GRID, chosen - step as f64 * GRID] {
            assert!(
                (other - pointer).abs() > best - 1e-9,
                "{axis}: {other} is nearer to {pointer} than the chosen {chosen}"
            );
        }
    }
}

/// P3. Both gestures, swept. With the grid snapping, the endpoint is on an intersection
/// and within half a cell per axis; with Ctrl held it is the raw pointer exactly. Camera
/// scale and pan are in the sweep, because a snap computed in screen pixels would pass at
/// 1:1 and fail at 2x.
#[test]
fn both_gestures_land_on_the_raw_pointer_unless_the_grid_is_snapping() {
    for camera in cameras() {
        the_line_is_on_screen(camera);
        for world in [(37.0, 23.0), (-10.0, -30.0), (137.0, 123.0), (0.5, 0.5)] {
            check_placement(camera, world);
            check_dragged_endpoint(camera, world);
        }
    }
}

/// The screen position that names the world position `world` under `camera` — the
/// engine's own `world_to_screen` (`camera.rs:147-151`), so this cannot drift from it.
fn screen_of(camera: Camera, world: (f64, f64)) -> (f64, f64) {
    let p = world_to_screen(camera, world.0, world.1);
    (p.x, p.y)
}

fn check_placement(camera: Camera, world: (f64, f64)) {
    let (sx, sy) = screen_of(camera, world);
    let mut snapped = line_engine_at(camera);
    snapped.begin_pointer(sx, sy, false, false);
    snapped.end_pointer();
    let mut free = line_engine_at(camera);
    free.set_ctrl_held(true);
    free.begin_pointer(sx, sy, false, false);
    free.end_pointer();
    assert_landing(
        "placement",
        world,
        first_point(&snapped),
        first_point(&free),
    );
}

fn check_dragged_endpoint(camera: Camera, world: (f64, f64)) {
    // Dragged to a point offset from `world`, so the four cases are four different drags
    // and not four copies of one.
    let to = (world.0 + 37.0, world.1 + 23.0);
    let mut snapped = placed_line_at(camera);
    drag_end_to(&mut snapped, to);
    let mut free = placed_line_at(camera);
    free.set_ctrl_held(true);
    drag_end_to(&mut free, to);
    assert_landing(
        "dragged endpoint",
        to,
        last_point(&snapped),
        last_point(&free),
    );
}

fn line_engine_at(camera: Camera) -> DrawEngine {
    let mut engine = engine();
    engine.set_camera(camera);
    engine.set_grid(grid_on());
    engine.set_tool(DrawTool::Line);
    engine
}

/// A two-point line on grid intersections, left selected, so an endpoint can be grabbed.
fn placed_line_at(camera: Camera) -> DrawEngine {
    let mut engine = line_engine_at(camera);
    let first = screen_of(camera, HOME);
    let second = screen_of(camera, TIP);
    place(&mut engine, &[(first.0, first.1), (second.0, second.1)]);
    engine.finish_linear();
    engine
}

fn assert_landing(where_: &str, world: (f64, f64), snapped: (f64, f64), raw: (f64, f64)) {
    let moved = (snapped.0 - world.0).hypot(snapped.1 - world.1);
    assert!(
        moved <= diagonal_reach() + 1e-6,
        "{where_}: the grid snap threw the endpoint {moved} from the pointer"
    );
    for (got, axis) in [(snapped.0, "x"), (snapped.1, "y")] {
        assert!(
            (got / GRID - (got / GRID).round()).abs() < 1e-6,
            "{where_}: {axis} {got} is not on a grid intersection"
        );
    }
    assert!(
        (raw.0 - world.0).abs() < 1e-6 && (raw.1 - world.1).abs() < 1e-6,
        "{where_}: with Ctrl held the endpoint is {raw:?}, not the pointer {world:?}"
    );
}
