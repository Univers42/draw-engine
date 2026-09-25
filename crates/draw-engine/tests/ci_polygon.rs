//! A closed line as a filled polygon — Excalidraw's `ExcalidrawLineElement.polygon`
//! (`packages/element/src/types.ts@1118751f:382`).
//!
//! The drawing-gesture side (closing on the first point, and the degenerate two-vertex
//! case that does not count) lives beside the rest of multi-point drawing in
//! `ci_line_multipoint.rs`. This file covers what happens once a line already is one: the
//! panel toggle, the round trip through JSON, hit-testing its interior, what it paints,
//! and how an older document that never carried the field comes back.

mod common;
use common::*;
use draw_engine::*;

fn triangle() -> DrawElement {
    let mut el = create_element_default(
        DrawElementType::Line,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    );
    el.points = Some(vec![[0.0, 0.0], [100.0, 0.0], [50.0, 100.0], [0.0, 0.0]]);
    el.polygon = Some(true);
    // `create_element_default` carries the general style default (`roundness: Some(8.0)`,
    // meant for a rectangle's rounded corner), not what the line tool actually draws with.
    // Sharp here, or `element_drawable` takes the curved-arc branch instead of the
    // straight polygon/linear-path one these tests are about.
    el.roundness = None;
    el
}

fn open_triangle() -> DrawElement {
    let mut el = triangle();
    el.points = Some(vec![[0.0, 0.0], [100.0, 0.0], [50.0, 100.0], [10.0, 90.0]]);
    el.polygon = None;
    el
}

// ---------------------------------------------------------------------------
// The toggle
// ---------------------------------------------------------------------------

#[test]
fn the_toggle_is_offered_only_for_lines_with_four_or_more_points() {
    let mut three_points = open_triangle();
    three_points.points = Some(vec![[0.0, 0.0], [100.0, 0.0], [50.0, 100.0]]);
    let id = three_points.id.clone();
    let mut engine = engine_with_scene(vec![three_points]);
    engine.select(vec![id]);
    assert!(!engine.can_toggle_polygon(), "only three points so far");

    let four_points = open_triangle(); // four points, open
    let id = four_points.id.clone();
    let mut engine = engine_with_scene(vec![four_points]);
    engine.select(vec![id]);
    assert!(engine.can_toggle_polygon());
}

#[test]
fn toggling_closes_an_open_line_into_a_filled_polygon() {
    let mut open = open_triangle();
    open.background_color = "#ffec99".into();
    let id = open.id.clone();
    let mut engine = engine_with_scene(vec![open]);
    engine.select(vec![id.clone()]);

    assert!(engine.can_toggle_polygon());
    engine.toggle_polygon_selection();

    let line = engine.get_scene().into_iter().find(|e| e.id == id).unwrap();
    assert!(line.is_polygon());
    let points = line.points.unwrap();
    assert_eq!(*points.first().unwrap(), *points.last().unwrap());
    // The background survives closing — only opening clears it
    // (`actionLinearEditor.tsx@1118751f:159-164`).
    assert_eq!(line.background_color, "#ffec99");
}

#[test]
fn toggling_a_polygon_open_again_clears_its_background() {
    let mut closed = triangle();
    closed.background_color = "#ffec99".into();
    let id = closed.id.clone();
    let mut engine = engine_with_scene(vec![closed]);
    engine.select(vec![id.clone()]);

    engine.toggle_polygon_selection();

    let line = engine.get_scene().into_iter().find(|e| e.id == id).unwrap();
    assert!(!line.is_polygon());
    assert_eq!(line.background_color, "transparent");
}

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn the_polygon_flag_survives_a_json_round_trip() {
    let json = scene_to_json(&[triangle()]);
    assert!(json.contains("\"polygon\": true"));
    let parsed = elements_from_json(&json).unwrap();
    assert!(parsed[0].is_polygon());
}

#[test]
fn an_open_line_carries_no_polygon_key_at_all() {
    let json = scene_to_json(&[open_triangle()]);
    assert!(
        !json.contains("\"polygon\""),
        "false/absent should round-trip as nothing written, like every other \
         skip_serializing_if option on this element"
    );
}

// ---------------------------------------------------------------------------
// Hit inside
// ---------------------------------------------------------------------------

#[test]
fn a_click_inside_a_filled_polygon_hits_it() {
    let mut line = triangle();
    line.background_color = "#ffec99".into();
    let elements = vec![line];
    // The centroid of (0,0) (100,0) (50,100), comfortably away from every edge.
    let hit = hit_test(&elements, 50.0, 40.0, 1.0);
    assert!(
        hit.is_some(),
        "the interior of a filled polygon is a hit target"
    );
}

#[test]
fn a_click_inside_an_unfilled_polygon_does_not_hit_it() {
    let line = triangle(); // background_color defaults to transparent
    let elements = vec![line];
    let hit = hit_test(&elements, 50.0, 40.0, 1.0);
    assert!(
        hit.is_none(),
        "a transparent fill has nothing to grab from inside"
    );
}

// ---------------------------------------------------------------------------
// Paint / fill
// ---------------------------------------------------------------------------

#[test]
fn a_filled_polygon_paints_as_a_filled_rough_shape() {
    let mut line = triangle();
    line.background_color = "#ffec99".into();
    let drawable = draw_engine::render::shape::element_drawable(&line).unwrap();
    assert_eq!(drawable.shape, "polygon");
}

