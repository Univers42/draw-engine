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
    let scale_init = engine.camera.scale;
    engine.zoom_in();
    assert!(engine.camera.scale > scale_init);
    engine.zoom_out();
    assert_close(engine.camera.scale, scale_init);
}

#[test]
fn engine_zoom_reset() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.zoom_in();
    engine.zoom_in();
    engine.zoom_reset();
    assert_close(engine.camera.scale, 1.0);
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
