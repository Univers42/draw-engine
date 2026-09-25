//! Which elements a style change reaches.
//!
//! Once a shape carries a label, the shape is the only thing a click can select — so a
//! change that stops at the selection never reaches the text inside it. The oracle's
//! `changeProperty(…, includeBoundText = true)` (`actions/actionProperties.tsx@1118751f:193-227`)
//! takes the label along for the stroke colour (`:381-397`) and the opacity (`:962-970`),
//! and leaves it out of the fill, the stroke width, the dash and the sloppiness, which a
//! text has no use for.

mod common;
use common::*;
use draw_engine::*;

/// A rectangle holding a label, as the engine makes one: the label points at the shape
/// and the shape at the label.
fn labelled_box() -> Vec<DrawElement> {
    let mut shape = box_at(0.0, 0.0, 200.0, 100.0);
    shape.id = "box".into();
    shape.bound_text_id = Some("label".into());
    let mut label = text_at(10.0, 40.0, 180.0, 20.0);
    label.id = "label".into();
    label.text = Some("hello".into());
    label.container_id = Some("box".into());
    vec![shape, label]
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("element exists")
}

#[test]
fn the_stroke_colour_reaches_the_label_of_a_selected_shape() {
    let mut engine = engine_with_measure(labelled_box());
    engine.select(vec!["box".into()]);

    engine.apply_style(stroke_patch("#e03131"));

    assert_eq!(element(&engine, "box").stroke_color, "#e03131");
    assert_eq!(
        element(&engine, "label").stroke_color,
        "#e03131",
        "the label is the text colour of the shape it sits in"
    );
}

#[test]
fn the_opacity_reaches_the_label_of_a_selected_shape() {
    let mut engine = engine_with_measure(labelled_box());
    engine.select(vec!["box".into()]);

    engine.apply_style(DrawElementStylePatch {
        opacity: Some(30.0),
        ..Default::default()
    });

    assert_eq!(element(&engine, "box").opacity, 30.0);
    assert_eq!(element(&engine, "label").opacity, 30.0);
}

/// `changeBackgroundColor`, `changeStrokeWidth`, `changeFillStyle` and the rest pass no
/// `includeBoundText`: a label has no background, and a width would change nothing.
#[test]
fn the_fill_and_the_line_stay_on_the_shape() {
    let mut engine = engine_with_measure(labelled_box());
    engine.select(vec!["box".into()]);
    let before = element(&engine, "label");

    engine.apply_style(DrawElementStylePatch {
        background_color: Some("#ffc9c9".into()),
        stroke_width: Some(4.0),
        roughness: Some(2.0),
        ..Default::default()
    });

    let label = element(&engine, "label");
    assert_eq!(label.background_color, before.background_color);
    assert_eq!(label.stroke_width, before.stroke_width);
    assert_eq!(label.roughness, before.roughness);
    assert_eq!(
        label.version, before.version,
        "nothing reached it, so no stamp"
    );
}

/// The shape and its label change together, so they undo together.
#[test]
fn the_shape_and_its_label_are_one_step_of_undo() {
    let mut engine = engine_with_measure(labelled_box());
    engine.select(vec!["box".into()]);

    engine.apply_style(stroke_patch("#e03131"));
    engine.undo();

    assert_eq!(element(&engine, "box").stroke_color, "#1e1e1e");
    assert_eq!(element(&engine, "label").stroke_color, "#1e1e1e");
}

/// Choosing the colour everything already has is not an edit: stamped, it would be
/// saved, sent to every peer and cost a step of undo that puts nothing back — the
/// oracle's `newElementWith` returns the element untouched when nothing differs
/// (`packages/element/src/mutateElement.ts@1118751f:170-172`).
#[test]
fn a_style_that_changes_nothing_is_not_an_edit() {
    let mut engine = engine_with_measure(labelled_box());
    engine.select(vec!["box".into()]);
    let before = element(&engine, "box");

    engine.apply_style(stroke_patch(&before.stroke_color));

    assert_eq!(element(&engine, "box").version, before.version);
    let events = engine.drain_events();
    assert!(events.scene_json.is_none());
    assert!(
        events
            .scene_delta
            .is_none_or(|delta| delta.updated.is_empty() && delta.removed.is_empty()),
        "nothing changed, so nothing is sent"
    );
}

