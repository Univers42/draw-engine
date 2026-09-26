//! Tab and Shift+Tab switch the selected shapes between rectangle, diamond and ellipse —
//! the generic branch of Excalidraw's shape switch
//! (`ConvertElementTypePopup.tsx@1118751f:418-500`). The label sizes and outline points
//! below are worked out by hand from `engine_with_measure`'s metrics: a character is half
//! the font size wide plus four, and a line is 1.25 font sizes high.

mod common;
use common::*;
use draw_engine::scene::element::BindMode;
use draw_engine::*;

fn with_id(mut element: DrawElement, id: &str) -> DrawElement {
    element.id = id.into();
    element
}

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("element exists")
}

fn selected(elements: Vec<DrawElement>, ids: &[&str]) -> DrawEngine {
    let mut engine = engine_with_measure(elements);
    engine.select(ids.iter().map(|id| id.to_string()).collect());
    engine
}

/// A 160x60 rectangle "s" holding the label "hello world" at size 20.
fn labelled_rectangle() -> Vec<DrawElement> {
    let mut shape = with_id(box_at(0.0, 0.0, 160.0, 60.0), "s");
    shape.bound_text_id = Some("label".into());
    let mut label = with_id(text_at(23.0, 17.5, 114.0, 25.0), "label");
    label.text = Some("hello world".into());
    label.original_text = Some("hello world".into());
    label.font_size = Some(20.0);
    label.container_id = Some("s".into());
    vec![shape, label]
}

/// A 200x100 rectangle "b" at the origin, and an arrow whose end is bound to it at
/// `fixed_point`.
fn arrow_into_b(fixed_point: [f64; 2]) -> Vec<DrawElement> {
    let b = with_id(filled(box_at(0.0, 0.0, 200.0, 100.0)), "b");
    let mut arrow = with_id(
        connector(400.0, -200.0, 200.0, 0.0, DrawElementType::Arrow),
        "arrow",
    );
    arrow.end_binding = Some("b".into());
    arrow.end_fixed_point = Some(fixed_point);
    arrow.end_bind_mode = Some(BindMode::Orbit);
    vec![b, arrow]
}

#[test]
fn tab_walks_rectangle_diamond_ellipse_and_round() {
    let mut engine = selected(vec![with_id(box_at(0.0, 0.0, 100.0, 60.0), "s")], &["s"]);
    let mut kinds = Vec::new();
    for _ in 0..3 {
        assert!(engine.convert_selection(None, true));
        kinds.push(get(&engine, "s").kind);
    }
    assert_eq!(
        kinds,
        [
            DrawElementType::Diamond,
            DrawElementType::Ellipse,
            DrawElementType::Rectangle
        ]
    );
    assert!(engine.convert_selection(None, false));
    assert_eq!(get(&engine, "s").kind, DrawElementType::Ellipse);
}

#[test]
fn shapes_of_different_kinds_all_become_the_first_or_second_type() {
    let elements = vec![
        with_id(box_at(0.0, 0.0, 100.0, 60.0), "r"),
        with_id(ellipse_at(200.0, 0.0, 100.0, 60.0), "e"),
    ];
    let mut engine = selected(elements.clone(), &["r", "e"]);
    engine.convert_selection(None, true);
    assert_eq!(get(&engine, "r").kind, DrawElementType::Rectangle);
    assert_eq!(get(&engine, "e").kind, DrawElementType::Rectangle);

    let mut engine = selected(elements, &["r", "e"]);
    engine.convert_selection(None, false);
    assert_eq!(get(&engine, "r").kind, DrawElementType::Diamond);
    assert_eq!(get(&engine, "e").kind, DrawElementType::Diamond);
}

