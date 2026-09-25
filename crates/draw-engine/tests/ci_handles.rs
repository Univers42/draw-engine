//! What a pointer can grab, and where.
//!
//! These cover the class of bug where the painter and the hit test disagree about the
//! selection UI. That disagreement is invisible by construction — the user aims at what
//! is drawn and gets whatever is hit-tested — so it needs tests rather than review.

mod common;
use common::*;
use draw_engine::*;

/// The layout the engine actually uses, at 1:1 zoom.
///
/// Excalidraw's numbers: a 4px frame margin, 8px handles whose inner edge is on that
/// frame, so centres sit 8px out and each reaches 4px.
fn layout() -> HandleLayout {
    HandleLayout::screen(8.0, 26.0, 1.0)
}

fn kinds(points: &[HandlePoint]) -> Vec<HandleKind> {
    points.iter().map(|p| p.kind).collect()
}

/// A shape wide and tall enough gets all eight resize handles plus rotation.
#[test]
fn a_large_shape_offers_every_handle() {
    let element = box_at(0.0, 0.0, 300.0, 200.0);
    let got = kinds(&selection_handles(&element, layout()));
    for kind in RESIZE_HANDLES {
        assert!(got.contains(&kind), "{kind:?} missing on a large shape");
    }
    assert!(got.contains(&HandleKind::Rotate));
}

/// Excalidraw's `minimumSizeForEightHandles`: on a short edge the side handle sits so
/// close to both corners that it cannot be aimed at, and only steals drags meant for a
/// corner. Below the threshold it is dropped.
#[test]
fn a_short_edge_drops_its_side_handles() {
    // 30 wide is under 5 * 8 = 40, so no north/south. 200 tall keeps east/west.
    let element = box_at(0.0, 0.0, 30.0, 200.0);
    let got = kinds(&selection_handles(&element, layout()));

    assert!(!got.contains(&HandleKind::N));
    assert!(!got.contains(&HandleKind::S));
    assert!(got.contains(&HandleKind::E));
    assert!(got.contains(&HandleKind::W));
    assert!(got.contains(&HandleKind::Nw));
    assert!(got.contains(&HandleKind::Rotate));
}

#[test]
fn a_tiny_shape_keeps_only_its_corners() {
    let element = box_at(0.0, 0.0, 12.0, 12.0);
    let got = kinds(&selection_handles(&element, layout()));
    assert_eq!(
        got,
        vec![
            HandleKind::Nw,
            HandleKind::Ne,
            HandleKind::Se,
            HandleKind::Sw,
            HandleKind::Rotate,
        ]
    );
}

/// The regression that started this: a shape has to be grabbable.
///
/// With the frame sitting *on* the outline, every point inside a small shape was also
/// inside a handle, so picking one up always resized it instead. Padding the frame
/// outward is what gives the element back its own interior.
#[test]
fn a_small_shape_can_be_grabbed_without_resizing_it() {
    let element = box_at(0.0, 0.0, 16.0, 16.0);
    let handles = selection_handles(&element, layout());
    // The centre, and every point on the element's own outline.
    let probes = [
        (8.0, 8.0),
        (0.0, 0.0),
        (16.0, 0.0),
        (16.0, 16.0),
        (0.0, 16.0),
        (8.0, 0.0),
        (0.0, 8.0),
    ];
    for (x, y) in probes {
        assert_eq!(
            hit_handle(&handles, x, y, layout().hit),
            None,
            "({x}, {y}) on a 16x16 shape should move it, not resize it"
        );
    }
}

/// The corner is still reachable — just from outside the shape, where the handle is
/// drawn.
#[test]
fn the_padded_corner_is_still_grabbable() {
    let element = box_at(0.0, 0.0, 16.0, 16.0);
    let handles = selection_handles(&element, layout());
    // Centres sit 8px out, so the north-west handle is at (-8, -8).
    assert_eq!(
        hit_handle(&handles, -8.0, -8.0, layout().hit),
        Some(HandleKind::Nw)
    );
}

