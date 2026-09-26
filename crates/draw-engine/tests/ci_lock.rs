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
