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

/// The Delete key, the same defect the eraser and the vectorize had and the same release
/// they make: an arrow that stays is let go of the shape deleted from under it, as
/// Excalidraw's `fixBindingsAfterDeletion` does (`binding.ts@1118751f:2297-2311` — it
/// releases, it never moves a binding to another id, `binding.ts:2569`, `:2577`). Its other
/// end, on a shape nobody deleted, is still good and stays bound to it: the rule is
/// one-directional (`delta.ts@1118751f:1976-1979`).
#[test]
fn delete_selection_lets_an_arrow_go_of_the_shape_it_deleted() {
    let (mut engine, arrow_id, left_id, right_id) = bound_arrow_over_two_shapes();
    engine.select(vec![right_id.clone()]);
    engine.delete_selection();

    let scene = engine.get_scene();
    assert!(scene.iter().find(|e| e.id == right_id).unwrap().is_deleted);
    let arrow = scene.iter().find(|e| e.id == arrow_id).unwrap();
    assert!(!arrow.is_deleted, "the arrow was not selected");
    assert_eq!(arrow.end_binding, None, "let go of the deleted shape");
    assert_eq!(arrow.end_fixed_point, None, "with no anchor left");
    assert_eq!(
        arrow.start_binding.as_deref(),
        Some(left_id.as_str()),
        "the other shape is still there"
    );
}

/// The release is part of the same step of history as the tombstone, as it is for the
/// eraser: one undo gives back both, the shape and the arrow's hold on it.
#[test]
fn one_undo_binds_the_arrow_to_the_deleted_shape_again() {
    let (mut engine, arrow_id, _left_id, right_id) = bound_arrow_over_two_shapes();
    engine.select(vec![right_id.clone()]);
    engine.delete_selection();

    engine.undo();

    let scene = engine.get_scene();
    let arrow = scene.iter().find(|e| e.id == arrow_id).unwrap();
    assert_eq!(arrow.end_binding.as_deref(), Some(right_id.as_str()));
    assert!(arrow.end_fixed_point.is_some(), "with its anchor back too");
    assert!(
        !scene
            .iter()
            .find(|e| e.id == right_id)
            .expect("the shape is back")
            .is_deleted
    );
}

/// Two filled boxes with an arrow drawn from inside one to inside the other, so both its
/// ends are bound — the binding the engine itself makes, not one assigned to it.
fn bound_arrow_over_two_shapes() -> (DrawEngine, String, String, String) {
    let left = filled(box_at(0.0, 0.0, 100.0, 60.0));
    let right = filled(box_at(300.0, 0.0, 100.0, 60.0));
    let (left_id, right_id) = (left.id.clone(), right.id.clone());
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(50.0, 30.0, false, false);
    engine.move_pointer(200.0, 30.0, false, false);
    engine.move_pointer(350.0, 30.0, false, false);
    engine.end_pointer();
    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow was drawn");
    assert_eq!(
        arrow.start_binding.as_deref(),
        Some(left_id.as_str()),
        "setup"
    );
    assert_eq!(
        arrow.end_binding.as_deref(),
        Some(right_id.as_str()),
        "setup"
    );
    (engine, arrow.id, left_id, right_id)
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
    let svg = svg_of(&engine, 10.0);
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