#[test]
fn an_open_line_paints_as_a_bare_stroke() {
    let line = open_triangle();
    let drawable = draw_engine::render::shape::element_drawable(&line).unwrap();
    assert_eq!(drawable.shape, "linearPath");
}

// ---------------------------------------------------------------------------
// Restoring a legacy loop
// ---------------------------------------------------------------------------

/// A board saved before `polygon` existed can still hold a line whose points happen to
/// close a loop (hand-drawn, or from an older recognizer). The oracle's restore does not
/// promote it on the strength of geometry alone — only an explicit `polygon: true` ever
/// did (`packages/excalidraw/data/restore.ts@1118751f:645-651`) — and this engine reads
/// the same document the same way: `polygon` is simply absent from old JSON, and its
/// absence must not be treated as `true` no matter how closed the shape looks.
#[test]
fn a_legacy_closed_loop_with_no_polygon_key_does_not_become_one() {
    let json = r##"{
        "type": "osidraw",
        "version": 1,
        "elements": [{
            "id": "legacy-loop",
            "type": "line",
            "x": 0, "y": 0, "width": 100, "height": 100, "angle": 0,
            "strokeColor": "#1e1e1e", "backgroundColor": "#ffec99",
            "fillStyle": "hachure", "strokeWidth": 2, "strokeStyle": "solid",
            "roughness": 1, "opacity": 100, "roundness": null, "seed": 1,
            "points": [[0,0],[100,0],[50,100],[0,0]],
            "version": 1, "versionNonce": 1, "updated": 0, "isDeleted": false
        }]
    }"##;
    let parsed = elements_from_json(json).unwrap();
    assert_eq!(parsed.len(), 1);
    assert!(
        !parsed[0].is_polygon(),
        "a geometric loop with no `polygon` key reads as an open line, not a polygon"
    );
}

/// A document that *does* carry `polygon: true` but whose points were hand-edited (or
/// truncated) below what `isValidPolygon` requires is corrected on the way in rather than
/// trusted — `normalize_polygon`, the same rule the toggle and the drawing gesture both
/// already enforce live.
#[test]
fn a_stale_polygon_flag_over_invalid_points_is_corrected_on_load() {
    let json = r##"{
        "type": "osidraw",
        "version": 1,
        "elements": [{
            "id": "stale-flag",
            "type": "line",
            "x": 0, "y": 0, "width": 100, "height": 0, "angle": 0,
            "strokeColor": "#1e1e1e", "backgroundColor": "#ffec99",
            "fillStyle": "hachure", "strokeWidth": 2, "strokeStyle": "solid",
            "roughness": 1, "opacity": 100, "roundness": null, "seed": 1,
            "points": [[0,0],[100,0]],
            "polygon": true,
            "version": 1, "versionNonce": 1, "updated": 0, "isDeleted": false
        }]
    }"##;
    let parsed = elements_from_json(json).unwrap();
    assert!(
        !parsed[0].is_polygon(),
        "two points cannot be a polygon whatever the flag says"
    );
}

// ---------------------------------------------------------------------------
// Editing points keeps a polygon closed
// ---------------------------------------------------------------------------

#[test]
fn dragging_the_first_point_of_a_polygon_drags_the_last_one_with_it() {
    let line = triangle();
    let moved = selection::linear::move_handle(
        &line,
        selection::LinearHandle::Point(0),
        Point { x: 40.0, y: -20.0 },
    );
    assert!(moved.is_polygon());
    let points = moved.points.unwrap();
    assert_eq!(*points.first().unwrap(), *points.last().unwrap());
}

#[test]
fn removing_a_vertex_below_three_distinct_points_opens_the_polygon() {
    // A square: removing one vertex still leaves a valid (triangular) polygon...
    let mut square = triangle();
    square.points = Some(vec![
        [0.0, 0.0],
        [100.0, 0.0],
        [100.0, 100.0],
        [0.0, 100.0],
        [0.0, 0.0],
    ]);
    let still_valid = selection::linear::remove_point(&square, 1);
    assert!(
        still_valid.is_polygon(),
        "a square minus one vertex is still a triangle"
    );

    // ...but a triangle minus one vertex is only a segment folded onto itself.
    let opened = selection::linear::remove_point(&triangle(), 1);
    assert!(!opened.is_polygon());
}

// ---------------------------------------------------------------------------
// Bucket fill recognises paint by shape, polygon flag or not
// ---------------------------------------------------------------------------

/// `is_bucket_fill_compatible` recognises paint by its shape — a closed, strokeless,
/// filled line — not by the `polygon` flag: a hand-drawn strokeless closed line is
/// restylable exactly like paint the bucket tool generated
/// (`ci_bucket_fill.rs::generated_paint_is_recognised_by_its_shape_not_by_a_marker`), and
/// the flag would go stale the moment someone restyles a fill anyway.
#[test]
fn a_hand_drawn_closed_line_is_bucket_fill_paint_by_shape_alone() {
    let mut hand_drawn = triangle();
    hand_drawn.polygon = None; // closed by geometry alone, never toggled or drawn-closed
    hand_drawn.background_color = "#ffec99".into();
    hand_drawn.stroke_color = "transparent".into();
    assert!(is_bucket_fill_compatible(&hand_drawn));

    let mut generated = triangle(); // polygon: Some(true)
    generated.background_color = "#ffec99".into();
    generated.stroke_color = "transparent".into();
    assert!(is_bucket_fill_compatible(&generated));
}
