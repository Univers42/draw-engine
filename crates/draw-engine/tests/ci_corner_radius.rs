//! Corner radius, set in place by dragging a handle inside each corner.
//!
//! Not an Excalidraw feature — theirs offers only Sharp and Round — but it writes into a
//! slot their model already has. `roundness` there is `{type, value?}` and
//! `getCornerRadius` (`packages/element/src/utils.ts:527-548`) reads the value as a fixed
//! radius, with no UI anywhere to set it. So this is a handle for data the oracle already
//! understands, not a new concept.
//!
//! One deliberate divergence, stated here so nobody "fixes" it back: theirs caps any
//! radius at a quarter of the short side. A handle that stops a quarter of the way in
//! feels broken, so ours runs to half — a pill.
//!
//! Stored in a **new** optional field, `corner_radius`, rather than in `roundness`. Every
//! rounded rectangle ever saved carries `roundness: 8`, and the renderer has always
//! ignored that number in favour of the adaptive rule. Starting to honour it would
//! sharpen every existing board from a 32px corner to an 8px one. Absent means adaptive,
//! exactly as before.

mod common;
use common::*;
use draw_engine::*;

fn rect(x: f64, y: f64, w: f64, h: f64, radius: Option<f64>) -> DrawElement {
    let mut element = filled(box_at(x, y, w, h));
    element.corner_radius = radius;
    element
}

/// One selected rectangle, 200x120 at (100, 100), with the given explicit radius.
fn selected(radius: Option<f64>) -> (DrawEngine, String) {
    let element = rect(100.0, 100.0, 200.0, 120.0, radius);
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    (engine, id)
}

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

fn handles(engine: &DrawEngine) -> Vec<Point> {
    engine.paint_view().radius_handles.clone()
}

/// The radius the renderer will actually draw.
fn drawn_radius(element: &DrawElement) -> f64 {
    let short = element.width.abs().min(element.height.abs());
    draw_engine::render::shape::corner_radius(short, element)
}

fn drag(engine: &mut DrawEngine, from: Point, dx: f64, dy: f64) {
    engine.begin_pointer(from.x, from.y, false, false);
    for step in 1..=4 {
        let t = step as f64 / 4.0;
        engine.move_pointer(from.x + dx * t, from.y + dy * t, false, false);
    }
    engine.end_pointer();
}

// ---------------------------------------------------------------------------
// What the renderer draws
// ---------------------------------------------------------------------------

/// The guard for every board already saved: no explicit radius, adaptive corners, exactly
/// as before.
#[test]
fn an_ordinary_rounded_rectangle_keeps_its_adaptive_corners() {
    let big = rect(0.0, 0.0, 400.0, 400.0, None);
    assert_close(drawn_radius(&big), 32.0);
    let small = rect(0.0, 0.0, 60.0, 60.0, None);
    assert_close(drawn_radius(&small), 15.0);
}

#[test]
fn an_explicit_radius_is_honoured() {
    assert_close(
        drawn_radius(&rect(0.0, 0.0, 200.0, 120.0, Some(20.0))),
        20.0,
    );
}

/// Past the oracle's quarter-of-the-short-side cap, deliberately.
#[test]
fn an_explicit_radius_can_run_past_the_adaptive_cap() {
    assert_close(
        drawn_radius(&rect(0.0, 0.0, 200.0, 120.0, Some(50.0))),
        50.0,
    );
}

/// Half the short side is a pill. Anything more would make opposite corners overlap.
#[test]
fn an_explicit_radius_stops_at_half_the_short_side() {
    assert_close(
        drawn_radius(&rect(0.0, 0.0, 200.0, 120.0, Some(500.0))),
        60.0,
    );
}

/// Sharp wins. The radius is remembered, not applied, so toggling back to Round restores
/// it — but while the panel says Sharp, the corner is sharp.
#[test]
fn a_sharp_rectangle_draws_no_radius_whatever_is_stored() {
    let mut element = rect(0.0, 0.0, 200.0, 120.0, Some(40.0));
    element.roundness = None;
    assert_close(drawn_radius(&element), 0.0);
}

/// The render caches are keyed by this. If the radius were left out, changing it would
/// keep serving the old geometry from cache and the handle would appear to do nothing.
#[test]
fn the_radius_is_part_of_the_geometry_fingerprint() {
    let a = rect(0.0, 0.0, 200.0, 120.0, Some(20.0));
    let mut b = a.clone();
    b.corner_radius = Some(40.0);
    assert_ne!(
        draw_engine::render::cache::shape_fingerprint(&a),
        draw_engine::render::cache::shape_fingerprint(&b)
    );
}

