//! A style patch has to be able to say "set this to nothing".
//!
//! `roundness` is the only field where absent and null mean different things: absent is
//! "leave the corners as they are", null is "make them sharp". The default serde mapping
//! collapses the two, and the consequence was visible on the canvas — corners could be
//! rounded and then never un-rounded, because "Sharp" deserialized as "say nothing".

mod common;
use common::*;
use draw_engine::*;

fn patch(json: &str) -> DrawElementStylePatch {
    serde_json::from_str(json).expect("patch should parse")
}

#[test]
fn an_absent_field_leaves_the_corners_alone() {
    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    element.roundness = Some(8.0);
    apply_style_patch(&mut element, &patch(r#"{"strokeWidth": 4}"#));
    assert_eq!(element.roundness, Some(8.0));
    assert_eq!(element.stroke_width, 4.0);
}

#[test]
fn an_explicit_null_makes_the_corners_sharp() {
    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    element.roundness = Some(8.0);
    apply_style_patch(&mut element, &patch(r#"{"roundness": null}"#));
    assert_eq!(
        element.roundness, None,
        "null has to reach the element, or corners can be turned on and never off"
    );
}

#[test]
fn a_value_makes_them_round() {
    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    element.roundness = None;
    apply_style_patch(&mut element, &patch(r#"{"roundness": 8}"#));
    assert_eq!(element.roundness, Some(8.0));
}

/// Round, sharp, round again — the sequence the Edges toggle produces.
#[test]
fn the_edges_toggle_works_in_both_directions() {
    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    for (json, expected) in [
        (r#"{"roundness": 8}"#, Some(8.0)),
        (r#"{"roundness": null}"#, None),
        (r#"{"roundness": 8}"#, Some(8.0)),
        (r#"{"roundness": null}"#, None),
    ] {
        apply_style_patch(&mut element, &patch(json));
        assert_eq!(element.roundness, expected, "after {json}");
    }
}

/// Round-tripping a patch must not turn an explicit null into an absent field.
#[test]
fn a_null_survives_a_round_trip() {
    let original = patch(r#"{"roundness": null}"#);
    let json = serde_json::to_string(&original).unwrap();
    let back = patch(&json);

    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    element.roundness = Some(8.0);
    apply_style_patch(&mut element, &back);
    assert_eq!(element.roundness, None, "round-tripped through {json}");
}

/// Every other field keeps its ordinary two-state behaviour.
#[test]
fn the_other_fields_are_unaffected() {
    let mut element = box_at(0.0, 0.0, 100.0, 100.0);
    element.opacity = 50.0;
    element.roughness = 2.0;

    apply_style_patch(&mut element, &patch(r#"{}"#));
    assert_eq!(element.opacity, 50.0);
    assert_eq!(element.roughness, 2.0);

    apply_style_patch(&mut element, &patch(r#"{"opacity": 100, "roughness": 0}"#));
    assert_eq!(element.opacity, 100.0);
    assert_eq!(element.roughness, 0.0);
}
