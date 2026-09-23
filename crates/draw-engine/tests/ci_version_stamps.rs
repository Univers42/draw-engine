//! Every local edit moves the element's stamp; undo is a new edit; a peer's edit is
//! nobody else's.
//!
//! `version` / `versionNonce` is the only change signal the autosave, the server's merge
//! and every peer have. Moves, resizes, rotations, nudges and re-routed arrows never
//! moved it — so none of them was saved, and a peer's later recolour of the same element
//! reverted the move on every screen, including the one that made it. See
//! `src/engine/stamp.rs` for the rule and Excalidraw's version of it.

mod common;
use common::*;
use draw_engine::*;

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

/// A filled box, so a press anywhere inside it grabs it.
fn one_box() -> (DrawEngine, String) {
    let element = filled(box_at(100.0, 100.0, 120.0, 80.0));
    let id = element.id.clone();
    (engine_with_scene(vec![element]), id)
}

fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) {
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=5 {
        let t = step as f64 / 5.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
}

/// A peer receiving everything this engine holds, tombstones included — what the
/// realtime channel and a reload both amount to.
fn sync(from: &DrawEngine, to: &mut DrawEngine) -> bool {
    to.apply_remote_patch(&scene_to_json(&from.get_scene()))
}

fn stamp(element: &DrawElement) -> (u32, u32) {
    (element.version, element.version_nonce)
}

// ---------------------------------------------------------------------------
// Gestures stamp
// ---------------------------------------------------------------------------

#[test]
fn a_new_shape_commits_at_version_1() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine, (100.0, 100.0), (300.0, 220.0));

    let scene = engine.get_scene();
    assert_eq!(scene.len(), 1);
    // However many moves sized the draft: they were one edit, the creation.
    assert_eq!(scene[0].version, 1);
}

#[test]
fn a_move_bumps_the_version_once() {
    let (mut engine, id) = one_box();
    let before = get(&engine, &id);

    drag(&mut engine, (160.0, 140.0), (260.0, 200.0));

    let after = get(&engine, &id);
    assert_close(after.x - before.x, 100.0);
    assert_eq!(after.version, before.version + 1, "five moves, one edit");
    assert_ne!(after.version_nonce, before.version_nonce);
}

/// What the autosave is actually handed.
#[test]
fn the_move_is_in_what_the_host_is_told() {
    let (mut engine, id) = one_box();
    let _ = engine.drain_events();

    drag(&mut engine, (160.0, 140.0), (260.0, 200.0));

    let delta = engine
        .drain_events()
        .scene_delta
        .expect("a move is a delta");
    let sent = delta
        .updated
        .iter()
        .find(|el| el.id == id)
        .expect("the moved box is in it");
    assert_eq!(sent.version, 2);
    assert_close(sent.x, 200.0);
}

#[test]
fn a_resize_bumps_it() {
    let (mut engine, id) = one_box();
    engine.select(vec![id.clone()]);

    // The south-east handle sits just outside the corner at (220, 180).
    drag(&mut engine, (228.0, 188.0), (268.0, 228.0));

    let after = get(&engine, &id);
    assert!(after.width > 130.0, "setup: it was resized");
    assert_eq!(after.version, 2);
}

#[test]
fn a_nudge_bumps_it() {
    let (mut engine, id) = one_box();
    engine.select(vec![id.clone()]);

    engine.nudge_selection(1.0, 0.0);

    assert_eq!(get(&engine, &id).version, 2);
}

/// Derived edits are edits: an arrow that re-routed after its shape moved has a new
/// shape of its own, and a peer that never hears of it draws the old one.
#[test]
fn an_arrow_that_follows_its_shape_is_bumped() {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(350.0, 0.0, 100.0, 80.0));
    let right_id = right.id.clone();
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_tool(DrawTool::Arrow);
    drag(&mut engine, (50.0, 40.0), (400.0, 40.0));
    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow was drawn");
    assert_eq!(
        arrow.end_binding.as_deref(),
        Some(right_id.as_str()),
        "setup"
    );

    engine.select(vec![right_id]);
    drag(&mut engine, (400.0, 40.0), (400.0, 160.0));

    let after = get(&engine, &arrow.id);
    assert!(after.height.abs() > 60.0, "setup: the arrow followed");
    assert_eq!(after.version, arrow.version + 1);
}

/// One edit, one bump — a path that stamps its own change is not stamped again.
#[test]
fn a_style_change_is_bumped_once() {
    let (mut engine, id) = one_box();
    engine.select(vec![id.clone()]);

    engine.apply_style(DrawElementStylePatch {
        stroke_color: Some("#e03131".into()),
        ..Default::default()
    });

    assert_eq!(get(&engine, &id).version, 2);
}