/// Styling a selection also sets up the next element, as the oracle's actions update
/// `currentItem*` alongside the elements (`actionProperties.tsx@1118751f:622`, `:721`,
/// `:778`, `:914`, `:971`; `actions/colorTargets.ts@1118751f:178-192`): pick red for a
/// shape and the next shape drawn is red too.
#[test]
fn a_style_chosen_for_the_selection_is_the_next_style_too() {
    let mut engine = engine_with_measure(labelled_box());
    engine.select(vec!["box".into()]);

    engine.apply_style(DrawElementStylePatch {
        stroke_color: Some("#e03131".into()),
        opacity: Some(60.0),
        ..Default::default()
    });

    let next = engine.get_next_style();
    assert_eq!(next.stroke_color, "#e03131");
    assert_eq!(next.opacity, 60.0);
}

/// Select All takes a label along with its shape — this engine's does, where the
/// oracle's skips bound text (`actions/actionSelectAll.ts@1118751f:32-38`) — and a label
/// held that way is still its shape's words. The panel read it as a second selected
/// element: "2 selected", the width mixed, and a fill or a width reached the text.
#[test]
fn a_label_select_all_takes_is_still_carried_by_its_shape() {
    let mut engine = engine_with_measure(labelled_box());
    engine.select(vec!["box".into()]);
    let clicked = engine.selection_style();
    engine.select_all();
    assert_eq!(engine.get_selection().len(), 2, "setup: the label is held");
    assert_eq!(
        engine.selection_style(),
        clicked,
        "read as a click reads it"
    );
    let before = element(&engine, "label");

    engine.apply_style(DrawElementStylePatch {
        stroke_color: Some("#e03131".into()),
        background_color: Some("#ffc9c9".into()),
        stroke_width: Some(4.0),
        ..Default::default()
    });

    assert_eq!(element(&engine, "box").background_color, "#ffc9c9");
    let label = element(&engine, "label");
    assert_eq!(
        label.stroke_color, "#e03131",
        "the text colour still reaches it"
    );
    assert_eq!(label.background_color, before.background_color);
    assert_eq!(label.stroke_width, before.stroke_width);
}

/// Select All over a locked shape holding a label and a free shape, as this engine's
/// takes locked elements — so they can be unlocked from the menu — where the oracle
/// cannot select one at all (`shouldIgnoreElementFromSelection`,
/// `packages/element/src/selection.ts@1118751f:33-34`).
fn locked_box_and_a_free_one() -> DrawEngine {
    let mut elements = labelled_box();
    elements[0].locked = Some(true);
    let mut free = box_at(300.0, 0.0, 100.0, 100.0);
    free.id = "free".into();
    free.stroke_color = "#2f9e44".into();
    elements.push(free);
    let mut engine = engine_with_measure(elements);
    engine.select_all();
    assert_eq!(engine.get_selection().len(), 3, "setup: all three are held");
    engine
}

/// A locked element is never restyled, nor the label of a locked shape: what the oracle
/// cannot select, no style of its reaches.
#[test]
fn a_style_chosen_after_select_all_passes_locked_elements_by() {
    let mut engine = locked_box_and_a_free_one();
    let (locked, label) = (element(&engine, "box"), element(&engine, "label"));

    engine.apply_style(stroke_patch("#e03131"));
    engine.set_font_size(36.0);

    assert_eq!(element(&engine, "free").stroke_color, "#e03131");
    let after = element(&engine, "box");
    assert_eq!(
        (after.stroke_color.as_str(), after.version),
        (locked.stroke_color.as_str(), locked.version),
        "the locked shape keeps its colour and its stamp"
    );
    assert_eq!(element(&engine, "label"), label, "and so does its label");
}