// ---------------------------------------------------------------------------
// Where the handles are
// ---------------------------------------------------------------------------

#[test]
fn a_selected_rectangle_offers_four_radius_handles() {
    let (engine, _) = selected(Some(30.0));
    assert_eq!(handles(&engine).len(), 4);
}

/// On the corner's inward diagonal, at the radius — the centre of the arc it controls.
#[test]
fn each_handle_sits_at_the_centre_of_its_corner_arc() {
    let (engine, _) = selected(Some(30.0));
    let h = handles(&engine);
    // Clockwise from the top-left of (100, 100, 200, 120).
    assert_close(h[0].x, 130.0);
    assert_close(h[0].y, 130.0);
    assert_close(h[1].x, 270.0);
    assert_close(h[1].y, 130.0);
    assert_close(h[2].x, 270.0);
    assert_close(h[2].y, 190.0);
    assert_close(h[3].x, 130.0);
    assert_close(h[3].y, 190.0);
}

/// A sharp corner still gets a handle, inset far enough to be grabbed — that is how a
/// sharp rectangle is rounded in the first place.
#[test]
fn a_sharp_corner_still_offers_a_handle_to_grab() {
    let element = {
        let mut e = rect(100.0, 100.0, 200.0, 120.0, None);
        e.roundness = None;
        e
    };
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec![id]);
    let h = handles(&engine);
    assert_eq!(h.len(), 4);
    assert!(
        h[0].x > 100.0 && h[0].y > 100.0,
        "inset from the corner, not on it"
    );
}

#[test]
fn handles_turn_with_the_shape() {
    let mut element = rect(100.0, 100.0, 200.0, 120.0, Some(30.0));
    element.angle = std::f64::consts::FRAC_PI_2;
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec![id]);

    // Centre (200, 160). The unrotated top-left handle (130, 130) is (-70, -30) from it;
    // a quarter turn clockwise in screen space takes that to (30, -70).
    let h = handles(&engine);
    assert_close(h[0].x, 230.0);
    assert_close(h[0].y, 90.0);
}

#[test]
fn an_ellipse_offers_none() {
    let element = filled(ellipse_at(100.0, 100.0, 200.0, 120.0));
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec![id]);
    assert!(handles(&engine).is_empty(), "an ellipse has no corners");
}

#[test]
fn a_multi_selection_offers_none() {
    let a = rect(100.0, 100.0, 200.0, 120.0, None);
    let b = rect(400.0, 100.0, 200.0, 120.0, None);
    let ids = vec![a.id.clone(), b.id.clone()];
    let mut engine = engine_with_scene(vec![a, b]);
    engine.select(ids);
    assert!(handles(&engine).is_empty());
}

/// Four handles inside a shape a few pixels across would cover it entirely and fight the
/// resize handles for every press.
#[test]
fn a_rectangle_too_small_on_screen_offers_none() {
    let element = rect(100.0, 100.0, 30.0, 24.0, None);
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec![id]);
    assert!(handles(&engine).is_empty());
}

#[test]
fn a_locked_rectangle_offers_none() {
    let mut element = rect(100.0, 100.0, 200.0, 120.0, None);
    element.locked = Some(true);
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec![id]);
    assert!(handles(&engine).is_empty());
}

// ---------------------------------------------------------------------------
// Dragging one
// ---------------------------------------------------------------------------

/// The handle sits inside a filled shape, where a press would otherwise pick the whole
/// shape up. It has to win.
#[test]
fn pressing_a_handle_starts_a_radius_drag_not_a_move() {
    let (mut engine, _) = selected(Some(30.0));
    let h = handles(&engine)[0];
    engine.begin_pointer(h.x, h.y, false, false);
    assert_eq!(engine.debug_state().interaction.kind, Some("corner-radius"));
    engine.end_pointer();
}

#[test]
fn dragging_a_handle_inward_rounds_the_corners() {
    let (mut engine, id) = selected(Some(30.0));
    let h = handles(&engine)[0];

    drag(&mut engine, h, 12.0, 12.0);

    assert_close(drawn_radius(&get(&engine, &id)), 42.0);
}

