//! Groups against the two things that also decide what moves: locks and frames.
//!
//! A group is one thing. The oracle holds to that at every border a group can meet:
//!
//! - **a lock** stops an element being picked up on its own, but not being carried by the
//!   group it belongs to — the group's members are selected with no lock filter
//!   (`packages/element/src/groups.ts:94-132`) and the drag refuses only when *every*
//!   selected element is locked (`packages/excalidraw/components/App.tsx:10899-10904`);
//! - **a frame** takes a group in or lets it go whole — `omitPartialGroups`
//!   (`packages/element/src/frame.ts:395-434`) and the group walk of `isElementInFrame`
//!   (`frame.ts:857-906`) — so a group never ends up half in a frame;
//! - **deleting a frame** deletes the frame, not the work in it: the children stay, lose
//!   their `frameId`, and are selected (`actionDeleteSelected.tsx:57-73,115-122`, pinned
//!   by `actionDeleteSelected.test.tsx:11`).
//!
//! Every scene here starts from the same probe, `nested_plus_h`: five filled 80×80 boxes
//! in a row, A B C grouped with A+B nested inside, and D+E a group of their own.
//!
//! ```text
//!   A(0)  B(150)  C(300)        D(600)  E(750)
//!   [[A B]  C]                  [D E]
//! ```

mod common;
use common::*;
use draw_engine::*;

struct Probe {
    engine: DrawEngine,
    a: String,
    b: String,
    c: String,
    d: String,
    e: String,
}