/// The property the whole layout exists for: a handle must never reach back over the
/// element it belongs to, or the outline stops being a place you can grab to move it.
///
/// The bound is the distance from the handle centre to the element's edge, and *not* to
/// the frame line drawn around it — which is what this used to assert, on the strength of
/// a claim that Excalidraw "leaves a clear 4px ring". It does not: their corner grab box
/// starts two pixels outside the element and so overlaps their own dashed frame, which
/// sits four pixels out (`transformHandles.ts@1118751f:138-200`, `dashedLineMargin` against
/// `centeringOffset`). Holding the reach inside the frame made the target smaller than
/// the handle that was painted on top of it.
#[test]
fn no_handle_reaches_back_inside_the_element() {
    let l = layout();
    assert!(
        l.hit <= l.handle_offset,
        "reach {} must not cross the {}px from the handle centre to the element edge",
        l.hit,
        l.handle_offset
    );

    let element = box_at(0.0, 0.0, 300.0, 200.0);
    let handles = selection_handles(&element, l);
    // Every point on the element's own outline, including the edge midpoints where the
    // side handles live.
    let probes = [
        (150.0, 0.0),
        (150.0, 200.0),
        (0.0, 100.0),
        (300.0, 100.0),
        (0.0, 0.0),
        (300.0, 0.0),
        (300.0, 200.0),
        (0.0, 200.0),
    ];
    for (x, y) in probes {
        assert_eq!(
            hit_handle(&handles, x, y, l.hit),
            None,
            "({x}, {y}) is on the element's outline and must move it, not resize it"
        );
    }
}

/// Reach is radial, so a rotated element behaves the same in every direction. The old
/// box test was aligned to the world axes: it reached 1.41x as far diagonally as it did
/// along an axis, and did not turn with the shape.
#[test]
fn handle_reach_is_radial_not_axis_aligned() {
    let element = box_at(0.0, 0.0, 200.0, 200.0);
    let handles = selection_handles(&element, layout());
    let nw = handles.iter().find(|h| h.kind == HandleKind::Nw).unwrap();

    // Just inside the radius, on the diagonal.
    let d = 3.5 / std::f64::consts::SQRT_2;
    assert_eq!(
        hit_handle(&handles, nw.x + d, nw.y + d, 4.0),
        Some(HandleKind::Nw)
    );
    // Just outside it, on the same diagonal. A square test would have accepted this,
    // because each axis is still well within 4.
    let d = 4.5 / std::f64::consts::SQRT_2;
    assert_eq!(hit_handle(&handles, nw.x + d, nw.y + d, 4.0), None);
}

/// When two handles overlap, the nearer one wins rather than whichever came first in the
/// list — otherwise a corner permanently shadows the rotation handle above it.
#[test]
fn the_nearest_handle_wins() {
    let element = box_at(0.0, 0.0, 200.0, 100.0);
    let handles = selection_handles(&element, layout());
    let rotate = handles
        .iter()
        .find(|h| h.kind == HandleKind::Rotate)
        .unwrap();
    assert_eq!(
        hit_handle(&handles, rotate.x, rotate.y, 400.0),
        Some(HandleKind::Rotate),
        "a reach covering every handle must still pick the one under the pointer"
    );
}

/// A shape dragged out leftwards has negative width. Its "west" handle must stay west.
#[test]
fn negative_extent_does_not_mirror_the_handles() {
    let element = box_at(100.0, 100.0, -60.0, -40.0);
    let handles = selection_handles(&element, layout());
    let nw = handles.iter().find(|h| h.kind == HandleKind::Nw).unwrap();
    let se = handles.iter().find(|h| h.kind == HandleKind::Se).unwrap();
    assert!(nw.x < se.x, "north-west must sit left of south-east");
    assert!(nw.y < se.y, "north-west must sit above south-east");
}

