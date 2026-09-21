mod common;
use common::*;
use draw_engine::*;

#[test]
fn normalize_rect_both_positive() {
    let r = normalize_rect(10.0, 20.0, 100.0, 50.0);
    assert_rect_close(
        r,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        },
    );
}

#[test]
fn normalize_rect_negative_width_only() {
    let r = normalize_rect(50.0, 20.0, -40.0, 30.0);
    assert_rect_close(
        r,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 40.0,
            height: 30.0,
        },
    );
}

#[test]
fn normalize_rect_negative_height_only() {
    let r = normalize_rect(10.0, 80.0, 60.0, -50.0);
    assert_rect_close(
        r,
        Rect {
            x: 10.0,
            y: 30.0,
            width: 60.0,
            height: 50.0,
        },
    );
}

#[test]
fn normalize_rect_both_negative() {
    let r = normalize_rect(100.0, 100.0, -60.0, -40.0);
    assert_rect_close(
        r,
        Rect {
            x: 40.0,
            y: 60.0,
            width: 60.0,
            height: 40.0,
        },
    );
}

#[test]
fn normalize_rect_zero_width() {
    let r = normalize_rect(25.0, 30.0, 0.0, 50.0);
    assert_rect_close(
        r,
        Rect {
            x: 25.0,
            y: 30.0,
            width: 0.0,
            height: 50.0,
        },
    );
}

#[test]
fn normalize_rect_zero_height() {
    let r = normalize_rect(15.0, 45.0, 80.0, 0.0);
    assert_rect_close(
        r,
        Rect {
            x: 15.0,
            y: 45.0,
            width: 80.0,
            height: 0.0,
        },
    );
}

#[test]
fn element_bounds_rectangle() {
    let rect = box_at(20.0, 30.0, 70.0, 50.0);
    let bounds = element_bounds(&rect);
    assert_close(bounds.min_x, 20.0);
    assert_close(bounds.min_y, 30.0);
    assert_close(bounds.max_x, 90.0);
    assert_close(bounds.max_y, 80.0);
}

#[test]
fn element_bounds_ellipse() {
    let el = ellipse_at(-10.0, -20.0, 40.0, 60.0);
    let bounds = element_bounds(&el);
    assert_close(bounds.min_x, -10.0);
    assert_close(bounds.min_y, -20.0);
    assert_close(bounds.max_x, 30.0);
    assert_close(bounds.max_y, 40.0);
}

#[test]
fn element_bounds_diamond() {
    let dia = diamond_at(0.0, 0.0, 100.0, 80.0);
    let bounds = element_bounds(&dia);
    assert_close(bounds.min_x, 0.0);
    assert_close(bounds.min_y, 0.0);
    assert_close(bounds.max_x, 100.0);
    assert_close(bounds.max_y, 80.0);
}

#[test]
fn hit_test_rectangle_exact_center() {
    let rect = filled(box_at(10.0, 10.0, 100.0, 60.0));
    assert!(hit_test_element(&rect, 60.0, 40.0, 0.0));
}

#[test]
fn hit_test_rectangle_outside_margin() {
    let rect = box_at(10.0, 10.0, 100.0, 60.0);
    assert!(!hit_test_element(&rect, 5.0, 40.0, 0.0));
    assert!(!hit_test_element(&rect, 115.0, 40.0, 0.0));
}

#[test]
fn hit_test_ellipse_outside_corner() {
    let el = filled(ellipse_at(0.0, 0.0, 100.0, 100.0));
    assert!(hit_test_element(&el, 50.0, 50.0, 0.0));
    // The point (5, 5) is inside the bounding box [0, 100]x[0, 100], but outside the inscribed ellipse:
    assert!(!hit_test_element(&el, 5.0, 5.0, 0.0));
}

#[test]
fn hit_test_diamond_outside_corner() {
    let dia = filled(diamond_at(0.0, 0.0, 100.0, 100.0));
    assert!(hit_test_element(&dia, 50.0, 50.0, 0.0));
    // The point (10, 10) is inside the bounding box, but outside the inscribed diamond:
    assert!(!hit_test_element(&dia, 10.0, 10.0, 0.0));
}

#[test]
fn hit_test_with_tolerance_padding() {
    let rect = filled(box_at(20.0, 20.0, 60.0, 40.0));
    assert!(!hit_test_element(&rect, 15.0, 30.0, 0.0));
    // With 10.0 padding, 15.0 is within reach (distance is 5.0):
    assert!(hit_test_element(&rect, 15.0, 30.0, 10.0));
}

#[test]
fn scene_bounds_empty_is_none() {
    assert_eq!(scene_bounds(&[]), None);
}

#[test]
fn scene_bounds_single_element() {
    let rect = box_at(15.0, 25.0, 50.0, 70.0);
    let bounds = scene_bounds(&[rect]).unwrap();
    assert_close(bounds.min_x, 15.0);
    assert_close(bounds.min_y, 25.0);
    assert_close(bounds.max_x, 65.0);
    assert_close(bounds.max_y, 95.0);
}

#[test]
fn scene_bounds_disjoint_elements() {
    let a = box_at(0.0, 0.0, 10.0, 10.0);
    let b = box_at(100.0, 200.0, 50.0, 50.0);
    let bounds = scene_bounds(&[a, b]).unwrap();
    assert_close(bounds.min_x, 0.0);
    assert_close(bounds.min_y, 0.0);
    assert_close(bounds.max_x, 150.0);
    assert_close(bounds.max_y, 250.0);
}
