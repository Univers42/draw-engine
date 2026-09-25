//! Align and distribute act on **units**, not on elements.
//!
//! A unit is what a click at the current level would select: a group, taken whole, or an
//! element on its own. Each unit moves by one translation, so a group keeps its shape
//! (`alignElements`, `packages/element/src/align.ts@1118751f:19-50`, over
//! `getSelectedElementsByGroup`, `packages/element/src/groups.ts@1118751f:417-466`). A selection
//! that is exactly one group is cut one level further in, so its inner units line up
//! (`isSingleSelectedGroupCase`, `groups.ts@1118751f:424-426`).
//!
//! Distribute spaces the units with **equal gaps** between them, and only when they
//! overlap too much for that does it fall back to spacing their centres between the two
//! units that reach the selection's ends (`distributeElements`,
//! `packages/element/src/distribute.ts@1118751f:19-114`).
//!
//! Both are refused while a frame is selected, as the oracle hides them
//! (`actionAlign.tsx@1118751f:39-52`, `actionDistribute.tsx@1118751f:35-46`): moving a frame
//! without its children, or its children out from under it, is not what either action means.

mod common;
use common::*;
use draw_engine::*;

fn x_of(engine: &DrawEngine, id: &str) -> f64 {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
        .x
}

fn y_of(engine: &DrawEngine, id: &str) -> f64 {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
        .y
}

fn group(engine: &mut DrawEngine, ids: &[&String]) {
    engine.select(ids.iter().map(|id| (*id).clone()).collect());
    engine.group_selection();
    engine.clear_selection();
}

fn click(engine: &mut DrawEngine, x: f64, y: f64, additive: bool) {
    engine.begin_pointer(x, y, additive, false);
    engine.end_pointer();
}

/// Filled 80×80 boxes, so a press anywhere inside one grabs it.
fn boxes(xs: &[(f64, f64)]) -> (DrawEngine, Vec<String>) {
    let elements: Vec<DrawElement> = xs
        .iter()
        .map(|&(x, y)| filled(box_at(x, y, 80.0, 80.0)))
        .collect();
    let ids = elements.iter().map(|el| el.id.clone()).collect();
    let mut engine = engine_with_scene(elements);
    engine.set_tool(DrawTool::Select);
    (engine, ids)
}

// ---------------------------------------------------------------------------
// Align
// ---------------------------------------------------------------------------

/// The probe as reported: the group collapsed, A and B both ending at x=0.
#[test]
fn align_moves_a_group_as_one_block() {
    let (mut engine, ids) = boxes(&[(100.0, 0.0), (250.0, 0.0), (0.0, 200.0)]);
    let [a, b, c] = [&ids[0], &ids[1], &ids[2]];
    group(&mut engine, &[a, b]);
    engine.select(vec![a.clone(), b.clone(), c.clone()]);

    engine.align_selection(AlignMode::Left);

    assert_close(x_of(&engine, a), 0.0);
    assert_close(x_of(&engine, b), 150.0);
    assert_close(x_of(&engine, c), 0.0);
}

/// One group selected on its own is cut one level in: its inner group and its loose
/// member are the units, so the inner group lines up with C and keeps its spacing.
#[test]
fn align_one_selected_group_aligns_its_inner_units() {
    let (mut engine, ids) = boxes(&[(100.0, 0.0), (250.0, 0.0), (0.0, 200.0)]);
    let [a, b, c] = [&ids[0], &ids[1], &ids[2]];
    group(&mut engine, &[a, b]);
    group(&mut engine, &[a, b, c]);
    // A click on any member takes the outermost group.
    click(&mut engine, 140.0, 40.0, false);
    assert_eq!(engine.get_selection().len(), 3, "setup");

    engine.align_selection(AlignMode::Left);

    assert_close(x_of(&engine, a), 0.0);
    assert_close(x_of(&engine, b), 150.0);
    assert_close(x_of(&engine, c), 0.0);
}

