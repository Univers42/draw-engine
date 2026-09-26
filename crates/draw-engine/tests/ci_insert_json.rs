//! `insert_json`: a scene made elsewhere — the Mermaid import — placed as a paste places
//! it, with every text laid out in this engine's fonts, as the oracle's
//! `convertToExcalidrawElements` puts a skeleton's labels through `redrawTextBoundingBox`
//! (`packages/element/src/transform.ts@1118751f:259-298`).

mod common;
use common::*;
use draw_engine::*;

/// A shape labelled with far more text than it holds, a free text with no size at all,
/// and an arrow from the shape — each text sized 0, as a converter that cannot measure
/// leaves it.
fn made_elsewhere() -> String {
    let mut shape = box_at(0.0, 0.0, 60.0, 30.0);
    shape.id = "shape".into();
    shape.bound_text_id = Some("label".into());
    let mut label = text_at(0.0, 0.0, 0.0, 0.0);
    label.id = "label".into();
    label.container_id = Some("shape".into());
    label.text = Some("a label far too long for the shape".into());
    label.original_text = label.text.clone();
    let mut free = text_at(0.0, 200.0, 0.0, 0.0);
    free.id = "free".into();
    free.text = Some("free".into());
    free.original_text = free.text.clone();
    let mut arrow = connector(60.0, 15.0, 200.0, 15.0, DrawElementType::Arrow);
    arrow.id = "arrow".into();
    arrow.start_binding = Some("shape".into());
    serde_json::json!({
        "type": "osidraw",
        "version": 1,
        "elements": [shape, label, free, arrow],
    })
    .to_string()
}

fn live(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|element| !element.is_deleted)
        .collect()
}

fn by_kind(elements: &[DrawElement], kind: DrawElementType, labelled: bool) -> DrawElement {
    elements
        .iter()
        .find(|element| element.kind == kind && element.container_id.is_some() == labelled)
        .cloned()
        .expect("inserted")
}

#[test]
fn a_label_is_set_smaller_to_fit_the_shape_it_was_made_for() {
    let mut shape = box_at(0.0, 0.0, 120.0, 30.0);
    shape.id = "shape".into();
    shape.bound_text_id = Some("label".into());
    let mut label = text_at(0.0, 0.0, 0.0, 0.0);
    label.id = "label".into();
    label.container_id = Some("shape".into());
    label.font_size = Some(20.0);
    label.text = Some("twelve chars".into());
    label.original_text = label.text.clone();
    let json = serde_json::json!({ "type": "osidraw", "version": 1, "elements": [shape, label] });
    let mut engine = engine_with_measure(vec![]);
    assert!(engine.insert_json(&json.to_string(), None));

    let scene = live(&engine);
    let shape = by_kind(&scene, DrawElementType::Rectangle, false);
    let label = by_kind(&scene, DrawElementType::Text, true);
    assert_eq!(
        (shape.width, shape.height),
        (120.0, 30.0),
        "the shape kept its size"
    );
    // 20 wraps it to two lines, 17 is one line too tall; 16 is the largest that fits.
    assert_eq!(label.font_size, Some(16.0));
    assert_eq!(label.text.as_deref(), Some("twelve chars"));
    assert!(label.y >= shape.y && label.y + label.height <= shape.y + shape.height);
}

#[test]
fn a_label_is_laid_out_and_its_shape_grown_to_hold_it() {
    let mut engine = engine_with_measure(vec![]);
    assert!(engine.insert_json(&made_elsewhere(), Some((300.0, 300.0))));
    let scene = live(&engine);

    let shape = by_kind(&scene, DrawElementType::Rectangle, false);
    let label = by_kind(&scene, DrawElementType::Text, true);
    assert_eq!(label.container_id.as_deref(), Some(shape.id.as_str()));
    assert!(label.width > 0.0 && label.height > 0.0, "measured");
    assert!(shape.height > 30.0, "grown from 30 to {}", shape.height);
    assert_eq!(
        label.font_size,
        Some(12.0),
        "only once no smaller size would do"
    );
    assert!(label.x >= shape.x && label.x + label.width <= shape.x + shape.width);
    assert!(label.y >= shape.y && label.y + label.height <= shape.y + shape.height);

    let free = by_kind(&scene, DrawElementType::Text, false);
    assert!(
        free.width > 0.0 && free.height > 0.0,
        "a free text is measured too"
    );
    let arrow = by_kind(&scene, DrawElementType::Arrow, false);
    assert_eq!(arrow.start_binding.as_deref(), Some(shape.id.as_str()));
}

#[test]
fn one_undo_takes_it_all_back_and_the_clipboard_is_untouched() {
    let mut engine = engine_with_measure(vec![box_at(0.0, 0.0, 10.0, 10.0)]);
    let original = engine.get_scene()[0].id.clone();
    engine.select(vec![original]);
    let copied = engine.copy_selection();
    assert!(copied.is_some());

    assert!(engine.insert_json(&made_elsewhere(), None));
    assert_eq!(live(&engine).len(), 5);
    engine.undo();
    assert_eq!(live(&engine).len(), 1, "one step of undo");

    assert!(engine.paste_json(None, None));
    let kinds: Vec<DrawElementType> = live(&engine).iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![DrawElementType::Rectangle, DrawElementType::Rectangle],
        "the paste after is what was copied, not what was inserted"
    );
}

#[test]
fn a_plain_paste_lays_nothing_out() {
    let mut engine = engine_with_measure(vec![]);
    assert!(engine.paste_json(Some(&made_elsewhere()), None));
    let label = by_kind(&live(&engine), DrawElementType::Text, true);
    assert_eq!((label.width, label.height), (0.0, 0.0));
}
