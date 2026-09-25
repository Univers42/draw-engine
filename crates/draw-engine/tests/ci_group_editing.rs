//! The group being edited: how you get out of it, and when it stops being true.
//!
//! A double click steps into a group (`ci_groups_nested.rs`), and every click after that
//! is resolved relative to it. That makes the edited group a claim about the selection —
//! "what is held is inside this" — and the claim has to be dropped the moment it stops
//! holding. It used to be dropped in two places only, a click on something outside and
//! Escape, so it leaked: past a click on empty canvas, past select-all, past an undo that
//! removed the group, past a deletion that emptied it. A leaked edited group is not
//! harmless. Every later marquee is resolved inside a group it has nothing to do with,
//! so it selects half of some *other* group, and Ctrl+G on that builds a hierarchy that
//! contradicts itself.
//!
//! The oracle drops it in each of those places (`packages/excalidraw/components/App.tsx`,
//! `actions/actionSelectAll.ts`, `actions/actionDeleteSelected.tsx`,
//! `packages/element/src/delta.ts`), and steps up one level on Escape
//! (`actions/actionDeselect.ts`). Here the rule lives where the selection is set, so a
//! path that forgets it cannot exist.
//!
//! ```text
//! A [g1, g2]   B [g1, g2]   C [g2]        D [h]   E [h]
//!   0            150          300           600     750
//! ```

mod common;
use common::*;
use draw_engine::*;
use std::collections::HashSet;

/// Filled, so the middle of each box is a hit target rather than a hole.
fn square(x: f64) -> DrawElement {
    filled(box_at(x, 0.0, 80.0, 80.0))
}

/// The middle of the box that starts at `x`.
fn middle(x: f64) -> (f64, f64) {
    (x + 40.0, 40.0)
}

struct Board {
    engine: DrawEngine,
    a: String,
    b: String,
    c: String,
    d: String,
    e: String,
}

/// A and B grouped, then all three grouped; D and E a separate group beside them.
fn board() -> Board {
    let (a, b, c, d, e) = (
        square(0.0),
        square(150.0),
        square(300.0),
        square(600.0),
        square(750.0),
    );
    let ids = [&a, &b, &c, &d, &e].map(|el| el.id.clone());
    let mut engine = engine_with_scene(vec![a, b, c, d, e]);
    engine.set_tool(DrawTool::Select);
    let [a, b, c, d, e] = ids;
    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();
    engine.select(vec![a.clone(), b.clone(), c.clone()]);
    engine.group_selection();
    engine.select(vec![d.clone(), e.clone()]);
    engine.group_selection();
    engine.clear_selection();
    Board {
        engine,
        a,
        b,
        c,
        d,
        e,
    }
}

/// `A [g1, g2, g3]  B [g1, g2, g3]  C [g2, g3]  D [g3]` — three levels, one member added
/// at each, so every level is distinguishable by what it holds.
fn three_levels() -> (DrawEngine, [String; 4]) {
    let boxes = [square(0.0), square(150.0), square(300.0), square(450.0)];
    let ids = boxes.clone().map(|el| el.id);
    let mut engine = engine_with_scene(boxes.to_vec());
    engine.set_tool(DrawTool::Select);
    for level in 2..=4 {
        engine.select(ids[..level].to_vec());
        engine.group_selection();
    }
    engine.clear_selection();
    (engine, ids)
}

fn click(engine: &mut DrawEngine, (x, y): (f64, f64), shift: bool) {
    engine.begin_pointer(x, y, shift, false);
    engine.end_pointer();
}

/// A drag in steps, so it reads as a drag and not as a click.
fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64), shift: bool) {
    engine.begin_pointer(from.0, from.1, shift, false);
    for step in 1..=5 {
        let t = f64::from(step) / 5.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            shift,
            false,
        );
    }
    engine.end_pointer();
}

/// Click A, then double click it: inside g2, holding g1 = {A, B}.
fn enter_outer(board: &mut Board) {
    click(&mut board.engine, middle(0.0), false);
    board.engine.handle_double_click(40.0, 40.0);
    assert!(board.engine.editing_group_id().is_some());
    assert!(holds(&board.engine, &[&board.a, &board.b]));
}

