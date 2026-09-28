//! A freedraw stroke's press origin and its points are **both** the raw pointer.
//!
//! `App.tsx@1118751f:9899-9902` passes a **bare `null`** grid to `getGridPoint` — the only
//! one of the sixteen call sites that does — and `getGridPoint` with a `null` grid
//! returns the point unchanged (`packages/common/src/points.ts@1118751f:69-81`). The
//! reason is written in the oracle a page above it: `// TODO: Rounding this point causes
//! some shake when free drawing` (`:68`).
//!
//! It is not a press-only rule, and the other half is the proof. The move handler appends
//! `pointerCoords - newElement.x` (`App.tsx@1118751f:11179-11196`), where
//! `pointerCoords` is the **raw** scene coordinate and `newElement.x` is the **unrounded**
//! press origin. The origin is unrounded *because* the points are measured from it: a
//! stroke's local points are the difference between the raw pointer and the raw press, so
//! rounding only the press would shift every point by up to half a cell — the shake, as an
//! arithmetic identity rather than a comment.
//!
//! So the answer is not "the press inherits the move", and it is not "the move inherits
//! the press": there is one rule, and freedraw is outside the grid entirely. This engine
//! rounded both — `engine/pointer.rs` snapped the press before `begin_freedraw` saw it and
//! `engine/pointer_move.rs` snapped the pointer before the freedraw arm did — so it
//! diverged from the oracle in both halves at once.
//!
//! ## The numbers are hand-computed, and they pin each half separately
//!
//! One move, press (503, 300) to (563, 340), grid on at 20. The stroke's origin is the
//! press, and its one new point is `streamline([0, 0], raw − press, 0.5)`, which with a
//! strength of 0.5 is half of the raw offset — the oracle streamlines the same way.
//!
//! | what | origin | the new point |
//! | --- | --- | --- |
//! | **both raw, which is the oracle** | 503 | (30, 20) |
//! | press rounded, move raw | 500 | (31.5, 20) |
//! | press raw, move rounded | 503 | (28.5, 20) |
//!
//! So `x == 503` and `last == (30, 20)` are two independent statements: the first can only
//! hold if the press is raw, the second can only hold if the pointer is.

mod common;
use common::*;
use draw_engine::*;

/// A 20-grid that is on and snapping, so the difference is visible at all.
fn grid_on() -> GridSettings {
    GridSettings {
        enabled: true,
        snap: true,
        size: 20.0,
        step: 5,
    }
}

/// Presses at `press` and drags to `to`, in world units at 1×, with the freedraw tool.
fn stroke_to(engine: &mut DrawEngine, press: (f64, f64), to: (f64, f64)) -> DrawElement {
    engine.set_tool(DrawTool::Freedraw);
    engine.begin_pointer(press.0, press.1, false, false);
    engine.move_pointer(to.0, to.1, false, false);
    engine.end_pointer();
    engine.get_scene().pop().expect("no stroke was drawn")
}

fn points_of(element: &DrawElement) -> Vec<[f64; 2]> {
    element.points.clone().expect("a freedraw has points")
}

fn last_of(element: &DrawElement) -> [f64; 2] {
    *points_of(element)
        .last()
        .expect("a stroke has at least one point")
}

/// The same press and drag, as a rectangle: a shape is placed from the grid, and this is
/// the control every stroke case below is read against.
fn shape_to(engine: &mut DrawEngine, press: (f64, f64), to: (f64, f64)) -> DrawElement {
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(press.0, press.1, false, false);
    engine.move_pointer(to.0, to.1, false, false);
    engine.end_pointer();
    engine.get_scene().pop().expect("no shape was drawn")
}

// ------------------------------------------------------------------- the press

/// The press origin is the raw pointer: 503, not the 500 a 20-grid would put it on.
#[test]
fn the_press_origin_is_the_raw_pointer() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_grid(grid_on());

    let stroke = stroke_to(&mut engine, (503.0, 300.0), (563.0, 340.0));

    assert_close(stroke.x, 503.0);
    assert_close(stroke.y, 300.0);
    assert_close_msg(stroke.width, 30.0, "half of the raw 60-unit drag");
    assert_close_msg(stroke.height, 20.0, "half of the raw 40-unit drag");
}

/// **The other half, pinned separately.** The new point is measured from the raw pointer,
/// so a pointer that had been rounded to 560 would have given 28.5 where the raw gives 30.
#[test]
fn the_point_is_measured_from_the_raw_pointer_too() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_grid(grid_on());

    let stroke = stroke_to(&mut engine, (503.0, 300.0), (563.0, 340.0));

    assert_eq!(last_of(&stroke), [30.0, 20.0]);
}

/// **A shape's press is not a stroke's press.** The same press, the same grid, the same
/// cell: a rectangle is born on 500 and a freedraw on 503. The grid rounds what a shape is
/// *placed* by and never touches what a stroke is *drawn* with.
#[test]
fn the_grid_rounds_a_shape_press_and_not_a_stroke_press() {
    let mut shapes = engine_with_scene(vec![]);
    let mut strokes = engine_with_scene(vec![]);
    shapes.set_grid(grid_on());
    strokes.set_grid(grid_on());

    let shape = shape_to(&mut shapes, (503.0, 300.0), (563.0, 340.0));
    let stroke = stroke_to(&mut strokes, (503.0, 300.0), (563.0, 340.0));

    assert_close_msg(shape.x, 500.0, "the shape is on the grid");
    assert_close_msg(stroke.x, 503.0, "and the stroke is not");
}

