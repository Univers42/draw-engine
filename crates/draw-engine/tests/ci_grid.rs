//! The canvas grid: a mode you opt into, sized in world units, that things land on.
//!
//! What it replaced was unconditional, hard-coded to a 40px step, uniform in weight, and
//! snapped to by nothing. A grid you cannot turn off, cannot resize and cannot align to
//! is decoration, not a tool.

mod common;
use common::*;
use draw_engine::*;

fn engine() -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine
}

#[test]
fn the_grid_is_off_until_asked_for() {
    let grid = GridSettings::default();
    assert!(!grid.enabled, "a grid is a mode, not the normal appearance");
    assert_eq!(grid.size, DEFAULT_GRID_SIZE);
    assert_eq!(grid.step, DEFAULT_GRID_STEP);
    assert_eq!(DEFAULT_GRID_SIZE, 20.0, "Excalidraw's default");
    assert_eq!(DEFAULT_GRID_STEP, 5, "Excalidraw's default");
}

#[test]
fn a_disabled_grid_changes_nothing() {
    let grid = GridSettings::default();
    assert_eq!(grid.snap_point(13.0, 47.0), (13.0, 47.0));
}

#[test]
fn snapping_rounds_to_the_nearest_intersection() {
    let grid = GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: true,
    };
    // Excalidraw's `getGridPoint`: round(v / size) * size.
    assert_eq!(grid.snap_point(0.0, 0.0), (0.0, 0.0));
    assert_eq!(grid.snap_point(9.0, 11.0), (0.0, 20.0));
    assert_eq!(grid.snap_point(-9.0, -11.0), (-0.0, -20.0));
    assert_eq!(grid.snap_point(30.0, 50.0), (40.0, 60.0));
}

/// Seeing a grid and being held to it are different requests.
#[test]
fn a_visible_grid_need_not_snap() {
    let grid = GridSettings {
        enabled: true,
        snap: false,
        ..GridSettings::default()
    };
    assert_eq!(grid.snap_point(13.0, 47.0), (13.0, 47.0));
}

/// A zero or negative size would divide by zero when snapping and loop forever when
/// drawing, so it falls back rather than reaching either.
#[test]
fn a_nonsense_size_falls_back() {
    for size in [0.0, -20.0, f64::NAN, 0.4] {
        let grid = GridSettings {
            enabled: true,
            size,
            step: 5,
            snap: true,
        };
        assert_eq!(grid.effective_size(), DEFAULT_GRID_SIZE, "size {size}");
        let (x, y) = grid.snap_point(31.0, 9.0);
        assert!(x.is_finite() && y.is_finite());
    }
}

/// The size is in world units, so the grid belongs to the drawing rather than to how far
/// you happen to be zoomed in — two elements snapped at different zooms land on the same
/// coordinates.
#[test]
fn snapping_is_independent_of_zoom() {
    let mut engine = engine();
    engine.set_grid(GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: true,
    });

    let draw_at = |engine: &mut DrawEngine, sx: f64, sy: f64| {
        engine.set_tool(DrawTool::Rectangle);
        engine.begin_pointer(sx, sy, false, false);
        engine.move_pointer(sx + 100.0, sy + 80.0, false, false);
        engine.end_pointer();
        let el = engine.get_scene().into_iter().last().unwrap();
        (el.x, el.y)
    };

    let at_100 = draw_at(&mut engine, 103.0, 97.0);
    engine.zoom_in();
    engine.zoom_in();
    assert!(engine.camera.scale > 1.0, "zoomed in");

    // The same *world* point, reached through a different camera.
    let screen = world_to_screen(engine.camera, 103.0, 97.0);
    let at_zoom = draw_at(&mut engine, screen.x, screen.y);

    assert_close(at_100.0, at_zoom.0);
    assert_close(at_100.1, at_zoom.1);
}

/// Everything that positions something has to go through the snap, or one gesture ends
/// up exempt and the grid only half works.
#[test]
fn drawing_and_dragging_both_land_on_the_grid() {
    let mut engine = engine();
    engine.set_grid(GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: true,
    });
    let on_grid = |v: f64| (v / 20.0).fract().abs() < 1e-9;

    // Drawn.
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(103.0, 97.0, false, false);
    engine.move_pointer(211.0, 189.0, false, false);
    engine.end_pointer();
    let drawn = engine.get_scene().into_iter().last().unwrap();
    assert!(on_grid(drawn.x), "x {} off grid", drawn.x);
    assert!(on_grid(drawn.y), "y {} off grid", drawn.y);
    assert!(on_grid(drawn.width), "width {} off grid", drawn.width);

    // Dragged.
    let id = drawn.id.clone();
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    engine.begin_pointer(drawn.x + 10.0, drawn.y + 10.0, false, false);
    engine.move_pointer(drawn.x + 77.0, drawn.y + 53.0, false, false);
    engine.end_pointer();
    let moved = engine.get_scene().into_iter().find(|e| e.id == id).unwrap();
    assert!(on_grid(moved.x), "moved x {} off grid", moved.x);
    assert!(on_grid(moved.y), "moved y {} off grid", moved.y);
}

/// With the grid off, nothing is quantised — the previous behaviour is unchanged for
/// anyone who never turns it on.
#[test]
fn the_grid_does_not_interfere_when_off() {
    let mut engine = engine();
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(103.0, 97.0, false, false);
    engine.move_pointer(211.0, 189.0, false, false);
    engine.end_pointer();
    let drawn = engine.get_scene().into_iter().last().unwrap();
    assert_close(drawn.x, 103.0);
    assert_close(drawn.y, 97.0);
}

#[test]
fn the_settings_round_trip_through_the_engine() {
    let mut engine = engine();
    let wanted = GridSettings {
        enabled: true,
        size: 32.0,
        step: 4,
        snap: false,
    };
    engine.set_grid(wanted);
    assert_eq!(engine.grid(), wanted);
}