fn selection(engine: &DrawEngine) -> HashSet<String> {
    engine.get_selection().into_iter().collect()
}

fn holds(engine: &DrawEngine, ids: &[&String]) -> bool {
    let actual = selection(engine);
    actual.len() == ids.len() && ids.iter().all(|id| actual.contains(*id))
}

fn group_ids_of(engine: &DrawEngine, id: &str) -> Vec<String> {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .map(|el| el.group_ids)
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Leaving
// ---------------------------------------------------------------------------

/// `clearSelection(null)` resets the edited group (`App.tsx:12889-12902`), so a click on
/// nothing is a way out. Left set, the next marquee around D was resolved inside g2,
/// where D has no group to expand to — and took D without E.
#[test]
fn a_click_on_empty_canvas_leaves_the_edited_group() {
    let mut board = board();
    enter_outer(&mut board);

    click(&mut board.engine, (500.0, 400.0), false);

    assert_eq!(board.engine.editing_group_id(), None);
    drag(&mut board.engine, (560.0, -40.0), (700.0, 120.0), false);
    assert!(
        holds(&board.engine, &[&board.d, &board.e]),
        "a marquee around D takes its whole group: {:?}",
        selection(&board.engine)
    );
}

/// A marquee starts on empty canvas, and that press already clears the edited group
/// (`App.tsx:9580`, `:11332`) — so it resolves at the top level, and a box around A
/// takes everything A is grouped with.
#[test]
fn a_marquee_inside_the_edited_group_selects_whole_top_level_groups() {
    let mut board = board();
    enter_outer(&mut board);

    drag(&mut board.engine, (-40.0, -40.0), (100.0, 120.0), false);

    assert_eq!(board.engine.editing_group_id(), None);
    assert!(
        holds(&board.engine, &[&board.a, &board.b, &board.c]),
        "{:?}",
        selection(&board.engine)
    );
}

/// Picking up the lasso puts the selection down, and the edited group with it
/// (`setActiveTool`, `App.tsx:6206-6218`). Kept, the loop around D was resolved inside
/// g2 and took D without E.
#[test]
fn picking_up_the_lasso_leaves_the_edited_group() {
    let mut board = board();
    enter_outer(&mut board);

    board.engine.set_tool(DrawTool::Lasso);
    assert_eq!(board.engine.editing_group_id(), None);

    board.engine.begin_pointer(570.0, -30.0, false, false);
    for (x, y) in [(690.0, -30.0), (690.0, 110.0), (570.0, 110.0)] {
        board.engine.move_pointer(x, y, false, false);
    }
    board.engine.end_pointer();
    assert!(
        holds(&board.engine, &[&board.d, &board.e]),
        "a loop around D takes its whole group: {:?}",
        selection(&board.engine)
    );
}

/// Shift or not, pressing something outside the edited group drops the selection and the
/// group before the press is resolved (`App.tsx:9639-9650`). Keeping the inner selection
/// under a shift-click held half of g2 beside all of h.
#[test]
fn a_shift_click_outside_the_edited_group_takes_only_what_was_clicked() {
    let mut board = board();
    enter_outer(&mut board);

    click(&mut board.engine, middle(600.0), true);

    assert_eq!(board.engine.editing_group_id(), None);
    assert!(
        holds(&board.engine, &[&board.d, &board.e]),
        "{:?}",
        selection(&board.engine)
    );
}

/// A shift-click that lets go of the last thing held lets go of the group with it, as the
/// oracle's empty selection does (`groups.ts:188-196`). The toggle used to edit the
/// selection behind `set_selection`'s back, and left g1 open around nothing.
#[test]
fn a_shift_click_that_empties_the_selection_leaves_the_edited_group() {
    let mut board = board();
    enter_outer(&mut board);
    board.engine.handle_double_click(40.0, 40.0);
    assert!(holds(&board.engine, &[&board.a]));

    click(&mut board.engine, middle(0.0), true);

    assert!(selection(&board.engine).is_empty());
    assert_eq!(board.engine.editing_group_id(), None);
}

/// A shift-drag that stays inside the edited group adds to it at that level, and the group
/// stays open — the oracle keeps both under shift (`App.tsx:11275-11296`).
#[test]
fn a_shift_marquee_inside_the_edited_group_stays_in_it() {
    let mut board = board();
    let outer = group_ids_of(&board.engine, &board.a)[1].clone();
    enter_outer(&mut board);

    drag(&mut board.engine, (260.0, -40.0), (400.0, 120.0), true);

    assert_eq!(board.engine.editing_group_id(), Some(outer));
    assert!(
        holds(&board.engine, &[&board.a, &board.b, &board.c]),
        "{:?}",
        selection(&board.engine)
    );
}

/// One that reaches outside leaves the group, and everything it holds is taken whole at
/// the top. Grown inside first and left after, it held D without E beside half of g2 —
/// a selection Ctrl+G turns into two groups that overlap. A deliberate divergence: the
/// oracle keeps g2 open over that same mixed selection (`App.tsx:11331-11345`).
#[test]
fn a_shift_marquee_reaching_outside_the_edited_group_takes_whole_groups() {
    let mut board = board();
    enter_outer(&mut board);

    drag(&mut board.engine, (560.0, -40.0), (700.0, 120.0), true);

    assert_eq!(board.engine.editing_group_id(), None);
    assert!(
        holds(
            &board.engine,
            &[&board.a, &board.b, &board.c, &board.d, &board.e]
        ),
        "{:?}",
        selection(&board.engine)
    );
}

/// Escape goes up **one** level and holds that level's group (`actionDeselect.ts:36-62`,
/// `:72-111`); only at the top does it let go. Leaving every level at once made three
/// double clicks down a one-key trip back to nothing.
#[test]
fn escape_goes_up_one_level_at_a_time() {
    let (mut engine, [a, b, c, d]) = three_levels();
    let groups = group_ids_of(&engine, &a);
    click(&mut engine, middle(0.0), false);
    engine.handle_double_click(40.0, 40.0);
    engine.handle_double_click(40.0, 40.0);
    assert_eq!(engine.editing_group_id().as_ref(), Some(&groups[1]));
    assert!(holds(&engine, &[&a, &b]));

    engine.cancel_pointer();
    assert_eq!(engine.editing_group_id().as_ref(), Some(&groups[2]));
    assert!(holds(&engine, &[&a, &b, &c]), "{:?}", selection(&engine));

    engine.cancel_pointer();
    assert_eq!(engine.editing_group_id(), None);
    assert!(
        holds(&engine, &[&a, &b, &c, &d]),
        "{:?}",
        selection(&engine)
    );

    engine.cancel_pointer();
    assert!(selection(&engine).is_empty());
}

/// Select-all is a top-level selection (`actionSelectAll.ts:49` passes `editingGroupId:
/// null`). Kept, Ctrl+G inserted the new level *inside* g2 while wrapping D and E from
/// outside it, and the result no longer nested.
#[test]
fn select_all_leaves_the_edited_group() {
    let mut board = board();
    enter_outer(&mut board);

    board.engine.select_all();
    assert_eq!(board.engine.editing_group_id(), None);
    board.engine.group_selection();

    let all = [&board.a, &board.b, &board.c, &board.d, &board.e];
    let outermost: HashSet<String> = all
        .iter()
        .filter_map(|id| group_ids_of(&board.engine, id).last().cloned())
        .collect();
    assert_eq!(
        outermost.len(),
        1,
        "one new outermost group around all five"
    );
    for x in [0.0, 150.0, 300.0, 600.0, 750.0] {
        board.engine.clear_selection();
        click(&mut board.engine, middle(x), false);
        assert_eq!(
            selection(&board.engine).len(),
            5,
            "clicking any member takes the lot"
        );
    }
}

/// When everything is inside the edited group, select-all still resolves at the top: the
/// selection *is* g2, and grouping it again is the no-op it is anywhere else.
#[test]
fn select_all_inside_a_group_that_holds_everything_is_still_top_level() {
    let (mut engine, [a, _, _, _]) = three_levels();
    let before = group_ids_of(&engine, &a);
    click(&mut engine, middle(0.0), false);
    engine.handle_double_click(40.0, 40.0);
    assert!(engine.editing_group_id().is_some());

    engine.select_all();
    engine.group_selection();

    assert_eq!(engine.editing_group_id(), None);
    assert_eq!(
        group_ids_of(&engine, &a),
        before,
        "not wrapped a second time"
    );
}

/// Undo can remove the edited group itself; the oracle then drops it (`delta.ts:806-818`).
/// Left set, it named a group nothing carried, and every click and marquee on the board
/// was resolved inside it — so no group anywhere expanded.
#[test]
fn undo_leaves_no_stale_edited_group() {
    let mut board = board();
    enter_outer(&mut board);

    board.engine.undo(); // h
    board.engine.undo(); // g2

    assert_eq!(board.engine.editing_group_id(), None);
    drag(&mut board.engine, (-40.0, -40.0), (100.0, 120.0), false);
    assert!(
        holds(&board.engine, &[&board.a, &board.b]),
        "A still expands to g1: {:?}",
        selection(&board.engine)
    );
}

// ---------------------------------------------------------------------------
// Narrowing
// ---------------------------------------------------------------------------

/// A press on something already selected might be the start of a drag, so it keeps the
/// selection; released without moving, it was a click, and a click selects what it hit
/// (`App.tsx:12183-12190`, `:12322-12345`).
#[test]
fn a_click_on_a_selected_member_narrows_to_its_group() {
    let mut board = board();
    board.engine.select_all();

    click(&mut board.engine, middle(0.0), false);

    assert!(
        holds(&board.engine, &[&board.a, &board.b, &board.c]),
        "{:?}",
        selection(&board.engine)
    );
}

/// The same press, dragged, moves everything that was selected — narrowing is for clicks.
#[test]
fn a_drag_from_a_selected_member_keeps_the_whole_selection() {
    let mut board = board();
    board.engine.select_all();

    drag(&mut board.engine, middle(0.0), (40.0, 240.0), false);

    assert_eq!(selection(&board.engine).len(), 5);
    let moved = board
        .engine
        .get_scene()
        .iter()
        .filter(|el| (el.y - 200.0).abs() < 1e-9)
        .count();
    assert_eq!(moved, 5, "all five moved together");
}

/// Inside a group the click narrows at that level, and stays inside it.
#[test]
fn a_click_inside_the_edited_group_narrows_at_that_level() {
    let mut board = board();
    enter_outer(&mut board);
    click(&mut board.engine, middle(300.0), true);
    assert!(holds(&board.engine, &[&board.a, &board.b, &board.c]));

    click(&mut board.engine, middle(0.0), false);

    assert!(board.engine.editing_group_id().is_some());
    assert!(
        holds(&board.engine, &[&board.a, &board.b]),
        "g1, the group A has at this level: {:?}",
        selection(&board.engine)
    );
}

// ---------------------------------------------------------------------------
// Deleting
// ---------------------------------------------------------------------------

/// With two or more left, the group stays open and its first member is selected
/// (`actionDeleteSelected.tsx:130-137`, then `handleGroupEditingState`, `:190-205`).
#[test]
fn deleting_inside_a_group_selects_the_next_sibling() {
    let (mut engine, [a, _, _, d]) = three_levels();
    let groups = group_ids_of(&engine, &a);
    click(&mut engine, middle(0.0), false);
    engine.handle_double_click(40.0, 40.0);
    click(&mut engine, middle(450.0), false);
    assert!(holds(&engine, &[&d]));

    engine.delete_selection();

    assert_eq!(engine.editing_group_id().as_ref(), Some(&groups[2]));
    assert!(holds(&engine, &[&a]), "{:?}", selection(&engine));
}

/// One left: the level is not a group any more, so it steps up to the group around it,
/// if that one still is (`actionDeleteSelected.tsx:138-167`).
#[test]
fn deleting_down_to_one_steps_up_a_level() {
    let mut board = board();
    let outer = group_ids_of(&board.engine, &board.a)[1].clone();
    enter_outer(&mut board);
    board.engine.handle_double_click(40.0, 40.0);
    assert!(holds(&board.engine, &[&board.a]));

    board.engine.delete_selection();

    assert_eq!(board.engine.editing_group_id(), Some(outer));
    assert!(
        holds(&board.engine, &[&board.b]),
        "{:?}",
        selection(&board.engine)
    );
}

/// Nothing around it: the group is left, and the survivor is what is held.
#[test]
fn deleting_down_to_one_at_the_outermost_level_leaves_the_group() {
    let mut board = board();
    enter_outer(&mut board);

    board.engine.delete_selection();

    assert_eq!(board.engine.editing_group_id(), None);
    assert!(
        holds(&board.engine, &[&board.c]),
        "{:?}",
        selection(&board.engine)
    );
}

// ---------------------------------------------------------------------------
// A group of one
// ---------------------------------------------------------------------------

/// A and B grouped, then B deleted: A still carries the id, but a group of one is not a
/// group (`groups.ts:42-54`, `:134-141`).
fn lone_member() -> (DrawEngine, String) {
    let (a, b) = (square(0.0), square(150.0));
    let (ia, ib) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![ia.clone(), ib.clone()]);
    engine.group_selection();
    engine.select(vec![ib]);
    engine.delete_selection();
    assert_eq!(group_ids_of(&engine, &ia).len(), 1);
    (engine, ia)
}