/// The frame is drawn from the padded corners, and the handles are placed on them.
#[test]
fn the_frame_and_the_handles_share_their_corners() {
    let element = box_at(10.0, 20.0, 120.0, 80.0);
    let l = layout();
    // The handles are placed on the *handle* ring, which is further out than the frame.
    let corners = selection_corners_padded(&element, l.handle_offset);
    let handles = selection_handles(&element, l);

    for (kind, corner) in [
        (HandleKind::Nw, corners[0]),
        (HandleKind::Ne, corners[1]),
        (HandleKind::Se, corners[2]),
        (HandleKind::Sw, corners[3]),
    ] {
        let handle = handles.iter().find(|h| h.kind == kind).unwrap();
        assert_close(handle.x, corner.x);
        assert_close(handle.y, corner.y);
    }
}

/// Rotation carries the frame with it, so the handles stay on the shape's real corners.
#[test]
fn rotation_carries_the_handles() {
    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    element.angle = std::f64::consts::FRAC_PI_2;
    let handles = selection_handles(&element, layout());
    let nw = handles.iter().find(|h| h.kind == HandleKind::Nw).unwrap();
    // A quarter turn sends the top-left corner to the top-right.
    assert_close(nw.x, 100.0 + layout().handle_offset);
    assert_close(nw.y, -layout().handle_offset);
}

// -----------------------------------------------------------------------------
// how close you have to be
// -----------------------------------------------------------------------------
//
// A handle is drawn eight pixels across, and the reach used to be four — the drawn
// radius. So the target was the painted square and nothing more, and a grab three pixels
// inside it fell through to the element and *moved the shape* instead of resizing it,
// while a grab four pixels outside did nothing at all. Both read as "the handles do not
// work", and the second is worse than the first because at least a move is visible.
//
// The linear handles have always used `HANDLE_HIT_PX` — ten — which is why dragging the
// end of an arrow has never had this problem. The box handles ignored it and used half
// their own drawn size. One question, two answers, again.
//
// Reach is not the same as size: what is drawn says "here is the thing", what is hit says
// "and here is how near you must be to take hold of it".
//
// The number comes from Excalidraw, worked out from their geometry rather than guessed.
// `getTransformHandles` places a corner's grab box at `x2 + dashedLineMargin -
// centeringOffset`, which at 1:1 for a mouse is `x2 + 4 - 2`, and the box is
// `transformHandleSizes.mouse` = 8 across (`transformHandles.ts@1118751f:49-53, :138-200`). So
// their grab region spans two to ten pixels outside the element and reaches **half the
// handle's diagonal** — 5.66px — from its centre, which is the direction a person aims
// from at a corner. Ours is radial rather than a box, deliberately: their box stays
// square to the world while the painted handle turns with the element, so a rotated
// shape's corner can be grabbed from 1.41x the distance on one side and not at all on
// the other.

/// The reach a grab has, at 1:1, in world units.
fn reach() -> f64 {
    layout().hit
}

#[test]
fn a_handle_can_be_grabbed_from_further_than_it_is_drawn() {
    // The drawn handle is `HANDLE_PX` across, so it reaches 4 from its centre. A person
    // aiming at it misses by a few pixels routinely, and a miss is not a near miss — it
    // silently becomes a different gesture.
    assert!(
        reach() > 4.0,
        "the reach must exceed the drawn half-size, or the target is only the ink itself"
    );
}

#[test]
fn a_grab_a_few_pixels_off_still_finds_the_handle() {
    let element = box_at(0.0, 0.0, 200.0, 150.0);
    let handles = selection_handles(&element, layout());
    let se = handles
        .iter()
        .find(|p| p.kind == HandleKind::Se)
        .expect("a 200x150 shape has a south-east corner");

    // Straight at it, and then missing by six pixels diagonally — which is about what a
    // hand does at speed, and used to be a move.
    for (dx, dy, why) in [
        (0.0, 0.0, "dead centre"),
        (5.0, 0.0, "five out along one axis"),
        (-4.0, -4.0, "inwards, towards the shape"),
        (4.0, 4.0, "outwards, away from it"),
    ] {
        assert_eq!(
            hit_handle(&handles, se.x + dx, se.y + dy, reach()),
            Some(HandleKind::Se),
            "a grab {why} should still take the corner"
        );
    }
}