#[test]
fn a_switch_keeps_place_size_and_style_and_drops_an_explicit_radius() {
    let mut shape = with_id(box_at(10.0, 20.0, 100.0, 60.0), "s");
    shape.stroke_color = "#e03131".into();
    shape.background_color = "#a5d8ff".into();
    shape.fill_style = FillStyle::Solid;
    shape.stroke_width = 4.0;
    shape.roughness = 2.0;
    shape.opacity = 50.0;
    shape.roundness = Some(8.0);
    shape.corner_radius = Some(12.0);
    let mut engine = selected(vec![shape.clone()], &["s"]);

    assert!(engine.convert_selection(Some(DrawElementType::Diamond), true));

    let after = get(&engine, "s");
    let mut expected = shape;
    expected.kind = DrawElementType::Diamond;
    expected.corner_radius = None;
    // The commit stamps it, as every edit is stamped.
    expected.version = after.version;
    expected.version_nonce = after.version_nonce;
    expected.updated = after.updated;
    assert_eq!(after, expected);
    assert!(engine.get_selection().contains(&"s".to_string()));
    assert_eq!(engine.get_tool(), DrawTool::Select);
}

#[test]
fn only_rectangles_diamonds_and_ellipses_switch() {
    let mut text = with_id(text_at(300.0, 0.0, 60.0, 25.0), "t");
    text.text = Some("hi".into());
    let elements = vec![with_id(box_at(0.0, 0.0, 100.0, 60.0), "s"), text];

    let mut engine = selected(elements.clone(), &["t"]);
    assert!(!engine.can_convert_selection());
    assert!(!engine.convert_selection(None, true));

    let mut engine = selected(elements.clone(), &["s", "t"]);
    assert!(engine.can_convert_selection());
    assert!(engine.convert_selection(None, true));
    assert_eq!(get(&engine, "s").kind, DrawElementType::Diamond);
    assert_eq!(get(&engine, "t"), elements[1]);
}

#[test]
fn a_switch_is_one_step_of_undo() {
    let mut engine = selected(vec![with_id(box_at(0.0, 0.0, 100.0, 60.0), "s")], &["s"]);
    engine.convert_selection(None, true);
    engine.undo();
    assert_eq!(get(&engine, "s").kind, DrawElementType::Rectangle);
}

/// Into the diamond, "hello world" may be 70 wide and 20 high: at 12 it is 70 wide.
/// Into the ellipse, 103 wide and 32 high: at 18 it is 103. The rectangle holds it at 20.
#[test]
fn a_label_shrinks_to_fit_and_comes_back_while_the_switch_is_open() {
    let mut engine = selected(labelled_rectangle(), &["s"]);
    engine.begin_conversion();
    let mut sizes = Vec::new();
    for _ in 0..3 {
        engine.convert_selection(None, true);
        sizes.push(get(&engine, "label").font_size);
    }
    assert_eq!(sizes, [Some(12.0), Some(18.0), Some(20.0)]);
    // Shrunk to fit, the shape did not have to grow.
    assert_close(get(&engine, "s").height, 60.0);
}

#[test]
fn with_the_switch_closed_a_shrunk_label_stays_shrunk() {
    let mut engine = selected(labelled_rectangle(), &["s"]);
    engine.begin_conversion();
    engine.convert_selection(None, true);
    engine.end_conversion();
    engine.convert_selection(Some(DrawElementType::Rectangle), true);
    assert_eq!(get(&engine, "label").font_size, Some(12.0));
}

/// A corner of the rectangle is well outside the ellipse, so the end moves to where the
/// ray from the centre (100, 50) toward it leaves the ellipse: (100, 50) + (100, -50)/√2.
#[test]
fn an_end_bound_at_a_corner_moves_onto_the_ellipse() {
    let mut engine = selected(arrow_into_b([1.0, 0.0]), &["b"]);
    engine.convert_selection(Some(DrawElementType::Ellipse), true);
    let [fx, fy] = get(&engine, "arrow").end_fixed_point.expect("still bound");
    let half = std::f64::consts::FRAC_1_SQRT_2 / 2.0;
    assert_close(fx, 0.5 + half);
    assert_close(fy, 0.5 - half);
    assert_eq!(get(&engine, "arrow").end_binding.as_deref(), Some("b"));
}

/// The top middle of the rectangle is on the ellipse too: that end stays where it was.
#[test]
fn an_end_still_on_the_new_outline_stays() {
    let mut engine = selected(arrow_into_b([0.5, 0.0]), &["b"]);
    engine.convert_selection(Some(DrawElementType::Ellipse), true);
    assert_eq!(get(&engine, "arrow").end_fixed_point, Some([0.5, 0.0]));
}
