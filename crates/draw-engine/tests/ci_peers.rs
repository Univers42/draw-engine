//! Other people in the room: what they hold nobody else can touch, and what they are
//! doing shows up while they do it.
//!
//! Drawing together, a shape someone was moving appeared on everyone else's screen only
//! when they let go, and two people could take the same shape at once — one erased it
//! while the other was moving it, and the other's next edit brought it back. See
//! `engine/peers.rs`.

mod common;
use common::*;
use draw_engine::*;

fn peer(id: &str, holds: &[&str]) -> Peer {
    Peer {
        id: id.into(),
        name: format!("{id}'s name"),
        color: "#e03131".into(),
        holds: holds.iter().map(|id| (*id).to_string()).collect(),
        preview: Vec::new(),
    }
}

fn selected(engine: &DrawEngine) -> Vec<String> {
    engine
        .get_selected_elements()
        .into_iter()
        .map(|element| element.id)
        .collect()
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("in the scene")
}

/// Two filled shapes: `a` at (100, 100), `b` at (400, 100), both 100 × 80.
fn two_shapes() -> (DrawEngine, String, String) {
    let a = filled(box_at(100.0, 100.0, 100.0, 80.0));
    let b = filled(box_at(400.0, 100.0, 100.0, 80.0));
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    (engine_with_scene(vec![a, b]), a_id, b_id)
}

#[test]
fn what_a_peer_holds_cannot_be_clicked_or_dragged() {
    let (mut engine, a, _) = two_shapes();
    engine.set_peers(vec![peer("ana", &[&a])]);

    engine.begin_pointer(150.0, 140.0, false, false);
    engine.move_pointer(250.0, 240.0, false, false);
    engine.end_pointer();

    assert!(selected(&engine).is_empty(), "a held shape was selected");
    assert_close(element(&engine, &a).x, 100.0);
}

#[test]
fn a_marquee_and_select_all_leave_it_out() {
    let (mut engine, a, b) = two_shapes();
    engine.set_peers(vec![peer("ana", &[&a])]);

    engine.begin_pointer(20.0, 20.0, false, false);
    engine.move_pointer(700.0, 400.0, false, false);
    engine.end_pointer();
    assert_eq!(selected(&engine), vec![b.clone()], "the marquee took it");

    engine.clear_selection();
    engine.select_all();
    assert_eq!(selected(&engine), vec![b.clone()], "select all took it");

    // So nothing done to a selection — delete, style, nudge — can reach it.
    engine.delete_selection();
    assert!(!element(&engine, &a).is_deleted);
}

#[test]
fn the_eraser_passes_over_it() {
    // The one that brought this about: erased while a colleague was moving it, and back
    // on their next edit.
    let (mut engine, a, b) = two_shapes();
    engine.set_peers(vec![peer("ana", &[&a])]);
    engine.set_tool(DrawTool::Eraser);

    engine.begin_pointer(0.0, 140.0, false, false);
    engine.move_pointer(700.0, 140.0, false, false);
    engine.end_pointer();

    assert!(
        !element(&engine, &a).is_deleted,
        "the eraser took a held shape"
    );
    assert!(element(&engine, &b).is_deleted, "and still takes the rest");
}

#[test]
fn it_shows_no_move_cursor() {
    let (mut engine, a, _) = two_shapes();
    assert_eq!(engine.hover_cursor(150.0, 140.0), HoverCursor::Move);
    engine.set_peers(vec![peer("ana", &[&a])]);
    assert_ne!(engine.hover_cursor(150.0, 140.0), HoverCursor::Move);
}

#[test]
fn a_click_on_it_can_say_who_has_it() {
    let (mut engine, a, _) = two_shapes();
    engine.set_peers(vec![peer("ana", &[&a])]);
    assert_eq!(
        engine.peer_at(150.0, 140.0).map(|p| p.id.as_str()),
        Some("ana")
    );
    assert!(engine.peer_at(450.0, 140.0).is_none(), "nobody holds b");
    assert!(engine.peer_at(700.0, 500.0).is_none(), "nothing there");
}

