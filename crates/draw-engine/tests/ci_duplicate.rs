//! Ctrl+D: where a copy lands in the stack.
//!
//! The oracle puts each copy directly above its original, a group or a frame's children
//! moving with it as the run they already are (`duplicateElements`,
//! `duplicate.ts@1118751f:322-436`). Not the z-order commands (Bring forward and kin,
//! `ci_zorder.rs`), and not the group/frame *membership* rules `duplicate_selection`
//! leaves alone — only where the copies land.

mod common;
use common::*;
use draw_engine::*;

fn with_id(mut element: DrawElement, id: &str) -> DrawElement {
    element.id = id.to_string();
    element
}

fn engine_of(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_scene(Scene::new(elements));
    engine
}

/// Live ids, bottom first — a copy's minted id is unpredictable, so tests read the stack
/// by shape rather than asserting an id.
fn stack(engine: &DrawEngine) -> Vec<String> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| !el.is_deleted)
        .map(|el| el.id)
        .collect()
}

/// A plain, ungrouped Ctrl+D: the copy goes directly above its source, not on top of the
/// board (`duplicate.ts@1118751f:426-431`; the wave-3 gap in `followups.md`, "U").
#[test]
fn a_plain_duplicate_lands_directly_above_its_source() {
    let mut engine = engine_of(vec![
        with_id(box_at(0.0, 0.0, 10.0, 10.0), "a"),
        with_id(box_at(0.0, 0.0, 10.0, 10.0), "b"),
        with_id(box_at(0.0, 0.0, 10.0, 10.0), "c"),
    ]);
    engine.select(vec!["b".into()]);

    engine.duplicate_selection(2.0, 2.0);

    let stack = stack(&engine);
    assert_eq!(stack.len(), 4);
    assert_eq!(stack[0], "a");
    assert_eq!(stack[1], "b");
    assert_eq!(stack[3], "c");
    // stack[2] is the copy: above its source, below what was above it.
    assert_ne!(stack[2], "a");
    assert_ne!(stack[2], "b");
    assert_ne!(stack[2], "c");
    assert_eq!(engine.get_selection(), vec![stack[2].clone()]);
}

/// Duplicating several independent elements together: each copy stays directly above its
/// own source, none of it collapsing onto one board-wide top.
#[test]
fn independent_elements_duplicated_together_each_land_above_their_own_source() {
    let mut engine = engine_of(vec![
        with_id(box_at(0.0, 0.0, 10.0, 10.0), "a"),
        with_id(box_at(0.0, 0.0, 10.0, 10.0), "b"),
        with_id(box_at(0.0, 0.0, 10.0, 10.0), "c"),
    ]);
    engine.select(vec!["a".into(), "c".into()]);

    engine.duplicate_selection(2.0, 2.0);

    let stack = stack(&engine);
    assert_eq!(stack.len(), 5);
    let at = |name: &str| stack.iter().position(|id| id == name).unwrap();
    // a, its copy, b, c, its copy — a's copy never floats above b or c.
    assert_eq!(at("a") + 1, at("b") - 1, "a's copy sits directly above a");
    assert!(at("b") < at("c"), "b keeps its place under c");
    assert_eq!(at("c"), stack.len() - 2, "c's copy is the very top");
}

/// A whole group duplicated lands as one consecutive block directly above the group's own
/// top member, not split across the board (`duplicate.ts@1118751f:333-348`).
#[test]
fn a_duplicated_group_lands_as_one_block_above_the_group() {
    // Different starting x so a copy can be told from the other by its offset position.
    let mut a = with_id(box_at(0.0, 0.0, 10.0, 10.0), "a");
    a.group_ids = vec!["g".into()];
    let mut b = with_id(box_at(50.0, 0.0, 10.0, 10.0), "b");
    b.group_ids = vec!["g".into()];
    let mut engine = engine_of(vec![a, with_id(box_at(0.0, 0.0, 10.0, 10.0), "x"), b]);
    engine.select(vec!["a".into(), "b".into()]);

    engine.duplicate_selection(2.0, 2.0);

    let stack = stack(&engine);
    assert_eq!(stack.len(), 5);
    assert_eq!(&stack[0..3], &["a", "x", "b"], "the group is untouched");
    let x_of = |id: &str| {
        engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == *id)
            .unwrap()
            .x
    };
    // The two copies are the top two, in the same relative order as their sources.
    assert_eq!(x_of(&stack[3]), 2.0, "a's copy comes first, as a did");
    assert_eq!(x_of(&stack[4]), 52.0, "then b's copy");
}

