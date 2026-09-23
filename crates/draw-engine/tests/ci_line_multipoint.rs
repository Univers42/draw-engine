//! Placing a line point by point.
//!
//! A line was a single drag: press, pull, release, two points, done. That is one of the
//! two gestures Excalidraw offers and the less interesting one — the other is that a
//! *click* starts a path which keeps taking points until you say stop, and a path that
//! comes back to where it started closes into a shape. Every polygon on an Excalidraw
//! board is made that way; without it the only closed shapes available are the three on
//! the toolbar.
//!
//! The rules here are transcribed from the oracle at the SHA pinned in
//! `scripts/oracle-sha.txt`, and the two constants are theirs:
//!
//! - `LINE_CONFIRM_THRESHOLD = 8` (`packages/common/src/constants.ts:23`) — how near the
//!   point you just placed a click has to land to mean "finish here", and the radius the
//!   next segment refuses to grow inside.
//! - `MINIMUM_ARROW_SIZE = 20` (`constants.ts:22`) — below this the press-and-release was
//!   a *click*, which starts a path, rather than a drag, which draws one segment and
//!   ends (`App.tsx:11705-11745`).
//!
//! Two of their findings are worth stating because they are counter-intuitive:
//!
//! - **Double click is not handled at all.** `handleCanvasDoubleClick` returns early
//!   while a path is being placed (`App.tsx:7195-7199`). A double click ends the path
//!   anyway, and it is worth understanding why: its second click lands on the point its
//!   first click just placed, and clicking the last placed point is the rule that ends a
//!   path. So the gesture works without anybody implementing it, and it keeps working if
//!   the two clicks are slow enough not to be a double click at all.
//! - **Closing the loop keeps the closing point.** The preview point that follows the
//!   cursor is normally thrown away when the path ends, but a click that lands back on
//!   the first point commits it first (`App.tsx:10117-10134`), and then finalize snaps it
//!   exactly onto the first point so the loop stays shut at every zoom
//!   (`actionFinalize.tsx:308-325`).

mod common;
use common::*;
use draw_engine::*;

/// Excalidraw's `LINE_CONFIRM_THRESHOLD`. The viewport is 1:1, so screen and world agree.
const CONFIRM: f64 = 8.0;

fn line_engine() -> DrawEngine {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Line);
    engine
}

/// A press and release in one spot, which is how a point is placed.
fn click(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.begin_pointer(x, y, false, false);
    engine.end_pointer();
}

/// A pointer move with no button down — the path follows the cursor between clicks.
fn hover(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.move_pointer(x, y, false, false);
}

/// Places a point at each position in turn, leaving the path open.
fn place(engine: &mut DrawEngine, path: &[(f64, f64)]) {
    for &(x, y) in path {
        hover(engine, x, y);
        click(engine, x, y);
    }
}

fn only_line(engine: &DrawEngine) -> DrawElement {
    let lines: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == DrawElementType::Line)
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one line on the board");
    lines.into_iter().next().unwrap()
}

/// The line's points in world space, which is what the assertions are about.
fn world_points(engine: &DrawEngine) -> Vec<(f64, f64)> {
    let line = only_line(engine);
    line.points
        .unwrap_or_default()
        .iter()
        .map(|p| (line.x + p[0], line.y + p[1]))
        .collect()
}

fn assert_points(engine: &DrawEngine, expected: &[(f64, f64)]) {
    let actual = world_points(engine);
    assert_eq!(
        actual.len(),
        expected.len(),
        "expected {expected:?}, got {actual:?}"
    );
    for (got, want) in actual.iter().zip(expected) {
        assert!(
            (got.0 - want.0).abs() < 0.001 && (got.1 - want.1).abs() < 0.001,
            "expected {expected:?}, got {actual:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Starting a path
// ---------------------------------------------------------------------------

/// The whole premise. A click used to leave nothing at all: `end_linear` measured the
/// drag, found it degenerate and discarded the element, so the line tool could only ever
/// be used by dragging.
#[test]
fn a_click_starts_a_path_rather_than_drawing_nothing() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);

    assert!(
        engine.linear_in_progress().is_some(),
        "a click should have left a path waiting for its next point"
    );
    assert_eq!(
        engine.get_tool(),
        DrawTool::Line,
        "the tool must stay put while the path is still being placed"
    );
}

/// A path being placed is not a finished drawing, so it must not be reported as one.
#[test]
fn a_path_being_placed_is_not_selected() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    assert!(
        engine.get_selection().is_empty(),
        "selection handles over a path still being drawn would fight the next click"
    );
}