#[test]
fn the_reach_still_stops_somewhere() {
    // Generous is not unbounded: a handle that answers from far away steals the clicks
    // meant for the shape itself, and then the shape cannot be moved.
    let element = box_at(0.0, 0.0, 200.0, 150.0);
    let handles = selection_handles(&element, layout());
    let se = handles.iter().find(|p| p.kind == HandleKind::Se).unwrap();
    assert_eq!(
        hit_handle(&handles, se.x + 40.0, se.y + 40.0, reach()),
        None,
        "well away from every handle is not a handle"
    );
}

#[test]
fn neighbouring_handles_do_not_swallow_each_other() {
    // The reach is radial and the corners of a small shape are close together, so a wider
    // reach must not make one corner answer for its neighbour — that would resize the
    // wrong way round, which is harder to understand than nothing happening.
    let element = box_at(0.0, 0.0, 60.0, 60.0);
    let handles = selection_handles(&element, layout());
    for point in &handles {
        assert_eq!(
            hit_handle(&handles, point.x, point.y, reach()),
            Some(point.kind),
            "{:?} must answer for its own centre",
            point.kind
        );
    }
}

#[test]
fn the_reach_is_half_the_handles_diagonal() {
    // Excalidraw's corner reach, arrived at from their box rather than copied as a
    // number: an 8px square grabbed from its centre reaches 5.66px at the corners.
    let want = 8.0 * std::f64::consts::SQRT_2 / 2.0;
    assert!(
        (reach() - want).abs() < 1e-9,
        "reach {} want {want}",
        reach()
    );
}

#[test]
fn the_reach_shrinks_with_the_zoom_so_it_stays_constant_on_screen() {
    // `hit` is in world units; at 2x zoom a screen pixel is half a world unit, so the
    // reach must halve to stay the same distance under the hand.
    let zoomed = HandleLayout::screen(8.0, 26.0, 2.0);
    assert!((zoomed.hit - HandleLayout::screen(8.0, 26.0, 1.0).hit / 2.0).abs() < 1e-9);
}

// -----------------------------------------------------------------------------
// a polygon is a shape, not a path
// -----------------------------------------------------------------------------
//
// A line element with more than two points gets a bounding box with resize and rotation
// handles, exactly like any other shape. Only a *two-point* line is grabbed by its ends,
// because a box around one is degenerate — for a dead-horizontal arrow it has no height,
// so every handle lands on the same spot — and because "point this end somewhere else"
// is not something a box can express.
//
// Excalidraw's rule, and it is a count and not a kind: `hasBoundingBox` returns
// `element.points.length > 2` for a linear element (`transformHandles.ts@1118751f:352`), and the
// point circles are drawn only while the line editor is open or the line has exactly two
// points (`interactiveScene.ts@1118751f:1198`, `:1628`).
//
// Ours was kind-based, so *every* line and arrow was grabbed by its points however many
// it had. The element that made this matter is the bucket fill: it is a closed line of
// five points or more, so selecting it produced a circle on every vertex of the region
// and no box at all — it could not be resized, could not be rotated, and the circles
// dragged single corners of the paint away from the outline they were traced from.

/// A closed line carrying a background — which is what a bucket fill is, and the element
/// this whole section is about. Built the way the engine builds one: `points[0]` at the
/// origin, and `width`/`height` the span the ring actually covers.
fn poly_line(points: &[(f64, f64)]) -> DrawElement {
    let (ox, oy) = points[0];
    let local: Vec<[f64; 2]> = points.iter().map(|&(x, y)| [x - ox, y - oy]).collect();
    let span = |axis: usize| {
        let lo = local.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let hi = local
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        hi - lo
    };
    let mut element = create_element_default(
        DrawElementType::Line,
        Geometry {
            x: ox,
            y: oy,
            width: span(0),
            height: span(1),
        },
    );
    element.points = Some(local);
    element.background_color = "#b2f2bb".into();
    element.stroke_color = "transparent".into();
    element
}

