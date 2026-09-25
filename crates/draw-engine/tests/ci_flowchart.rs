//! Ctrl/Cmd+Arrow builds a connected diagram; Alt+Arrow walks it. Port of Excalidraw's
//! `packages/element/src/flowchart.ts` and `packages/excalidraw/components/App.flowchart.ts`
//! (`@1118751f`) — see `engine/flowchart.rs` for what is ported and what deliberately
//! diverges (no elbow routing, a simplified navigation heading).

mod common;
use common::*;
use draw_engine::*;

fn one_rect() -> (DrawEngine, DrawElement) {
    let rect = filled(box_at(0.0, 0.0, 100.0, 60.0));
    let mut engine = engine_with_scene(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    (engine, rect)
}

fn arrows(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == DrawElementType::Arrow && !el.is_deleted)
        .collect()
}

fn live_kind(engine: &DrawEngine, kind: DrawElementType) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == kind && !el.is_deleted)
        .collect()
}

// ------------------------------------------------------------------------------ creation

#[test]
fn creation_in_four_directions() {
    let cases = [
        (LinkDirection::Right, 1.0, 0.0),
        (LinkDirection::Left, -1.0, 0.0),
        (LinkDirection::Down, 0.0, 1.0),
        (LinkDirection::Up, 0.0, -1.0),
    ];
    for (direction, sign_x, sign_y) in cases {
        let (mut engine, rect) = one_rect();
        engine.flowchart_create(direction);
        engine.flowchart_commit();

        let nodes: Vec<DrawElement> = live_kind(&engine, DrawElementType::Rectangle)
            .into_iter()
            .filter(|el| el.id != rect.id)
            .collect();
        assert_eq!(nodes.len(), 1, "{direction:?}: exactly one new node");
        let node = &nodes[0];

        if sign_x > 0.0 {
            assert!(
                node.x > rect.x + rect.width,
                "{direction:?}: right of the parent"
            );
        } else if sign_x < 0.0 {
            assert!(
                node.x + node.width < rect.x,
                "{direction:?}: left of the parent"
            );
        }
        if sign_y > 0.0 {
            assert!(
                node.y > rect.y + rect.height,
                "{direction:?}: below the parent"
            );
        } else if sign_y < 0.0 {
            assert!(
                node.y + node.height < rect.y,
                "{direction:?}: above the parent"
            );
        }
    }
}

#[test]
fn siblings_on_repeat_presses() {
    let (mut engine, _rect) = one_rect();

    engine.flowchart_create(LinkDirection::Right);
    assert_eq!(
        engine.pending_flowchart_elements().len(),
        2,
        "one node, one arrow"
    );

    // A repeat press in the same direction — key repeat, or a second tap — grows the
    // cluster by one rather than replacing it.
    engine.flowchart_create(LinkDirection::Right);
    let pending = engine.pending_flowchart_elements();
    assert_eq!(pending.len(), 4, "two nodes, two arrows");

    let node_ys: Vec<f64> = pending
        .iter()
        .filter(|el| el.kind == DrawElementType::Rectangle)
        .map(|el| el.y)
        .collect();
    assert_eq!(node_ys.len(), 2);
    assert!(
        (node_ys[0] - node_ys[1]).abs() > 1.0,
        "siblings sit apart on the cross axis, not stacked on each other"
    );

    engine.flowchart_commit();
    assert_eq!(
        live_kind(&engine, DrawElementType::Rectangle).len(),
        3,
        "the parent plus both siblings"
    );
    assert_eq!(arrows(&engine).len(), 2, "one arrow per sibling");
}

#[test]
fn pending_not_in_scene_before_commit() {
    let (mut engine, rect) = one_rect();
    let before = engine.debug_state().scene.element_count;

    engine.flowchart_create(LinkDirection::Right);

    assert_eq!(
        engine.debug_state().scene.element_count,
        before,
        "the pending cluster must not appear in the scene"
    );
    assert_eq!(
        engine.get_scene().len(),
        1,
        "get_scene sees only the parent, still"
    );
    assert_eq!(engine.pending_flowchart_elements().len(), 2);
    assert!(engine.is_creating_flowchart());

    // Cancelling (Escape) leaves no trace at all.
    engine.flowchart_cancel();
    assert!(!engine.is_creating_flowchart());
    assert!(engine.pending_flowchart_elements().is_empty());
    assert_eq!(engine.get_scene().len(), 1);
    assert_eq!(engine.get_selection(), vec![rect.id]);
}

#[test]
fn arrow_bound_both_ends() {
    let (mut engine, rect) = one_rect();
    engine.flowchart_create(LinkDirection::Right);
    engine.flowchart_commit();

    let new_nodes: Vec<DrawElement> = live_kind(&engine, DrawElementType::Rectangle)
        .into_iter()
        .filter(|el| el.id != rect.id)
        .collect();
    assert_eq!(new_nodes.len(), 1);
    let node = &new_nodes[0];

    let arrows = arrows(&engine);
    assert_eq!(arrows.len(), 1);
    let arrow = &arrows[0];
    assert_eq!(arrow.start_binding.as_deref(), Some(rect.id.as_str()));
    assert_eq!(arrow.end_binding.as_deref(), Some(node.id.as_str()));

    // Bound, not merely coincident: the endpoints sit just outside each shape's own edge,
    // not at an arbitrary point that would drift the moment either shape moved.
    let (start, end) = linear_endpoints(arrow);
    assert!(
        start.x > rect.x + rect.width,
        "starts past the parent's right edge"
    );
    assert!(end.x < node.x, "ends before the child's left edge");
}