/// Filled, so the middle of each box is a hit target: a transparent shape is hit on its
/// outline only, and every press here would otherwise be a test of the fill rule.
fn nested_plus_h() -> Probe {
    let boxes: Vec<DrawElement> = [0.0, 150.0, 300.0, 600.0, 750.0]
        .into_iter()
        .map(|x| filled(box_at(x, 0.0, 80.0, 80.0)))
        .collect();
    let [a, b, c, d, e] = std::array::from_fn(|i| boxes[i].id.clone());
    let mut engine = engine_with_scene(boxes);
    engine.set_tool(DrawTool::Select);
    for group in [vec![&a, &b], vec![&a, &b, &c], vec![&d, &e]] {
        engine.select(group.into_iter().cloned().collect());
        engine.group_selection();
    }
    engine.clear_selection();
    Probe {
        engine,
        a,
        b,
        c,
        d,
        e,
    }
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

fn frame_of(engine: &DrawEngine, id: &str) -> Option<String> {
    element(engine, id).frame_id
}

/// Locks exactly these elements, through the same command the context menu runs.
fn lock(engine: &mut DrawEngine, ids: &[&String]) {
    engine.select(ids.iter().map(|id| (*id).clone()).collect());
    engine.toggle_lock_selection();
    engine.clear_selection();
}

fn click(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.begin_pointer(x, y, false, false);
    engine.end_pointer();
}

/// A press, a few moves and a release: one gesture, so one history entry.
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

/// A frame dragged out corner to corner with the frame tool.
fn draw_frame(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) -> String {
    engine.set_tool(DrawTool::Frame);
    drag(engine, from, to);
    engine.set_tool(DrawTool::Select);
    engine
        .get_scene()
        .into_iter()
        .rev()
        .find(|el| is_frame(el) && !el.is_deleted)
        .map(|el| el.id)
        .expect("no frame was drawn")
}

fn selection(engine: &DrawEngine) -> std::collections::HashSet<String> {
    engine.get_selection().into_iter().collect()
}

// ---------------------------------------------------------------------------
// A locked member travels with its group
// ---------------------------------------------------------------------------

/// The probe as reported: dragging the group left B behind at y=0 while A went to y=200.
#[test]
fn a_locked_member_moves_with_its_group() {
    let Probe {
        mut engine,
        a,
        b,
        c,
        d,
        ..
    } = nested_plus_h();
    lock(&mut engine, &[&b]);

    drag(&mut engine, (40.0, 40.0), (40.0, 240.0));

    for id in [&a, &b, &c] {
        assert_close(element(&engine, id).y, 200.0);
    }
    assert_close(element(&engine, &d).y, 0.0);
}

/// Resize and rotate read the same frame the move does. With the locked member left out
/// of it the handles were computed around A and B alone, so the handle the painter draws
/// at the far corner of C was not there to be grabbed — and C stayed unscaled.
#[test]
fn a_locked_member_is_scaled_with_its_group() {
    let Probe {
        mut engine, a, c, ..
    } = nested_plus_h();
    lock(&mut engine, &[&c]);
    click(&mut engine, 40.0, 40.0);

    // The south-east handle's centre sits 8px out from the union's corner (380, 80);
    // dragged to (760, 160) it doubles the group about its north-west corner.
    engine.begin_pointer(388.0, 88.0, false, false);
    for step in 1..=4 {
        let t = f64::from(step) / 4.0;
        engine.move_pointer(380.0 + 380.0 * t, 80.0 + 80.0 * t, false, false);
    }
    engine.end_pointer();

    let c = element(&engine, &c);
    assert_close(c.x, 600.0);
    assert_close(c.width, 160.0);
    assert_close(element(&engine, &a).width, 160.0);
}

/// The keyboard is the other way a selection moves, and it left the same member behind.
/// The oracle's arrow keys move every selected element (`App.tsx:5770-5830`).
#[test]
fn nudging_a_group_carries_its_locked_member() {
    let Probe {
        mut engine, a, b, ..
    } = nested_plus_h();
    lock(&mut engine, &[&b]);
    click(&mut engine, 40.0, 40.0);

    engine.nudge_selection(0.0, 10.0);

    assert_close(element(&engine, &a).y, 10.0);
    assert_close(element(&engine, &b).y, 10.0);
}

/// Only membership carries a locked element. Pressed on directly it is not there to be
/// hit — the oracle nulls a hit on a locked element (`App.tsx:9510-9517`) — so the press
/// starts a marquee and nothing moves.
#[test]
fn a_locked_member_is_not_a_handle_on_its_group() {
    let Probe {
        mut engine,
        a,
        b,
        c,
        ..
    } = nested_plus_h();
    lock(&mut engine, &[&b]);

    drag(&mut engine, (190.0, 40.0), (190.0, 240.0));

    for id in [&a, &b, &c] {
        assert_close(element(&engine, id).y, 0.0);
    }
    assert!(selection(&engine).is_empty(), "{:?}", selection(&engine));
}

/// A group every member of which is locked is refused whole, as the oracle refuses a
/// selection that is all locked.
///
/// Select All is the one door that lets such a group into the selection here — it takes
/// locked elements so they can be unlocked from the menu, where the oracle's skips them
/// (`actionSelectAll.ts:32-38`) — and dragging the rest of that selection must not pick
/// it up with them.
#[test]
fn a_group_that_is_locked_throughout_stays_put() {
    let Probe {
        mut engine,
        a,
        d,
        e,
        ..
    } = nested_plus_h();
    lock(&mut engine, &[&d, &e]);

    click(&mut engine, 640.0, 40.0);
    assert!(
        selection(&engine).is_empty(),
        "a locked group cannot be clicked"
    );

    engine.select_all();
    drag(&mut engine, (40.0, 40.0), (40.0, 240.0));

    assert_close(element(&engine, &a).y, 200.0);
    assert_close(element(&engine, &d).y, 0.0);
    assert_close(element(&engine, &e).y, 0.0);
}

/// A frame carries what it holds the way a group does: `dragSelectedElements` adds every
/// child of a dragged frame with no lock filter (`packages/element/src/dragElements.ts:
/// 75-84`). Left behind, a locked child ends up outside the frame that still claims it.
#[test]
fn a_locked_child_moves_with_its_frame() {
    let Probe {
        mut engine, a, b, ..
    } = nested_plus_h();
    let frame = draw_frame(&mut engine, (-20.0, -20.0), (400.0, 100.0));
    lock(&mut engine, &[&b]);

    engine.select(vec![frame.clone()]);
    // By its top edge: a frame is hollow, and its middle belongs to what it holds.
    drag(&mut engine, (100.0, -20.0), (100.0, 280.0));

    assert_close(element(&engine, &frame).y, 280.0);
    assert_close(element(&engine, &a).y, 300.0);
    assert_close(element(&engine, &b).y, 300.0);
    assert_eq!(frame_of(&engine, &b).as_deref(), Some(frame.as_str()));
}

/// The arrow keys move what a drag moves. The oracle nudges with
/// `includeElementsInFrames: true` (`App.tsx:5770-5775`); here the frame moved alone,
/// out from under everything it still claimed.
#[test]
fn nudging_a_frame_carries_what_it_holds() {
    let Probe { mut engine, a, .. } = nested_plus_h();
    let frame = draw_frame(&mut engine, (-20.0, -20.0), (400.0, 100.0));
    engine.select(vec![frame.clone()]);

    engine.nudge_selection(10.0, 0.0);

    assert_close(element(&engine, &frame).x, -10.0);
    assert_close(element(&engine, &a).x, 10.0);
}

// ---------------------------------------------------------------------------
// A group joins or leaves a frame whole
// ---------------------------------------------------------------------------

/// Drawn over A alone, a frame took A out of the middle of its group. The oracle omits a
/// group that is not wholly inside a new frame (`omitPartialGroups`, `frame.ts:395-434`).
#[test]
fn a_frame_drawn_over_part_of_a_group_takes_none_of_it() {
    let Probe {
        mut engine,
        a,
        b,
        c,
        ..
    } = nested_plus_h();

    draw_frame(&mut engine, (-20.0, -20.0), (100.0, 100.0));

    for id in [&a, &b, &c] {
        assert_eq!(frame_of(&engine, id), None, "the group was split");
    }
}

#[test]
fn a_frame_drawn_over_a_whole_group_takes_all_of_it() {
    let Probe {
        mut engine,
        a,
        b,
        c,
        d,
        e,
    } = nested_plus_h();

    let frame = draw_frame(&mut engine, (-20.0, -20.0), (400.0, 100.0));

    for id in [&a, &b, &c] {
        assert_eq!(frame_of(&engine, id).as_deref(), Some(frame.as_str()));
    }
    assert_eq!(frame_of(&engine, &d), None);
    assert_eq!(frame_of(&engine, &e), None);
}

/// Dragged so that it straddles the frame's edge, the group was split: A and B in, C
/// out. Whole or nothing — and it is judged as one element would be, by containment
/// (`ci_frame.rs` › `an_element_half_out_of_a_frame_does_not_belong_to_it`), so a group
/// with a member sticking out is out.
#[test]
fn a_group_dragged_across_a_frame_edge_does_not_split() {
    let Probe {
        mut engine,
        a,
        b,
        c,
        ..
    } = nested_plus_h();
    let frame = draw_frame(&mut engine, (-20.0, 300.0), (420.0, 500.0));

    drag(&mut engine, (40.0, 40.0), (40.0, 390.0));
    for id in [&a, &b, &c] {
        assert_eq!(
            frame_of(&engine, id).as_deref(),
            Some(frame.as_str()),
            "the whole group was dropped inside the frame"
        );
    }

    // C now pokes 60 past the frame's right edge; A and B are still inside.
    drag(&mut engine, (40.0, 390.0), (140.0, 390.0));
    for id in [&a, &b, &c] {
        assert_eq!(frame_of(&engine, id), None, "the group was split");
    }
}

/// A frame holds no frame, so it holds no group with a frame in it either — not even the
/// part lying inside it (`omitGroupsContainingFrameLikes`, `frame.ts:747-788`).
#[test]
fn a_group_holding_a_frame_is_held_by_no_frame() {
    let outer = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        },
    );
    let mut inner = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 150.0,
        },
    );
    let mut shape = box_at(400.0, 100.0, 40.0, 40.0);
    inner.group_ids = vec!["g".to_string()];
    shape.group_ids = vec!["g".to_string()];

    assert!(elements_captured_by([&inner, &shape].into_iter(), &outer).is_empty());
}