#[test]
fn a_two_point_line_is_still_grabbed_by_its_ends() {
    let line = poly_line(&[(0.0, 0.0), (100.0, 0.0)]);
    assert!(
        draw_engine::selection::linear::is_point_edited(&line),
        "a straight line has no box worth drawing — it would have no height at all"
    );
}

#[test]
fn a_polygon_is_grabbed_by_a_box() {
    let polygon = poly_line(&[
        (0.0, 0.0),
        (100.0, 0.0),
        (100.0, 80.0),
        (0.0, 80.0),
        (0.0, 0.0),
    ]);
    assert!(
        !draw_engine::selection::linear::is_point_edited(&polygon),
        "five points enclose an area, and an area is resized by its box"
    );
    let handles = selection_handles(&polygon, layout());
    assert!(
        kinds(&handles).contains(&HandleKind::Rotate),
        "and it can be turned, like anything else with a shape"
    );
}

/// Through the engine: the thing the person actually does.
#[test]
fn a_polygon_can_be_resized_by_its_corner() {
    let polygon = poly_line(&[
        (0.0, 0.0),
        (200.0, 0.0),
        (200.0, 150.0),
        (0.0, 150.0),
        (0.0, 0.0),
    ]);
    let id = polygon.id.clone();
    let mut engine = engine_with_scene(vec![polygon]);
    engine.select(vec![id.clone()]);

    // The south-east handle sits `handle_offset` beyond the corner.
    let l = layout();
    engine.begin_pointer(
        200.0 + l.handle_offset,
        150.0 + l.handle_offset,
        false,
        false,
    );
    engine.move_pointer(
        300.0 + l.handle_offset,
        230.0 + l.handle_offset,
        false,
        false,
    );
    engine.end_pointer();

    let after = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == id)
        .expect("still there");
    let points = after.points.as_deref().expect("still a polygon");
    let width = points.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
    assert!(
        width > 250.0,
        "dragging the corner should have stretched the ring, got {width}"
    );
}

/// The points are not lost — they are behind a double click, as they are upstream.
#[test]
fn double_clicking_a_polygon_opens_its_points() {
    let polygon = poly_line(&[
        (0.0, 0.0),
        (200.0, 0.0),
        (200.0, 150.0),
        (0.0, 150.0),
        (0.0, 0.0),
    ]);
    let id = polygon.id.clone();
    let mut engine = engine_with_scene(vec![polygon]);
    engine.select(vec![id.clone()]);
    assert!(engine.linear_points().is_empty(), "a box to begin with");

    engine.handle_double_click(100.0, 75.0);
    assert!(
        !engine.linear_points().is_empty(),
        "double clicking the paint should offer its corners"
    );

    // Dragging one now moves that corner rather than the whole thing.
    engine.begin_pointer(200.0, 0.0, false, false);
    engine.move_pointer(260.0, -40.0, false, false);
    engine.end_pointer();
    let after = engine.get_scene().into_iter().find(|e| e.id == id).unwrap();
    assert_eq!(
        after.points.as_deref().unwrap()[1],
        [260.0, -40.0],
        "the corner followed the pointer"
    );
}

#[test]
fn letting_go_of_the_polygon_closes_its_points() {
    let polygon = poly_line(&[
        (0.0, 0.0),
        (200.0, 0.0),
        (200.0, 150.0),
        (0.0, 150.0),
        (0.0, 0.0),
    ]);
    let id = polygon.id.clone();
    let mut engine = engine_with_scene(vec![polygon]);
    engine.select(vec![id]);
    engine.handle_double_click(100.0, 75.0);
    assert!(!engine.linear_points().is_empty());

    engine.clear_selection();
    assert!(
        engine.linear_points().is_empty(),
        "deselecting it must close the editor, or the next thing selected shows its corners"
    );
}

