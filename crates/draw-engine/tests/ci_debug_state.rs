//! The debug snapshot tells the truth.
//!
//! This is instrumentation, so it has one job: report what is actually there. That makes
//! it *more* worth testing than most code, not less — a metric that lies is worse than no
//! metric, because it is believed. Every case here changes the engine and then asserts the
//! snapshot noticed.
//!
//! It also pins the shape of the JSON, which an external inspector reads. Renaming a field
//! silently is how a debugging tool starts reporting `undefined` and nobody finds out
//! until they are already three hypotheses deep into something unrelated.

mod common;
use common::*;
use draw_engine::*;

fn engine_with_two() -> (DrawEngine, String, String) {
    let a = filled(box_at(0.0, 0.0, 100.0, 60.0));
    let b = filled(box_at(200.0, 0.0, 100.0, 60.0));
    let (ida, idb) = (a.id.clone(), b.id.clone());
    (engine_with_scene(vec![a, b]), ida, idb)
}

#[test]
fn it_counts_the_live_elements() {
    let (engine, _a, _b) = engine_with_two();
    let state = engine.debug_state();
    assert_eq!(state.scene.element_count, 2);
    assert_eq!(state.scene.deleted_count, 0);
}

/// A tombstone is still in the store — a deletion has to be *sent* to the other editors,
/// so it is a live element carrying `is_deleted`, not an absence. The snapshot has to
/// report both numbers or "why is this board slow with nothing on it" has no answer.
#[test]
fn a_deletion_moves_an_element_from_live_to_deleted() {
    let (mut engine, a, _b) = engine_with_two();
    engine.select(vec![a]);
    engine.delete_selection();

    let state = engine.debug_state();
    assert_eq!(state.scene.element_count, 1);
    assert_eq!(state.scene.deleted_count, 1);
}

#[test]
fn it_reports_the_selection_sorted() {
    let (mut engine, a, b) = engine_with_two();
    engine.select(vec![b.clone(), a.clone()]);

    let state = engine.debug_state();
    assert_eq!(state.scene.selected_count, 2);
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(
        state.scene.selected_ids, expected,
        "a HashSet has no stable order, so an unsorted list would make every \
         differential comparison fail for no reason"
    );
}

#[test]
fn the_revision_moves_when_the_scene_does() {
    let (mut engine, a, _b) = engine_with_two();
    let before = engine.debug_state().scene.revision;

    engine.select(vec![a]);
    engine.nudge_selection(10.0, 0.0);

    assert_ne!(
        engine.debug_state().scene.revision,
        before,
        "the revision is the closest thing here to a state-update count"
    );
}

#[test]
fn it_reports_the_camera_and_the_canvas() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(800.0, 600.0, 2.0);
    engine.zoom_at(400.0, 300.0, 2.0);

    let state = engine.debug_state();
    assert_close(state.viewport.width, 800.0);
    assert_close(state.viewport.height, 600.0);
    assert_close(state.viewport.dpr, 2.0);
    assert!(
        state.viewport.zoom > 1.0,
        "zooming in should show as a zoom above 1"
    );
}

/// The visible rectangle is what culling keeps, so it is the number that explains
/// "elements rendered" being far below "element count".
#[test]
fn the_visible_rectangle_shrinks_as_you_zoom_in() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(800.0, 600.0, 1.0);
    let [x0, y0, x1, y1] = engine.debug_state().viewport.visible_world;
    let before = (x1 - x0) * (y1 - y0);

    engine.zoom_at(400.0, 300.0, 2.0);
    let [x0, y0, x1, y1] = engine.debug_state().viewport.visible_world;
    let after = (x1 - x0) * (y1 - y0);

    assert!(
        after < before,
        "zooming in shows less of the world: {before} -> {after}"
    );
}

#[test]
fn it_names_the_gesture_in_progress() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Rectangle);
    assert_eq!(engine.debug_state().interaction.kind, None);

    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(200.0, 180.0, false, false);
    let state = engine.debug_state();
    assert_eq!(state.interaction.kind, Some("draft"));
    assert!(state.interaction.dragging);
    assert!(!state.interaction.resizing);

    engine.end_pointer();
    assert_eq!(
        engine.debug_state().interaction.kind,
        None,
        "the gesture is over, so nothing should still be reported as in progress"
    );
}

#[test]
fn it_reports_a_resize_as_resizing_rather_than_dragging() {
    let (mut engine, a, _b) = engine_with_two();
    engine.select(vec![a]);
    // The south-east handle sits `handle_offset` outside the corner.
    engine.begin_pointer(108.0, 68.0, false, false);
    engine.move_pointer(160.0, 120.0, false, false);

    let state = engine.debug_state();
    assert_eq!(state.interaction.kind, Some("resize"));
    assert!(state.interaction.resizing);
    assert!(!state.interaction.dragging);
}

#[test]
fn it_reports_a_path_being_placed_point_by_point() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Line);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();

    let state = engine.debug_state();
    assert!(
        state.interaction.placing_linear.is_some(),
        "a click with the line tool leaves a path waiting for its next point"
    );
    assert_eq!(state.interaction.tool, "line");
}

/// The JSON is the contract an external inspector reads. These names are load-bearing.
#[test]
fn the_json_carries_the_documented_shape() {
    let (mut engine, a, _b) = engine_with_two();
    engine.select(vec![a]);
    let json = engine.debug_state_json();

    for key in [
        "\"scene\"",
        "\"viewport\"",
        "\"interaction\"",
        "\"elementCount\"",
        "\"deletedCount\"",
        "\"selectedIds\"",
        "\"revision\"",
        "\"canUndo\"",
        "\"zoom\"",
        "\"dpr\"",
        "\"visibleWorld\"",
        "\"tool\"",
        "\"placingLinear\"",
    ] {
        assert!(json.contains(key), "{key} missing from {json}");
    }
}