/// Inside an edited group the units are cut at that level. `{top {G {inner {A, B}, C}},
/// D}`: with G entered and A, B, C held, the units are `inner` and C — read from the top
/// level instead, the whole selection was one unit and nothing moved.
#[test]
fn align_inside_an_edited_group_cuts_units_at_that_level() {
    let (mut engine, ids) = boxes(&[(100.0, 0.0), (250.0, 0.0), (0.0, 200.0), (600.0, 400.0)]);
    let [a, b, c, d] = [&ids[0], &ids[1], &ids[2], &ids[3]];
    group(&mut engine, &[a, b]);
    group(&mut engine, &[a, b, c]);
    group(&mut engine, &[a, b, c, d]);
    // Two double clicks on A: into `top`, then into G.
    engine.handle_double_click(140.0, 40.0);
    engine.handle_double_click(140.0, 40.0);
    assert!(
        engine.editing_group_id().is_some(),
        "setup: a group is entered"
    );
    click(&mut engine, 40.0, 240.0, true);
    let mut held = engine.get_selection();
    held.sort();
    let mut want = vec![a.clone(), b.clone(), c.clone()];
    want.sort();
    assert_eq!(held, want, "setup: A, B and C held inside G");

    engine.align_selection(AlignMode::Left);

    assert_close(x_of(&engine, a), 0.0);
    assert_close(x_of(&engine, b), 150.0);
    assert_close(x_of(&engine, c), 0.0);
    assert_close(x_of(&engine, d), 600.0);
}

/// A label is not a unit: it follows its shape, laid out again in the same step.
#[test]
fn a_label_rides_with_its_shape() {
    let (mut engine, ids) = boxes(&[(100.0, 0.0), (0.0, 200.0)]);
    let [a, c] = [&ids[0], &ids[1]];
    engine.set_measure_text(measure_text);
    engine.handle_double_click(140.0, 40.0);
    let typed = engine.get_selection().pop().expect("setup: a label opened");
    engine.set_element_text(&typed, "hi");
    let label = engine
        .get_scene()
        .into_iter()
        .find(|el| el.container_id.as_deref() == Some(a.as_str()))
        .expect("setup: A has a label");
    let offset = label.x - x_of(&engine, a);
    engine.select(vec![a.clone(), c.clone(), label.id.clone()]);

    engine.align_selection(AlignMode::Left);

    assert_close(x_of(&engine, a), 0.0);
    assert_close(x_of(&engine, &label.id) - x_of(&engine, a), offset);
}

// ---------------------------------------------------------------------------
// Distribute
// ---------------------------------------------------------------------------

/// Equal gaps, not equal centre steps: A[0,100], B[150,170], C[300,320] leaves gaps of
/// 90 either side of B, so B goes to 190. Spaced by centres it stayed at 170.
#[test]
fn distribute_uses_equal_gaps() {
    let a = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let b = filled(box_at(150.0, 0.0, 20.0, 80.0));
    let c = filled(box_at(300.0, 0.0, 20.0, 80.0));
    let ids = [a.id.clone(), b.id.clone(), c.id.clone()];
    let mut engine = engine_with_scene(vec![a, b, c]);
    engine.select(ids.to_vec());

    engine.distribute_selection('x');

    assert_close(x_of(&engine, &ids[0]), 0.0);
    assert_close(x_of(&engine, &ids[1]), 190.0);
    assert_close(x_of(&engine, &ids[2]), 300.0);
}

