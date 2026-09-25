//! Undo and redo put back the selection the step recorded.
//!
//! The oracle's history carries the selection in each entry's app-state delta
//! (`AppStateDelta`, `packages/element/src/delta.ts@1118751f:526-1015`), taken between two
//! captures of the store (`packages/element/src/store.ts@1118751f:376-385`): undo applies
//! the side before the step, redo the side after, keeping only what is still on the board
//! (`filterSelectedElements`, `delta.ts@1118751f:875-902`). Here every undo and redo let go
//! of everything, so the properties panel went away under the Ctrl+Z that was meant to
//! check what it showed.
//!
//! Each case was run on excalidraw.com (2026-09-25, `scratchpad/history-frames/oracle.cjs`).
//! A selection change is a capture there when a gesture ends or a command runs, never at the
//! press: a shape dragged from unselected comes back unselected.

mod common;
use common::*;
use draw_engine::*;
use std::collections::HashSet;

fn selection(engine: &DrawEngine) -> HashSet<String> {
    engine.get_selection().into_iter().collect()
}

fn set(ids: &[&String]) -> HashSet<String> {
    ids.iter().map(|id| (*id).clone()).collect()
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("no such element")
}

fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) {
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=5 {
        let t = f64::from(step) / 5.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
}

fn click(engine: &mut DrawEngine, at: (f64, f64)) {
    engine.begin_pointer(at.0, at.1, false, false);
    engine.end_pointer();
}

/// A at (100, 100) and B at (100, 400), filled so a press anywhere inside grabs them.
fn two_boxes() -> (DrawEngine, String, String) {
    let a = filled(box_at(100.0, 100.0, 100.0, 80.0));
    let b = filled(box_at(100.0, 400.0, 100.0, 80.0));
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Select);
    (engine, a_id, b_id)
}

const EMPTY: (f64, f64) = (700.0, 550.0);
const ON_A: (f64, f64) = (150.0, 140.0);
const ON_B: (f64, f64) = (150.0, 440.0);

/// OBSERVED: click A, Delete, Ctrl+Z → A is back and selected; Ctrl+Shift+Z → deleted,
/// nothing selected; Ctrl+Z → selected again.
#[test]
fn undoing_a_delete_selects_what_it_brings_back() {
    let (mut engine, a, _) = two_boxes();
    click(&mut engine, ON_A);
    engine.delete_selection();
    assert!(selection(&engine).is_empty(), "setup");

    engine.undo();
    assert!(!element(&engine, &a).is_deleted);
    assert_eq!(selection(&engine), set(&[&a]));

    engine.redo();
    assert!(element(&engine, &a).is_deleted);
    assert!(selection(&engine).is_empty());

    engine.undo();
    assert_eq!(selection(&engine), set(&[&a]));
}

/// The panel's own case: a style change undone keeps what it was made on selected, so
/// the panel stays up and shows the value undo put back.
#[test]
fn undoing_a_style_change_keeps_the_selection() {
    let (mut engine, a, _) = two_boxes();
    click(&mut engine, ON_A);
    engine.apply_style(DrawElementStylePatch {
        stroke_color: Some("#e03131".into()),
        ..Default::default()
    });

    engine.undo();
    assert_eq!(element(&engine, &a).stroke_color, "#1e1e1e");
    assert_eq!(selection(&engine), set(&[&a]));

    engine.redo();
    assert_eq!(element(&engine, &a).stroke_color, "#e03131");
    assert_eq!(selection(&engine), set(&[&a]));
}

/// Undoing a new shape gives back what was selected before it was drawn; redoing it
/// selects the shape, as drawing it did.
#[test]
fn undoing_a_new_shape_gives_back_the_selection_it_replaced() {
    let (mut engine, a, _) = two_boxes();
    click(&mut engine, ON_A);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine, (400.0, 100.0), (500.0, 180.0));
    let drawn: Vec<String> = engine.get_selection();
    assert_eq!(drawn.len(), 1, "setup");
    assert_ne!(drawn[0], a, "setup");

    engine.undo();
    assert!(element(&engine, &drawn[0]).is_deleted);
    assert_eq!(selection(&engine), set(&[&a]));

    engine.redo();
    assert_eq!(selection(&engine), set(&[&drawn[0]]));
}