/// Grouping an element inside a frame with one outside takes the first out of the frame,
/// as `actionGroup.tsx:138-150` does, in the same step — so one undo puts it back.
#[test]
fn grouping_across_a_frame_edge_takes_the_group_out_whole() {
    let inside = filled(box_at(40.0, 340.0, 80.0, 80.0));
    let outside = filled(box_at(300.0, 340.0, 80.0, 80.0));
    let (inside_id, outside_id) = (inside.id.clone(), outside.id.clone());
    let mut engine = engine_with_scene(vec![inside, outside]);
    let frame = draw_frame(&mut engine, (0.0, 300.0), (200.0, 500.0));
    assert_eq!(
        frame_of(&engine, &inside_id).as_deref(),
        Some(frame.as_str())
    );

    engine.select(vec![inside_id.clone(), outside_id.clone()]);
    engine.group_selection();

    assert_eq!(frame_of(&engine, &inside_id), None);
    assert_eq!(frame_of(&engine, &outside_id), None);

    engine.undo();
    assert!(element(&engine, &inside_id).group_ids.is_empty());
    assert_eq!(
        frame_of(&engine, &inside_id).as_deref(),
        Some(frame.as_str())
    );
}

// ---------------------------------------------------------------------------
// Deleting a frame keeps what it held
// ---------------------------------------------------------------------------