#[test]
fn the_path_follows_the_cursor_between_clicks() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    hover(&mut engine, 260.0, 100.0);

    assert_points(&engine, &[(100.0, 100.0), (260.0, 100.0)]);
}

/// The commit zone. Inside it the click means "finish", so a segment must not start
/// growing there — otherwise the gesture that ends a path also extends it.
#[test]
fn no_segment_grows_inside_the_commit_zone() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    hover(&mut engine, 100.0 + CONFIRM - 1.0, 100.0);

    assert_points(&engine, &[(100.0, 100.0)]);
}

// ---------------------------------------------------------------------------
// Extending it
// ---------------------------------------------------------------------------

#[test]
fn each_click_places_a_point() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0), (200.0, 200.0)]);
    hover(&mut engine, 300.0, 300.0);

    assert_points(
        &engine,
        &[
            (100.0, 100.0),
            (200.0, 100.0),
            (200.0, 200.0),
            // the preview, following the cursor
            (300.0, 300.0),
        ],
    );
}

/// Points are stored relative to the element, so the origin and the extent have to keep
/// up with a path that grows in any direction — including backwards past where it began.
#[test]
fn the_box_follows_a_path_that_grows_backwards() {
    let mut engine = line_engine();
    click(&mut engine, 300.0, 300.0);
    place(&mut engine, &[(200.0, 260.0), (100.0, 400.0)]);
    engine.finish_linear();

    let line = only_line(&engine);
    assert_close(line.x, 100.0);
    assert_close(line.y, 260.0);
    assert_close(line.width, 200.0);
    assert_close(line.height, 140.0);
    assert_points(&engine, &[(300.0, 300.0), (200.0, 260.0), (100.0, 400.0)]);
}

/// Backing out of a segment without placing it: Excalidraw takes the preview point away
/// again when the cursor comes back inside the last point's commit zone
/// (`App.tsx:8025-8060`).
#[test]
fn the_preview_backs_out_when_the_cursor_returns() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0)]);
    hover(&mut engine, 200.0, 200.0);
    // Sanity: the preview is out before the cursor comes back for it.
    assert_points(&engine, &[(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]);

    hover(&mut engine, 200.0 + CONFIRM - 2.0, 100.0);
    assert_points(&engine, &[(100.0, 100.0), (200.0, 100.0)]);
}

// ---------------------------------------------------------------------------
// Ending it
// ---------------------------------------------------------------------------

#[test]
fn clicking_the_point_just_placed_finishes_the_path() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0), (200.0, 200.0)]);
    click(&mut engine, 200.0, 200.0);

    assert!(
        engine.linear_in_progress().is_none(),
        "the path should be finished"
    );
    // The preview point is not kept.
    assert_points(&engine, &[(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]);
    assert_eq!(engine.get_tool(), DrawTool::Select, "the tool settles");
    assert_eq!(engine.get_selection().len(), 1, "and the path is selected");
}

/// The gesture the request names, and it needs no double-click handler of its own: the
/// second click of the pair lands on the point the first one placed.
#[test]
fn a_double_click_finishes_the_path() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0)]);
    hover(&mut engine, 300.0, 100.0);

    // The double click: two presses in one spot with no movement between them.
    click(&mut engine, 300.0, 100.0);
    click(&mut engine, 300.0, 100.0);

    assert!(engine.linear_in_progress().is_none());
    assert_points(&engine, &[(100.0, 100.0), (200.0, 100.0), (300.0, 100.0)]);
}

/// Escape finishes rather than discards. It is Excalidraw's binding for `actionFinalize`,
/// and the distinction matters: a path of six points thrown away by a reflexive Escape is
/// six points of work gone.
#[test]
fn escape_keeps_the_path_it_finishes() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0), (200.0, 200.0)]);
    hover(&mut engine, 400.0, 400.0);

    engine.finish_linear();

    assert!(engine.linear_in_progress().is_none());
    assert_points(&engine, &[(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]);
}

/// A double click on an empty board with the line tool: the first click starts a path,
/// the second lands on its only point and ends it. One point is not a path, so nothing is
/// left behind — an invisible one-point element on the board would be unselectable and
/// permanent.
#[test]
fn a_path_of_one_point_is_thrown_away() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    click(&mut engine, 100.0, 100.0);

    assert!(engine.get_scene().is_empty(), "nothing should be left");
    assert!(engine.linear_in_progress().is_none());
}

/// Reaching for another tool is an answer too, and leaving the path open would strand it:
/// nothing else would ever finish it, and the next click of the line tool would extend a
/// path from wherever it was abandoned.
#[test]
fn choosing_another_tool_finishes_the_path() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0), (200.0, 200.0)]);

    engine.set_tool(DrawTool::Rectangle);

    assert!(engine.linear_in_progress().is_none());
    assert_points(&engine, &[(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]);
}

