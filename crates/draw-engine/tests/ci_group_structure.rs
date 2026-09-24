//! What grouping, duplicating and reordering do to the structure of a group.
//!
//! A group is an id several elements carry, innermost first (`ci_groups_nested.rs` has
//! the model). Four operations had to respect that and did not:
//!
//! - **Duplicating inside an entered group** regenerated every level of `group_ids`, so
//!   the copy of something inside a group landed outside it. The oracle regenerates only
//!   the levels *inside* the group being edited and keeps that group and everything
//!   around it — `getNewGroupIdsForDuplication`, `packages/element/src/groups.ts:397-413`,
//!   called by `duplicateElement`, `packages/element/src/duplicate.ts:123-132`, for both
//!   Ctrl+D and Alt-drag.
//! - **Grouping** left the members wherever they were in the stack. The oracle gathers
//!   them directly under the topmost one, so a group is one run of the z-order —
//!   `packages/excalidraw/actions/actionGroup.tsx:170-186`.
//! - **Z-order** was a plain array shuffle, blind to groups: stepping forward slid a shape
//!   into the middle of a group, and bring-to-front inside an entered group carried the
//!   element out of it. The oracle steps over a group as one block and, inside an entered
//!   group, never leaves its range — `getTargetIndex` and `shiftElementsToEnd`,
//!   `packages/element/src/zindex.ts:205-311` and `:443-553`.
//! - **A label** stayed out of the group its shape joined, and z-order left it behind its
//!   shape. The oracle reads the selection with `includeBoundTextElement: true` for both —
//!   `actionGroup.tsx:96-101`, `zindex.ts:51-54` — and steps over a shape and its label as
//!   one, `zindex.ts:91-130`.
//!
//! Every board is a row of filled boxes, bottom of the stack first, so a stack reads left
//! to right in the assertions. Filled, so the middle of each is a hit target.

mod common;
use common::*;
use draw_engine::*;

type Cast = Vec<(&'static str, String)>;

fn x_of(index: usize) -> f64 {
    150.0 * index as f64
}

/// A row of boxes in stack order, each already carrying the groups listed for it.
///
/// Built directly rather than by grouping, so a z-order test does not also depend on what
/// grouping does to the order.
fn board(members: &[(&'static str, &[&str])]) -> (DrawEngine, Cast) {
    let mut elements = Vec::new();
    let mut cast = Vec::new();
    for (index, (name, groups)) in members.iter().enumerate() {
        let mut element = filled(box_at(x_of(index), 0.0, 80.0, 80.0));
        element.group_ids = groups.iter().map(|g| g.to_string()).collect();
        cast.push((*name, element.id.clone()));
        elements.push(element);
    }
    let mut engine = engine_with_scene(elements);
    engine.set_tool(DrawTool::Select);
    (engine, cast)
}

fn id(cast: &Cast, name: &str) -> String {
    cast.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, id)| id.clone())
        .expect("no such name")
}

/// The stack by name, bottom first. Anything the cast does not name is a `"copy"`.
fn stack(engine: &DrawEngine, cast: &Cast) -> Vec<&'static str> {
    engine
        .get_scene()
        .iter()
        .filter(|el| !el.is_deleted)
        .map(|el| {
            cast.iter()
                .find(|(_, id)| *id == el.id)
                .map_or("copy", |(name, _)| *name)
        })
        .collect()
}

fn copies(engine: &DrawEngine, cast: &Cast) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| !el.is_deleted && !cast.iter().any(|(_, id)| *id == el.id))
        .collect()
}

fn groups_of(engine: &DrawEngine, id: &str) -> Vec<String> {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .map(|el| el.group_ids)
        .unwrap_or_default()
}

fn middle(cast: &Cast, name: &str) -> (f64, f64) {
    let index = cast
        .iter()
        .position(|(n, _)| *n == name)
        .expect("no such name");
    (x_of(index) + 40.0, 40.0)
}