#[test]
fn pasted_styles_pass_locked_elements_by() {
    let mut engine = locked_box_and_a_free_one();
    let (locked, label) = (element(&engine, "box"), element(&engine, "label"));
    engine.select(vec!["free".into()]);
    assert!(engine.copy_styles());
    engine.select_all();

    engine.paste_styles();

    assert_eq!(element(&engine, "box"), locked);
    assert_eq!(element(&engine, "label"), label);
}

/// The panel shows what a style would change: the free shape's colour, not "mixed" with
/// the locked one's, and no text rows for the locked shape's label.
#[test]
fn the_panel_reads_only_what_a_style_would_change() {
    let engine = locked_box_and_a_free_one();
    let summary = engine.selection_style();
    assert_eq!(summary.stroke_color.as_deref(), Some("#2f9e44"));
    assert_eq!(summary.kinds, vec![DrawElementType::Rectangle]);
    assert_eq!(summary.font_size, None);
    assert_eq!(
        summary.count, 2,
        "the layer, flip and group rows still act on both"
    );
}

/// A free box `a` and the locked labelled `box` in one group, selected as a click on `a`
/// selects them. The oracle selects every member of a clicked group with no lock filter
/// (`selectGroupsForSelectedElements`, `packages/element/src/groups.ts@1118751f:66-140`,
/// from `App.tsx@1118751f:8917-8925`) and restyles every selected element
/// (`changeProperty`, `actionProperties.tsx@1118751f:193-223`); its lock filter
/// (`shouldIgnoreElementFromSelection`, `selection.ts@1118751f:33-34`) is the marquee's
/// alone (`:70-96`).
fn a_group_with_a_locked_member() -> DrawEngine {
    let mut elements = labelled_box();
    elements[0].locked = Some(true);
    for element in &mut elements {
        element.group_ids = vec!["g".into()];
    }
    let mut free = filled(box_at(0.0, 200.0, 100.0, 100.0));
    free.id = "a".into();
    free.group_ids = vec!["g".into()];
    elements.push(free);
    let mut source = box_at(400.0, 0.0, 100.0, 100.0);
    source.id = "source".into();
    source.stroke_color = "#2f9e44".into();
    elements.push(source);
    let mut engine = engine_with_measure(elements);
    engine.begin_pointer(50.0, 250.0, false, false);
    engine.end_pointer();
    let mut selected = engine.get_selection();
    selected.sort();
    assert_eq!(
        selected,
        vec!["a", "box", "label"],
        "setup: the whole group"
    );
    engine
}

/// So a locked member takes the group's colour, and its label the text colour and size.
#[test]
fn a_locked_member_of_a_selected_group_is_restyled() {
    let mut engine = a_group_with_a_locked_member();
    assert!(
        engine
            .selection_style()
            .kinds
            .contains(&DrawElementType::Text),
        "the locked member's label offers the text rows"
    );

    engine.apply_style(stroke_patch("#e03131"));
    engine.set_font_size(36.0);

    assert_eq!(element(&engine, "a").stroke_color, "#e03131");
    assert_eq!(element(&engine, "box").stroke_color, "#e03131");
    let label = element(&engine, "label");
    assert_eq!(label.stroke_color, "#e03131");
    assert_eq!(label.font_size, Some(36.0));
}

#[test]
fn a_style_pasted_on_a_group_reaches_its_locked_member() {
    let mut engine = a_group_with_a_locked_member();
    engine.select(vec!["source".into()]);
    assert!(engine.copy_styles());
    engine.begin_pointer(50.0, 250.0, false, false);
    engine.end_pointer();

    engine.paste_styles();

    assert_eq!(element(&engine, "box").stroke_color, "#2f9e44");
}