/// The same drag, delivered as a person delivers it.
///
/// A pointer sends a stream of moves, not one. Scaling the ring from the *live* element
/// means every move after the first divides by a span an earlier move already stretched,
/// so the ring drifts out from under the box: measured at 235 against a box of 288. A
/// single `move_pointer` cannot see it — the first version of this test had exactly one
/// and passed.
#[test]
fn a_ring_keeps_up_with_the_box_across_a_whole_drag() {
    let polygon = poly_line(&[
        (0.0, 0.0),
        (200.0, 0.0),
        (200.0, 150.0),
        (0.0, 150.0),
        (0.0, 0.0),
    ]);
    let id = polygon.id.clone();
    let mut engine = engine_with_scene(vec![polygon]);
    engine.select(vec![id.clone()]);

    let l = layout();
    let (sx, sy) = (200.0 + l.handle_offset, 150.0 + l.handle_offset);
    engine.begin_pointer(sx, sy, false, false);
    for step in 1..=8 {
        let t = f64::from(step) / 8.0;
        engine.move_pointer(sx + 100.0 * t, sy + 80.0 * t, false, false);
    }
    engine.end_pointer();

    let after = engine.get_scene().into_iter().find(|e| e.id == id).unwrap();
    let points = after.points.as_deref().unwrap();
    let span = |axis: usize| {
        let lo = points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let hi = points
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        hi - lo
    };
    // The ring and the box agree — the property this test exists for.
    assert_close(span(0), after.width.abs());
    assert_close(span(1), after.height.abs());
    // And they agree on the right number: the corner moved exactly as far as the pointer
    // did, wherever in the handle the pointer took hold of it.
    assert_close(after.width, 200.0 + 100.0);
    assert_close(after.height, 150.0 + 80.0);
}

// -----------------------------------------------------------------------------
// where in the handle it was taken
// -----------------------------------------------------------------------------
//
// A handle is drawn `handle_offset` outside the edge it moves. The oracle records where
// the press landed relative to that edge (`getResizeOffsetXY`,
// `packages/element/src/resizeElements.ts@1118751f:497-554`, taken at pointer-down in
// `App.tsx@1118751f:9406-9416`) and takes it off every move (`:13580-13581`), so the edge
// moves by exactly what the pointer does. Put at the raw pointer instead, the edge leapt
// the eight pixels to the handle the moment it was pulled.

/// Presses `handle` of the only selected element where it is drawn, moves by `by` in two
/// steps, and returns the element as the drag leaves it.
fn pull(element: DrawElement, handle: HandleKind, by: (f64, f64)) -> DrawElement {
    let id = element.id.clone();
    let at = selection_handles(&element, layout())
        .into_iter()
        .find(|h| h.kind == handle)
        .expect("the handle is drawn");
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec![id.clone()]);
    engine.begin_pointer(at.x, at.y, false, false);
    engine.move_pointer(at.x + by.0 / 2.0, at.y + by.1 / 2.0, false, false);
    engine.move_pointer(at.x + by.0, at.y + by.1, false, false);
    engine.end_pointer();
    engine.get_scene().into_iter().find(|e| e.id == id).unwrap()
}

#[test]
fn a_handle_taken_where_it_is_drawn_does_not_jump() {
    for handle in [HandleKind::Se, HandleKind::E, HandleKind::Nw, HandleKind::N] {
        let after = pull(box_at(100.0, 100.0, 200.0, 150.0), handle, (0.0, 0.0));
        assert_close(after.x, 100.0);
        assert_close(after.y, 100.0);
        assert_close(after.width, 200.0);
        assert_close(after.height, 150.0);
    }
}