fn click(engine: &mut DrawEngine, cast: &Cast, name: &str) {
    let (x, y) = middle(cast, name);
    engine.begin_pointer(x, y, false, false);
    engine.end_pointer();
}

/// One double click on `name`: one level further into its groups.
fn step_in(engine: &mut DrawEngine, cast: &Cast, name: &str) {
    let (x, y) = middle(cast, name);
    engine.handle_double_click(x, y);
}

/// A and B share an inner group, and with C an outer one; D and E are a group of their
/// own beside them.
fn nested_beside_another() -> (DrawEngine, Cast) {
    board(&[
        ("A", &["g1", "g2"]),
        ("B", &["g1", "g2"]),
        ("C", &["g2"]),
        ("D", &["h"]),
        ("E", &["h"]),
    ])
}

// ---------------------------------------------------------------------------
// Duplicating inside an entered group
// ---------------------------------------------------------------------------

/// Inside g2 with the inner group held, the copy gets a fresh inner group but stays in g2.
/// Regenerating g2 as well put the copy in a group of its own, outside the one it was
/// made in — a second click on the original then no longer reached it.
#[test]
fn a_copy_made_inside_a_group_stays_in_that_group() {
    let (mut engine, cast) = nested_beside_another();
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");
    assert_eq!(engine.editing_group_id().as_deref(), Some("g2"), "setup");

    engine.duplicate_selection(0.0, 200.0);

    let copies = copies(&engine, &cast);
    assert_eq!(copies.len(), 2, "A and B are copied");
    for copy in &copies {
        assert_eq!(
            copy.group_ids.len(),
            2,
            "both levels survive: {:?}",
            copy.group_ids
        );
        assert_eq!(
            copy.group_ids[1], "g2",
            "the group being edited is kept, or the copy leaves it"
        );
        assert_ne!(
            copy.group_ids[0], "g1",
            "the level inside it is fresh, or the copy joins the original's inner group"
        );
    }
    assert_eq!(
        copies[0].group_ids, copies[1].group_ids,
        "and the two copies still share their inner group"
    );
    assert_eq!(engine.editing_group_id().as_deref(), Some("g2"));
}

/// Two levels in, the element itself is held: nothing lies inside the edited group, so
/// every level is kept and the copy is a sibling of the original.
#[test]
fn a_copy_of_a_leaf_joins_the_group_it_was_made_in() {
    let (mut engine, cast) = nested_beside_another();
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");
    assert_eq!(engine.editing_group_id().as_deref(), Some("g1"), "setup");

    engine.duplicate_selection(0.0, 200.0);

    let copies = copies(&engine, &cast);
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].group_ids, vec!["g1", "g2"]);
}

/// Alt-drag duplicates through the same door as Ctrl+D, `App.duplicate.ts:195-199`.
#[test]
fn alt_drag_inside_a_group_leaves_the_copy_in_it() {
    let (mut engine, cast) = nested_beside_another();
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");

    let (x, y) = middle(&cast, "A");
    engine.begin_pointer(x, y, false, true);
    engine.move_pointer(x, y + 100.0, false, false);
    engine.move_pointer(x, y + 200.0, false, false);
    engine.end_pointer();

    let copies = copies(&engine, &cast);
    assert_eq!(copies.len(), 2, "A and B are copied");
    for copy in &copies {
        assert_eq!(copy.group_ids.get(1).map(String::as_str), Some("g2"));
    }
}

// ---------------------------------------------------------------------------
// Grouping gathers the members
// ---------------------------------------------------------------------------

/// A group is one run of the stack. Left apart, whatever lay between the members would be
/// painted *through* the group, and stepping the group forward or back could only move
/// its members past each other's neighbours.
#[test]
fn grouping_gathers_the_members_under_the_topmost_one() {
    let (mut engine, cast) = board(&[("A", &[]), ("X", &[]), ("B", &[]), ("Y", &[])]);
    engine.select(vec![id(&cast, "A"), id(&cast, "B")]);

    engine.group_selection();

    assert_eq!(
        stack(&engine, &cast),
        vec!["X", "A", "B", "Y"],
        "gathered under B, the topmost member — not raised to the top of the board"
    );
}

