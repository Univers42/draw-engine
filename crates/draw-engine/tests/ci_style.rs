mod common;
use common::*;
use draw_engine::*;

#[test]
fn apply_style_empty_scene_updates_next_style() {
    let mut engine = engine_with_scene(vec![]);
    engine.apply_style(stroke_patch("#123456"));
    assert_eq!(engine.get_next_style().stroke_color, "#123456");
}

#[test]
fn apply_style_selected_element_updates_color() {
    let rect = box_at(0.0, 0.0, 100.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.apply_style(stroke_patch("#aabbcc"));

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == rect.id)
        .unwrap();
    assert_eq!(updated.stroke_color, "#aabbcc");
}

#[test]
fn apply_style_increments_version() {
    let rect = box_at(0.0, 0.0, 100.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    assert_eq!(rect.version, 1);

    engine.apply_style(stroke_patch("#ffffff"));
    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == rect.id)
        .unwrap();
    assert_eq!(updated.version, 2);
}

#[test]
fn apply_style_sets_timestamp() {
    let rect = box_at(0.0, 0.0, 100.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.set_now(1234.5);
    engine.apply_style(stroke_patch("#000000"));

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == rect.id)
        .unwrap();
    assert_close(updated.updated, 1234.5);
}

#[test]
fn apply_style_multiple_selected_elements() {
    let a = box_at(0.0, 0.0, 50.0, 50.0);
    let b = box_at(100.0, 0.0, 50.0, 50.0);
    let mut engine = engine_with_scene(vec![a.clone(), b.clone()]);
    engine.select(vec![a.id.clone(), b.id.clone()]);
    engine.apply_style(stroke_patch("#ff0000"));

    for el in engine.get_scene() {
        assert_eq!(el.stroke_color, "#ff0000");
    }
}

#[test]
fn set_next_style_persists() {
    let mut engine = DrawEngine::new();
    engine.set_next_style(DrawElementStylePatch {
        stroke_color: Some("#334455".into()),
        background_color: Some("#667788".into()),
        ..Default::default()
    });
    assert_eq!(engine.get_next_style().stroke_color, "#334455");
    assert_eq!(engine.get_next_style().background_color, "#667788");
}

#[test]
fn set_arrowheads_connector_start() {
    let line = connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow);
    let mut engine = engine_with_scene(vec![line.clone()]);
    engine.select(vec![line.id.clone()]);
    engine.set_arrowheads(Some(Arrowhead::Diamond), None);

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == line.id)
        .unwrap();
    assert_eq!(updated.start_arrowhead, Some(Arrowhead::Diamond));
}

#[test]
fn set_arrowheads_connector_end() {
    let line = connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Line);
    let mut engine = engine_with_scene(vec![line.clone()]);
    engine.select(vec![line.id.clone()]);
    engine.set_arrowheads(None, Some(Arrowhead::Bar));

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == line.id)
        .unwrap();
    assert_eq!(updated.end_arrowhead, Some(Arrowhead::Bar));
}

#[test]
fn set_arrowheads_rectangle_ignored() {
    let rect = box_at(0.0, 0.0, 100.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.set_arrowheads(Some(Arrowhead::Arrow), Some(Arrowhead::Arrow));

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == rect.id)
        .unwrap();
    assert_eq!(updated.kind, DrawElementType::Rectangle);
}

#[test]
fn set_font_size_updates_element_geometry() {
    let text = text_at(0.0, 0.0, 10.0, 20.0);
    let mut engine = engine_with_measure(vec![text.clone()]);
    engine.select(vec![text.id.clone()]);
    engine.set_font_size(32.0);

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == text.id)
        .unwrap();
    assert_eq!(updated.font_size, Some(32.0));
    assert_close(engine.get_font_size(), 32.0);
}

// -------------------------------------------------------------------- style presets' fonts