/// Any corner drives the same single radius, and each measures inward from its own
/// corner — so the bottom-right handle rounds by moving up and left.
#[test]
fn every_corner_measures_inward_from_itself() {
    let (mut engine, id) = selected(Some(30.0));
    let h = handles(&engine)[2];

    drag(&mut engine, h, -10.0, -10.0);

    assert_close(drawn_radius(&get(&engine, &id)), 40.0);
}

/// A handle drawn inset past a small radius must not jump to where it is drawn the moment
/// it is grabbed. Measured from the grab, not from the corner.
#[test]
fn grabbing_a_handle_without_moving_leaves_the_radius_alone() {
    let (mut engine, id) = selected(Some(4.0));
    let h = handles(&engine)[0];

    engine.begin_pointer(h.x, h.y, false, false);
    engine.move_pointer(h.x, h.y, false, false);
    engine.end_pointer();

    assert_close(drawn_radius(&get(&engine, &id)), 4.0);
}

#[test]
fn dragging_back_past_the_corner_makes_it_sharp() {
    let (mut engine, id) = selected(Some(20.0));
    let h = handles(&engine)[0];

    drag(&mut engine, h, -40.0, -40.0);

    let element = get(&engine, &id);
    assert_eq!(
        element.roundness, None,
        "the panel should read Sharp, not Round-at-zero"
    );
    assert_close(drawn_radius(&element), 0.0);
}

#[test]
fn dragging_a_sharp_rectangles_handle_rounds_it() {
    let element = {
        let mut e = rect(100.0, 100.0, 200.0, 120.0, None);
        e.roundness = None;
        e
    };
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.select(vec![id.clone()]);
    let h = handles(&engine)[0];

    drag(&mut engine, h, 15.0, 15.0);

    let after = get(&engine, &id);
    assert!(after.roundness.is_some(), "it is round now");
    assert_close(drawn_radius(&after), 15.0);
}

#[test]
fn a_radius_drag_leaves_position_and_size_alone() {
    let (mut engine, id) = selected(Some(30.0));
    let before = get(&engine, &id);
    let h = handles(&engine)[1];

    drag(&mut engine, h, -20.0, 20.0);

    let after = get(&engine, &id);
    assert_close(after.x, before.x);
    assert_close(after.y, before.y);
    assert_close(after.width, before.width);
    assert_close(after.height, before.height);
}

#[test]
fn one_undo_reverts_the_radius() {
    let (mut engine, id) = selected(Some(30.0));
    let h = handles(&engine)[0];
    drag(&mut engine, h, 15.0, 15.0);
    assert_close(drawn_radius(&get(&engine, &id)), 45.0);

    engine.undo();

    assert_close(drawn_radius(&get(&engine, &id)), 30.0);
}

// ---------------------------------------------------------------------------
// The file format
// ---------------------------------------------------------------------------

#[test]
fn the_radius_survives_a_save_and_load() {
    let (engine, id) = selected(Some(37.0));
    let loaded = elements_from_json(&engine.export_json()).expect("the scene reloads");
    let element = loaded.into_iter().find(|el| el.id == id).unwrap();
    assert_eq!(element.corner_radius, Some(37.0));
}

/// Old boards stay byte-identical: an element nobody gave a radius writes no field.
#[test]
fn an_element_without_a_radius_writes_no_field() {
    let (engine, _) = selected(None);
    assert!(
        !engine.export_json().contains("cornerRadius"),
        "absent means adaptive, and must stay absent on the way out"
    );
}

// ---------------------------------------------------------------------------
// Export agrees with the canvas
// ---------------------------------------------------------------------------

/// The exporter used to write `rx = roundness` — the `8` every rounded shape carries and
/// the canvas has always ignored — so an export never matched the board.
#[test]
fn an_svg_export_carries_the_radius_the_canvas_draws() {
    let big = rect(0.0, 0.0, 400.0, 400.0, None);
    let bounds = element_bounds(&big);
    let svg = scene_to_svg(&[big], bounds, 10.0, "#ffffff");
    assert!(
        svg.contains("rx=\"32\""),
        "adaptive corner, as drawn: {svg}"
    );
}

#[test]
fn an_svg_export_carries_an_explicit_radius() {
    let element = rect(0.0, 0.0, 200.0, 120.0, Some(45.0));
    let bounds = element_bounds(&element);
    let svg = scene_to_svg(&[element], bounds, 10.0, "#ffffff");
    assert!(svg.contains("rx=\"45\""), "{svg}");
}
