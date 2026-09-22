//! Getting around the board: fit, zoom to selection, and paging.
//!
//! The camera maths (`ci_camera.rs`) and the wheel (`ci_zoom_wheel.rs`) are settled
//! elsewhere. What is here is the set of *destinations* a person can ask for by name —
//! show me everything, show me what I selected, move down a screen — which is a different
//! question from how the camera gets there.
//!
//! All of it is arithmetic over bounds and the viewport, so it belongs in the motor: a
//! host that worked out "one page down" itself would pick a different distance from the
//! next host, and paging that differs per platform is worse than no paging.

mod common;
use common::*;
use draw_engine::*;

/// A board with two shapes far apart, so fitting has something to decide.
fn spread_board() -> DrawEngine {
    let mut engine = engine_with_scene(vec![
        filled(box_at(0.0, 0.0, 100.0, 100.0)),
        filled(box_at(2000.0, 1500.0, 200.0, 200.0)),
    ]);
    engine.set_viewport(800.0, 600.0, 1.0);
    engine
}

/// Where an element's centre lands on screen.
fn centre_on_screen(engine: &DrawEngine, id: &str) -> Point {
    let element = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == id)
        .expect("element");
    let centre = element_center(&element);
    world_to_screen(engine.camera, centre.x, centre.y)
}

/// The viewport size, which the engine keeps private.
fn viewport(engine: &DrawEngine) -> (f64, f64) {
    let (width, height, _dpr) = engine.viewport();
    (width, height)
}

fn on_screen(engine: &DrawEngine, point: Point) -> bool {
    point.x >= 0.0
        && point.x <= viewport(engine).0
        && point.y >= 0.0
        && point.y <= viewport(engine).1
}

#[test]
fn fitting_brings_the_whole_board_into_view() {
    let mut engine = spread_board();
    let ids: Vec<String> = engine.get_scene().into_iter().map(|e| e.id).collect();

    engine.fit(96.0);

    for id in &ids {
        assert!(
            on_screen(&engine, centre_on_screen(&engine, id)),
            "{id} is off screen after a fit"
        );
    }
}

#[test]
fn zooming_to_the_selection_frames_the_selection_and_not_the_board() {
    // The difference that makes this worth a separate command. A fit has to hold both
    // shapes, so the far one is tiny; zoom-to-selection is how you get in close on the
    // one you are working on without hunting for it with the wheel.
    let mut engine = spread_board();
    let far = engine
        .get_scene()
        .into_iter()
        .nth(1)
        .expect("two shapes")
        .id;
    engine.select(vec![far.clone()]);
    engine.fit(96.0);
    let fitted_scale = engine.camera.scale;

    engine.zoom_to_selection(96.0);

    assert!(
        engine.camera.scale > fitted_scale,
        "zooming to one shape should be closer than fitting both: {} vs {fitted_scale}",
        engine.camera.scale
    );
    let centre = centre_on_screen(&engine, &far);
    assert_close(centre.x, viewport(&engine).0 / 2.0);
    assert_close(centre.y, viewport(&engine).1 / 2.0);
}

#[test]
fn zooming_to_an_empty_selection_leaves_the_camera_alone() {
    // Pressing the key with nothing selected. Framing "nothing" has no meaning, and
    // jumping somewhere arbitrary is the worst of the available answers.
    let mut engine = spread_board();
    let before = engine.camera;

    engine.zoom_to_selection(96.0);

    assert_eq!(engine.camera, before);
}

#[test]
fn zooming_to_a_selection_of_several_holds_all_of_them() {
    let mut engine = spread_board();
    let ids: Vec<String> = engine.get_scene().into_iter().map(|e| e.id).collect();
    engine.select(ids.clone());

    engine.zoom_to_selection(96.0);

    for id in &ids {
        assert!(
            on_screen(&engine, centre_on_screen(&engine, id)),
            "{id} is off screen after zooming to a selection containing it"
        );
    }
}

#[test]
fn a_page_moves_by_most_of_a_screen_rather_than_all_of_it() {
    // Paging that moves exactly one screen leaves nothing in common between the two
    // views, so you lose your place at every press. Excalidraw's own scroll keeps an
    // overlap, and the overlap is the whole reason paging beats dragging.
    let mut engine = spread_board();
    let before = engine.camera;

    engine.page_by(0.0, 1.0);

    let moved = before.y - engine.camera.y;
    assert!(
        moved > viewport(&engine).1 * 0.5 && moved < viewport(&engine).1,
        "one page moved {moved}px of a {}px viewport",
        viewport(&engine).1
    );
}

#[test]
fn paging_down_then_up_returns_to_where_it_started() {
    // Paging is how you read a long board. If the two directions disagree by a pixel,
    // reading down and back leaves you somewhere you did not choose.
    let mut engine = spread_board();
    let before = engine.camera;

    engine.page_by(0.0, 1.0);
    engine.page_by(0.0, -1.0);

    assert_close(engine.camera.x, before.x);
    assert_close(engine.camera.y, before.y);
}

#[test]
fn paging_sideways_moves_by_the_width_rather_than_the_height() {
    let mut engine = spread_board();
    let before = engine.camera;

    engine.page_by(1.0, 0.0);

    let moved = before.x - engine.camera.x;
    assert!(
        moved > viewport(&engine).0 * 0.5 && moved < viewport(&engine).0,
        "a sideways page moved {moved}px of a {}px viewport",
        viewport(&engine).0
    );
    assert_close(engine.camera.y, before.y);
}

#[test]
fn a_page_is_the_same_distance_on_screen_at_every_zoom() {
    // A page is a screenful, so it is specified in screen pixels. Scaling it by the zoom
    // — an easy thing to "fix" — makes paging useless when zoomed in, which is exactly
    // when you most need it.
    let mut engine = spread_board();
    let mut distances = Vec::new();
    for scale in [0.25, 1.0, 8.0] {
        engine.set_camera(zoom_to(engine.camera, 400.0, 300.0, scale));
        let before = engine.camera.y;
        engine.page_by(0.0, 1.0);
        distances.push(before - engine.camera.y);
    }
    for pair in distances.windows(2) {
        assert_close(pair[0], pair[1]);
    }
}

#[test]
fn paging_does_not_change_the_zoom() {
    let mut engine = spread_board();
    engine.set_camera(zoom_to(engine.camera, 400.0, 300.0, 2.5));

    engine.page_by(0.0, 1.0);
    engine.page_by(-1.0, 0.0);

    assert_close(engine.camera.scale, 2.5);
}
