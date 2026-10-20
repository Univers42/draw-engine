//! When a font finishes loading, the next frame must repaint the whole picture
//! even if no text's layout changed — the cached layer was painted with the fallback.
//!
//! Excalidraw: `Fonts.onLoaded` (`packages/excalidraw/fonts/Fonts.ts@1118751f:98-147`)
//! invalidates every text's cached shape and re-renders whether or not any size
//! changed.

mod common;
use common::*;
use draw_engine::render::scroll::{layer_key, plan_layer, LayerPlan};
use draw_engine::*;

/// A measure hook that returns the same width both times (fallback ≈ real face).
fn same_width(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.len() as f64 * font_size * 0.5 + 4.0)
        .fold(0.0, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * TEXT_LINE_HEIGHT).max(font_size),
    )
}

#[test]
fn fonts_loaded_requests_a_redraw_even_when_no_layout_changes() {
    // A text that measures the same with the fallback and the real face.
    // "Hello phone" at 20px: Roboto 106.27 vs Excalifont 106.54 — close enough
    // that the re-layout produces the same box.
    let mut text = text_at(100.0, 100.0, 110.0, 25.0);
    text.font_family = Some(5); // Excalifont
    let text_id = text.id.clone();
    let mut engine = engine_with_measure(vec![text]);
    engine.set_element_text(&text_id, "Hello phone");

    // Paint once with the "fallback" metrics.
    engine.set_measure_text(same_width);
    let _ = engine.paint_view();
    // Clear events from set_element_text
    let _ = engine.drain_events();
    // Capture version before fonts_loaded
    let before = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == text_id)
        .unwrap()
        .clone();
    let key_before = layer_key(&engine.paint_view());

    // The font "arrives" — same metrics, so no text changes size.
    engine.set_measure_text(same_width);
    engine.fonts_loaded();

    // The engine must ask for a frame.
    assert!(
        engine.needs_frame(),
        "fonts_loaded() must mark the engine dirty even when no text changed size"
    );

    // The layer key must differ so plan_layer returns Redraw, not Reuse.
    let key_after = layer_key(&engine.paint_view());
    let plan = plan_layer(Some((key_before, engine.camera)), key_after, engine.camera);
    assert_eq!(
        plan,
        LayerPlan::Redraw,
        "the layer key after fonts_loaded() must force a Redraw, got {:?}",
        plan
    );

    // Nothing is stamped: no version bump, no history, no scene delta.
    let after = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == text_id)
        .unwrap();
    assert_eq!(
        after.version, before.version,
        "text version must not change on fonts_loaded()"
    );
    assert_eq!(
        after.version_nonce, before.version_nonce,
        "text version_nonce must not change on fonts_loaded()"
    );
    let events = engine.drain_events();
    assert!(
        events.scene_delta.is_none(),
        "fonts_loaded() must not emit a scene delta"
    );
}

#[test]
fn fonts_loaded_with_no_text_family_requests_nothing() {
    // A text with no font family uses the system stack, which was never loading.
    let mut text = text_at(100.0, 100.0, 200.0, 25.0);
    text.font_family = None;
    let text_id = text.id.clone();
    let mut engine = engine_with_measure(vec![text]);
    engine.set_element_text(&text_id, "hello");

    engine.set_measure_text(same_width);
    let _ = engine.paint_view();
    // Clear the dirty flag from set_element_text
    engine.take_dirty();
    let key_before = layer_key(&engine.paint_view());

    engine.fonts_loaded();

    // No frame requested, no key change.
    assert!(
        !engine.needs_frame(),
        "a text with no family must not trigger a repaint"
    );
    let key_after = layer_key(&engine.paint_view());
    let plan = plan_layer(Some((key_before, engine.camera)), key_after, engine.camera);
    assert_eq!(plan, LayerPlan::Reuse);
}