/// A style preset's font reaches a selected text like `set_font_size` itself does —
/// `apply_style` is what a preset ultimately is, several rows applied together.
#[test]
fn a_style_patchs_font_relays_out_a_selected_text() {
    let text = text_at(0.0, 0.0, 10.0, 20.0);
    let mut engine = engine_with_measure(vec![text.clone()]);
    engine.select(vec![text.id.clone()]);
    engine.apply_style(DrawElementStylePatch {
        font_size: Some(32.0),
        font_family: Some(5),
        text_align: Some(TextAlign::Center),
        ..Default::default()
    });

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == text.id)
        .unwrap();
    assert_eq!(updated.font_size, Some(32.0));
    assert_eq!(updated.font_family, Some(5));
    assert_eq!(updated.text_align, Some(TextAlign::Center));
}

/// A preset also reaches the label a selected shape carries — laid out again — and the
/// font and the shape's own colour land together as one step of undo.
#[test]
fn a_style_patchs_font_relays_out_a_selected_shapes_label_as_one_step_with_the_rest() {
    let mut shape = box_at(0.0, 0.0, 200.0, 100.0);
    shape.id = "box".into();
    shape.bound_text_id = Some("label".into());
    let mut label = text_at(10.0, 40.0, 180.0, 20.0);
    label.id = "label".into();
    label.container_id = Some("box".into());
    label.font_size = Some(20.0);
    let mut engine = engine_with_measure(vec![shape, label]);
    engine.select(vec!["box".to_string()]);

    engine.apply_style(DrawElementStylePatch {
        font_size: Some(32.0),
        stroke_color: Some("#ff0000".to_string()),
        ..Default::default()
    });

    let scene = engine.get_scene();
    let label = scene.iter().find(|e| e.id == "label").unwrap();
    let box_el = scene.iter().find(|e| e.id == "box").unwrap();
    assert_eq!(label.font_size, Some(32.0), "the label took the font");
    assert_eq!(box_el.stroke_color, "#ff0000", "the shape took the colour");

    engine.undo();
    let scene = engine.get_scene();
    let label = scene.iter().find(|e| e.id == "label").unwrap();
    let box_el = scene.iter().find(|e| e.id == "box").unwrap();
    assert_eq!(label.font_size, Some(20.0), "one undo takes back both");
    assert_ne!(box_el.stroke_color, "#ff0000");
}

/// With nothing selected, a preset's font becomes only the next text's style — the same
/// rule `set_font_size`/`set_font_family`/`set_text_align` already follow on their own.
#[test]
fn a_style_patchs_font_with_nothing_selected_is_only_the_next_texts() {
    let mut engine = engine_with_scene(vec![]);
    engine.apply_style(DrawElementStylePatch {
        font_size: Some(48.0),
        ..Default::default()
    });
    assert_close(engine.get_font_size(), 48.0);
    assert!(
        engine.get_scene().is_empty(),
        "nothing was created or changed"
    );
}

#[test]
fn stroke_width_style_patch() {
    let rect = box_at(0.0, 0.0, 100.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.apply_style(DrawElementStylePatch {
        stroke_width: Some(4.0),
        ..Default::default()
    });

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == rect.id)
        .unwrap();
    assert_close(updated.stroke_width, 4.0);
}

#[test]
fn background_color_style_patch() {
    let rect = box_at(0.0, 0.0, 100.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.apply_style(DrawElementStylePatch {
        background_color: Some("#cafeba".into()),
        ..Default::default()
    });

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == rect.id)
        .unwrap();
    assert_eq!(updated.background_color, "#cafeba");
}

#[test]
fn opacity_style_patch() {
    let rect = box_at(0.0, 0.0, 100.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.apply_style(DrawElementStylePatch {
        opacity: Some(0.4),
        ..Default::default()
    });

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == rect.id)
        .unwrap();
    assert_close(updated.opacity, 0.4);
}

#[test]
fn engine_zoom_in_and_zoom_out() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_now(0.0);
    let scale_init = engine.camera.scale;
    engine.zoom_in();
    engine.set_now(250.0);
    assert!(engine.camera.scale > scale_init);
    engine.zoom_out();
    engine.set_now(2.0 * 250.0);
    assert_close(engine.camera.scale, scale_init);
}