/// A group is spaced as one unit and keeps its inner gap. A[0,40], the group [100,240],
/// D[600,640]: 420 of free space, 210 each side, so the group lands at 250.
#[test]
fn distribute_keeps_a_group_whole() {
    let a = filled(box_at(0.0, 0.0, 40.0, 40.0));
    let g1 = filled(box_at(100.0, 0.0, 40.0, 40.0));
    let g2 = filled(box_at(200.0, 0.0, 40.0, 40.0));
    let d = filled(box_at(600.0, 0.0, 40.0, 40.0));
    let (ai, g1i, g2i, di) = (a.id.clone(), g1.id.clone(), g2.id.clone(), d.id.clone());
    let mut engine = engine_with_scene(vec![a, g1, g2, d]);
    group(&mut engine, &[&g1i, &g2i]);
    engine.select(vec![ai.clone(), g1i.clone(), g2i.clone(), di.clone()]);

    engine.distribute_selection('x');

    assert_close(x_of(&engine, &g1i), 250.0);
    assert_close(x_of(&engine, &g2i), 350.0);
    assert_close(x_of(&engine, &ai), 0.0);
    assert_close(x_of(&engine, &di), 600.0);
}

/// Units too wide for gaps: the oracle's centre fallback, transcribed with its quirk.
/// The two units that reach the selection's ends stay put — here A[0,200] and
/// C[150,250], found by their edges and not by being first and last by centre — and the
/// others are stepped from A's centre by (C's centre − A's centre)/(n−1). So B, whose
/// centre sorts first, lands at centre 150.
#[test]
fn overlapping_units_fall_back_to_centres_between_the_end_units() {
    let a = filled(box_at(0.0, 0.0, 200.0, 40.0));
    let b = filled(box_at(50.0, 100.0, 20.0, 40.0));
    let c = filled(box_at(150.0, 200.0, 100.0, 40.0));
    let ids = [a.id.clone(), b.id.clone(), c.id.clone()];
    let mut engine = engine_with_scene(vec![a, b, c]);
    engine.select(ids.to_vec());

    engine.distribute_selection('x');

    assert_close(x_of(&engine, &ids[0]), 0.0);
    assert_close(x_of(&engine, &ids[1]), 140.0);
    assert_close(x_of(&engine, &ids[2]), 150.0);
}

/// Vertically too, on the same rule.
#[test]
fn distribute_vertically_uses_equal_gaps() {
    let a = filled(box_at(0.0, 0.0, 80.0, 100.0));
    let b = filled(box_at(0.0, 150.0, 80.0, 20.0));
    let c = filled(box_at(0.0, 300.0, 80.0, 20.0));
    let ids = [a.id.clone(), b.id.clone(), c.id.clone()];
    let mut engine = engine_with_scene(vec![a, b, c]);
    engine.select(ids.to_vec());

    engine.distribute_selection('y');

    assert_close(y_of(&engine, &ids[1]), 190.0);
}

// ---------------------------------------------------------------------------
// Refused
// ---------------------------------------------------------------------------

/// A frame in the selection refuses both: the frame moved and its children stayed,
/// still claiming it. Nothing changes and no step is recorded.
#[test]
fn align_and_distribute_refuse_a_selection_with_a_frame() {
    let child = filled(box_at(60.0, 60.0, 80.0, 80.0));
    let other = filled(box_at(600.0, 400.0, 80.0, 80.0));
    let third = filled(box_at(400.0, 100.0, 80.0, 80.0));
    let (child_id, other_id, third_id) = (child.id.clone(), other.id.clone(), third.id.clone());
    let mut engine = engine_with_scene(vec![child, other, third]);
    engine.set_tool(DrawTool::Frame);
    engine.begin_pointer(20.0, 20.0, false, false);
    engine.move_pointer(300.0, 300.0, false, false);
    engine.end_pointer();
    let frame = engine
        .get_scene()
        .into_iter()
        .find(is_frame)
        .expect("setup: a frame")
        .id;
    engine.select(vec![frame.clone(), other_id.clone(), third_id]);
    let before = engine.get_scene();
    let _ = engine.drain_events();

    engine.align_selection(AlignMode::Bottom);
    engine.distribute_selection('x');

    assert_eq!(engine.get_scene(), before);
    assert!(engine.drain_events().scene_delta.is_none(), "nothing sent");
    assert_close(y_of(&engine, &frame), 20.0);
    assert_close(y_of(&engine, &child_id), 60.0);
}

