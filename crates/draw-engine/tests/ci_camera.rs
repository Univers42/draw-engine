mod common;
use common::*;
use draw_engine::*;

fn assert_camera_roundtrip(camera: Camera, world: Point) {
    let screen = world_to_screen(camera, world.x, world.y);
    let back = screen_to_world(camera, screen.x, screen.y);
    assert_point_close(back, world);
}

#[test]
fn default_viewport_identity_transform() {
    assert_camera_roundtrip(IDENTITY, Point { x: 0.0, y: 0.0 });
    assert_camera_roundtrip(IDENTITY, Point { x: 120.0, y: 250.0 });
}

#[test]
fn negative_screen_coordinates_roundtrip() {
    let camera = Camera {
        x: -300.0,
        y: -450.0,
        scale: 1.25,
    };
    assert_camera_roundtrip(
        camera,
        Point {
            x: -80.0,
            y: -120.0,
        },
    );
}

#[test]
fn large_world_offset_roundtrip() {
    let camera = Camera {
        x: 1000.0,
        y: -2000.0,
        scale: 0.75,
    };
    assert_camera_roundtrip(
        camera,
        Point {
            x: 50_000.0,
            y: -80_000.0,
        },
    );
}

#[test]
fn subpixel_fractional_roundtrip() {
    let camera = Camera {
        x: 12.3456,
        y: -78.9101,
        scale: 1.4567,
    };
    assert_camera_roundtrip(
        camera,
        Point {
            x: 3.45678,
            y: 2.34567,
        },
    );
}

#[test]
fn pan_zero_delta_is_noop() {
    let camera = Camera {
        x: 50.0,
        y: 60.0,
        scale: 1.5,
    };
    let panned = pan_by(camera, 0.0, 0.0);
    assert_eq!(camera, panned);
}

#[test]
fn pan_negative_displacement() {
    let camera = Camera {
        x: 100.0,
        y: 200.0,
        scale: 2.0,
    };
    let panned = pan_by(camera, -40.0, -90.0);
    assert_close(panned.x, 60.0);
    assert_close(panned.y, 110.0);
    assert_close(panned.scale, 2.0);
}

#[test]
fn pan_cumulative_sequence() {
    let mut camera = IDENTITY;
    for step in 1..=5 {
        camera = pan_by(camera, step as f64 * 2.0, -(step as f64));
    }
    assert_close(camera.x, 30.0);
    assert_close(camera.y, -15.0);
}

#[test]
fn zoom_at_origin_preserves_anchor() {
    let camera = Camera {
        x: 0.0,
        y: 0.0,
        scale: 1.0,
    };
    let screen = Point { x: 0.0, y: 0.0 };
    let zoomed = zoom_at(camera, screen.x, screen.y, 2.0);
    assert_point_close(
        screen_to_world(zoomed, screen.x, screen.y),
        Point { x: 0.0, y: 0.0 },
    );
}

#[test]
fn zoom_at_arbitrary_cursor_preserves_anchor() {
    let camera = Camera {
        x: -80.0,
        y: 120.0,
        scale: 0.8,
    };
    let cursor = Point { x: 350.0, y: 220.0 };
    let world_before = screen_to_world(camera, cursor.x, cursor.y);
    let zoomed = zoom_at(camera, cursor.x, cursor.y, 1.75);
    let world_after = screen_to_world(zoomed, cursor.x, cursor.y);
    assert_point_close(world_before, world_after);
}

#[test]
fn zoom_sequence_in_then_out_preserves_world_position() {
    let camera = Camera {
        x: 40.0,
        y: -30.0,
        scale: 1.0,
    };
    let cursor = Point { x: 400.0, y: 300.0 };
    let target = Point { x: 150.0, y: 75.0 };
    let screen_initial = world_to_screen(camera, target.x, target.y);

    let zoomed_in = zoom_at(camera, cursor.x, cursor.y, 2.0);
    let zoomed_back = zoom_at(zoomed_in, cursor.x, cursor.y, 0.5);
    let screen_final = world_to_screen(zoomed_back, target.x, target.y);
    assert_point_close(screen_initial, screen_final);
}

#[test]
fn zoom_upper_clamping_limit() {
    let mut camera = IDENTITY;
    for _ in 0..20 {
        camera = zoom_at(camera, 100.0, 100.0, 3.0);
    }
    assert!(camera.scale <= MAX_ZOOM + EPS);
    assert_close(camera.scale, MAX_ZOOM);
}

#[test]
fn zoom_lower_clamping_limit() {
    let mut camera = IDENTITY;
    for _ in 0..20 {
        camera = zoom_at(camera, 100.0, 100.0, 0.2);
    }
    assert!(camera.scale >= MIN_ZOOM - EPS);
    assert_close(camera.scale, MIN_ZOOM);
}

#[test]
fn zoom_to_direct_scale_assignment() {
    let camera = Camera {
        x: 10.0,
        y: 20.0,
        scale: 1.0,
    };
    let zoomed = zoom_to(camera, 200.0, 150.0, 2.5);
    assert_close(zoomed.scale, 2.5);
    assert_point_close(
        screen_to_world(camera, 200.0, 150.0),
        screen_to_world(zoomed, 200.0, 150.0),
    );
}

#[test]
fn fit_bounds_zero_width_vertical_line() {
    let line = WorldBounds {
        min_x: 50.0,
        min_y: 0.0,
        max_x: 50.0,
        max_y: 200.0,
    };
    let camera = fit_bounds(line, 800.0, 600.0, 20.0);
    let center = world_to_screen(camera, 50.0, 100.0);
    assert_point_close(center, Point { x: 400.0, y: 300.0 });
}

#[test]
fn fit_bounds_zero_height_horizontal_line() {
    let line = WorldBounds {
        min_x: 0.0,
        min_y: 80.0,
        max_x: 400.0,
        max_y: 80.0,
    };
    let camera = fit_bounds(line, 800.0, 600.0, 20.0);
    let center = world_to_screen(camera, 200.0, 80.0);
    assert_point_close(center, Point { x: 400.0, y: 300.0 });
}

#[test]
fn visible_world_rect_matches_viewport() {
    let camera = Camera {
        x: 100.0,
        y: 50.0,
        scale: 2.0,
    };
    let bounds = visible_world_rect(camera, 800.0, 600.0);
    let top_left = world_to_screen(camera, bounds.min_x, bounds.min_y);
    let bottom_right = world_to_screen(camera, bounds.max_x, bounds.max_y);
    assert_point_close(top_left, Point { x: 0.0, y: 0.0 });
    assert_point_close(bottom_right, Point { x: 800.0, y: 600.0 });
}