/// `a` of [`two_shapes`] with a label on it, and the label's id.
fn labelled_a() -> (DrawEngine, String, String, String) {
    let mut a = filled(box_at(100.0, 100.0, 100.0, 80.0));
    let mut label = text_at(110.0, 128.0, 80.0, 25.0);
    a.bound_text_id = Some(label.id.clone());
    label.container_id = Some(a.id.clone());
    let b = filled(box_at(400.0, 100.0, 100.0, 80.0));
    let ids = (a.id.clone(), b.id.clone(), label.id.clone());
    (engine_with_scene(vec![a, label, b]), ids.0, ids.1, ids.2)
}

/// A click on the label of what they hold says who has the shape: a label stands for its
/// shape, as the press reads it (`DrawEngine::element_at`).
#[test]
fn a_click_on_the_label_of_what_they_hold_says_who_has_it() {
    let (mut engine, a, _, _) = labelled_a();
    engine.set_peers(vec![peer("ana", &[&a])]);
    assert_eq!(
        engine.peer_at(150.0, 140.0).map(|p| p.id.as_str()),
        Some("ana")
    );
}

#[test]
fn what_you_had_is_let_go_when_someone_else_takes_it() {
    let (mut engine, a, b) = two_shapes();
    engine.select(vec![a.clone(), b.clone()]);
    engine.set_peers(vec![peer("ana", &[&a])]);
    assert_eq!(selected(&engine), vec![b]);
}

#[test]
fn a_drag_in_progress_is_abandoned_not_committed_over_them() {
    let (mut engine, a, _) = two_shapes();
    engine.begin_pointer(150.0, 140.0, false, false);
    engine.move_pointer(250.0, 240.0, false, false);
    assert_close(element(&engine, &a).x, 200.0);

    engine.set_peers(vec![peer("ana", &[&a])]);
    engine.move_pointer(350.0, 340.0, false, false);
    engine.end_pointer();

    assert_close(element(&engine, &a).x, 100.0);
    assert!(selected(&engine).is_empty());
}

#[test]
fn undo_leaves_what_they_hold_as_it_is() {
    let (mut engine, a, _) = two_shapes();
    engine.begin_pointer(150.0, 140.0, false, false);
    engine.move_pointer(250.0, 140.0, false, false);
    engine.end_pointer();
    assert_close(element(&engine, &a).x, 200.0);

    engine.set_peers(vec![peer("ana", &[&a])]);
    engine.undo();
    assert_close(element(&engine, &a).x, 200.0);

    // And once they let go, it is yours to undo again.
    engine.set_peers(Vec::new());
    engine.redo();
    engine.undo();
    assert_close(element(&engine, &a).x, 100.0);
}

#[test]
fn letting_go_makes_it_touchable_again() {
    let (mut engine, a, _) = two_shapes();
    engine.set_peers(vec![peer("ana", &[&a])]);
    engine.set_peers(Vec::new());
    engine.begin_pointer(150.0, 140.0, false, false);
    engine.end_pointer();
    assert_eq!(selected(&engine), vec![a]);
}

#[test]
fn their_gesture_is_painted_while_it_runs_and_saved_nowhere() {
    let (mut engine, a, _) = two_shapes();
    let before = element(&engine, &a);
    let mut moving = before.clone();
    moving.x = 300.0;
    let mut ana = peer("ana", &[]);
    ana.preview = vec![moving];
    engine.set_peers(vec![ana]);

    let view = engine.paint_view();
    let painted = view.elements.iter().find(|e| e.id == a).unwrap();
    assert_close(painted.x, 300.0);
    assert!(
        view.live.contains(&a),
        "painted fresh, over the cached rest"
    );
    drop(view);

    assert_eq!(element(&engine, &a), before, "a preview is not an edit");
    // And it is theirs while they move it.
    engine.begin_pointer(300.0 + 50.0, 140.0, false, false);
    engine.end_pointer();
    assert!(selected(&engine).is_empty());
}