/// The frame goes; the work in it stays, out of any frame, and selected — so the next
/// Delete takes the contents too, if that is what was meant.
#[test]
fn deleting_a_frame_keeps_its_children_and_selects_them() {
    let Probe {
        mut engine,
        a,
        b,
        c,
        d,
        e,
    } = nested_plus_h();
    let frame = draw_frame(&mut engine, (-20.0, -20.0), (400.0, 100.0));

    engine.select(vec![frame.clone()]);
    engine.delete_selection();

    assert!(element(&engine, &frame).is_deleted, "the frame survived");
    for id in [&a, &b, &c] {
        let child = element(&engine, id);
        assert!(!child.is_deleted, "a child went with its frame");
        assert_eq!(child.frame_id, None, "a child still names a deleted frame");
    }
    assert_eq!(
        selection(&engine),
        [a, b, c].into_iter().collect(),
        "the children are what is selected now"
    );
    assert!(!element(&engine, &d).is_deleted && !element(&engine, &e).is_deleted);
}

/// `actionDeleteSelected.test.tsx` › "frame + children selected": a child selected along
/// with its frame is kept too — deleting the frame is taken to mean the frame.
#[test]
fn a_child_selected_with_its_frame_is_kept() {
    let child = filled(box_at(60.0, 60.0, 80.0, 60.0));
    let child_id = child.id.clone();
    let mut engine = engine_with_scene(vec![child]);
    let frame = draw_frame(&mut engine, (20.0, 20.0), (400.0, 300.0));

    engine.select(vec![frame.clone(), child_id.clone()]);
    engine.delete_selection();

    assert!(element(&engine, &frame).is_deleted);
    assert!(!element(&engine, &child_id).is_deleted);
    assert_eq!(engine.get_selection(), vec![child_id]);
}

#[test]
fn undo_brings_the_frame_back_with_its_children_in_it() {
    let Probe {
        mut engine, a, c, ..
    } = nested_plus_h();
    let frame = draw_frame(&mut engine, (-20.0, -20.0), (400.0, 100.0));
    engine.select(vec![frame.clone()]);
    engine.delete_selection();

    engine.undo();

    assert!(!element(&engine, &frame).is_deleted, "undo lost the frame");
    for id in [&a, &c] {
        assert_eq!(
            frame_of(&engine, id).as_deref(),
            Some(frame.as_str()),
            "undo brought the frame back empty"
        );
    }
}

// ---------------------------------------------------------------------------
// Found in review
// ---------------------------------------------------------------------------

