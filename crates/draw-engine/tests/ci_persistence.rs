mod common;
use common::*;
use draw_engine::*;

#[test]
fn copy_selection_empty_returns_none() {
    let mut engine = DrawEngine::new();
    assert!(engine.copy_selection().is_none());
}

#[test]
fn copy_selection_single_element_serializes() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id]);
    let json = engine.copy_selection();
    assert!(json.is_some());
    assert!(json.unwrap().contains("\"type\": \"osidraw\""));
}

#[test]
fn copy_and_paste_remaps_ids() {
    let a = box_at(0.0, 0.0, 50.0, 50.0);
    let mut engine = engine_with_scene(vec![a.clone()]);
    engine.select(vec![a.id.clone()]);
    engine.copy_selection();
    assert!(engine.paste_json(None, None));

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 2);
    assert_ne!(scene[0].id, scene[1].id);
}

#[test]
fn copy_and_paste_preserves_internal_connector_bindings() {
    let left = box_at(0.0, 0.0, 100.0, 60.0);
    let right = box_at(300.0, 0.0, 100.0, 60.0);
    let mut link = connector(100.0, 30.0, 300.0, 30.0, DrawElementType::Arrow);
    link.start_binding = Some(left.id.clone());
    link.end_binding = Some(right.id.clone());

    let mut engine = engine_with_scene(vec![left.clone(), right.clone(), link.clone()]);
    engine.select(vec![left.id.clone(), right.id.clone(), link.id.clone()]);
    engine.copy_selection();
    engine.paste_json(None, None);

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 6);
    let new_arrow = scene
        .iter()
        .find(|e| e.id != link.id && e.kind == DrawElementType::Arrow)
        .unwrap();
    let start_id = new_arrow.start_binding.as_ref().unwrap();
    let end_id = new_arrow.end_binding.as_ref().unwrap();
    assert_ne!(start_id, &left.id);
    assert_ne!(end_id, &right.id);
}

#[test]
fn cut_selection_returns_json_and_deletes() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    let json = engine.cut_selection();

    assert!(json.is_some());
    assert!(deleted_count(&engine.get_scene()) >= 1);
    assert!(engine.get_selected_elements().is_empty());
}

#[test]
fn cut_empty_selection_returns_none() {
    let mut engine = DrawEngine::new();
    assert!(engine.cut_selection().is_none());
}

#[test]
fn duplicate_selection_with_offset() {
    let rect = box_at(10.0, 10.0, 50.0, 50.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.duplicate_selection(25.0, 35.0);

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 2);
    let copy = scene.iter().find(|e| e.id != rect.id).unwrap();
    assert_close(copy.x, 35.0);
    assert_close(copy.y, 45.0);
}

#[test]
fn duplicate_selection_empty_is_noop() {
    let mut engine = DrawEngine::new();
    engine.duplicate_selection(10.0, 10.0);
    assert!(engine.get_scene().is_empty());
}

#[test]
fn delete_selection_marks_elements_deleted() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    engine.delete_selection();

    let scene = engine.get_scene();
    assert!(scene.iter().find(|e| e.id == rect.id).unwrap().is_deleted);
}

#[test]
fn delete_selection_unbinds_container_label() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut label = text_at(0.0, 0.0, 20.0, 20.0);
    label.container_id = Some(rect.id.clone());

    let mut engine = engine_with_measure(vec![rect.clone(), label.clone()]);
    engine.select(vec![label.id.clone()]);
    engine.delete_selection();

    let scene = engine.get_scene();
    let host = scene.iter().find(|e| e.id == rect.id).unwrap();
    assert!(host.bound_text_id.is_none());
}

#[test]
fn undo_redo_color_change_cycle() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    let initial_color = engine.get_scene()[0].stroke_color.clone();

    engine.apply_style(stroke_patch("#123456"));
    assert_eq!(engine.get_scene()[0].stroke_color, "#123456");

    engine.undo();
    assert_eq!(engine.get_scene()[0].stroke_color, initial_color);

    engine.redo();
    assert_eq!(engine.get_scene()[0].stroke_color, "#123456");
}

#[test]
fn undo_at_stack_root_is_noop() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect]);
    engine.undo();
    assert_eq!(engine.get_scene().len(), 1);
}

#[test]
fn redo_at_stack_tip_is_noop() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![rect]);
    engine.redo();
    assert_eq!(engine.get_scene().len(), 1);
}

#[test]
fn export_json_valid_header() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let engine = engine_with_scene(vec![rect]);
    let json = engine.export_json();
    assert!(json.contains("\"type\": \"osidraw\""));
    assert!(json.contains("\"version\": 1"));
}

#[test]
fn export_svg_starts_with_svg_tag() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let engine = engine_with_scene(vec![rect]);
    let svg = engine.export_svg(10.0).unwrap();
    assert!(svg.starts_with("<svg "));
    assert!(svg.ends_with("</svg>"));
}

#[test]
fn load_scene_replaces_current_scene() {
    let a = box_at(0.0, 0.0, 50.0, 50.0);
    let engine = engine_with_scene(vec![a]);
    let json = engine.export_json();

    let mut new_engine = DrawEngine::new();
    assert!(new_engine.load_scene(&json));
    assert_eq!(new_engine.get_scene().len(), 1);
}