// ---------------------------------------------------------------------------
// Closing it into a shape
// ---------------------------------------------------------------------------

/// "Making a shape of whatever we want": a path that comes back to its own beginning is
/// a polygon, and this is the only way to draw one that is not a rectangle, a diamond or
/// an ellipse.
#[test]
fn a_path_that_returns_to_its_start_closes_into_a_shape() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(300.0, 100.0), (300.0, 300.0)]);
    hover(&mut engine, 100.0 + 3.0, 100.0 - 2.0);
    click(&mut engine, 100.0 + 3.0, 100.0 - 2.0);

    assert!(engine.linear_in_progress().is_none());
    assert_points(
        &engine,
        &[
            (100.0, 100.0),
            (300.0, 100.0),
            (300.0, 300.0),
            // Snapped exactly onto the first point, not left where the click landed:
            // a loop closed to within three pixels comes apart when you zoom in.
            (100.0, 100.0),
        ],
    );
}

/// The closing rule needs three points before it can apply, or the second click of every
/// path — which is necessarily near the first point — would close a two-point loop.
#[test]
fn two_points_cannot_close_a_loop() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    hover(&mut engine, 140.0, 100.0);
    click(&mut engine, 140.0, 100.0);
    // Back near the start, with only two points placed.
    hover(&mut engine, 103.0, 100.0);

    assert!(
        engine.linear_in_progress().is_some(),
        "a two-point path has nothing to close"
    );
}

// ---------------------------------------------------------------------------
// A drag is still a drag
// ---------------------------------------------------------------------------

#[test]
fn a_drag_still_draws_one_segment_and_ends() {
    let mut engine = line_engine();
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(400.0, 220.0, false, false);
    engine.end_pointer();

    assert!(
        engine.linear_in_progress().is_none(),
        "a drag finishes on release"
    );
    assert_points(&engine, &[(100.0, 100.0), (400.0, 220.0)]);
    assert_eq!(engine.get_tool(), DrawTool::Select);
    assert_eq!(engine.get_selection().len(), 1);
}

/// The fork between the two gestures, at Excalidraw's own threshold. A drag shorter than
/// this is a click with a shaky hand, and treating it as a segment leaves a 12px line
/// nobody meant to draw.
#[test]
fn a_drag_shorter_than_the_minimum_starts_a_path_instead() {
    let mut engine = line_engine();
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(112.0, 100.0, false, false);
    engine.end_pointer();

    assert!(
        engine.linear_in_progress().is_some(),
        "a 12px drag is a click, and a click starts a path"
    );
}

/// Arrows take the same gesture — Excalidraw's multi-point handling is shared between
/// the two, and a bent arrow is drawn exactly this way.
#[test]
fn an_arrow_is_placed_point_by_point_too() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Arrow);
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0), (200.0, 200.0)]);
    engine.finish_linear();

    let arrow = engine.get_scene().into_iter().next().unwrap();
    assert_eq!(arrow.kind, DrawElementType::Arrow);
    assert_eq!(arrow.points.unwrap_or_default().len(), 3);
}

// ---------------------------------------------------------------------------
// A line is not an arrow
// ---------------------------------------------------------------------------

/// "The line should not rely or be stick to other elements."
///
/// Excalidraw agrees: `isBindingElementType` is `elementType === "arrow"` and nothing
/// else (`typeChecks.ts:178-182`). Binding a line meant a line drawn across a diagram
/// silently attached itself to whatever its ends happened to touch, and then moved on its
/// own whenever those shapes did.
#[test]
fn a_line_never_binds_to_a_shape() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Line);
    engine.begin_pointer(50.0, 30.0, false, false);
    engine.move_pointer(350.0, 30.0, false, false);
    engine.end_pointer();

    let line = only_line(&engine);
    assert_eq!(line.start_binding, None, "a line must not attach its tail");
    assert_eq!(line.end_binding, None, "nor its head");
}

/// The control. An arrow is the element binding exists for, and it has to keep working —
/// otherwise this is not a fix, it is a removal.
#[test]
fn an_arrow_still_binds() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(50.0, 30.0, false, false);
    engine.move_pointer(350.0, 30.0, false, false);
    engine.end_pointer();

    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow should exist");
    assert_eq!(arrow.start_binding.as_deref(), Some(a_id.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(b_id.as_str()));
}