#[test]
fn a_shape_they_are_drawing_is_painted_on_top_before_it_exists() {
    let (mut engine, _, _) = two_shapes();
    let drawing = filled(box_at(120.0, 120.0, 40.0, 40.0));
    let id = drawing.id.clone();
    let mut ana = peer("ana", &[]);
    ana.preview = vec![drawing];
    engine.set_peers(vec![ana]);

    let view = engine.paint_view();
    assert_eq!(
        view.elements.last().map(|e| e.id.as_str()),
        Some(id.as_str())
    );
    drop(view);
    assert_eq!(engine.get_scene().len(), 2);
}

#[test]
fn the_commit_outranks_the_preview_whichever_arrives_first() {
    // The preview carries the version from before the gesture; the commit stamps above
    // it. Once the commit is here the preview is stale, even if its end has not arrived.
    let (mut engine, a, _) = two_shapes();
    let before = element(&engine, &a);
    let mut preview = before.clone();
    preview.x = 300.0;
    let mut ana = peer("ana", &[]);
    ana.preview = vec![preview.clone()];
    engine.set_peers(vec![ana]);

    let mut committed = preview;
    committed.x = 320.0;
    committed.version = before.version + 1;
    committed.version_nonce = before.version_nonce.wrapping_add(1);
    let patch = serde_json::json!({ "type": "osidraw", "elements": [committed] });
    assert!(engine.apply_remote_patch(&patch.to_string()));

    let view = engine.paint_view();
    let painted = view.elements.iter().find(|e| e.id == a).unwrap();
    assert_close(painted.x, 320.0);
}

#[test]
fn what_they_hold_is_marked_with_their_name_and_colour() {
    let (mut engine, a, b) = two_shapes();
    let mut moving = element(&engine, &b);
    moving.x = 500.0;
    let mut ana = peer("ana", &[&a]);
    ana.preview = vec![moving];
    engine.set_peers(vec![ana, peer("ben", &[])]);

    let view = engine.paint_view();
    assert_eq!(
        view.peer_marks.len(),
        1,
        "ben holds nothing, so has no mark"
    );
    let mark = &view.peer_marks[0];
    assert_eq!(mark.name, "ana's name");
    assert_eq!(mark.color, "#e03131");
    let marked: Vec<(&str, f64)> = mark.elements.iter().map(|e| (e.id.as_str(), e.x)).collect();
    assert_eq!(
        marked,
        vec![(a.as_str(), 100.0), (b.as_str(), 500.0)],
        "the preview is outlined where it is shown"
    );
}

#[test]
fn only_this_engines_own_gesture_is_sent_to_peers() {
    let (mut engine, a, b) = two_shapes();
    assert!(engine.gesture_elements().is_empty());

    let mut moving = element(&engine, &b);
    moving.x = 500.0;
    let mut ana = peer("ana", &[]);
    ana.preview = vec![moving];
    engine.set_peers(vec![ana]);
    assert!(
        engine.gesture_elements().is_empty(),
        "a peer's preview would be echoed back to them as ours"
    );

    engine.begin_pointer(150.0, 140.0, false, false);
    engine.move_pointer(170.0, 150.0, false, false);
    let sent = engine.gesture_elements();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].id, a);
    assert_close(sent[0].x, 120.0);

    engine.end_pointer();
    assert!(engine.gesture_elements().is_empty());
}

#[test]
fn an_abandoned_drag_takes_what_they_sent_meanwhile() {
    // Their edit arrived while the drag was running and was refused, as a peer's copy of
    // anything being changed here is. With the drag put back, theirs is the newest.
    let (mut engine, a, _) = two_shapes();
    let before = element(&engine, &a);
    engine.begin_pointer(150.0, 140.0, false, false);
    engine.move_pointer(250.0, 240.0, false, false);

    let mut theirs = before.clone();
    theirs.x = 600.0;
    theirs.version = before.version + 1;
    let patch = serde_json::json!({ "type": "osidraw", "elements": [theirs] });
    engine.apply_remote_patch(&patch.to_string());
    assert_close(element(&engine, &a).x, 200.0);

    engine.set_peers(vec![peer("ana", &[&a])]);
    assert_close(element(&engine, &a).x, 600.0);
    assert_eq!(element(&engine, &a).version, before.version + 1);
}