#[test]
fn one_undo_takes_the_grouping_and_the_gathering_away() {
    let (mut engine, cast) = board(&[("A", &[]), ("X", &[]), ("B", &[])]);
    engine.select(vec![id(&cast, "A"), id(&cast, "B")]);
    engine.group_selection();
    assert_eq!(stack(&engine, &cast), vec!["X", "A", "B"], "setup");

    engine.undo();

    assert_eq!(stack(&engine, &cast), vec!["A", "X", "B"]);
    assert!(groups_of(&engine, &id(&cast, "A")).is_empty());
}

// ---------------------------------------------------------------------------
// Z-order steps over groups
// ---------------------------------------------------------------------------

#[test]
fn bring_forward_steps_over_a_whole_group() {
    let (mut engine, cast) = board(&[("S", &[]), ("A", &["g"]), ("B", &["g"])]);
    engine.select(vec![id(&cast, "S")]);

    engine.reorder_selection(ZOrderMode::Forward);

    assert_eq!(
        stack(&engine, &cast),
        vec!["A", "B", "S"],
        "one step past the group, not into the middle of it"
    );
}

#[test]
fn send_backward_steps_under_a_whole_group() {
    let (mut engine, cast) = board(&[("A", &["g"]), ("B", &["g"]), ("S", &[])]);
    engine.select(vec![id(&cast, "S")]);

    engine.reorder_selection(ZOrderMode::Backward);

    assert_eq!(stack(&engine, &cast), vec!["S", "A", "B"]);
}

/// Nested groups: the block stepped over is the outermost group, the one a click takes.
#[test]
fn bring_forward_steps_over_the_outermost_group_of_a_nest() {
    let (mut engine, cast) = board(&[
        ("S", &[]),
        ("A", &["g1", "g2"]),
        ("B", &["g1", "g2"]),
        ("C", &["g2"]),
    ]);
    engine.select(vec![id(&cast, "S")]);

    engine.reorder_selection(ZOrderMode::Forward);

    assert_eq!(stack(&engine, &cast), vec!["A", "B", "C", "S"]);
}

#[test]
fn send_backward_steps_under_the_outermost_group_of_a_nest() {
    let (mut engine, cast) = board(&[
        ("A", &["g1", "g2"]),
        ("B", &["g1", "g2"]),
        ("C", &["g2"]),
        ("S", &[]),
    ]);
    engine.select(vec![id(&cast, "S")]);

    engine.reorder_selection(ZOrderMode::Backward);

    assert_eq!(stack(&engine, &cast), vec!["S", "A", "B", "C"]);
}

#[test]
fn a_group_steps_forward_over_another_group_as_one_block() {
    let (mut engine, cast) = board(&[("A", &["g"]), ("B", &["g"]), ("C", &["h"]), ("D", &["h"])]);
    click(&mut engine, &cast, "A");

    engine.reorder_selection(ZOrderMode::Forward);

    assert_eq!(stack(&engine, &cast), vec!["C", "D", "A", "B"]);
}

#[test]
fn a_group_steps_backward_under_another_group_as_one_block() {
    let (mut engine, cast) = board(&[("A", &["g"]), ("B", &["g"]), ("C", &["h"]), ("D", &["h"])]);
    click(&mut engine, &cast, "C");

    engine.reorder_selection(ZOrderMode::Backward);

    assert_eq!(stack(&engine, &cast), vec!["C", "D", "A", "B"]);
}

/// Inside an entered group, "front" is the front of the group: `shiftElementsToEnd` takes
/// the last member of the edited group as the end, `zindex.ts:489-497`.
#[test]
fn bring_to_front_inside_a_group_stops_at_the_groups_top() {
    let (mut engine, cast) = board(&[("A", &["g"]), ("B", &["g"]), ("X", &[])]);
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");
    assert_eq!(engine.get_selection(), vec![id(&cast, "A")], "setup");

    engine.reorder_selection(ZOrderMode::Front);

    assert_eq!(stack(&engine, &cast), vec!["B", "A", "X"]);
}