/// Fewer than three units is nothing to space, however many elements they hold.
#[test]
fn two_groups_are_two_units_and_distribute_nothing() {
    let (mut engine, ids) = boxes(&[(100.0, 0.0), (250.0, 30.0), (0.0, 200.0), (400.0, 90.0)]);
    let [a, b, c, d] = [&ids[0], &ids[1], &ids[2], &ids[3]];
    group(&mut engine, &[a, b]);
    group(&mut engine, &[c, d]);
    engine.select(vec![a.clone(), b.clone(), c.clone(), d.clone()]);
    let before = engine.get_scene();

    // Four elements, two units.
    engine.distribute_selection('x');

    assert_eq!(engine.get_scene(), before);
}

// ---------------------------------------------------------------------------
// What the host offers
// ---------------------------------------------------------------------------

/// The host offers each action exactly when it would do something: counted in units,
/// not elements, and never with a frame selected (`alignActionsPredicate`,
/// `actionAlign.tsx@1118751f:39-52`; `actionDistribute.tsx@1118751f:35-46`).
#[test]
fn the_host_is_offered_align_and_distribute_by_units() {
    let (mut engine, ids) = boxes(&[(100.0, 0.0), (250.0, 30.0), (0.0, 200.0), (400.0, 90.0)]);
    let [a, b, c, d] = [&ids[0], &ids[1], &ids[2], &ids[3]];
    group(&mut engine, &[a, b]);
    let offered = |engine: &DrawEngine| (engine.can_align(), engine.can_distribute());

    assert_eq!(offered(&engine), (false, false), "nothing selected");
    engine.select(vec![c.clone()]);
    assert_eq!(offered(&engine), (false, false), "one shape");
    engine.select(vec![a.clone(), b.clone()]);
    assert_eq!(
        offered(&engine),
        (true, false),
        "one group: its two members"
    );
    engine.select(vec![a.clone(), b.clone(), c.clone()]);
    assert_eq!(
        offered(&engine),
        (true, false),
        "a group and a shape: two units"
    );
    engine.select(vec![a.clone(), b.clone(), c.clone(), d.clone()]);
    assert_eq!(offered(&engine), (true, true), "three units");

    engine.set_tool(DrawTool::Frame);
    engine.begin_pointer(600.0, 300.0, false, false);
    engine.move_pointer(700.0, 400.0, false, false);
    engine.end_pointer();
    let frame = engine.get_selection();
    engine.select([frame, vec![c.clone(), d.clone()]].concat());
    assert_eq!(offered(&engine), (false, false), "a frame is selected");
}

/// Align, distribute and flip commit through the same step, which re-resolves the
/// arrows of what it moved: an arrow bound to a moved shape follows it though the arrow
/// itself is not selected, stamped once.
#[test]
fn an_arrow_bound_to_an_aligned_shape_follows_it() {
    let r1 = filled(box_at(0.0, 0.0, 100.0, 100.0));
    let r2 = filled(box_at(300.0, 200.0, 100.0, 100.0));
    let x = filled(box_at(0.0, 400.0, 100.0, 100.0));
    let mut arrow = connector(100.0, 50.0, 300.0, 250.0, DrawElementType::Arrow);
    arrow.start_binding = Some(r1.id.clone());
    arrow.end_binding = Some(r2.id.clone());
    let (r1_id, x_id, arrow_id) = (r1.id.clone(), x.id.clone(), arrow.id.clone());
    let mut engine = engine_with_scene(vec![r1, r2, x, arrow.clone()]);
    engine.select(vec![r1_id.clone(), x_id]);

    engine.align_selection(AlignMode::Bottom);

    assert_close(y_of(&engine, &r1_id), 400.0);
    let after = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == arrow_id)
        .expect("the arrow");
    assert!(after.y > 400.0, "the tail followed R1 down: {after:?}");
    assert_eq!(after.version, arrow.version + 1);
}
