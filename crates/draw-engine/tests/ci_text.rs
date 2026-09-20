mod common;
use common::*;
use draw_engine::*;

#[test]
fn double_click_empty_canvas_creates_text() {
    let mut engine = engine_with_measure(vec![]);
    engine.handle_double_click(100.0, 150.0);
    let events = engine.drain_events();
    assert!(events.text_edit.is_some());
    assert_eq!(engine.get_scene().len(), 1);
    assert_eq!(engine.get_scene()[0].kind, DrawElementType::Text);
}

#[test]
fn double_click_existing_text_starts_edit() {
    let text = text_at(50.0, 50.0, 100.0, 30.0);
    let mut engine = engine_with_measure(vec![text.clone()]);
    engine.handle_double_click(60.0, 60.0);
    let events = engine.drain_events();
    assert!(events.text_edit.is_some());
    assert_eq!(events.text_edit.unwrap().id, text.id);
}

#[test]
fn double_click_box_creates_bound_label() {
    let rect = box_at(0.0, 0.0, 120.0, 80.0);
    let mut engine = engine_with_measure(vec![rect.clone()]);
    engine.handle_double_click(60.0, 40.0);
    let scene = engine.get_scene();
    assert_eq!(scene.len(), 2);
    let label = scene
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .unwrap();
    let host = scene.iter().find(|e| e.id == rect.id).unwrap();
    assert_eq!(label.container_id.as_deref(), Some(rect.id.as_str()));
    assert_eq!(host.bound_text_id.as_deref(), Some(label.id.as_str()));
}

#[test]
fn double_click_ellipse_creates_bound_label() {
    let el = ellipse_at(0.0, 0.0, 100.0, 100.0);
    let mut engine = engine_with_measure(vec![el.clone()]);
    engine.handle_double_click(50.0, 50.0);
    let scene = engine.get_scene();
    let label = scene
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .unwrap();
    assert_eq!(label.container_id.as_deref(), Some(el.id.as_str()));
}

#[test]
fn double_click_diamond_creates_bound_label() {
    let dia = diamond_at(0.0, 0.0, 100.0, 100.0);
    let mut engine = engine_with_measure(vec![dia.clone()]);
    engine.handle_double_click(50.0, 50.0);
    let scene = engine.get_scene();
    let label = scene
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .unwrap();
    assert_eq!(label.container_id.as_deref(), Some(dia.id.as_str()));
}

#[test]
fn edit_selected_text_with_selection() {
    let text = text_at(0.0, 0.0, 20.0, 20.0);
    let mut engine = engine_with_measure(vec![text.clone()]);
    engine.select(vec![text.id]);
    assert!(engine.edit_selected_text());
}

#[test]
fn edit_selected_text_without_selection_returns_false() {
    let text = text_at(0.0, 0.0, 20.0, 20.0);
    let mut engine = engine_with_measure(vec![text]);
    assert!(!engine.edit_selected_text());
}

#[test]
fn edit_selected_text_on_box_creates_label_if_missing() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_measure(vec![rect.clone()]);
    engine.select(vec![rect.id]);
    assert!(engine.edit_selected_text());
    assert_eq!(engine.get_scene().len(), 2);
}

#[test]
fn set_element_text_multiline_increases_height() {
    let mut label = text_at(0.0, 0.0, 50.0, 20.0);
    label.font_size = Some(20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "line 1\nline 2\nline 3\nline 4");

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == label.id)
        .unwrap();
    assert!(updated.height >= 4.0 * 20.0 * TEXT_LINE_HEIGHT);
}

#[test]
fn set_element_text_single_line() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "Quick brown fox");

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == label.id)
        .unwrap();
    assert_eq!(updated.text.as_deref(), Some("Quick brown fox"));
}

#[test]
fn set_element_text_unicode_emojis() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "Hello 👋 World 🌍");

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == label.id)
        .unwrap();
    assert_eq!(updated.text.as_deref(), Some("Hello 👋 World 🌍"));
}

#[test]
fn set_element_text_empty_string_deletes_label() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut label = text_at(0.0, 0.0, 50.0, 20.0);
    label.container_id = Some(rect.id.clone());

    let mut engine = engine_with_measure(vec![rect.clone(), label.clone()]);
    engine.set_element_text(&label.id, "");

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 1);
    assert_eq!(scene[0].id, rect.id);
    assert!(scene[0].bound_text_id.is_none());
}

#[test]
fn set_element_text_whitespace_only_deletes_label() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "   \n\t  ");
    assert!(engine.get_scene().is_empty());
}

#[test]
fn set_element_text_non_existent_id_is_noop() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text("ghost_id", "nothing happens");
    assert_eq!(engine.get_scene().len(), 1);
}

#[test]
fn measure_text_empty_string_min_bounds() {
    let (w, h) = measure_text("", 20.0);
    assert!(w >= 4.0);
    assert!(h >= 20.0);
}

#[test]
fn measure_text_long_line_expands_width() {
    let (short_w, _) = measure_text("Hi", 20.0);
    let (long_w, _) = measure_text("This is a significantly longer string of text", 20.0);
    assert!(long_w > short_w);
}