/// Outside a drag the oracle's align, distribute and flip leave `frameId` alone —
/// `isElementInFrame` is true unless the selection is being dragged
/// (`packages/element/src/frame.ts:845-855`).
#[test]
fn aligning_a_child_out_of_its_frame_keeps_it_in_the_frame() {
    let child = filled(box_at(60.0, 60.0, 80.0, 80.0));
    let far = filled(box_at(600.0, 60.0, 80.0, 80.0));
    let (child_id, far_id) = (child.id.clone(), far.id.clone());
    let mut engine = engine_with_scene(vec![child, far]);
    let frame = draw_frame(&mut engine, (20.0, 20.0), (300.0, 300.0));
    assert_eq!(
        frame_of(&engine, &child_id).as_deref(),
        Some(frame.as_str()),
        "setup"
    );

    engine.select(vec![child_id.clone(), far_id]);
    engine.align_selection(AlignMode::Right);

    assert!(element(&engine, &child_id).x > 300.0, "setup: aligned out");
    assert_eq!(
        frame_of(&engine, &child_id).as_deref(),
        Some(frame.as_str())
    );
}

/// A lock touches what is locked and nothing else: a pass over the whole board rewrote
/// and re-stamped an element straddling a frame's edge, as a board from Excalidraw can
/// hold one, and sent it to every peer.
#[test]
fn locking_one_element_touches_no_other() {
    let straddler = filled(box_at(250.0, 60.0, 100.0, 80.0));
    let bystander = filled(box_at(600.0, 60.0, 80.0, 80.0));
    let (straddler_id, bystander_id) = (straddler.id.clone(), bystander.id.clone());
    let mut drawn = engine_with_scene(vec![straddler, bystander]);
    let frame = draw_frame(&mut drawn, (20.0, 20.0), (300.0, 300.0));
    let mut scene = drawn.get_scene();
    for el in &mut scene {
        if el.id == straddler_id {
            el.frame_id = Some(frame.clone());
        }
    }
    let mut engine = engine_with_scene(scene);
    let before = element(&engine, &straddler_id);

    lock(&mut engine, &[&bystander_id]);

    let after = element(&engine, &straddler_id);
    assert_eq!(after.frame_id, before.frame_id);
    assert_eq!(after.version, before.version, "re-stamped");
}

/// What a peer holds is untouchable, frame or no frame: moved anyway, both sides stamped
/// the same version and the nonce decided whose move survived.
#[test]
fn moving_a_frame_leaves_a_child_a_peer_holds() {
    let child = filled(box_at(60.0, 60.0, 80.0, 80.0));
    let child_id = child.id.clone();
    let mut engine = engine_with_scene(vec![child]);
    let frame = draw_frame(&mut engine, (20.0, 20.0), (300.0, 300.0));
    engine.set_peers(vec![Peer {
        id: "ana".into(),
        name: "Ana".into(),
        color: "#e03131".into(),
        holds: [child_id.clone()].into_iter().collect(),
        preview: Vec::new(),
    }]);
    engine.select(vec![frame.clone()]);

    engine.nudge_selection(10.0, 0.0);
    drag(&mut engine, (150.0, 20.0), (150.0, 120.0));

    assert_close(element(&engine, &frame).x, 30.0);
    assert_close(element(&engine, &frame).y, 120.0);
    assert_close(element(&engine, &child_id).x, 60.0);
    assert_close(element(&engine, &child_id).y, 60.0);
}

/// Select All holds a loose locked element so it can be unlocked, but nothing transforms
/// it — so the box and its handles are drawn around what does, where a press finds them.
#[test]
fn the_selection_box_is_drawn_where_its_handles_are_hit() {
    let a = filled(box_at(0.0, 0.0, 80.0, 80.0));
    let b = filled(box_at(150.0, 0.0, 80.0, 80.0));
    let mut locked = filled(box_at(600.0, 0.0, 80.0, 80.0));
    locked.locked = Some(true);
    let mut engine = engine_with_scene(vec![a, b, locked]);
    engine.select_all();
    assert_eq!(engine.get_selection().len(), 3, "setup");

    let drawn = engine.paint_view().group_box.expect("a box around A and B");

    assert_close(drawn.min_x, 0.0);
    assert_close(drawn.max_x, 230.0);
}