/// The same guarantee as `an_arrow_still_binds`, but placed point by point instead of
/// dragged. The two gestures share `begin_linear`/`end_linear` up to the release and then
/// fork into `multi_linear.rs`, which has to keep offering the binding the drag gesture
/// does — a waypoint in open space in between should not lose it.
#[test]
fn a_multi_click_arrow_still_binds() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Arrow);

    // Click on A to start the path, a waypoint in open space, then finish on B.
    click(&mut engine, 50.0, 30.0);
    place(&mut engine, &[(200.0, 150.0)]);
    hover(&mut engine, 350.0, 30.0);
    click(&mut engine, 350.0, 30.0);
    engine.finish_linear();

    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow should exist");
    assert_eq!(arrow.start_binding.as_deref(), Some(a_id.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(b_id.as_str()));
}

/// The selection frame has to actually contain the arrow it is drawn around.
///
/// `x`/`y` is a linear element's **first point**, not a box corner
/// (`scene::geometry::is_point_based`) — reading `x + width` as an edge is wrong for any
/// arrow that does not run left-to-right or top-to-bottom, and `selection_corners_padded`
/// used to do exactly that. It read as fine for a two-point drag, where the sign of
/// `width` happens to keep the arithmetic correct, and broke silently for a bound,
/// waypoint-carrying one: the frame stayed pinned near the first point while the arrow
/// itself moved to its attach point underneath it — the selection sitting where the
/// arrow used to be, not where it now is.
#[test]
fn the_selection_frame_contains_a_bound_waypoint_arrow() {
    let a = box_at(300.0, 0.0, 100.0, 60.0);
    let b = box_at(600.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Arrow);

    // The waypoint sits well to the left of A, so the point the retargeted start aims
    // toward — its own adjacent point — pulls it leftward past the waypoint: exactly the
    // shape that leaves `x` (pinned to the first point, the retargeted start) short of
    // the box's true left edge.
    click(&mut engine, 350.0, 30.0);
    place(&mut engine, &[(100.0, 30.0)]);
    hover(&mut engine, 650.0, 30.0);
    click(&mut engine, 650.0, 30.0);

    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow should exist");
    assert!(arrow.start_binding.is_some(), "setup: bound at the start");
    assert!(arrow.end_binding.is_some(), "setup: bound at the end");

    assert_frame_contains_points(&arrow);
}

/// The same guarantee, after the shape moves again. `linear_retarget` runs on every move
/// of a bound shape, not only once at bind time, so a fix reaching only the selection
/// code and not every caller of it would still miss this — the exact "the div stays
/// where it was originally drawn, and moves the trail independently from the shape"
/// symptom this is guarding against.
#[test]
fn the_selection_frame_contains_a_bound_waypoint_arrow_after_the_shape_moves() {
    let a = box_at(300.0, 0.0, 100.0, 60.0);
    let b = box_at(600.0, 0.0, 100.0, 60.0);
    let a_id = a.id.clone();
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Arrow);

    click(&mut engine, 350.0, 30.0);
    place(&mut engine, &[(100.0, 30.0)]);
    hover(&mut engine, 650.0, 30.0);
    click(&mut engine, 650.0, 30.0);

    // Grab A off its top edge — y=10, clear of the arrow's own y=30, since every point
    // in this path shares that y and a grab on it would pick up the arrow instead
    // (transparent shapes are only grabbable by their outline unless selected first,
    // and an explicit select puts the whole frame up for it).
    engine.set_tool(DrawTool::Select);
    engine.select(vec![a_id]);
    engine.begin_pointer(350.0, 10.0, false, false);
    engine.move_pointer(350.0, 480.0, false, false);
    engine.end_pointer();

    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow should exist");
    assert!(
        arrow.start_binding.is_some() && arrow.end_binding.is_some(),
        "setup: still bound at both ends after the move"
    );

    assert_frame_contains_points(&arrow);
}

/// Every point of `element`, in world space, must fall within the padding-free selection
/// frame drawn around it — the same corners the painter strokes and the marquee/handles
/// are laid out from (`selection_corners_padded(element, 0.0)`).
fn assert_frame_contains_points(element: &DrawElement) {
    let corners = selection_corners_padded(element, 0.0);
    let min_x = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);

    for &[px, py] in element.points.as_deref().unwrap_or_default().iter() {
        let wx = element.x + px;
        let wy = element.y + py;
        assert!(
            wx >= min_x - 0.001 && wx <= max_x + 0.001,
            "a point at world x={wx} falls outside the selection frame [{min_x}, {max_x}]"
        );
        assert!(
            wy >= min_y - 0.001 && wy <= max_y + 0.001,
            "a point at world y={wy} falls outside the selection frame [{min_y}, {max_y}]"
        );
    }
}

