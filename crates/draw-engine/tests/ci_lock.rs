//! Lock and unlock: what the toggle acts on, what it leaves selected, and every way a
//! locked element is reached again — `actionElementLock.ts@1118751f` and the hit tests in
//! `App.tsx@1118751f` that read `locked`.
//!
//! How a locked element travels with its group or frame is `ci_group_locks_frames.rs`.
//! This file is the command around it, which is where "lock and unlock with groups"
//! came apart: a lock that kept its selection handed the locked group to the next Delete,
//! a right-click on a locked member freed that member alone, and a group holding one
//! locked member offered to lock it all over again.
//!
//! Every scene starts from the same probe: three filled 80×80 boxes in a row, A and B
//! grouped, C loose.
//!
//! ```text
//!   A(0)  B(150)      C(300)
//!   [A B]
//! ```

mod common;
use common::*;
use draw_engine::*;
use std::collections::HashSet;

struct Probe {
    engine: DrawEngine,
    a: String,
    b: String,
    c: String,
}

/// Filled, so the middle of each box is a hit target.
fn probe() -> Probe {
    let boxes: Vec<DrawElement> = [0.0, 150.0, 300.0]
        .into_iter()
        .map(|x| filled(box_at(x, 0.0, 80.0, 80.0)))
        .collect();
    let [a, b, c] = std::array::from_fn(|i| boxes[i].id.clone());
    let mut engine = engine_with_scene(boxes);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();
    engine.clear_selection();
    Probe { engine, a, b, c }
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

fn locked(engine: &DrawEngine, id: &str) -> bool {
    element(engine, id).locked()
}

fn selection(engine: &DrawEngine) -> HashSet<String> {
    engine.get_selection().into_iter().collect()
}

fn set(ids: &[&String]) -> HashSet<String> {
    ids.iter().map(|id| (*id).clone()).collect()
}

fn click(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.begin_pointer(x, y, false, false);
    engine.end_pointer();
}

fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) {
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
}

// ---------------------------------------------------------------------------
// The toggle
// ---------------------------------------------------------------------------

/// Locking lets go of what it locked (`nextSelectedElementIds = {}`,
/// `actionElementLock.ts@1118751f:108-110`). Kept selected, the locked group was still
/// what Delete, Ctrl+D and the style panel acted on — the lock locked nothing.
#[test]
fn locking_a_group_lets_go_of_it() {
    let Probe {
        mut engine, a, b, ..
    } = probe();
    click(&mut engine, 40.0, 40.0);
    assert_eq!(selection(&engine), set(&[&a, &b]), "setup: the group");

    engine.toggle_lock_selection();

    assert!(locked(&engine, &a) && locked(&engine, &b));
    assert!(selection(&engine).is_empty(), "{:?}", selection(&engine));
    engine.delete_selection();
    assert!(!element(&engine, &a).is_deleted && !element(&engine, &b).is_deleted);
}

/// One step of undo takes the lock back and the selection with it, and redo lets go of it
/// again — the oracle captures both (`CaptureUpdateAction.IMMEDIATELY`, `:147`).
#[test]
fn undo_gives_back_the_lock_and_the_selection() {
    let Probe {
        mut engine, a, b, ..
    } = probe();
    click(&mut engine, 40.0, 40.0);
    engine.toggle_lock_selection();

    engine.undo();
    assert!(!locked(&engine, &a) && !locked(&engine, &b));
    assert_eq!(selection(&engine), set(&[&a, &b]));

    engine.redo();
    assert!(locked(&engine, &a) && locked(&engine, &b));
    assert!(selection(&engine).is_empty());
}

/// Lock only when nothing is locked, unlock otherwise (`shouldLock`, `:22-23`). A group
/// holding one locked member is selected whole through the other, and the toggle then
/// unlocks — it used to offer Lock, and lock the rest too, so the one locked member could
/// never be freed with the group.
#[test]
fn a_group_holding_a_locked_member_is_unlocked() {
    let Probe {
        mut engine, a, b, ..
    } = probe();
    engine.select(vec![b.clone()]);
    engine.toggle_lock_selection();
    click(&mut engine, 40.0, 40.0);
    assert_eq!(
        selection(&engine),
        set(&[&a, &b]),
        "setup: A takes its group"
    );

    assert!(engine.selection_locked(), "the menu reads Unlock");
    engine.toggle_lock_selection();

    assert!(!locked(&engine, &a) && !locked(&engine, &b));
    assert_eq!(
        selection(&engine),
        set(&[&a, &b]),
        "what was unlocked stays in hand"
    );
}