#[test]
fn engine_zoom_reset() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_now(0.0);
    engine.zoom_in();
    engine.set_now(250.0);
    engine.zoom_in();
    engine.set_now(2.0 * 250.0);
    engine.zoom_reset();
    engine.set_now(3.0 * 250.0);
    assert_close(engine.camera.scale, 1.0);
}

// ----------------------------------------------------------------- eased zoom/fit (Track B)

/// Shift+1/Shift+2, the zoom in/out/reset keys and the zoom-bar buttons all land here
/// (`DrawEngine::zoom_in`/`zoom_out`/`zoom_reset`/`fit`/`zoom_to_selection`), so one eased
/// path covers keyboard and mouse alike. Mirrors `commit_eases_the_camera_to_an_offscreen_node`
/// (`ci_flowchart.rs`), the existing precedent for `animate_camera_to`.
#[test]
fn zoom_in_eases_rather_than_jumps() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_now(0.0);
    let before = engine.camera;

    engine.zoom_in();
    assert_eq!(
        engine.camera, before,
        "the call starts the ease; it does not jump"
    );
    assert!(engine.needs_frame(), "an eased zoom is now in flight");

    engine.set_now(250.0 / 2.0);
    assert!(
        engine.camera.scale > before.scale && engine.camera.scale < before.scale * 1.2,
        "midway through, the scale is between the start and the end"
    );

    engine.set_now(250.0);
    // Lands exactly where the instant version would: `zoom_at` at the viewport centre.
    let expected = zoom_at(before, 400.0, 300.0, 1.2);
    assert_eq!(engine.camera, expected);
    engine.take_dirty();
    assert!(
        !engine.needs_frame(),
        "the ease is done: nothing left to animate"
    );
}

#[test]
fn fit_eases_to_the_same_camera_an_instant_fit_would_land_on() {
    let rect = box_at(2000.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect]);
    engine.set_now(0.0);
    let before = engine.camera;

    engine.fit(96.0);
    assert_eq!(engine.camera, before, "the ease has not started moving yet");

    engine.set_now(250.0);
    let expected = fit_bounds(
        WorldBounds {
            min_x: 2000.0,
            min_y: 0.0,
            max_x: 2100.0,
            max_y: 60.0,
        },
        800.0,
        600.0,
        96.0,
    );
    assert_eq!(engine.camera, expected);
}

#[test]
fn zoom_to_selection_eases_to_the_selection_bounds() {
    let rect = box_at(2000.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id]);
    engine.set_now(0.0);
    let before = engine.camera;

    engine.zoom_to_selection(96.0);
    assert_eq!(engine.camera, before);

    engine.set_now(250.0);
    let expected = fit_bounds(
        WorldBounds {
            min_x: 2000.0,
            min_y: 0.0,
            max_x: 2100.0,
            max_y: 60.0,
        },
        800.0,
        600.0,
        96.0,
    );
    assert_eq!(engine.camera, expected);
}

#[test]
fn reduced_motion_makes_zoom_land_at_once() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_reduced_motion(true);
    engine.set_now(0.0);
    let before = engine.camera;

    engine.zoom_in();
    let expected = zoom_at(before, 400.0, 300.0, 1.2);
    assert_eq!(
        engine.camera, expected,
        "prefers-reduced-motion: lands at once, no animation in flight"
    );
    // `take_dirty` first: the set itself asked for a repaint, same as any camera change —
    // what matters here is that no *animation* is left running behind it.
    engine.take_dirty();
    assert!(!engine.needs_frame());
}

#[test]
fn content_in_view_checks() {
    let rect = box_at(100.0, 100.0, 50.0, 50.0);
    let mut engine = engine_with_scene(vec![rect]);
    engine.set_viewport(800.0, 600.0, 1.0);
    assert!(engine.content_in_view());

    // Pan camera 50,000 px away:
    engine.pan_by(-50_000.0, -50_000.0);
    assert!(!engine.content_in_view());
}