/// There is nothing inside to step into, so the double click does what it does on any
/// lone shape — opens its label — rather than entering a group of one.
#[test]
fn a_double_click_does_not_enter_a_group_of_one() {
    let (mut engine, a) = lone_member();
    click(&mut engine, middle(0.0), false);
    assert!(holds(&engine, &[&a]));
    let _ = engine.drain_events();

    engine.handle_double_click(40.0, 40.0);

    assert_eq!(engine.editing_group_id(), None);
    assert!(
        engine.drain_events().text_edit.is_some(),
        "the label editor opens, as on any shape"
    );
}

/// A label stands for its shape (`DrawEngine::element_at`): a double click on the label
/// of a grouped shape steps into the group holding the shape, as a click there picks the
/// shape up — never the label on its own, which moves only with its shape.
#[test]
fn a_double_click_on_a_label_steps_in_to_its_shape() {
    let (mut shape, mut other) = (square(0.0), square(150.0));
    let mut label = text_at(20.0, 28.0, 40.0, 25.0);
    shape.bound_text_id = Some(label.id.clone());
    label.container_id = Some(shape.id.clone());
    for element in [&mut shape, &mut other, &mut label] {
        element.group_ids = vec!["g".into()];
    }
    let (shape_id, label_id) = (shape.id.clone(), label.id.clone());
    let mut engine = engine_with_scene(vec![shape, label, other]);
    engine.set_tool(DrawTool::Select);
    click(&mut engine, (40.0, 40.0), false);

    engine.handle_double_click(40.0, 40.0);

    assert_eq!(engine.editing_group_id().as_deref(), Some("g"));
    assert!(
        holds(&engine, &[&shape_id]),
        "{:?}, the label is {label_id}",
        selection(&engine)
    );
}