/// Undo and redo walk a lock and a grouping back and forth in any order, each step
/// taking back its own change and nothing else — the lock does not survive the undo of
/// the grouping under it, nor the grouping the undo of the lock.
#[test]
fn undo_and_redo_walk_a_lock_and_a_grouping_in_any_order() {
    let Probe {
        mut engine,
        a,
        b,
        c,
    } = probe();
    click(&mut engine, 40.0, 40.0);
    engine.begin_pointer(340.0, 40.0, true, false);
    engine.end_pointer();
    engine.group_selection();
    let depth = |engine: &DrawEngine| [&a, &b, &c].map(|id| element(engine, id).group_ids.len());
    let locks = |engine: &DrawEngine| [&a, &b, &c].map(|id| locked(engine, id));
    assert_eq!(depth(&engine), [2, 2, 1], "setup: [[A B] C]");
    engine.toggle_lock_selection();
    assert_eq!(locks(&engine), [true; 3]);

    engine.undo();
    assert_eq!((depth(&engine), locks(&engine)), ([2, 2, 1], [false; 3]));
    engine.undo();
    assert_eq!((depth(&engine), locks(&engine)), ([1, 1, 0], [false; 3]));
    engine.redo();
    engine.redo();
    assert_eq!((depth(&engine), locks(&engine)), ([2, 2, 1], [true; 3]));

    engine.select_element(&c);
    engine.ungroup_selection();
    assert_eq!((depth(&engine), locks(&engine)), ([1, 1, 0], [true; 3]));
    engine.undo();
    assert_eq!((depth(&engine), locks(&engine)), ([2, 2, 1], [true; 3]));
    engine.undo();
    assert_eq!((depth(&engine), locks(&engine)), ([2, 2, 1], [false; 3]));
}

/// A label is locked with its shape (`includeBoundTextElement: true`, `:52-56`), so the
/// file says what the board does.
#[test]
fn locking_a_labelled_shape_locks_its_label() {
    let mut shape = filled(box_at(0.0, 0.0, 120.0, 80.0));
    let mut label = text_at(20.0, 30.0, 80.0, 20.0);
    label.text = Some("hi".into());
    label.container_id = Some(shape.id.clone());
    shape.bound_text_id = Some(label.id.clone());
    let (shape_id, label_id) = (shape.id.clone(), label.id.clone());
    let mut engine = engine_with_scene(vec![shape, label]);
    engine.select(vec![shape_id.clone()]);

    engine.toggle_lock_selection();
    assert!(locked(&engine, &shape_id) && locked(&engine, &label_id));

    engine.select(vec![shape_id.clone()]);
    engine.toggle_lock_selection();
    assert!(!locked(&engine, &shape_id) && !locked(&engine, &label_id));
}

/// A frame is locked with what it holds (`includeElementsInFrames: true`, `:52-56`).
/// Locked alone, its children were still picked up and dragged out of the locked frame.
#[test]
fn locking_a_frame_locks_what_it_holds() {
    let frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        },
    );
    let mut child = filled(box_at(40.0, 40.0, 80.0, 80.0));
    child.frame_id = Some(frame.id.clone());
    let (frame_id, child_id) = (frame.id.clone(), child.id.clone());
    let mut engine = engine_with_scene(vec![child, frame]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![frame_id.clone()]);

    engine.toggle_lock_selection();
    assert!(locked(&engine, &frame_id) && locked(&engine, &child_id));
    drag(&mut engine, (80.0, 80.0), (80.0, 380.0));
    assert_close(element(&engine, &child_id).y, 40.0);

    engine.select(vec![frame_id.clone()]);
    engine.toggle_lock_selection();
    assert!(!locked(&engine, &frame_id) && !locked(&engine, &child_id));
}

// ---------------------------------------------------------------------------
// Reaching a locked element again
// ---------------------------------------------------------------------------

/// A right-click selects what it lands on as a click would, grown to its group — locked
/// or not (`openContextMenu` through `selectGroupsForSelectedElements`,
/// `App.tsx@1118751f:13276-13319`). Selected alone, the menu's Unlock freed one member of
/// a locked group and left the rest locked.
#[test]
fn a_right_click_on_a_locked_member_takes_its_group() {
    let Probe {
        mut engine, a, b, ..
    } = probe();
    click(&mut engine, 40.0, 40.0);
    engine.toggle_lock_selection();

    engine.select_element(&b);
    assert_eq!(selection(&engine), set(&[&a, &b]));
    assert!(engine.selection_locked(), "the menu reads Unlock");

    engine.toggle_lock_selection();
    assert!(!locked(&engine, &a) && !locked(&engine, &b));
    drag(&mut engine, (40.0, 40.0), (40.0, 240.0));
    assert_close(element(&engine, &a).y, 200.0);
    assert_close(element(&engine, &b).y, 200.0);
}

/// Inside the group being edited, at that level (`editingGroupId`, `:13301-13306`).
#[test]
fn a_right_click_inside_the_edited_group_takes_that_level() {
    let Probe {
        mut engine, a, b, ..
    } = probe();
    click(&mut engine, 40.0, 40.0);
    engine.handle_double_click(40.0, 40.0);
    assert_eq!(selection(&engine), set(&[&a]), "setup: stepped in");

    engine.select_element(&b);

    assert_eq!(selection(&engine), set(&[&b]));
    assert!(engine.editing_group_id().is_some());
}

