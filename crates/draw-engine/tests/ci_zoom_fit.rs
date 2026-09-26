//! Shift+1, Shift+2 and Shift+3: the oracle's three fits (`actions/actionCanvas.tsx@1118751f:
//! 290-410`), each `zoomToFitBounds` over the room the chrome leaves plus a 24px margin
//! (`App.viewport.ts@1118751f:586-606`), eased here where the oracle jumps.
//!
//! - Shift+1, `zoomToFit`: everything, zoomed out to hold it or in, up to 100%.
//! - Shift+2, `zoomToFitSelectionInViewport`: the selection the same way — everything
//!   when nothing is selected.
//! - Shift+3, `zoomToFitSelection`: the selection at whatever zoom fills the room.
//!
//! The expected cameras are worked out by hand on an 800x600 canvas, whose room is
//! 752x552 with its middle at (400, 300).

mod common;
use common::*;
use draw_engine::*;

/// Past the 250ms ease, where the camera lands.
fn landed(engine: &mut DrawEngine) -> Camera {
    engine.set_now(1000.0);
    engine.camera
}

fn assert_camera(ours: Camera, x: f64, y: f64, scale: f64) {
    assert_close(ours.scale, scale);
    assert_close(ours.x, x);
    assert_close(ours.y, y);
}

/// A 100x60 box at (0, 0) and another at (1900, 940): 2000x1000 in all.
fn two_far_boxes() -> (DrawElement, DrawElement) {
    (
        box_at(0.0, 0.0, 100.0, 60.0),
        box_at(1900.0, 940.0, 100.0, 60.0),
    )
}

#[test]
fn shift_one_holds_a_small_board_at_100_percent() {
    let mut engine = engine_with_scene(vec![box_at(2000.0, 0.0, 100.0, 60.0)]);
    engine.set_now(0.0);
    engine.zoom_to_fit();
    // 752/100 would be 7.52; scale-down stops at 1. Centre (2050, 30) to (400, 300).
    assert_camera(landed(&mut engine), -1650.0, 270.0, 1.0);
}

#[test]
fn shift_one_zooms_out_to_hold_a_large_board() {
    let (a, b) = two_far_boxes();
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_now(0.0);
    engine.zoom_to_fit();
    // min(752/2000, 552/1000) = 0.376; centre (1000, 500).
    assert_camera(landed(&mut engine), 400.0 - 376.0, 300.0 - 188.0, 0.376);
}

#[test]
fn shift_one_ignores_the_selection() {
    let (a, b) = two_far_boxes();
    let mut engine = engine_with_scene(vec![a.clone(), b]);
    engine.select(vec![a.id]);
    engine.set_now(0.0);
    engine.zoom_to_fit();
    assert_camera(landed(&mut engine), 24.0, 112.0, 0.376);
}

#[test]
fn shift_two_frames_the_selection_no_closer_than_100_percent() {
    let (a, b) = two_far_boxes();
    let mut engine = engine_with_scene(vec![a.clone(), b]);
    engine.select(vec![a.id]);
    engine.set_now(0.0);
    engine.zoom_to_fit_selection_in_viewport();
    // Centre (50, 30) to (400, 300), at 1.
    assert_camera(landed(&mut engine), 350.0, 270.0, 1.0);
}

#[test]
fn shift_three_fills_the_room_with_the_selection() {
    let (a, b) = two_far_boxes();
    let mut engine = engine_with_scene(vec![a.clone(), b]);
    engine.select(vec![a.id]);
    engine.set_now(0.0);
    engine.zoom_to_fit_selection();
    // min(752/100, 552/60) = 7.52.
    assert_camera(
        landed(&mut engine),
        400.0 - 50.0 * 7.52,
        300.0 - 30.0 * 7.52,
        7.52,
    );
}

#[test]
fn with_nothing_selected_shift_two_and_three_fit_everything() {
    let (a, b) = two_far_boxes();
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_now(0.0);
    engine.zoom_to_fit_selection_in_viewport();
    assert_camera(landed(&mut engine), 24.0, 112.0, 0.376);

    let mut engine = engine_with_scene(vec![box_at(0.0, 0.0, 100.0, 60.0)]);
    engine.set_now(0.0);
    engine.zoom_to_fit_selection();
    assert_camera(
        landed(&mut engine),
        400.0 - 50.0 * 7.52,
        300.0 - 30.0 * 7.52,
        7.52,
    );
}

#[test]
fn a_fit_keeps_clear_of_the_chrome() {
    let mut engine = engine_with_scene(vec![box_at(2000.0, 0.0, 100.0, 60.0)]);
    // A 60px toolbar across the top: the room runs from 84 to 576, its middle at 330.
    engine.set_viewport_offsets(Offsets {
        top: 60.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    });
    engine.set_now(0.0);
    engine.zoom_to_fit();
    assert_camera(landed(&mut engine), -1650.0, 300.0, 1.0);
}

/// `getCommonBounds` of nothing is `[0, 0, 0, 0]`, and the oracle divides by its size
/// unguarded: scale-down stops at 100%, contain runs to the zoom limit.
#[test]
fn an_empty_board_is_fitted_as_the_origin_as_the_oracle_does() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_now(0.0);
    engine.zoom_to_fit();
    assert_camera(landed(&mut engine), 400.0, 300.0, 1.0);

    let mut engine = engine_with_scene(vec![]);
    engine.set_now(0.0);
    engine.zoom_to_fit_selection();
    assert_camera(landed(&mut engine), 400.0, 300.0, MAX_ZOOM);
}
