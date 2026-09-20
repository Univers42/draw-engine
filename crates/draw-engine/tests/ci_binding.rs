#![allow(clippy::cloned_ref_to_slice_refs)]
mod common;
use common::*;
use draw_engine::*;

#[test]
fn bindable_element_detection() {
    assert!(is_bindable_element(&box_at(0.0, 0.0, 50.0, 50.0)));
    assert!(is_bindable_element(&ellipse_at(0.0, 0.0, 50.0, 50.0)));
    assert!(is_bindable_element(&diamond_at(0.0, 0.0, 50.0, 50.0)));
    assert!(!is_bindable_element(&connector(
        0.0,
        0.0,
        10.0,
        10.0,
        DrawElementType::Arrow
    )));
    assert!(!is_bindable_element(&text_at(0.0, 0.0, 10.0, 10.0)));
}

#[test]
fn bindable_at_zorder_and_exclusion() {
    let under = box_at(0.0, 0.0, 80.0, 50.0);
    let over = box_at(10.0, 10.0, 80.0, 50.0);
    let scene = vec![under.clone(), over.clone()];

    assert_eq!(
        bindable_at(&scene, 20.0, 20.0, 0.0, None).map(|h| &h.id),
        Some(&over.id)
    );
    assert_eq!(
        bindable_at(&scene, 20.0, 20.0, 0.0, Some(&over.id)).map(|h| &h.id),
        Some(&under.id)
    );
}

#[test]
fn bindable_at_miss_outside() {
    let rect = box_at(10.0, 10.0, 50.0, 50.0);
    assert_eq!(bindable_at(&[rect], 5.0, 5.0, 0.0, None), None);
}

#[test]
fn bindable_at_with_tolerance() {
    let rect = box_at(10.0, 10.0, 50.0, 50.0);
    assert!(bindable_at(&[rect], 5.0, 15.0, 6.0, None).is_some());
}

#[test]
fn element_center_calculation() {
    let rect = box_at(20.0, 40.0, 100.0, 60.0);
    let c = element_center(&rect);
    assert_close(c.x, 70.0);
    assert_close(c.y, 70.0);
}

#[test]
fn attach_point_box_direction() {
    let rect = box_at(0.0, 0.0, 100.0, 100.0);
    let pt = attach_point(&rect, Point { x: 200.0, y: 50.0 }, 6.0);
    assert_close(pt.x, 106.0);
    assert_close(pt.y, 50.0);
}

#[test]
fn attach_point_ellipse_direction() {
    let el = ellipse_at(0.0, 0.0, 100.0, 100.0);
    let pt = attach_point(&el, Point { x: 50.0, y: 200.0 }, 6.0);
    assert_close(pt.x, 50.0);
    assert_close(pt.y, 106.0);
}

#[test]
fn refresh_bindings_both_ends_connected() {
    let left = box_at(0.0, 0.0, 100.0, 60.0);
    let right = box_at(300.0, 0.0, 100.0, 60.0);
    let mut link = connector(100.0, 30.0, 300.0, 30.0, DrawElementType::Arrow);
    link.start_binding = Some(left.id.clone());
    link.end_binding = Some(right.id.clone());

    let bound = refresh_bindings(&[left.clone(), right.clone(), link]);
    let (start, end) = linear_endpoints(&bound[2]);
    assert_close(start.x, left.x + left.width + BINDING_GAP);
    assert_close(end.x, right.x - BINDING_GAP);
}

#[test]
fn refresh_bindings_tracks_shape_translation() {
    let left = box_at(0.0, 0.0, 100.0, 60.0);
    let mut right = box_at(300.0, 0.0, 100.0, 60.0);
    let mut link = connector(100.0, 30.0, 300.0, 30.0, DrawElementType::Arrow);
    link.start_binding = Some(left.id.clone());
    link.end_binding = Some(right.id.clone());

    right.y += 150.0;
    let bound = refresh_bindings(&[left, right, link]);
    let (_, end) = linear_endpoints(&bound[2]);
    assert!(end.y > 50.0);
}

#[test]
fn refresh_bindings_single_end_connected() {
    let left = box_at(0.0, 0.0, 100.0, 60.0);
    let mut link = connector(100.0, 30.0, 400.0, 200.0, DrawElementType::Arrow);
    link.start_binding = Some(left.id.clone());

    let bound = refresh_bindings(&[left, link]);
    let (start, end) = linear_endpoints(&bound[1]);
    assert_close(end.x, 400.0);
    assert_close(end.y, 200.0);
    assert!(start.x > 0.0);
}

#[test]
fn refresh_bindings_deleted_shape_ignored() {
    let mut shape = box_at(0.0, 0.0, 100.0, 60.0);
    shape.is_deleted = true;
    let mut link = connector(0.0, 0.0, 200.0, 50.0, DrawElementType::Arrow);
    link.start_binding = Some(shape.id.clone());

    let bound = refresh_bindings(&[shape, link.clone()]);
    assert_eq!(linear_endpoints(&bound[1]), linear_endpoints(&link));
}

#[test]
fn layout_label_centers_on_box() {
    let container = box_at(50.0, 50.0, 200.0, 100.0);
    let mut label = text_at(0.0, 0.0, 40.0, 20.0);
    label.container_id = Some(container.id.clone());

    let positioned = layout_label(label, &container);
    assert_close(positioned.y, 50.0 + 50.0 - 10.0);
}

#[test]
fn layout_label_centers_on_connector() {
    let link = connector(0.0, 0.0, 200.0, 100.0, DrawElementType::Line);
    let label = text_at(0.0, 0.0, 60.0, 20.0);
    let positioned = layout_label(label, &link);
    assert_close(positioned.x, 100.0 - 30.0);
    assert_close(positioned.y, 50.0 - 10.0);
}

#[test]
fn default_arrowheads_resolution() {
    let arrow = connector(0.0, 0.0, 10.0, 0.0, DrawElementType::Arrow);
    assert_eq!(default_arrowhead(&arrow, "end"), Arrowhead::Arrow);
    assert_eq!(default_arrowhead(&arrow, "start"), Arrowhead::None);

    let line = connector(0.0, 0.0, 10.0, 0.0, DrawElementType::Line);
    assert_eq!(default_arrowhead(&line, "end"), Arrowhead::None);
}

#[test]
fn explicit_arrowhead_preservation() {
    let mut arrow = connector(0.0, 0.0, 10.0, 0.0, DrawElementType::Arrow);
    arrow.start_arrowhead = Some(Arrowhead::Dot);
    arrow.end_arrowhead = Some(Arrowhead::Diamond);
    assert_eq!(default_arrowhead(&arrow, "start"), Arrowhead::Dot);
    assert_eq!(default_arrowhead(&arrow, "end"), Arrowhead::Diamond);
}

#[test]
fn linear_degeneracy_checks() {
    assert!(!is_degenerate_linear(10.0, 0.0, 4.0));
    assert!(!is_degenerate_linear(0.0, 10.0, 4.0));
    assert!(is_degenerate_linear(2.0, 2.0, 4.0));
    assert!(is_degenerate_linear(0.0, 0.0, 4.0));
}