#[test]
fn style_and_size_are_copied() {
    let mut template = filled(box_at(0.0, 0.0, 140.0, 90.0));
    template.stroke_color = "#e03131".to_string();
    template.stroke_width = 4.0;
    template.roughness = 2.0;
    let mut engine = engine_with_scene(vec![template.clone()]);
    engine.select(vec![template.id.clone()]);

    engine.flowchart_create(LinkDirection::Down);
    engine.flowchart_commit();

    let node = live_kind(&engine, DrawElementType::Rectangle)
        .into_iter()
        .find(|el| el.id != template.id)
        .expect("a new node");

    assert_eq!(node.width, template.width);
    assert_eq!(node.height, template.height);
    assert_eq!(node.stroke_color, template.stroke_color);
    assert_eq!(node.background_color, template.background_color);
    assert_eq!(node.stroke_width, template.stroke_width);
    assert_eq!(node.roughness, template.roughness);
}

#[test]
fn commit_is_one_undo_step() {
    let (mut engine, _rect) = one_rect();
    let before = engine.debug_state().scene.element_count;

    engine.flowchart_create(LinkDirection::Right);
    engine.flowchart_create(LinkDirection::Right); // a sibling too, so the step moves two nodes and two arrows
    engine.flowchart_commit();

    let after_commit = engine.debug_state();
    assert_eq!(after_commit.scene.element_count, before + 4);
    assert!(after_commit.scene.can_undo);

    engine.undo();

    let after_undo = engine.debug_state();
    assert_eq!(
        after_undo.scene.element_count, before,
        "one undo call removed the whole cluster — nodes and arrows together"
    );
    assert!(
        !after_undo.scene.can_undo,
        "nothing left to undo: the commit was a single step"
    );
    assert!(after_undo.scene.can_redo);
}

// ---------------------------------------------------------------------------- the extras

#[test]
fn shape_choice_switches_the_pending_and_committed_kind() {
    let (mut engine, _rect) = one_rect();
    engine.flowchart_create(LinkDirection::Right);

    let default_kind = engine.pending_flowchart_elements()[0].kind;
    assert_eq!(
        default_kind,
        DrawElementType::Rectangle,
        "copies the parent's kind by default"
    );

    engine.flowchart_set_shape(DrawElementType::Diamond);
    let pending = engine.pending_flowchart_elements();
    let node = pending
        .iter()
        .find(|el| el.kind != DrawElementType::Arrow)
        .expect("a pending node");
    assert_eq!(node.kind, DrawElementType::Diamond);

    engine.flowchart_commit();
    assert_eq!(live_kind(&engine, DrawElementType::Diamond).len(), 1);
    assert!(
        live_kind(&engine, DrawElementType::Rectangle).len() == 1,
        "only the parent"
    );
}

#[test]
fn shape_choice_is_ignored_with_nothing_being_created() {
    let (mut engine, rect) = one_rect();
    engine.flowchart_set_shape(DrawElementType::Ellipse);
    assert!(!engine.is_creating_flowchart());
    assert_eq!(engine.get_scene().len(), 1);
    assert_eq!(engine.get_selection(), vec![rect.id]);
}

// -------------------------------------------------------------------------- navigation

#[test]
fn navigation_by_direction() {
    let (mut engine, rect) = one_rect();
    engine.flowchart_create(LinkDirection::Right);
    engine.flowchart_commit();
    let sibling = live_kind(&engine, DrawElementType::Rectangle)
        .into_iter()
        .find(|el| el.id != rect.id)
        .expect("the sibling created to the right");

    engine.select(vec![rect.id.clone()]);
    let found = engine.flowchart_navigate(LinkDirection::Right);
    assert_eq!(found, Some(sibling.id.clone()));
    assert_eq!(engine.get_selection(), vec![sibling.id.clone()]);

    // And back, from the sibling's own point of view.
    let back = engine.flowchart_navigate(LinkDirection::Left);
    assert_eq!(back, Some(rect.id.clone()));
    assert_eq!(engine.get_selection(), vec![rect.id.clone()]);
}

#[test]
fn navigate_returns_none_with_nothing_linked() {
    let (mut engine, _rect) = one_rect();
    assert_eq!(engine.flowchart_navigate(LinkDirection::Up), None);
    assert_eq!(engine.flowchart_navigate(LinkDirection::Down), None);
}

/// With nothing directly `Up`, the fallback hops to a node linked in any other, unvisited
/// direction — the oracle's "speedier navigation... without the user having to change
/// arrow key" (`flowchart.ts@1118751f:534-545`).
#[test]
fn navigate_falls_back_to_any_unvisited_linked_node() {
    let (mut engine, rect) = one_rect();
    engine.flowchart_create(LinkDirection::Right);
    engine.flowchart_commit();
    let sibling = live_kind(&engine, DrawElementType::Rectangle)
        .into_iter()
        .find(|el| el.id != rect.id)
        .expect("the sibling created to the right");

    engine.select(vec![rect.id.clone()]);
    assert_eq!(
        engine.flowchart_navigate(LinkDirection::Up),
        Some(sibling.id)
    );
}

#[test]
fn navigate_requires_a_single_bindable_selection() {
    let (mut engine, rect) = one_rect();
    engine.flowchart_create(LinkDirection::Right);
    engine.flowchart_commit();

    engine.select(vec![]);
    assert_eq!(engine.flowchart_navigate(LinkDirection::Right), None);

    engine.select(vec![rect.id]);
    let scene_ids: Vec<String> = engine.get_scene().into_iter().map(|el| el.id).collect();
    engine.select(scene_ids);
    assert_eq!(engine.flowchart_navigate(LinkDirection::Right), None);
}