#[test]
fn send_to_back_inside_a_group_stops_at_the_groups_bottom() {
    let (mut engine, cast) = board(&[("X", &[]), ("A", &["g"]), ("B", &["g"])]);
    click(&mut engine, &cast, "B");
    step_in(&mut engine, &cast, "B");
    assert_eq!(engine.get_selection(), vec![id(&cast, "B")], "setup");

    engine.reorder_selection(ZOrderMode::Back);

    assert_eq!(stack(&engine, &cast), vec!["X", "B", "A"]);
}

/// Already the top of its group, so a step forward has nowhere to go: inside an entered
/// group only its members are candidates, `zindex.ts:226-230`.
#[test]
fn bring_forward_inside_a_group_does_not_leave_it() {
    let (mut engine, cast) = board(&[("A", &["g"]), ("B", &["g"]), ("X", &[])]);
    click(&mut engine, &cast, "B");
    step_in(&mut engine, &cast, "B");

    engine.reorder_selection(ZOrderMode::Forward);

    assert_eq!(stack(&engine, &cast), vec!["A", "B", "X"]);
}

#[test]
fn send_backward_inside_a_group_does_not_leave_it() {
    let (mut engine, cast) = board(&[("X", &[]), ("A", &["g"]), ("B", &["g"])]);
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");

    engine.reorder_selection(ZOrderMode::Backward);

    assert_eq!(stack(&engine, &cast), vec!["X", "A", "B"]);
}

/// Inside g2, the inner group g1 is a sibling like any other element, and a sibling
/// group is stepped over whole — `zindex.ts:292-308`.
#[test]
fn inside_a_group_an_inner_group_is_stepped_over_whole() {
    let (mut engine, cast) = board(&[("C", &["g2"]), ("A", &["g1", "g2"]), ("B", &["g1", "g2"])]);
    click(&mut engine, &cast, "C");
    step_in(&mut engine, &cast, "C");
    assert_eq!(engine.get_selection(), vec![id(&cast, "C")], "setup");

    engine.reorder_selection(ZOrderMode::Forward);

    assert_eq!(stack(&engine, &cast), vec!["A", "B", "C"]);
}

/// And an inner group held as a block goes to the front of the outer one, not the board.
#[test]
fn inside_a_group_an_inner_group_goes_to_the_front_as_one_block() {
    let (mut engine, cast) = board(&[
        ("A", &["g1", "g2"]),
        ("B", &["g1", "g2"]),
        ("C", &["g2"]),
        ("X", &[]),
    ]);
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");

    engine.reorder_selection(ZOrderMode::Front);

    assert_eq!(stack(&engine, &cast), vec!["C", "A", "B", "X"]);
}

#[test]
fn inside_a_group_an_inner_group_goes_to_the_back_as_one_block() {
    let (mut engine, cast) = board(&[
        ("X", &[]),
        ("C", &["g2"]),
        ("A", &["g1", "g2"]),
        ("B", &["g1", "g2"]),
    ]);
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");

    engine.reorder_selection(ZOrderMode::Back);

    assert_eq!(stack(&engine, &cast), vec!["X", "A", "B", "C"]);
}

/// Selecting something outside the entered group leaves it (`set_selection`), so the
/// selection is not confined to the group: confined, it stopped at the top of the group.
#[test]
fn a_selection_outside_the_entered_group_still_goes_to_the_front() {
    let (mut engine, cast) = board(&[("X", &[]), ("A", &["g"]), ("B", &["g"]), ("Y", &[])]);
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");
    engine.select(vec![id(&cast, "X")]);
    assert_eq!(engine.editing_group_id(), None, "setup");

    engine.reorder_selection(ZOrderMode::Front);

    assert_eq!(stack(&engine, &cast), vec!["A", "B", "Y", "X"]);
}

// ---------------------------------------------------------------------------
// Labels go where their shape goes
// ---------------------------------------------------------------------------