/// Ungroup has no group to remove (`actionGroup.tsx:222-231` finds no selected group), so
/// it changes nothing — not even the dead id.
#[test]
fn ungroup_leaves_a_group_of_one_alone() {
    let (mut engine, a) = lone_member();
    let before = group_ids_of(&engine, &a);
    engine.select(vec![a.clone()]);

    engine.ungroup_selection();

    assert_eq!(group_ids_of(&engine, &a), before);
}

// ---------------------------------------------------------------------------
// Found in review
// ---------------------------------------------------------------------------

/// A loop drawn with the lasso already in hand is resolved at the level being edited, as
/// the oracle's (`lasso/index.ts:72-89`, `:119-127`): its press empties the selection
/// but keeps the group, where emptying it here used to drop the group too.
#[test]
fn a_lasso_inside_the_edited_group_stays_at_that_level() {
    let mut board = board();
    board.engine.set_tool_locked(true);
    board.engine.set_tool(DrawTool::Lasso);
    let around = |engine: &mut DrawEngine, x0: f64, x1: f64| {
        engine.begin_pointer(x0, -30.0, false, false);
        for (x, y) in [(x1, -30.0), (x1, 110.0), (x0, 110.0)] {
            engine.move_pointer(x, y, false, false);
        }
        engine.end_pointer();
    };
    around(&mut board.engine, -30.0, 410.0);
    board.engine.handle_double_click(40.0, 40.0);
    let editing = board.engine.editing_group_id();
    assert!(editing.is_some(), "setup");

    around(&mut board.engine, 270.0, 410.0);

    assert_eq!(board.engine.editing_group_id(), editing);
    assert!(
        holds(&board.engine, &[&board.c]),
        "{:?}",
        selection(&board.engine)
    );
}

