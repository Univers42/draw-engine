//! The thumbnail export: the longest side is capped in Rust, the scene is not embedded,
//! and no scene shape makes the canvas zero pixels wide or tall.
//!
//! Rasterising needs a browser, so the PNG itself is not decoded here: what is pinned is
//! the canvas the browser is asked for (`pixel_width` x `pixel_height`) and the options the
//! binding builds, which are the two things that decide the picture's size and its bytes.

mod common;
use common::*;
use draw_engine::*;
use std::time::Instant;

const MAX: f64 = THUMBNAIL_MAX_SIDE;

fn thumbnail_canvas(engine: &DrawEngine) -> (u32, u32) {
    let options = thumbnail_options(false);
    let scope = engine.export_scope(false, &options);
    let frame = fit_thumbnail(scope.frame);
    (frame.pixel_width(), frame.pixel_height())
}

fn grid_of(count: usize) -> Vec<DrawElement> {
    (0..count)
        .map(|i| box_at((i % 200) as f64 * 40.0, (i / 200) as f64 * 40.0, 30.0, 30.0))
        .collect()
}

#[test]
fn the_scale_halves_a_frame_twice_the_cap() {
    assert_close(thumbnail_scale(960.0, 480.0, MAX), 0.5);
}

#[test]
fn a_small_frame_is_never_scaled_up() {
    assert_close(thumbnail_scale(200.0, 100.0, MAX), 1.0);
}

#[test]
fn a_degenerate_frame_scales_by_one() {
    for (w, h) in [
        (0.0, 0.0),
        (-5.0, 10.0),
        (10.0, 0.0),
        (f64::NAN, 10.0),
        (f64::INFINITY, 10.0),
        (10.0, f64::NEG_INFINITY),
    ] {
        assert_close(thumbnail_scale(w, h, MAX), 1.0);
    }
}

#[test]
fn the_thumbnail_options_embed_no_scene_at_scale_one() {
    let options = thumbnail_options(true);
    assert!(!options.embed_scene);
    assert!(options.dark_mode);
    assert_close(options.scale, 1.0);
    assert!(!thumbnail_options(false).dark_mode);
}

#[test]
fn a_big_scene_comes_out_at_most_the_cap_on_its_longest_side() {
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 4000.0, 1000.0)]);
    let (w, h) = thumbnail_canvas(&engine);
    assert!(w <= 480 && h <= 480, "{w}x{h}");
    assert!(w >= 479, "the longest side should use the cap, got {w}");
    assert!(h >= 1);
}

#[test]
fn a_sliver_scene_still_has_both_sides_at_least_one_pixel() {
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 100000.0, 1.0)]);
    let (w, h) = thumbnail_canvas(&engine);
    assert!(w <= 480, "{w}");
    assert!(h >= 1, "height collapsed to {h}");
}

#[test]
fn a_tall_sliver_scene_still_has_both_sides_at_least_one_pixel() {
    let engine = engine_with_scene(vec![box_at(0.0, 0.0, 1.0, 100000.0)]);
    let (w, h) = thumbnail_canvas(&engine);
    assert!(h <= 480, "{h}");
    assert!(w >= 1, "width collapsed to {w}");
}

#[test]
fn an_empty_scene_does_not_panic_and_is_at_least_one_pixel() {
    let engine = engine_with_scene(vec![]);
    let (w, h) = thumbnail_canvas(&engine);
    assert!(w >= 1 && h >= 1 && w <= 480 && h <= 480, "{w}x{h}");
}

#[test]
fn a_zero_by_zero_frame_is_fitted_without_panicking() {
    let options = thumbnail_options(false);
    let mut frame = ExportFrame::for_bounds(
        draw_engine::WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        },
        &ExportOptions {
            padding: 0.0,
            ..options
        },
    );
    frame = fit_thumbnail(frame);
    assert!(frame.pixel_width() >= 1 && frame.pixel_height() >= 1);
}

#[test]
fn twenty_thousand_elements_fit_the_cap() {
    let engine = engine_with_scene(grid_of(20_000));
    let started = Instant::now();
    let (w, h) = thumbnail_canvas(&engine);
    let elapsed = started.elapsed();
    eprintln!("native thumbnail framing of 20000 elements: {elapsed:?}");
    assert!(w <= 480 && h <= 480 && w >= 1 && h >= 1, "{w}x{h}");
}

#[test]
fn the_scene_is_not_embedded_by_the_thumbnail_options() {
    // The PNG path embeds exactly when `embed_scene` is set (wasm/export.rs), and the
    // default is true: the thumbnail must be the one that says otherwise.
    assert!(ExportOptions::default().embed_scene);
    assert!(!thumbnail_options(false).embed_scene);
}