/// A frame duplicated together with its children keeps them in one run directly above the
/// frame — the same run `stack_under_frame` keeps a frame's live children in
/// (`duplicate.ts@1118751f:364-379`).
#[test]
fn a_frame_and_its_duplicated_children_stay_in_one_run_above_it() {
    let mut child = with_id(box_at(20.0, 20.0, 10.0, 10.0), "child");
    child.frame_id = Some("frame".into());
    let mut frame = with_id(box_at(0.0, 0.0, 100.0, 100.0), "frame");
    frame.kind = DrawElementType::Frame;
    let mut engine = engine_of(vec![child, frame]);
    engine.select(vec!["child".into(), "frame".into()]);

    engine.duplicate_selection(2.0, 2.0);

    let stack = stack(&engine);
    assert_eq!(stack.len(), 4);
    assert_eq!(&stack[0..2], &["child", "frame"], "the frame is untouched");
    // Both copies sit above the untouched pair, in the run's own order: the child's first.
    let x_of = |id: &str| {
        engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == *id)
            .unwrap()
            .x
    };
    assert_eq!(
        x_of(&stack[2]),
        22.0,
        "the child's copy comes first, as the child did"
    );
    assert_eq!(x_of(&stack[3]), 2.0, "then the frame's copy");
}

/// A frame's child duplicated on its own, without the frame, is a run of one: the frame
/// branch only fires when the frame itself is also being duplicated
/// (`duplicate.ts@1118751f:349-352`).
#[test]
fn a_lone_frame_child_duplicated_without_its_frame_lands_above_itself() {
    let mut child = with_id(box_at(20.0, 20.0, 10.0, 10.0), "child");
    child.frame_id = Some("frame".into());
    let mut frame = with_id(box_at(0.0, 0.0, 100.0, 100.0), "frame");
    frame.kind = DrawElementType::Frame;
    let mut engine = engine_of(vec![child, frame]);
    engine.select(vec!["child".into()]);

    engine.duplicate_selection(2.0, 2.0);

    let stack = stack(&engine);
    assert_eq!(stack.len(), 3);
    assert_eq!(stack[0], "child");
    assert_eq!(stack[2], "frame", "the frame keeps its place, untouched");
    assert_ne!(stack[1], "frame");
}

/// A container duplicated with its label keeps the pair together, directly above the
/// label — the label sits above its container, so that is where the run lands
/// (`duplicate.ts@1118751f:381-397`).
#[test]
fn a_container_and_its_label_stay_paired_when_duplicated() {
    let mut container = with_id(box_at(0.0, 0.0, 40.0, 20.0), "box");
    container.bound_text_id = Some("label".into());
    let mut label = with_id(text_at(0.0, 0.0, 40.0, 20.0), "label");
    label.container_id = Some("box".into());
    let mut engine = engine_of(vec![container, label]);
    // Selecting the container alone: `with_labels` pulls its label along, as a click on a
    // shape carries its label into the selection too.
    engine.select(vec!["box".into()]);

    engine.duplicate_selection(2.0, 2.0);

    let stack = stack(&engine);
    assert_eq!(stack.len(), 4);
    assert_eq!(&stack[0..2], &["box", "label"], "the pair is untouched");
    assert!(stack[2] != "box" && stack[2] != "label");
    assert!(stack[3] != "box" && stack[3] != "label");
}
