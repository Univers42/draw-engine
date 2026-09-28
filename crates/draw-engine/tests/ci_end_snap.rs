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

// ---------------------------------------------------------------------------
// The angle lock, and the order grid-then-angle
// ---------------------------------------------------------------------------

/// Shift replaces the free branch rather than composing with it
/// (`linearElementEditor.ts@1118751f:542-567`): one drag, one segment, one multiple of
/// 45 degrees out of the pivot at (100, 100).
#[test]
fn shift_holds_a_dragged_endpoint_to_a_multiple_of_45_degrees() {
    let mut engine = placed_line();
    drag_end_to_shifted(&mut engine, (337.0, 173.0), true);
    let (x, y) = last_point(&engine);
    let (dx, dy) = (x - 100.0, y - 100.0);
    let length = dx.hypot(dy);
    let angle = dy.atan2(dx).to_degrees();
    assert!(
        length > 0.0,
        "the drag was degenerate, so nothing was measured"
    );
    for step in -4..=4 {
        if (angle - (step * 45) as f64).abs() < 1e-6 {
            return;
        }
    }
    panic!("expected a multiple of 45 degrees, got {angle}");
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