/// R with its label T, then S and X, in that stack order.
fn labelled() -> (DrawEngine, Cast) {
    let mut r = filled(box_at(0.0, 0.0, 80.0, 80.0));
    let mut t = text_at(10.0, 30.0, 60.0, 20.0);
    t.text = Some("hi".into());
    t.container_id = Some(r.id.clone());
    r.bound_text_id = Some(t.id.clone());
    let s = filled(box_at(150.0, 0.0, 80.0, 80.0));
    let x = filled(box_at(300.0, 0.0, 80.0, 80.0));
    let cast = vec![
        ("R", r.id.clone()),
        ("T", t.id.clone()),
        ("S", s.id.clone()),
        ("X", x.id.clone()),
    ];
    let mut engine = engine_with_measure(vec![r, t, s, x]);
    engine.set_tool(DrawTool::Select);
    (engine, cast)
}

/// A shape is selected without its label — clicking the words selects the shape — so the
/// label has to be read in with it, as the oracle's `includeBoundTextElement` does.
#[test]
fn grouping_a_labelled_shape_takes_its_label_into_the_group() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);

    engine.group_selection();

    let r = groups_of(&engine, &id(&cast, "R"));
    assert_eq!(r.len(), 1);
    assert_eq!(groups_of(&engine, &id(&cast, "T")), r);
    assert_eq!(groups_of(&engine, &id(&cast, "S")), r);
}

#[test]
fn grouping_gathers_a_label_with_its_shape() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "X")]);

    engine.group_selection();

    assert_eq!(stack(&engine, &cast), vec!["S", "R", "T", "X"]);
}

/// With the label in its shape's group, a click on the group selects the label too — and
/// a drag must still carry it once, on its shape, not move it a second time.
#[test]
fn dragging_a_grouped_labelled_shape_keeps_the_label_on_it() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.group_selection();
    engine.clear_selection();
    let place = |engine: &DrawEngine, name: &str| {
        let el = engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == id(&cast, name))
            .expect("still there");
        (el.x, el.y)
    };
    let (rx, ry) = place(&engine, "R");
    let (tx, ty) = place(&engine, "T");

    engine.begin_pointer(190.0, 40.0, false, false);
    assert!(
        engine.get_selection().contains(&id(&cast, "T")),
        "setup: the label is held"
    );
    engine.move_pointer(190.0, 90.0, false, false);
    engine.move_pointer(190.0, 140.0, false, false);
    engine.end_pointer();

    let (rx2, ry2) = place(&engine, "R");
    let (tx2, ty2) = place(&engine, "T");
    assert!(ry2 > ry + 50.0, "the group moved");
    assert_close(tx2 - rx2, tx - rx);
    assert_close(ty2 - ry2, ty - ry);
}

/// Selecting a shape does not select its label, so the front of the stack is where the
/// words go too — otherwise the shape, brought over them, hides them.
#[test]
fn bring_to_front_takes_a_label_with_its_shape() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R")]);

    engine.reorder_selection(ZOrderMode::Front);

    assert_eq!(stack(&engine, &cast), vec!["S", "X", "R", "T"]);
}

/// A shape and its label are one unit to step over, or the step lands between the two.
#[test]
fn stepping_past_a_labelled_shape_steps_past_its_label_too() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "S")]);

    engine.reorder_selection(ZOrderMode::Backward);

    assert_eq!(stack(&engine, &cast), vec!["S", "R", "T", "X"]);
}

/// The group already holds the label, so selecting its shapes without it is still
/// exactly that group, and grouping again must not wrap a second level around it.
#[test]
fn regrouping_a_labelled_group_is_still_a_no_op() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.group_selection();
    let first = groups_of(&engine, &id(&cast, "R"));

    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.group_selection();

    assert_eq!(groups_of(&engine, &id(&cast, "R")), first);
    assert_eq!(groups_of(&engine, &id(&cast, "T")), first);
}

