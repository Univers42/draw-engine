mod common;
use common::*;
use draw_engine::*;

fn assert_resize_anchor(handle: HandleKind, target: Point, aspect: Option<f64>, angle: f64) {
    let mut element = box_at(20.0, 10.0, 80.0, 40.0);
    element.angle = angle;
    let before = handle_world(&element, opposite_handle(handle));
    let geom = resize_element(&element, handle, target.x, target.y, 4.0, aspect);
    let mut resized = element.clone();
    resized.x = geom.x;
    resized.y = geom.y;
    resized.width = geom.width;
    resized.height = geom.height;
    let after = handle_world(&resized, opposite_handle(handle));
    assert_point_close(before, after);
    assert!(geom.width >= 4.0);
    assert!(geom.height >= 4.0);
}

#[test]
fn resize_nw_handle() {
    assert_resize_anchor(HandleKind::Nw, Point { x: 0.0, y: -5.0 }, None, 0.0);
}

#[test]
fn resize_ne_handle() {
    assert_resize_anchor(HandleKind::Ne, Point { x: 120.0, y: -5.0 }, None, 0.0);
}

#[test]
fn resize_se_handle() {
    assert_resize_anchor(HandleKind::Se, Point { x: 130.0, y: 70.0 }, None, 0.0);
}

#[test]
fn resize_sw_handle() {
    assert_resize_anchor(HandleKind::Sw, Point { x: -10.0, y: 70.0 }, None, 0.0);
}

#[test]
fn resize_n_handle_preserves_width() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::N, 60.0, -10.0, 4.0, None);
    assert_close(geom.width, 80.0);
    assert_close(geom.height, 60.0);
}

#[test]
fn resize_s_handle_preserves_width() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::S, 60.0, 90.0, 4.0, None);
    assert_close(geom.width, 80.0);
    assert_close(geom.height, 80.0);
}

#[test]
fn resize_e_handle_preserves_height() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::E, 140.0, 30.0, 4.0, None);
    assert_close(geom.height, 40.0);
    assert_close(geom.width, 120.0);
}

#[test]
fn resize_w_handle_preserves_height() {
    let element = box_at(20.0, 10.0, 80.0, 40.0);
    let geom = resize_element(&element, HandleKind::W, -20.0, 30.0, 4.0, None);
    assert_close(geom.height, 40.0);
    assert_close(geom.width, 120.0);
}

#[test]
fn resize_clamped_to_min_size() {
    let element = box_at(50.0, 50.0, 60.0, 60.0);
    // Drag handle past opposite side:
    let geom = resize_element(&element, HandleKind::Se, 51.0, 51.0, 4.0, None);
    assert_close(geom.width, 4.0);
    assert_close(geom.height, 4.0);
}

#[test]
fn resize_aspect_ratio_lock_se() {
    let element = box_at(0.0, 0.0, 100.0, 50.0);
    let ratio = 100.0 / 50.0;
    let geom = resize_element(&element, HandleKind::Se, 200.0, 80.0, 4.0, Some(ratio));
    assert_close(geom.width / geom.height, ratio);
}

#[test]
fn resize_aspect_ratio_lock_nw() {
    let element = box_at(50.0, 50.0, 80.0, 40.0);
    let ratio = 80.0 / 40.0;
    let geom = resize_element(&element, HandleKind::Nw, 10.0, 20.0, 4.0, Some(ratio));
    assert_close(geom.width / geom.height, ratio);
}

#[test]
fn resize_rotated_element_30deg() {
    assert_resize_anchor(
        HandleKind::Se,
        Point { x: 120.0, y: 80.0 },
        None,
        std::f64::consts::PI / 6.0,
    );
}

#[test]
fn resize_rotated_element_90deg() {
    assert_resize_anchor(
        HandleKind::Nw,
        Point { x: -10.0, y: -20.0 },
        None,
        std::f64::consts::PI / 2.0,
    );
}

#[test]
fn resize_rotate_handle_is_noop() {
    let element = box_at(10.0, 20.0, 100.0, 50.0);
    let geom = resize_element(&element, HandleKind::Rotate, 50.0, -100.0, 4.0, None);
    assert_eq!(geom.x, 10.0);
    assert_eq!(geom.y, 20.0);
    assert_eq!(geom.width, 100.0);
    assert_eq!(geom.height, 50.0);
}

#[test]
fn rotate_handle_north_is_zero_angle() {
    let element = box_at(0.0, 0.0, 100.0, 100.0);
    let angle = rotate_element(&element, 50.0, -50.0);
    assert!(angle.abs() < EPS || (angle - std::f64::consts::PI * 2.0).abs() < EPS);
}

#[test]
fn rotate_handle_east_is_pi_half() {
    let element = box_at(0.0, 0.0, 100.0, 100.0);
    let angle = rotate_element(&element, 150.0, 50.0);
    assert_close(angle, std::f64::consts::PI / 2.0);
}

#[test]
fn hit_handle_detection() {
    let element = box_at(10.0, 10.0, 100.0, 60.0);
    let handles = selection_handle_points(&element, 20.0);
    assert_eq!(handles.len(), 9);

    let nw = &handles[0];
    assert_eq!(hit_handle(&handles, nw.x, nw.y, 5.0), Some(nw.kind));
    assert_eq!(hit_handle(&handles, 500.0, 500.0, 5.0), None);
}
