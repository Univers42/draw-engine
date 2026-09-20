mod common;
use common::*;
use draw_engine::*;

#[test]
fn snap_move_no_statics_yields_zero_delta() {
    let moving = WorldBounds {
        min_x: 10.0,
        min_y: 10.0,
        max_x: 50.0,
        max_y: 50.0,
    };
    let res = snap_move(moving, &[], 5.0);
    assert_close(res.dx, 0.0);
    assert_close(res.dy, 0.0);
    assert!(res.guides.is_empty());
}

#[test]
fn snap_move_left_edge_aligns() {
    let target = WorldBounds {
        min_x: 100.0,
        min_y: 0.0,
        max_x: 150.0,
        max_y: 50.0,
    };
    let moving = WorldBounds {
        min_x: 98.0,
        min_y: 100.0,
        max_x: 138.0,
        max_y: 140.0,
    };
    let res = snap_move(moving, &[target], 4.0);
    assert_close(res.dx, 2.0);
}

#[test]
fn snap_move_center_x_aligns() {
    let target = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 100.0,
    }; // center is 50
    let moving = WorldBounds {
        min_x: 29.0,
        min_y: 200.0,
        max_x: 69.0,
        max_y: 240.0,
    }; // center is 49
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dx, 1.0);
}

#[test]
fn snap_move_right_edge_aligns() {
    let target = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 50.0,
    };
    let moving = WorldBounds {
        min_x: 63.0,
        min_y: 100.0,
        max_x: 103.0,
        max_y: 140.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dx, -3.0);
}

#[test]
fn snap_move_top_edge_aligns() {
    let target = WorldBounds {
        min_x: 0.0,
        min_y: 100.0,
        max_x: 50.0,
        max_y: 150.0,
    };
    let moving = WorldBounds {
        min_x: 100.0,
        min_y: 97.0,
        max_x: 140.0,
        max_y: 137.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dy, 3.0);
}

#[test]
fn snap_move_center_y_aligns() {
    let target = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 50.0,
        max_y: 100.0,
    }; // center y = 50
    let moving = WorldBounds {
        min_x: 100.0,
        min_y: 32.0,
        max_x: 140.0,
        max_y: 72.0,
    }; // center y = 52
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dy, -2.0);
}

#[test]
fn snap_move_bottom_edge_aligns() {
    let target = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 50.0,
        max_y: 200.0,
    };
    let moving = WorldBounds {
        min_x: 100.0,
        min_y: 158.0,
        max_x: 140.0,
        max_y: 198.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dy, 2.0);
}

#[test]
fn snap_move_both_axes_simultaneous_snap() {
    let target = WorldBounds {
        min_x: 100.0,
        min_y: 100.0,
        max_x: 200.0,
        max_y: 200.0,
    };
    let moving = WorldBounds {
        min_x: 98.0,
        min_y: 102.0,
        max_x: 148.0,
        max_y: 152.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dx, 2.0);
    assert_close(res.dy, -2.0);
    assert_eq!(res.guides.len(), 2);
}

#[test]
fn snap_move_outside_threshold_ignored() {
    let target = WorldBounds {
        min_x: 100.0,
        min_y: 100.0,
        max_x: 200.0,
        max_y: 200.0,
    };
    let moving = WorldBounds {
        min_x: 800.0,
        min_y: 800.0,
        max_x: 840.0,
        max_y: 840.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dx, 0.0);
    assert_close(res.dy, 0.0);
    assert!(res.guides.is_empty());
}

#[test]
fn snap_move_within_threshold_snaps() {
    let target = WorldBounds {
        min_x: 100.0,
        min_y: 100.0,
        max_x: 200.0,
        max_y: 200.0,
    };
    let moving = WorldBounds {
        min_x: 96.0,
        min_y: 0.0,
        max_x: 136.0,
        max_y: 40.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    assert_close(res.dx, 4.0);
}

#[test]
fn snap_move_picks_closest_target() {
    let a = WorldBounds {
        min_x: 100.0,
        min_y: 0.0,
        max_x: 150.0,
        max_y: 50.0,
    }; // delta = 3
    let b = WorldBounds {
        min_x: 102.0,
        min_y: 0.0,
        max_x: 152.0,
        max_y: 50.0,
    }; // delta = 1
    let moving = WorldBounds {
        min_x: 103.0,
        min_y: 100.0,
        max_x: 143.0,
        max_y: 140.0,
    };
    let res = snap_move(moving, &[a, b], 5.0);
    assert_close(res.dx, -1.0); // closest is b
}

#[test]
fn snap_move_creates_x_axis_guide() {
    let target = WorldBounds {
        min_x: 100.0,
        min_y: 0.0,
        max_x: 150.0,
        max_y: 50.0,
    };
    let moving = WorldBounds {
        min_x: 99.0,
        min_y: 100.0,
        max_x: 140.0,
        max_y: 140.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    let x_guide = res.guides.iter().find(|g| g.axis == Axis::X);
    assert!(x_guide.is_some());
    assert_close(x_guide.unwrap().at, 100.0);
}

#[test]
fn snap_move_creates_y_axis_guide() {
    let target = WorldBounds {
        min_x: 0.0,
        min_y: 100.0,
        max_x: 50.0,
        max_y: 150.0,
    };
    let moving = WorldBounds {
        min_x: 200.0,
        min_y: 99.0,
        max_x: 240.0,
        max_y: 140.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    let y_guide = res.guides.iter().find(|g| g.axis == Axis::Y);
    assert!(y_guide.is_some());
    assert_close(y_guide.unwrap().at, 100.0);
}

#[test]
fn snap_move_guide_spans_both_shapes() {
    let target = WorldBounds {
        min_x: 0.0,
        min_y: 50.0,
        max_x: 50.0,
        max_y: 100.0,
    };
    let moving = WorldBounds {
        min_x: 1.0,
        min_y: 200.0,
        max_x: 40.0,
        max_y: 250.0,
    };
    let res = snap_move(moving, &[target], 5.0);
    let guide = &res.guides[0];
    assert!(guide.from <= 50.0);
    assert!(guide.to >= 250.0);
}

#[test]
fn snap_move_constrain_to_angle_utility() {
    let (cx, cy) = constrain_to_angle(10.0, 9.0);
    assert_close(cx.abs(), cy.abs());
}
