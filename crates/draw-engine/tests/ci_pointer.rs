mod common;
use common::*;
use draw_engine::*;

#[test]
fn pointer_drag_rectangle() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(10.0, 10.0, false, false);
    engine.move_pointer(60.0, 50.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 1);
    assert_eq!(scene[0].kind, DrawElementType::Rectangle);
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

#[test]
fn pointer_drag_ellipse() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Ellipse);
    engine.begin_pointer(20.0, 20.0, false, false);
    engine.move_pointer(80.0, 70.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 1);
    assert_eq!(scene[0].kind, DrawElementType::Ellipse);
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

#[test]
fn pointer_drag_diamond() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Diamond);
    engine.begin_pointer(30.0, 30.0, false, false);
    engine.move_pointer(90.0, 80.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 1);
    assert_eq!(scene[0].kind, DrawElementType::Diamond);
}

#[test]
fn pointer_click_without_drag_does_not_create_shape() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(50.0, 50.0, false, false);
    engine.end_pointer();
    assert!(engine.get_scene().is_empty());
}

#[test]
fn pointer_arrow_creates_and_binds() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![a.clone(), b.clone()]);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(50.0, 30.0, false, false);
    engine.move_pointer(350.0, 30.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 3);
    let arrow = scene.last().unwrap();
    assert_eq!(arrow.start_binding.as_deref(), Some(a.id.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(b.id.as_str()));
}

#[test]
fn pointer_line_creates_and_binds() {
    let a = box_at(0.0, 0.0, 100.0, 60.0);
    let b = box_at(300.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_scene(vec![a.clone(), b.clone()]);
    engine.set_tool(DrawTool::Line);
    engine.begin_pointer(50.0, 30.0, false, false);
    engine.move_pointer(350.0, 30.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 3);
    let line = scene.last().unwrap();
    assert_eq!(line.kind, DrawElementType::Line);
}

#[test]
fn pointer_connector_tiny_drag_ignored() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(20.0, 20.0, false, false);
    engine.move_pointer(21.0, 20.0, false, false);
    engine.end_pointer();
    assert!(engine.get_scene().is_empty());
}

#[test]
fn pointer_freedraw_single_point_ignored() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Freedraw);
    engine.begin_pointer(20.0, 20.0, false, false);
    engine.end_pointer();
    assert!(engine.get_scene().is_empty());
}

#[test]
fn pointer_freedraw_multiple_points_creates_element() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Freedraw);
    engine.begin_pointer(10.0, 10.0, false, false);
    engine.move_pointer(20.0, 15.0, false, false);
    engine.move_pointer(30.0, 25.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 1);
    assert_eq!(scene[0].kind, DrawElementType::Freedraw);
}

#[test]
fn pointer_text_tool_click_creates_and_edits() {
    let mut engine = engine_with_measure(vec![]);
    engine.set_tool(DrawTool::Text);
    engine.begin_pointer(100.0, 100.0, false, false);
    let events = engine.drain_events();
    assert!(events.text_edit.is_some());
    assert_eq!(engine.get_scene().len(), 1);
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

#[test]
fn pointer_hand_tool_pans_camera() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Hand);
    engine.begin_pointer(50.0, 50.0, false, false);
    engine.move_pointer(80.0, 100.0, false, false);
    assert!(engine.in_motion());
    engine.end_pointer();
    assert_close(engine.camera.x, 30.0);
    assert_close(engine.camera.y, 50.0);
}

#[test]
fn pointer_select_tool_click_element() {
    // Filled, so a click in the middle lands on it: a transparent shape is hit only on
    // its outline. This test is about which tool does what, not about that rule.
    let rect = filled(box_at(20.0, 20.0, 60.0, 40.0));
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.set_tool(DrawTool::Select);
    engine.begin_pointer(40.0, 40.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_selection(), vec![rect.id]);
}

#[test]
fn pointer_select_tool_click_empty_canvas_clears_selection() {
    // Filled, so a click in the middle lands on it: a transparent shape is hit only on
    // its outline. This test is about which tool does what, not about that rule.
    let rect = filled(box_at(20.0, 20.0, 60.0, 40.0));
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id]);
    assert_eq!(engine.get_selection().len(), 1);

    engine.set_tool(DrawTool::Select);
    engine.begin_pointer(500.0, 500.0, false, false);
    engine.end_pointer();
    assert!(engine.get_selection().is_empty());
}

#[test]
fn pointer_eraser_tool_deletes_clicked_element() {
    // Filled, so a click in the middle lands on it: a transparent shape is hit only on
    // its outline. This test is about which tool does what, not about that rule.
    let rect = filled(box_at(20.0, 20.0, 60.0, 40.0));
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.set_tool(DrawTool::Eraser);
    engine.begin_pointer(40.0, 40.0, false, false);
    engine.end_pointer();
    assert!(engine
        .get_scene()
        .iter()
        .any(|e| e.id == rect.id && e.is_deleted));
}

#[test]
fn pointer_eraser_tool_empty_click_is_noop() {
    let rect = box_at(20.0, 20.0, 60.0, 40.0);
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.set_tool(DrawTool::Eraser);
    engine.begin_pointer(400.0, 400.0, false, false);
    engine.end_pointer();
    assert!(!engine.get_scene()[0].is_deleted);
}

#[test]
fn pointer_tool_resets_to_select_after_creation() {
    let mut engine = DrawEngine::new();
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(10.0, 10.0, false, false);
    engine.move_pointer(50.0, 50.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_tool(), DrawTool::Select);
}