/// Select All takes what can be picked up (`!element.locked`, `actionSelectAll.ts@1118751f:
/// 31-38`), then grows it to whole groups (`:45-53`). Taking a loose locked element handed
/// it to the Delete, the Ctrl+G and the Ctrl+D that follow a Select All — a locked
/// background deleted, or grouped with everything and dragged off with the next click.
#[test]
fn select_all_passes_a_locked_element_by() {
    let Probe {
        mut engine,
        a,
        b,
        c,
    } = probe();
    engine.select(vec![c.clone()]);
    engine.toggle_lock_selection();

    engine.select_all();
    assert_eq!(selection(&engine), set(&[&a, &b]));

    engine.group_selection();
    engine.delete_selection();
    let c = element(&engine, &c);
    assert!(!c.is_deleted && c.group_ids.is_empty());
}

/// A locked member comes in with its group, as a click on the group takes it.
#[test]
fn select_all_takes_a_locked_member_with_its_group() {
    let Probe {
        mut engine,
        a,
        b,
        c,
    } = probe();
    engine.select(vec![b.clone()]);
    engine.toggle_lock_selection();

    engine.select_all();

    assert_eq!(selection(&engine), set(&[&a, &b, &c]));
}

/// The board menu's "Unlock all" (`actionUnlockAllElements`,
/// `actionElementLock.ts@1118751f:161-217`): offered with nothing selected and something
/// locked, it unlocks everything and selects it, grown to groups — one step of undo.
#[test]
fn unlock_all_frees_every_locked_element_and_selects_it() {
    let Probe {
        mut engine,
        a,
        b,
        c,
    } = probe();
    assert!(!engine.can_unlock_all(), "nothing is locked");
    engine.select(vec![b.clone(), c.clone()]);
    engine.toggle_lock_selection();
    assert!(engine.can_unlock_all());
    engine.select(vec![a.clone()]);
    assert!(!engine.can_unlock_all(), "not with a selection");
    engine.clear_selection();

    engine.unlock_all();

    assert!(!locked(&engine, &b) && !locked(&engine, &c));
    assert_eq!(selection(&engine), set(&[&a, &b, &c]));
    engine.undo();
    assert!(locked(&engine, &b) && locked(&engine, &c));
}

/// A lock is two fields of the elements, and travels as they do: a peer's lock arrives
/// with the group it was made on, and is undone from here by the same right-click.
#[test]
fn a_peers_lock_arrives_with_its_group_and_is_undone_here() {
    let Probe {
        mut engine, a, b, ..
    } = probe();
    let mut peer = engine_with_scene(engine.get_scene());
    peer.set_tool(DrawTool::Select);
    click(&mut peer, 40.0, 40.0);
    peer.toggle_lock_selection();

    engine.apply_remote_patch(&scene_to_json(&peer.get_scene()));
    assert!(locked(&engine, &a) && locked(&engine, &b));
    click(&mut engine, 40.0, 40.0);
    assert!(selection(&engine).is_empty());

    engine.select_element(&a);
    engine.toggle_lock_selection();
    peer.apply_remote_patch(&scene_to_json(&engine.get_scene()));
    assert!(!locked(&peer, &a) && !locked(&peer, &b));
}

// ---------------------------------------------------------------------------
// A locked element in front
// ---------------------------------------------------------------------------

/// A locked element over another shields it: a press that lands on the locked one first
/// picks nothing up, unless what lies under it is already selected
/// (`hitElementMightBeLocked`, `App.tsx@1118751f:9488-9525`). It used to reach through and
/// pick up whatever was behind, and the cursor promised as much.
#[test]
fn a_locked_element_in_front_shields_what_is_behind_it() {
    let behind = filled(box_at(0.0, 0.0, 200.0, 200.0));
    let mut front = filled(box_at(50.0, 50.0, 100.0, 100.0));
    front.locked = Some(true);
    let behind_id = behind.id.clone();
    let mut engine = engine_with_scene(vec![behind, front]);
    engine.set_tool(DrawTool::Select);

    assert_eq!(engine.hover_cursor(100.0, 100.0), HoverCursor::Default);
    drag(&mut engine, (100.0, 100.0), (100.0, 300.0));
    assert_close(element(&engine, &behind_id).y, 0.0);
    assert!(selection(&engine).is_empty(), "{:?}", selection(&engine));

    // Beside the locked one it is still there to take, and once taken it can be dragged
    // from under the locked one.
    click(&mut engine, 20.0, 20.0);
    assert_eq!(selection(&engine), set(&[&behind_id]));
    assert_eq!(engine.hover_cursor(100.0, 100.0), HoverCursor::Move);
    drag(&mut engine, (100.0, 100.0), (100.0, 300.0));
    assert_close(element(&engine, &behind_id).y, 200.0);
}