// ---------------------------------------------------------------------------
// Nothing changed, nothing stamped
// ---------------------------------------------------------------------------

#[test]
fn a_drag_away_and_back_stamps_nothing_and_records_nothing() {
    let (mut engine, id) = one_box();
    let before = get(&engine, &id);

    engine.begin_pointer(160.0, 140.0, false, false);
    engine.move_pointer(260.0, 200.0, false, false);
    engine.move_pointer(160.0, 140.0, false, false);
    engine.end_pointer();

    assert_eq!(stamp(&get(&engine, &id)), stamp(&before));
    assert!(!engine.debug_state().scene.can_undo, "no step to undo");
}

#[test]
fn a_click_that_selects_stamps_nothing() {
    let (mut engine, id) = one_box();
    let before = get(&engine, &id);

    engine.begin_pointer(160.0, 140.0, false, false);
    engine.end_pointer();

    assert_eq!(stamp(&get(&engine, &id)), stamp(&before));
}

// ---------------------------------------------------------------------------
// Two editors
// ---------------------------------------------------------------------------

/// The reported scenario. A moves the box; B, having received the move, recolours it;
/// A receives that. Before, the move carried no stamp: B refused it as already known,
/// recoloured its unmoved copy, and that copy — newer by stamp — put the box back on
/// A's own screen.
#[test]
fn a_move_survives_a_peers_later_recolour() {
    let (mut a, id) = one_box();
    let mut b = engine_with_scene(a.get_scene());

    drag(&mut a, (160.0, 140.0), (260.0, 200.0));
    assert!(sync(&a, &mut b), "B takes A's move");
    assert_close(get(&b, &id).x, 200.0);

    b.select(vec![id.clone()]);
    b.apply_style(DrawElementStylePatch {
        stroke_color: Some("#e03131".into()),
        ..Default::default()
    });
    assert!(sync(&b, &mut a), "A takes B's recolour");

    for (name, engine) in [("A", &a), ("B", &b)] {
        let element = get(engine, &id);
        assert_close(element.x, 200.0);
        assert_eq!(element.stroke_color, "#e03131", "{name} has both edits");
    }
}

/// A peer's copy of an element this client is in the middle of dragging is refused, as
/// Excalidraw refuses it, and the drag then commits *above* the refused version — the
/// later edit wins, here and everywhere it is sent.
#[test]
fn a_peers_edit_mid_drag_does_not_take_the_drag_over() {
    let (mut a, id) = one_box();
    let mut peer_copy = get(&a, &id);
    peer_copy.x = 900.0;
    peer_copy.version = 5;

    a.begin_pointer(160.0, 140.0, false, false);
    a.move_pointer(200.0, 170.0, false, false);
    a.apply_remote_patch(&scene_to_json(&[peer_copy.clone()]));
    a.move_pointer(260.0, 200.0, false, false);
    a.end_pointer();

    let after = get(&a, &id);
    assert_close(after.x, 200.0);
    assert!(
        after.version > 5,
        "stamped above the refused v5: {}",
        after.version
    );

    let mut peer = engine_with_scene(vec![peer_copy]);
    assert!(sync(&a, &mut peer), "and the peer takes it");
    assert_close(get(&peer, &id).x, 200.0);
}

/// What a peer changed here is the peer's edit. The next local commit must not claim it
/// with a stamp of its own.
#[test]
fn a_peers_edit_is_not_stamped_as_ours() {
    let mine = filled(box_at(100.0, 100.0, 120.0, 80.0));
    let theirs = filled(box_at(400.0, 100.0, 120.0, 80.0));
    let (mine_id, theirs_id) = (mine.id.clone(), theirs.id.clone());
    let mut engine = engine_with_scene(vec![mine, theirs.clone()]);

    let mut edited = theirs;
    edited.x = 500.0;
    edited.version = 2;
    engine.apply_remote_patch(&scene_to_json(&[edited.clone()]));
    drag(&mut engine, (160.0, 140.0), (160.0, 240.0));

    assert_eq!(stamp(&get(&engine, &theirs_id)), stamp(&edited));
    assert_eq!(
        get(&engine, &mine_id).version,
        2,
        "control: ours was stamped"
    );
}

// ---------------------------------------------------------------------------
// Undo and redo
// ---------------------------------------------------------------------------