/// A locked member is never picked up, so a delete inside the group does not hand it to
/// the next Delete — two presses of the key used to take the locked one as well.
#[test]
fn deleting_inside_a_group_never_hands_on_a_locked_member() {
    let mut engine = engine_with_scene(vec![square(0.0), square(150.0), square(300.0)]);
    let ids: Vec<String> = engine.get_scene().into_iter().map(|el| el.id).collect();
    engine.set_tool(DrawTool::Select);
    engine.select(ids.clone());
    engine.group_selection();
    engine.select(vec![ids[0].clone()]);
    engine.toggle_lock_selection();
    click(&mut engine, middle(150.0), false);
    engine.handle_double_click(190.0, 40.0);
    assert!(holds(&engine, &[&ids[1]]), "setup");

    engine.delete_selection();
    assert!(
        holds(&engine, &[&ids[2]]),
        "the free sibling, not the locked one"
    );
    engine.delete_selection();

    assert!(selection(&engine).is_empty());
    engine.delete_selection();
    let survivor = engine.get_scene().into_iter().find(|el| el.id == ids[0]);
    assert!(
        survivor.is_some_and(|el| !el.is_deleted),
        "the locked member survives"
    );
}

/// Nor one a peer holds: skipped, the next free member is held and the group stays open.
#[test]
fn deleting_inside_a_group_skips_a_member_a_peer_holds() {
    let mut engine = engine_with_scene((0..4).map(|i| square(150.0 * f64::from(i))).collect());
    let ids: Vec<String> = engine.get_scene().into_iter().map(|el| el.id).collect();
    engine.set_tool(DrawTool::Select);
    engine.select(ids.clone());
    engine.group_selection();
    engine.clear_selection();
    engine.set_peers(vec![Peer {
        id: "ana".into(),
        name: "Ana".into(),
        color: "#e03131".into(),
        holds: [ids[0].clone()].into_iter().collect(),
        preview: Vec::new(),
    }]);
    click(&mut engine, middle(450.0), false);
    engine.handle_double_click(490.0, 40.0);
    assert!(holds(&engine, &[&ids[3]]), "setup");

    engine.delete_selection();

    assert!(engine.editing_group_id().is_some());
    assert!(holds(&engine, &[&ids[1]]), "{:?}", selection(&engine));
}