#[test]
fn the_edge_moves_as_far_as_the_pointer() {
    let after = pull(
        box_at(100.0, 100.0, 200.0, 150.0),
        HandleKind::Se,
        (30.0, 20.0),
    );
    assert_close(after.width, 230.0);
    assert_close(after.height, 170.0);
    let after = pull(
        box_at(100.0, 100.0, 200.0, 150.0),
        HandleKind::W,
        (-30.0, 50.0),
    );
    assert_close(after.x, 70.0);
    assert_close(after.width, 230.0);
    assert_close(after.height, 150.0);
}

/// The grab is measured in the element's own frame, so a turned element's handle does
/// not jump either.
#[test]
fn a_turned_elements_handle_does_not_jump() {
    let mut element = box_at(100.0, 100.0, 200.0, 150.0);
    element.angle = std::f64::consts::FRAC_PI_2;
    let after = pull(element, HandleKind::Se, (0.0, 0.0));
    assert_close(after.x, 100.0);
    assert_close(after.y, 100.0);
    assert_close(after.width, 200.0);
    assert_close(after.height, 150.0);
}

/// A line's `x`, `y` is its first point, which need not be a corner of the box its handles
/// are drawn on. The resize is measured from that box, as the oracle's is
/// (`previousOrigin`, `resizeElements.ts@1118751f:848-851`): taken where it is drawn, the
/// handle moves nothing, and a pull moves only the edges it holds.
#[test]
fn a_lines_handle_does_not_jump_when_its_first_point_is_no_corner() {
    for points in [
        [(100.0, 150.0), (200.0, 100.0), (300.0, 150.0)],
        [(300.0, 150.0), (200.0, 100.0), (100.0, 150.0)],
        [(200.0, 100.0), (300.0, 150.0), (100.0, 150.0)],
    ] {
        for handle in [HandleKind::Se, HandleKind::Nw, HandleKind::E] {
            let line = poly_line(&points);
            let bounds = |element: &DrawElement| {
                let b = element_bounds(element);
                [b.min_x, b.min_y, b.max_x, b.max_y]
            };
            assert_eq!(bounds(&line), [100.0, 100.0, 300.0, 150.0]);
            let after = pull(line.clone(), handle, (0.0, 0.0));
            for (got, want) in bounds(&after).into_iter().zip([100.0, 100.0, 300.0, 150.0]) {
                assert!(
                    (got - want).abs() < 1e-9,
                    "{points:?} {handle:?}: {:?}",
                    bounds(&after)
                );
            }
            let after = pull(line, handle, (20.0, 20.0));
            let want = match handle {
                HandleKind::Se => [100.0, 100.0, 320.0, 170.0],
                HandleKind::Nw => [120.0, 120.0, 300.0, 150.0],
                _ => [100.0, 100.0, 320.0, 150.0],
            };
            for (got, want) in bounds(&after).into_iter().zip(want) {
                assert!(
                    (got - want).abs() < 1e-9,
                    "{points:?} {handle:?}: {:?}",
                    bounds(&after)
                );
            }
        }
    }
}