/// OBSERVED: with nothing selected, B dragged and Ctrl+Z → B is back and not selected.
/// The press that picked B up is no capture; the selection the step began with is the
/// one the last gesture ended on.
#[test]
fn a_shape_dragged_from_unselected_comes_back_unselected() {
    let (mut engine, _, b) = two_boxes();
    click(&mut engine, EMPTY);
    drag(&mut engine, ON_B, (ON_B.0 + 30.0, ON_B.1));
    assert_eq!(selection(&engine), set(&[&b]), "setup");

    engine.undo();
    assert_close(element(&engine, &b).x, 100.0);
    assert!(selection(&engine).is_empty());

    engine.redo();
    assert_close(element(&engine, &b).x, 130.0);
    assert_eq!(selection(&engine), set(&[&b]));
}

/// The same with A selected first: undo gives A back, not B and not nothing.
#[test]
fn a_drag_that_changed_the_selection_is_undone_to_the_one_before() {
    let (mut engine, a, b) = two_boxes();
    click(&mut engine, ON_A);
    drag(&mut engine, ON_B, (ON_B.0 + 30.0, ON_B.1));
    assert_eq!(selection(&engine), set(&[&b]), "setup");

    engine.undo();
    assert_eq!(selection(&engine), set(&[&a]));
}

/// OBSERVED: Ctrl+A, Delete, Ctrl+Z → everything back and selected. A selection a command
/// made, with no gesture after it, is what the next step began with.
#[test]
fn select_all_then_delete_is_undone_to_everything_selected() {
    let (mut engine, a, b) = two_boxes();
    engine.select_all();
    engine.delete_selection();

    engine.undo();
    assert_eq!(selection(&engine), set(&[&a, &b]));
}

/// What a peer deleted since is not selected again (`filterSelectedElements`,
/// `delta.ts@1118751f:875-902`). The step undone is a new shape, so B is outside it:
/// a step that had changed B would put B's old content back over the tombstone, which is
/// what replay does to anything in the step (`replay_step`), not a selection question.
#[test]
fn undo_selects_only_what_is_still_there() {
    let (mut engine, a, b) = two_boxes();
    engine.select(vec![a.clone(), b.clone()]);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine, (400.0, 100.0), (500.0, 180.0));
    let mut gone = element(&engine, &b);
    gone.is_deleted = true;
    gone.version += 5;
    // Not `scene_to_json`, which drops tombstones; peers send them on the live wire.
    let patch = serde_json::json!({ "type": "osidraw", "version": 1, "elements": [gone] });
    assert!(engine.apply_remote_patch(&patch.to_string()), "setup");

    engine.undo();
    assert_eq!(selection(&engine), set(&[&a]));
}

/// Inside a group, the group being edited is part of what a step recorded: undo steps
/// back into it, as the oracle's `editingGroupId` is (`delta.ts@1118751f:806-818`).
#[test]
fn undo_steps_back_into_the_group_the_step_was_made_in() {
    let (mut engine, a, b) = two_boxes();
    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();
    click(&mut engine, EMPTY);
    click(&mut engine, ON_A);
    engine.handle_double_click(ON_A.0, ON_A.1);
    let group = engine.editing_group_id().expect("setup: inside the group");
    assert_eq!(selection(&engine), set(&[&a]), "setup");
    drag(&mut engine, ON_A, (ON_A.0 + 40.0, ON_A.1));
    click(&mut engine, EMPTY);
    assert_eq!(engine.editing_group_id(), None, "setup: out of it again");

    engine.undo();
    assert_close(element(&engine, &a).x, 100.0);
    assert_eq!(engine.editing_group_id(), Some(group));
    assert_eq!(selection(&engine), set(&[&a]));
}