/// Any move makes the press a drag (`drag.hasOccurred`, `App.tsx:10918-10921`), even one
/// that comes back to where it started — it is not a click, and does not narrow.
#[test]
fn a_drag_that_comes_back_home_does_not_narrow() {
    let mut board = board();
    board.engine.select_all();
    let (x, y) = middle(0.0);

    board.engine.begin_pointer(x, y, false, false);
    board
        .engine
        .move_pointer(x + 100.0, y + 100.0, false, false);
    board.engine.move_pointer(x, y, false, false);
    board.engine.end_pointer();

    assert_eq!(selection(&board.engine).len(), 5);
}

/// A peer ungrouping the group being edited here leaves it here too: kept, it named a
/// group nothing carries, and the host was told so.
#[test]
fn a_peer_ungrouping_the_edited_group_leaves_it() {
    let mut board = board();
    enter_outer(&mut board);
    let editing = board.engine.editing_group_id().expect("setup");
    let theirs: Vec<DrawElement> = board
        .engine
        .get_scene()
        .into_iter()
        .filter(|el| el.group_ids.contains(&editing))
        .map(|mut el| {
            el.group_ids.retain(|g| *g != editing);
            el.version += 1;
            el
        })
        .collect();

    assert!(board.engine.apply_remote_patch(&scene_to_json(&theirs)));

    assert_eq!(board.engine.editing_group_id(), None);
}