/// Clicking a shape the pending point would bind to finishes the path right there — no
/// Enter, no Escape, no second click on the point just placed. The suggestion already
/// promised the attach; asking for a separate confirmation after it would make the
/// highlight a lie for one more click.
#[test]
fn clicking_a_bind_suggestion_finishes_the_path() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Arrow);

    click(&mut engine, 50.0, 30.0);
    hover(&mut engine, 350.0, 30.0);
    click(&mut engine, 350.0, 30.0);

    assert!(
        engine.linear_in_progress().is_none(),
        "clicking a suggested shape should have finished the path on its own"
    );
    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow should exist");
    assert_eq!(arrow.start_binding.as_deref(), Some(a_id.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(b_id.as_str()));
}

/// The same, with a waypoint first — the click-to-bind-and-finish has to work from any
/// point in the path, not only the second click overall.
#[test]
fn clicking_a_bind_suggestion_finishes_the_path_after_a_waypoint() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Arrow);

    click(&mut engine, 50.0, 30.0);
    place(&mut engine, &[(200.0, 150.0)]);
    hover(&mut engine, 350.0, 30.0);
    click(&mut engine, 350.0, 30.0);

    assert!(engine.linear_in_progress().is_none());
    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow should exist");
    assert_eq!(arrow.start_binding.as_deref(), Some(a_id.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(b_id.as_str()));
}

/// A click nowhere near a bindable shape must not finish anything — only landing on one
/// does. Otherwise every ordinary waypoint would end the path.
#[test]
fn a_click_in_open_space_does_not_finish_the_path() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![a]);
    engine.set_tool(DrawTool::Arrow);

    click(&mut engine, 50.0, 30.0);
    hover(&mut engine, 400.0, 400.0);
    click(&mut engine, 400.0, 400.0);

    assert!(
        engine.linear_in_progress().is_some(),
        "a click in open space is an ordinary waypoint, not a finish"
    );
}

/// A line never binds, so clicking on a shape while placing one is just an ordinary
/// waypoint that happens to land there — the path stays open.
#[test]
fn clicking_a_shape_does_not_finish_a_line() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Line);

    click(&mut engine, 50.0, 30.0);
    hover(&mut engine, 350.0, 30.0);
    click(&mut engine, 350.0, 30.0);

    assert!(
        engine.linear_in_progress().is_some(),
        "a line does not bind, so landing on a shape is not a reason to finish"
    );
}

/// A line already on the board must not be dragged about by a shape it happens to
/// overlap either. Binding is refreshed for the whole scene whenever anything moves, so
/// the rule has to hold there as well as at creation.
#[test]
fn a_moving_shape_does_not_drag_a_line_with_it() {
    let shape = box_at(0.0, 0.0, 100.0, 60.0);
    let shape_id = shape.id.clone();
    let mut engine = engine_with_scene(vec![shape]);
    engine.set_tool(DrawTool::Line);
    engine.begin_pointer(50.0, 30.0, false, false);
    engine.move_pointer(350.0, 30.0, false, false);
    engine.end_pointer();
    let before = world_points(&engine);

    // Grabbed well clear of the line. The shape is transparent, so its middle is a hole
    // and only the selection makes this point a grab target — but the line runs straight
    // through the middle of it, and aiming there picks the line up instead, which would
    // move the line and prove nothing.
    engine.set_tool(DrawTool::Select);
    engine.select(vec![shape_id]);
    engine.begin_pointer(20.0, 15.0, false, false);
    engine.move_pointer(20.0, 215.0, false, false);
    engine.end_pointer();

    assert_eq!(
        world_points(&engine),
        before,
        "the line should not have followed the shape"
    );
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

/// One path, one undo. Every click mutates the element, and recording each of them would
/// make undoing a six-point path six keystrokes — and the intermediate states are paths
/// nobody ever chose to have.
#[test]
fn one_undo_takes_the_whole_path_away() {
    let mut engine = line_engine();
    click(&mut engine, 100.0, 100.0);
    place(&mut engine, &[(200.0, 100.0), (200.0, 200.0)]);
    engine.finish_linear();
    assert_eq!(engine.get_scene().len(), 1);

    engine.undo();

    // Live elements: an undone creation is a tombstone now, so that the deletion
    // reaches the server and the peers as a stamped element (see `engine/stamp.rs`).
    assert!(
        engine
            .get_scene()
            .iter()
            .filter(|el| !el.is_deleted)
            .all(|el| el.kind != DrawElementType::Line),
        "one undo should take the whole path"
    );
}