#[test]
fn ungrouping_keeps_a_label_level_with_its_shape() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.group_selection();
    engine.select(vec![id(&cast, "R"), id(&cast, "S"), id(&cast, "X")]);
    engine.group_selection();
    assert_eq!(groups_of(&engine, &id(&cast, "R")).len(), 2, "setup");

    engine.select(vec![id(&cast, "R"), id(&cast, "S"), id(&cast, "X")]);
    engine.ungroup_selection();

    let r = groups_of(&engine, &id(&cast, "R"));
    assert_eq!(r.len(), 1, "the outer level is gone, the inner one stays");
    assert_eq!(groups_of(&engine, &id(&cast, "T")), r);
}

#[test]
fn a_copy_of_a_grouped_labelled_shape_keeps_its_label_in_the_copied_group() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.group_selection();
    let original = groups_of(&engine, &id(&cast, "R"));

    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.duplicate_selection(0.0, 200.0);

    let copies = copies(&engine, &cast);
    assert_eq!(copies.len(), 3, "R, its label, and S");
    let label = copies
        .iter()
        .find(|el| el.container_id.is_some())
        .expect("the label is copied");
    let shape = copies
        .iter()
        .find(|el| el.bound_text_id.is_some())
        .expect("the shape is copied");
    assert_eq!(label.group_ids, shape.group_ids);
    assert_eq!(shape.group_ids.len(), 1);
    assert_ne!(shape.group_ids, original, "a fresh group for the copy");
}

/// Inside the group, a copy of the labelled shape and its label both stay in it.
#[test]
fn a_labelled_shape_copied_inside_its_group_stays_there_with_its_label() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.group_selection();
    let group = groups_of(&engine, &id(&cast, "R"));
    engine.clear_selection();
    engine.begin_pointer(150.0 + 40.0, 40.0, false, false);
    engine.end_pointer();
    engine.handle_double_click(150.0 + 40.0, 40.0);
    engine.begin_pointer(40.0, 10.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_selection(), vec![id(&cast, "R")], "setup");

    engine.duplicate_selection(0.0, 200.0);

    let copies = copies(&engine, &cast);
    assert_eq!(copies.len(), 2, "R and its label");
    for copy in &copies {
        assert_eq!(copy.group_ids, group);
    }
}

// ---------------------------------------------------------------------------
// Found in review
// ---------------------------------------------------------------------------

/// Select All leaves the group (`actionSelectAll.ts:49`), so a copy of the board made
/// from inside it is a copy of the board, not new members of the group.
#[test]
fn a_copy_of_everything_made_from_inside_a_group_does_not_join_it() {
    let (mut engine, cast) = board(&[("A", &["g"]), ("B", &["g"]), ("X", &[])]);
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");

    engine.select_all();
    engine.duplicate_selection(0.0, 200.0);

    for copy in copies(&engine, &cast) {
        assert!(
            !copy.group_ids.iter().any(|g| g == "g"),
            "{:?}",
            copy.group_ids
        );
    }
}

/// The copy keeps the edited group, so it goes where the group is — added on top of the
/// board, the group was split in the stack with X painted between its members.
#[test]
fn a_copy_made_inside_a_group_lands_in_its_run_of_the_stack() {
    let (mut engine, cast) = board(&[("A", &["g"]), ("B", &["g"]), ("X", &[])]);
    click(&mut engine, &cast, "A");
    step_in(&mut engine, &cast, "A");

    engine.duplicate_selection(0.0, 200.0);

    assert_eq!(stack(&engine, &cast), vec!["A", "B", "copy", "X"]);
}

/// Delete S from a group of R and S, and R with its label is what is left: still a group
/// to the oracle (`allElementsInSameGroup`, `actionGroup.tsx:73-83`). Counted as one
/// shape, each Ctrl+G wrapped it in another level.
#[test]
fn a_labelled_shape_left_alone_in_its_group_is_still_that_group() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);
    engine.group_selection();
    engine.select(vec![id(&cast, "S")]);
    engine.delete_selection();
    click(&mut engine, &cast, "R");
    assert_eq!(engine.get_selection().len(), 2, "setup: R and its label");

    engine.group_selection();
    engine.group_selection();
    assert_eq!(groups_of(&engine, &id(&cast, "R")).len(), 1);

    engine.toggle_group_selection();
    assert!(groups_of(&engine, &id(&cast, "R")).is_empty());
    assert!(groups_of(&engine, &id(&cast, "T")).is_empty());
}