/// With the grid on, a press takes the handle the cursor shows over it. Handles are hit
/// where the pointer is, as the oracle's are (`pointerDownState.origin`,
/// `App.tsx@1118751f:9220`, `:9366-9372`, `:9397-9404`); only what the drag then places
/// lands on the grid. Hit where the grid put the press, a handle a few pixels off a grid
/// line could not be taken: the press moved the shape instead.
#[test]
fn with_the_grid_on_a_press_takes_the_handle_the_cursor_shows() {
    let grid = GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: true,
    };
    // Every handle below sits 9 or so off the grid: snapped, the press would land out of
    // its reach.
    let rect = box_at(91.0, 91.0, 200.0, 150.0);
    let text = text_box(91.0, 91.0, 194.0, 25.0);
    let line = connector(91.0, 91.0, 289.0, 189.0, DrawElementType::Line);
    let group = [
        box_at(91.0, 91.0, 100.0, 90.0),
        box_at(201.0, 91.0, 80.0, 40.0),
    ];
    let cases: Vec<(&str, Vec<DrawElement>, (f64, f64))> = vec![
        ("a corner", vec![rect.clone()], (299.0, 249.0)),
        ("a text's side", vec![text], (289.0, 103.5)),
        ("a line's end", vec![line], (289.0, 189.0)),
        ("a group's corner", group.to_vec(), (289.0, 189.0)),
    ];
    for (what, elements, (x, y)) in cases {
        let ids: Vec<String> = elements.iter().map(|e| e.id.clone()).collect();
        let mut engine = engine_with_scene(elements);
        engine.set_grid(grid);
        engine.select(ids);
        let shown = engine.hover_cursor(x, y);
        assert!(
            !matches!(shown, HoverCursor::Default | HoverCursor::Move),
            "{what}: no handle under ({x}, {y})"
        );
        engine.begin_pointer(x, y, false, false);
        assert_eq!(engine.hover_cursor(x, y), shown, "{what}");
        engine.end_pointer();
    }

    // A radius handle, placed 9 off the grid on both axes.
    let at = |engine: &DrawEngine| engine.paint_view().radius_handles[0];
    let probe = {
        let mut engine = engine_with_scene(vec![filled(rect.clone())]);
        engine.select(vec![rect.id.clone()]);
        at(&engine)
    };
    let off = |v: f64| 9.0 - v.rem_euclid(20.0);
    let rect = filled(box_at(
        91.0 + off(probe.x),
        91.0 + off(probe.y),
        200.0,
        150.0,
    ));
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.set_grid(grid);
    engine.select(vec![rect.id]);
    let handle = at(&engine);
    assert_eq!(
        engine.hover_cursor(handle.x, handle.y),
        HoverCursor::PointHandle
    );
    engine.begin_pointer(handle.x, handle.y, false, false);
    assert_eq!(
        engine.hover_cursor(handle.x, handle.y),
        HoverCursor::PointHandle,
        "a radius handle"
    );
}

// -----------------------------------------------------------------------------
// a text's handles
// -----------------------------------------------------------------------------
//
// The oracle draws a text's four corners and its rotation handle, never its sides
// (`DEFAULT_OMIT_SIDES`, `transformHandles.ts@1118751f:58-63`, on a desktop); a side is
// taken by the frame line itself, anywhere within `SIDE_RESIZING_THRESHOLD` of it
// (`resizeTest.ts@1118751f:96-121`). A one-line text has no room for a side handle
// anyway, and its sides are what wrap it.

fn text_box(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    let mut text = text_at(x, y, width, height);
    text.text = Some("hello".into());
    text
}

#[test]
fn a_text_shows_its_corners_and_its_rotation_handle_only() {
    let got = kinds(&selection_handles(
        &text_box(0.0, 0.0, 300.0, 200.0),
        layout(),
    ));
    assert_eq!(
        got,
        vec![
            HandleKind::Nw,
            HandleKind::Ne,
            HandleKind::Se,
            HandleKind::Sw,
            HandleKind::Rotate,
        ]
    );
}

#[test]
fn a_texts_sides_are_taken_on_its_frame_line() {
    let text = text_box(100.0, 100.0, 194.0, 25.0);
    let id = text.id.clone();
    let mut engine = engine_with_scene(vec![text]);
    engine.select(vec![id]);
    // The frame line is 4px out; within 4px of it on either side is the side.
    for (x, y, want) in [
        (298.0, 112.5, HoverCursor::ResizeEw),
        (301.5, 112.5, HoverCursor::ResizeEw),
        (295.0, 112.5, HoverCursor::ResizeEw),
        (96.0, 112.5, HoverCursor::ResizeEw),
        (200.0, 96.0, HoverCursor::ResizeNs),
        (200.0, 129.0, HoverCursor::ResizeNs),
    ] {
        assert_eq!(engine.hover_cursor(x, y), want, "at ({x}, {y})");
    }
    assert_ne!(engine.hover_cursor(302.5, 112.5), HoverCursor::ResizeEw);
}