/// **The whole lattice, not one point.** Every one of these presses lands exactly where it
/// was put, and every one of them as a rectangle lands on a cell edge. A single example
/// could be an arithmetic coincidence; a sweep cannot.
#[test]
fn no_press_is_ever_rounded_and_a_shape_always_is() {
    // Inside one cell, at its far edge, at its centre, and in the next cell.
    for press in [501.0, 503.0, 507.0, 510.0, 511.0, 517.0, 519.0, 523.0] {
        let mut strokes = engine_with_scene(vec![]);
        strokes.set_grid(grid_on());
        let stroke = stroke_to(&mut strokes, (press, 300.0), (press + 40.0, 340.0));
        assert_close_msg(stroke.x, press, format!("a stroke at {press}"));

        let mut shapes = engine_with_scene(vec![]);
        shapes.set_grid(grid_on());
        let shape = shape_to(&mut shapes, (press, 300.0), (press + 40.0, 340.0));
        let cell = (press / 20.0).round() * 20.0;
        assert_close_msg(shape.x, cell, format!("a shape at {press}"));
    }
}

/// The tie is a whole cell and it breaks towards **+∞**, not away from zero. On a 20-grid
/// x = −10 belongs on 0, and `f64::round` would put it on −20
/// (`docs/reference/drawing.md` › "The tie is a whole cell"). A stroke must keep −10, and
/// the shape beside it must land on 0 — a test using only positive coordinates cannot see
/// that class of bug, and a snapping change has now had to relearn it twice.
#[test]
fn the_negative_half_cell_belongs_to_a_shape_and_not_to_a_stroke() {
    let mut strokes = engine_with_scene(vec![]);
    let mut shapes = engine_with_scene(vec![]);
    strokes.set_grid(grid_on());
    shapes.set_grid(grid_on());

    let stroke = stroke_to(&mut strokes, (-10.0, -10.0), (30.0, 30.0));
    let shape = shape_to(&mut shapes, (-10.0, -10.0), (30.0, 30.0));

    assert_close(stroke.x, -10.0);
    assert_close(stroke.y, -10.0);
    assert_close_msg(shape.x, 0.0, "a shape at −10 belongs on 0, not −20");
    assert_close(shape.y, 0.0);
}

// ------------------------------------------------------------------ the points

/// The first point is the origin, so the stroke starts under the press — again raw.
#[test]
fn the_stroke_starts_under_the_press() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_grid(grid_on());

    let stroke = stroke_to(&mut engine, (503.0, 300.0), (563.0, 340.0));

    assert_eq!(points_of(&stroke).first().copied(), Some([0.0, 0.0]));
}

/// Turning the grid off changes nothing about a freedraw press or its points. That is the
/// whole claim: the two halves of the oracle's rule agree, so the grid is not half of it.
#[test]
fn turning_the_grid_off_changes_nothing_about_a_stroke() {
    let mut snapped_grid = engine_with_scene(vec![]);
    let mut no_grid = engine_with_scene(vec![]);
    snapped_grid.set_grid(grid_on());
    no_grid.set_grid(GridSettings {
        enabled: false,
        snap: false,
        ..grid_on()
    });

    let on = stroke_to(&mut snapped_grid, (503.0, 300.0), (563.0, 340.0));
    let off = stroke_to(&mut no_grid, (503.0, 300.0), (563.0, 340.0));

    assert_close(on.x, off.x);
    assert_eq!(points_of(&on), points_of(&off));
}

/// Ctrl/Cmd releases the grid (`snap_gesture`, beside `snap`) — and for a freedraw it
/// releases nothing at all, which is the cheapest way to show that the press is not
/// *routed through* the grid gate and then opted out of, but was never on the gate.
#[test]
fn ctrl_releases_nothing_of_a_stroke() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_grid(grid_on());
    engine.set_ctrl_held(true);

    let stroke = stroke_to(&mut engine, (503.0, 300.0), (563.0, 340.0));

    assert_close(stroke.x, 503.0);
    assert_eq!(last_of(&stroke), [30.0, 20.0]);
}

/// A figure — a shape drawn freehand and recognised — is a freedraw with figure data, and
/// the oracle builds it with the very `newFreeDrawElement` the freedraw press does
/// (`App.tsx@1118751f:9914`), so it takes the same rule.
#[test]
fn a_figure_is_a_stroke_and_keeps_the_stroke_rule() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_grid(grid_on());
    engine.set_tool(DrawTool::AutoShape);

    engine.begin_pointer(503.0, 300.0, false, false);
    engine.move_pointer(563.0, 340.0, false, false);
    engine.end_pointer();

    let figure = engine.get_scene().pop().expect("no figure was drawn");
    assert_close(figure.x, 503.0);
    assert_eq!(last_of(&figure), [30.0, 20.0]);
}