/// A shape and its own words are one thing, not two to group (`enableActionGroup` reads
/// the selection without labels, `actionGroup.tsx:73-83`).
#[test]
fn a_shape_and_its_label_alone_are_not_grouped() {
    let (mut engine, cast) = labelled();
    engine.select(vec![id(&cast, "R"), id(&cast, "T")]);

    engine.group_selection();

    assert!(groups_of(&engine, &id(&cast, "R")).is_empty());
}

/// A new label is made in its shape's groups and directly above it, as the oracle makes
/// one (`App.tsx:7081`, `:7103-7108`).
#[test]
fn a_new_label_joins_its_shape_in_its_group_and_in_the_stack() {
    let (mut engine, cast) = board(&[("R", &["g"]), ("S", &["g"]), ("X", &[])]);
    click(&mut engine, &cast, "R");
    step_in(&mut engine, &cast, "R");
    assert_eq!(engine.get_selection(), vec![id(&cast, "R")], "setup");

    assert!(engine.edit_selected_text());

    let label = copies(&engine, &cast)
        .into_iter()
        .find(|el| el.container_id.as_deref() == Some(id(&cast, "R").as_str()))
        .expect("a label");
    assert_eq!(label.group_ids, vec!["g"]);
    assert_eq!(stack(&engine, &cast), vec!["R", "copy", "S", "X"]);
}

/// Someone may be typing into the label: grouping its shape leaves it to them, where
/// their next commit would have stamped above it and taken it back out anyway.
#[test]
fn grouping_leaves_a_label_a_peer_holds() {
    let (mut engine, cast) = labelled();
    engine.set_peers(vec![Peer {
        id: "ana".into(),
        name: "Ana".into(),
        color: "#e03131".into(),
        holds: [id(&cast, "T")].into_iter().collect(),
        preview: Vec::new(),
    }]);
    engine.select(vec![id(&cast, "R"), id(&cast, "S")]);

    engine.group_selection();

    assert_eq!(groups_of(&engine, &id(&cast, "R")).len(), 1, "grouped");
    assert!(groups_of(&engine, &id(&cast, "T")).is_empty());
}

/// A label saved before labels joined groups lies outside its shape's group. Front
/// inside the group stopped at the group's top, between that shape and its words.
#[test]
fn front_inside_a_group_does_not_come_between_a_shape_and_its_label() {
    let mut s = filled(box_at(0.0, 0.0, 80.0, 80.0));
    let mut r = filled(box_at(150.0, 0.0, 80.0, 80.0));
    let mut t = text_at(160.0, 30.0, 60.0, 20.0);
    let x = filled(box_at(300.0, 0.0, 80.0, 80.0));
    t.text = Some("hi".into());
    t.container_id = Some(r.id.clone());
    r.bound_text_id = Some(t.id.clone());
    s.group_ids = vec!["g".into()];
    r.group_ids = vec!["g".into()];
    let cast: Cast = vec![
        ("S", s.id.clone()),
        ("R", r.id.clone()),
        ("T", t.id.clone()),
        ("X", x.id.clone()),
    ];
    let mut engine = engine_with_measure(vec![s, r, t, x]);
    engine.set_tool(DrawTool::Select);
    engine.begin_pointer(40.0, 40.0, false, false);
    engine.end_pointer();
    engine.handle_double_click(40.0, 40.0);
    assert_eq!(engine.get_selection(), vec![id(&cast, "S")], "setup");

    engine.reorder_selection(ZOrderMode::Front);

    assert_eq!(stack(&engine, &cast), vec!["R", "T", "S", "X"]);
}
