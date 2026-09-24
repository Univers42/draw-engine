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
