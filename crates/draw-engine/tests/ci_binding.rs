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

/// The user-reported case: a big shape drawn or raised *after* a small one it happens to
/// enclose must not shadow it. Z-order alone used to decide this — the first match while
/// walking topmost-first — so a container added last always won, for every point inside
/// it, however small and specific a shape sat underneath. The rule now is: real
/// per-shape geometry decides who matches at all, and among the matches the smallest
/// bounding-box area wins; z-order is only the tiebreak (see the two z-order tests
/// above, both still passing on equal-sized boxes).
#[test]
fn bindable_at_prefers_the_smaller_nested_shape_even_when_it_is_not_topmost() {
    let small = ellipse_at(150.0, 150.0, 50.0, 50.0);
    let big = box_at(0.0, 0.0, 400.0, 400.0);
    let (small_id, big_id) = (small.id.clone(), big.id.clone());
    // small first (bottom of z-order), big last (topmost) — the exact "circle drawn,
    // then framed by a rectangle around it" workflow.
    let scene = vec![small, big];

    assert_eq!(
        bindable_at(&scene, 175.0, 175.0, 0.0, None).map(|h| &h.id),
        Some(&small_id),
        "the small shape should win even though the big one is on top"
    );
    assert_ne!(
        bindable_at(&scene, 175.0, 175.0, 0.0, None).map(|h| h.id.clone()),
        Some(big_id),
        "the big container must not shadow what it encloses"
    );
}

/// The same guarantee, now confirmed for the ordering that already worked before this
/// fix (small shape topmost) — locking in that the new area comparison doesn't disturb
/// the case that was never broken.
#[test]
fn bindable_at_prefers_the_smaller_nested_shape_when_it_is_topmost() {
    let big = box_at(0.0, 0.0, 400.0, 400.0);
    let small = ellipse_at(150.0, 150.0, 50.0, 50.0);
    let small_id = small.id.clone();
    let scene = vec![big, small];

    assert_eq!(
        bindable_at(&scene, 175.0, 175.0, 0.0, None).map(|h| &h.id),
        Some(&small_id)
    );
}

/// A bounding box is not a shape. A point in the corner of an ellipse's bounding square,
/// outside its actual round outline, must miss — the raw AABB check this replaces would
/// have matched it.
#[test]
fn bindable_at_tests_the_real_outline_not_the_bounding_box() {
    let circle = ellipse_at(0.0, 0.0, 100.0, 100.0);
    // (95, 95): 0.9 out on both axes of a circle of radius 50 centred at (50, 50) —
    // 0.9² + 0.9² = 1.62 > 1, outside the circle, but trivially inside its 100×100
    // bounding square.
    assert_eq!(bindable_at(&[circle], 95.0, 95.0, 0.0, None), None);
}

/// Rotation was ignored entirely — the check ran against the element's raw, unrotated
/// `x`/`y`/`width`/`height` regardless of `angle`. A point can sit squarely inside a
/// rotated shape's true, visible outline while falling outside the stale axis-aligned
/// box the old check actually tested.
#[test]
fn bindable_at_accounts_for_rotation() {
    // A 100×20 rectangle centred at (50, 10), turned 90°: its true footprint is now
    // ~20 wide by ~100 tall, centred the same place — so (50, 50) sits inside the turned
    // shape while sitting well outside the untouched box's original y-range of [0, 20].
    let mut rotated = box_at(0.0, 0.0, 100.0, 20.0);
    rotated.angle = std::f64::consts::FRAC_PI_2;
    assert!(bindable_at(&[rotated], 50.0, 50.0, 0.0, None).is_some());
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