/// Undo restored the old stamp, so the server and every peer refused it as stale and
/// the undone move came back on reload. It is a new edit, as in Excalidraw.
#[test]
fn undo_is_sent_as_a_new_edit() {
    let (mut engine, id) = one_box();
    let mut peer = engine_with_scene(engine.get_scene());
    drag(&mut engine, (160.0, 140.0), (260.0, 200.0));
    sync(&engine, &mut peer);

    engine.undo();

    let undone = get(&engine, &id);
    assert_close(undone.x, 100.0);
    assert_eq!(undone.version, 3, "above the move it undid");
    assert!(
        sync(&engine, &mut peer),
        "a peer that had the move takes the undo"
    );
    assert_close(get(&peer, &id).x, 100.0);
}

#[test]
fn redo_is_stamped_above_the_undo() {
    let (mut engine, id) = one_box();
    drag(&mut engine, (160.0, 140.0), (260.0, 200.0));
    engine.undo();

    engine.redo();

    let redone = get(&engine, &id);
    assert_close(redone.x, 200.0);
    assert_eq!(redone.version, 4);
}

/// A no-op after an undo — a click on empty canvas — must not become a step of its own
/// because the scene's stamps differ from the snapshot it was restored from. That would
/// throw the redo stack away.
#[test]
fn a_no_op_after_undo_keeps_redo() {
    let (mut engine, _id) = one_box();
    drag(&mut engine, (160.0, 140.0), (260.0, 200.0));
    engine.undo();

    engine.begin_pointer(700.0, 500.0, false, false);
    engine.end_pointer();

    assert!(engine.debug_state().scene.can_redo);
}

/// Undoing a creation deletes it as a stamped tombstone, so the deletion reaches
/// everyone else; redo brings it back above that tombstone.
#[test]
fn undoing_a_creation_sends_a_tombstone_and_redo_outranks_it() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine, (100.0, 100.0), (300.0, 220.0));
    let id = engine.get_scene()[0].id.clone();

    engine.undo();
    let tombstone = get(&engine, &id);
    assert!(tombstone.is_deleted);
    assert_eq!(tombstone.version, 2);

    engine.redo();
    let back = get(&engine, &id);
    assert!(!back.is_deleted);
    assert_eq!(back.version, 3);
}

/// The ordering that used to revert a peer: their edit arrives, then you commit
/// something unrelated — so their edit is in your snapshot — then you undo.
#[test]
fn undo_leaves_a_peers_earlier_edit_alone() {
    let mine = filled(box_at(100.0, 100.0, 120.0, 80.0));
    let theirs = filled(box_at(400.0, 100.0, 120.0, 80.0));
    let (mine_id, theirs_id) = (mine.id.clone(), theirs.id.clone());
    let mut engine = engine_with_scene(vec![mine, theirs.clone()]);

    let mut edited = theirs;
    edited.x = 500.0;
    edited.version = 2;
    engine.apply_remote_patch(&scene_to_json(&[edited]));
    engine.select(vec![mine_id.clone()]);
    engine.apply_style(DrawElementStylePatch {
        stroke_color: Some("#e03131".into()),
        ..Default::default()
    });

    engine.undo();

    assert_ne!(
        get(&engine, &mine_id).stroke_color,
        "#e03131",
        "ours undone"
    );
    assert_close(get(&engine, &theirs_id).x, 500.0);
    assert_eq!(get(&engine, &theirs_id).version, 2, "theirs untouched");
}

/// The other ordering, as a control: yours, then theirs, then undo.
#[test]
fn undo_leaves_a_peers_later_edit_alone() {
    let mine = filled(box_at(100.0, 100.0, 120.0, 80.0));
    let theirs = filled(box_at(400.0, 100.0, 120.0, 80.0));
    let (mine_id, theirs_id) = (mine.id.clone(), theirs.id.clone());
    let mut engine = engine_with_scene(vec![mine, theirs.clone()]);

    engine.select(vec![mine_id]);
    engine.apply_style(DrawElementStylePatch {
        stroke_color: Some("#e03131".into()),
        ..Default::default()
    });
    let mut edited = theirs;
    edited.x = 500.0;
    edited.version = 2;
    engine.apply_remote_patch(&scene_to_json(&[edited]));

    engine.undo();

    assert_close(get(&engine, &theirs_id).x, 500.0);
}

// ---------------------------------------------------------------------------
// Hygiene
// ---------------------------------------------------------------------------

/// A tombstone that kept the live nonce ties a peer's same-version edit on the nonce as
/// well, and is then decided by the clock — the tie-breaker that means least.
#[test]
fn a_delete_gets_a_fresh_nonce() {
    let (mut engine, id) = one_box();
    let before = get(&engine, &id);
    engine.select(vec![id.clone()]);

    engine.delete_selection();

    let tombstone = get(&engine, &id);
    assert!(tombstone.is_deleted);
    assert_eq!(tombstone.version, before.version + 1);
    assert_ne!(tombstone.version_nonce, before.version_nonce);
}
