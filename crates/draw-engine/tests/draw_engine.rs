#![allow(clippy::cloned_ref_to_slice_refs)]

use draw_engine::*;

fn box_at(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x,
            y,
            width: w,
            height: h,
        },
    )
}

#[test]
fn math_clamp_and_round_px() {
    assert_eq!(clamp(5.0, 0.0, 3.0), 3.0);
    assert_eq!(clamp(-1.0, 0.0, 3.0), 0.0);
    assert_eq!(round_px(2.4), 2.0);
    assert_eq!(round_px(2.6), 3.0);
}

#[test]
fn camera_screen_world_round_trips() {
    let camera = Camera {
        x: 120.0,
        y: -40.0,
        scale: 1.7,
    };
    for (wx, wy) in [(0.0, 0.0), (100.0, 50.0), (-30.0, 999.0)] {
        let screen = world_to_screen(camera, wx, wy);
        let world = screen_to_world(camera, screen.x, screen.y);
        assert!((world.x - wx).abs() < 1e-9);
        assert!((world.y - wy).abs() < 1e-9);
    }
}

#[test]
fn camera_zoom_at_keeps_cursor_world_point_fixed() {
    let camera = Camera {
        x: 0.0,
        y: 0.0,
        scale: 1.0,
    };
    let (sx, sy) = (200.0, 150.0);
    let before = screen_to_world(camera, sx, sy);
    let zoomed = zoom_at(camera, sx, sy, 2.0);
    let after = screen_to_world(zoomed, sx, sy);
    assert!((after.x - before.x).abs() < 1e-9);
    assert!((after.y - before.y).abs() < 1e-9);
    assert!(zoomed.scale > camera.scale);
}

#[test]
fn camera_zoom_clamps() {
    let mut camera = Camera {
        x: 0.0,
        y: 0.0,
        scale: 1.0,
    };
    for _ in 0..100 {
        camera = zoom_at(camera, 0.0, 0.0, 5.0);
    }
    assert!(camera.scale <= MAX_ZOOM + 1e-9);
    for _ in 0..200 {
        camera = zoom_at(camera, 0.0, 0.0, 0.1);
    }
    assert!(camera.scale >= MIN_ZOOM - 1e-9);
}

#[test]
fn camera_fit_bounds_centers() {
    let camera = fit_bounds(
        WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 100.0,
            max_y: 100.0,
        },
        800.0,
        600.0,
        0.0,
    );
    let center = world_to_screen(camera, 50.0, 50.0);
    assert!((center.x - 400.0).abs() < 1e-6);
    assert!((center.y - 300.0).abs() < 1e-6);
}

#[test]
fn element_create_and_bump_is_immutable() {
    let element = box_at(10.0, 20.0, 30.0, 40.0);
    assert_eq!(element.kind, DrawElementType::Rectangle);
    assert_eq!(element.x, 10.0);
    assert_eq!(element.width, 30.0);
    assert_eq!(element.opacity, 100.0);
    assert_eq!(element.version, 1);
    assert!(!element.is_deleted);

    let mut next = element.clone();
    next.x = 99.0;
    let next = bump_version(next, 5.0);
    assert_eq!(next.x, 99.0);
    assert_eq!(next.version, 2);
    assert_eq!(next.updated, 5.0);
    assert_eq!(element.x, 10.0);
}

#[test]
fn scene_zorder_tombstone_bring_to_front() {
    let a = box_at(0.0, 0.0, 10.0, 10.0);
    let b = create_element_default(
        DrawElementType::Ellipse,
        Geometry {
            x: 20.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
    );
    let mut scene = Scene::new([a.clone(), b.clone()]);
    assert_eq!(scene.size(), 2);
    assert_eq!(
        scene
            .ordered_cloned()
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>(),
        vec![a.id.clone(), b.id.clone()]
    );
    scene.bring_to_front(&a.id);
    assert_eq!(
        scene
            .ordered_cloned()
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>(),
        vec![b.id.clone(), a.id.clone()]
    );
    scene.remove(&b.id, 0.0);
    assert_eq!(scene.size(), 1);
    assert_eq!(scene.get(&b.id).map(|e| e.is_deleted), Some(true));
    assert_eq!(scene.to_array().len(), 2);
}

#[test]
fn geometry_normalize_and_bounds() {
    let rect = normalize_rect(50.0, 50.0, -20.0, -30.0);
    assert_eq!(
        rect,
        Rect {
            x: 30.0,
            y: 20.0,
            width: 20.0,
            height: 30.0
        }
    );
    let bounds = element_bounds(&box_at(5.0, 5.0, 10.0, 10.0));
    assert_eq!(
        bounds,
        WorldBounds {
            min_x: 5.0,
            min_y: 5.0,
            max_x: 15.0,
            max_y: 15.0
        }
    );
}

#[test]
fn geometry_hit_test_shape() {
    let rect = box_at(0.0, 0.0, 100.0, 100.0);
    let ellipse = create_element_default(
        DrawElementType::Ellipse,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    );
    let diamond = create_element_default(
        DrawElementType::Diamond,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    );
    // Given a background, because this is about which points each shape encloses, not
    // about the transparent-fill rule that `ci_hit_fill` covers: a transparent shape is
    // hit only on its outline, so leaving the default fill here would quietly turn this
    // into a test of something else.
    let mut rect = rect;
    let mut ellipse = ellipse;
    let mut diamond = diamond;
    for el in [&mut rect, &mut ellipse, &mut diamond] {
        el.background_color = "#ffec99".into();
    }
    assert!(hit_test_element(&rect, 5.0, 5.0, 0.0));
    assert!(!hit_test_element(&ellipse, 5.0, 5.0, 0.0));
    assert!(!hit_test_element(&diamond, 5.0, 5.0, 0.0));
    assert!(hit_test_element(&ellipse, 50.0, 50.0, 0.0));
    assert!(hit_test_element(&diamond, 50.0, 50.0, 0.0));
    assert!(!hit_test_element(&rect, 200.0, 200.0, 0.0));
}

#[test]
fn geometry_hit_test_topmost() {
    // Both given a background: this is about z-order, and a transparent shape would not
    // be hit in the middle at all.
    let mut bottom = box_at(0.0, 0.0, 100.0, 100.0);
    let mut top = box_at(0.0, 0.0, 100.0, 100.0);
    bottom.background_color = "#ffec99".into();
    top.background_color = "#ffec99".into();
    assert_eq!(
        hit_test(&[bottom.clone(), top.clone()], 50.0, 50.0, 0.0).map(|e| e.id.clone()),
        Some(top.id)
    );
    assert!(hit_test(&[], 50.0, 50.0, 0.0).is_none());
}

#[test]
fn geometry_scene_bounds() {
    let a = box_at(0.0, 0.0, 10.0, 10.0);
    let b = box_at(90.0, 40.0, 10.0, 10.0);
    assert_eq!(
        scene_bounds(&[a, b]),
        Some(WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 100.0,
            max_y: 50.0
        })
    );
    assert_eq!(scene_bounds(&[]), None);
}

#[test]
fn tools_hotkeys() {
    assert_eq!(tool_for_key("r"), Some(DrawTool::Rectangle));
    assert_eq!(tool_for_key("R"), Some(DrawTool::Rectangle));
    assert_eq!(tool_for_key("v"), Some(DrawTool::Select));
    assert_eq!(tool_for_key("1"), Some(DrawTool::Select));
    assert_eq!(tool_for_key("8"), Some(DrawTool::Text));
    assert_eq!(tool_for_key("e"), Some(DrawTool::Eraser));
    assert_eq!(tool_for_key("q"), None);
}

#[test]
fn tools_is_shape_tool() {
    assert!(is_shape_tool(DrawTool::Rectangle));
    assert!(is_shape_tool(DrawTool::Ellipse));
    assert!(is_shape_tool(DrawTool::Diamond));
    assert!(!is_shape_tool(DrawTool::Arrow));
    assert!(!is_shape_tool(DrawTool::Select));
    assert!(!is_shape_tool(DrawTool::Text));
}

#[test]
fn shape_drag_rect_from_drag() {
    assert_eq!(
        rect_from_drag(10.0, 10.0, 40.0, 30.0, false),
        Rect {
            x: 10.0,
            y: 10.0,
            width: 30.0,
            height: 20.0
        }
    );
    assert_eq!(
        rect_from_drag(40.0, 30.0, 10.0, 10.0, false),
        Rect {
            x: 10.0,
            y: 10.0,
            width: 30.0,
            height: 20.0
        }
    );
    assert_eq!(
        rect_from_drag(0.0, 0.0, 30.0, 10.0, true),
        Rect {
            x: 0.0,
            y: 0.0,
            width: 30.0,
            height: 30.0
        }
    );
    assert_eq!(
        rect_from_drag(0.0, 0.0, -10.0, -30.0, true),
        Rect {
            x: -30.0,
            y: -30.0,
            width: 30.0,
            height: 30.0
        }
    );
}

#[test]
fn shape_drag_degenerate() {
    assert!(is_degenerate_rect(
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0
        },
        2.0
    ));
    assert!(!is_degenerate_rect(
        Rect {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 1.0
        },
        2.0
    ));
}

fn handle_world(el: &DrawElement, kind: HandleKind) -> Point {
    let cx = el.x + el.width / 2.0;
    let cy = el.y + el.height / 2.0;
    let local = handle_local_point(kind, el.width / 2.0, el.height / 2.0);
    let c = el.angle.cos();
    let s = el.angle.sin();
    Point {
        x: cx + (local.x * c - local.y * s),
        y: cy + (local.x * s + local.y * c),
    }
}

#[test]
fn handles_local_offsets() {
    assert_eq!(
        handle_local_point(HandleKind::Nw, 50.0, 30.0),
        Point { x: -50.0, y: -30.0 }
    );
    assert_eq!(
        handle_local_point(HandleKind::Se, 50.0, 30.0),
        Point { x: 50.0, y: 30.0 }
    );
    assert_eq!(
        handle_local_point(HandleKind::N, 50.0, 30.0),
        Point { x: 0.0, y: -30.0 }
    );
    let el = box_at(0.0, 0.0, 100.0, 60.0);
    let points = selection_handle_points(&el, 20.0);
    assert_eq!(points.len(), 9);
    let se = points.iter().find(|p| p.kind == HandleKind::Se).unwrap();
    assert_eq!((se.x, se.y), (100.0, 60.0));
    let rotate = points
        .iter()
        .find(|p| p.kind == HandleKind::Rotate)
        .unwrap();
    assert_eq!((rotate.x, rotate.y), (50.0, -20.0));
}

#[test]
fn handles_hit() {
    let el = box_at(0.0, 0.0, 100.0, 60.0);
    let points = selection_handle_points(&el, 20.0);
    assert_eq!(hit_handle(&points, 100.0, 60.0, 6.0), Some(HandleKind::Se));
    assert_eq!(
        hit_handle(&points, 50.0, -20.0, 6.0),
        Some(HandleKind::Rotate)
    );
    assert_eq!(hit_handle(&points, 50.0, 30.0, 6.0), None);
}

#[test]
fn transform_resize_axis_aligned() {
    let el = box_at(0.0, 0.0, 100.0, 60.0);
    let geom = resize_element(&el, HandleKind::Se, 150.0, 90.0, 1.0, None);
    assert_eq!(
        (geom.x, geom.y, geom.width, geom.height),
        (0.0, 0.0, 150.0, 90.0)
    );
}

#[test]
fn transform_resize_rotated_keeps_anchor() {
    let mut el = box_at(0.0, 0.0, 100.0, 60.0);
    el.angle = std::f64::consts::PI / 6.0;
    let before = handle_world(&el, HandleKind::Nw);
    let geom = resize_element(&el, HandleKind::Se, 200.0, 140.0, 1.0, None);
    el.x = geom.x;
    el.y = geom.y;
    el.width = geom.width;
    el.height = geom.height;
    let after = handle_world(&el, HandleKind::Nw);
    assert!((after.x - before.x).abs() < 1e-9);
    assert!((after.y - before.y).abs() < 1e-9);
}

#[test]
fn transform_resize_min_size() {
    let el = box_at(0.0, 0.0, 100.0, 60.0);
    let geom = resize_element(&el, HandleKind::Se, 0.0, 0.0, 1.0, None);
    assert!(geom.width >= 1.0 && geom.height >= 1.0);
}

#[test]
fn transform_rotate_handle() {
    let el = box_at(0.0, 0.0, 100.0, 60.0);
    assert!((rotate_element(&el, 50.0, -100.0) - 0.0).abs() < 1e-9);
    assert!((rotate_element(&el, 150.0, 30.0) - std::f64::consts::PI / 2.0).abs() < 1e-9);
}

#[test]
fn marquee_selects_overlapping() {
    assert_eq!(
        marquee_rect(40.0, 60.0, 10.0, 20.0),
        WorldBounds {
            min_x: 10.0,
            min_y: 20.0,
            max_x: 40.0,
            max_y: 60.0
        }
    );
    let a = box_at(0.0, 0.0, 20.0, 20.0);
    let b = box_at(200.0, 200.0, 20.0, 20.0);
    let inside = elements_in_marquee(
        &[a.clone(), b.clone()],
        marquee_rect(-5.0, -5.0, 50.0, 50.0),
    );
    assert_eq!(inside, vec![a.id.clone()]);
    assert!(elements_in_marquee(&[a, b], marquee_rect(500.0, 500.0, 600.0, 600.0)).is_empty());
}

#[test]
fn freehand_points_bounds() {
    assert_eq!(
        points_bounds(&[[0.0, 0.0], [10.0, -4.0], [6.0, 8.0]]),
        [0.0, -4.0, 10.0, 8.0]
    );
    assert_eq!(points_bounds(&[]), [0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn history_undo_redo_dedupe() {
    let mut history = SnapshotHistory::new(
        Vec::<i32>::new(),
        |value: &Vec<i32>| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            value.hash(&mut h);
            h.finish()
        },
        200,
    );
    history.push(vec![1]);
    history.push(vec![1, 2]);
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo(), Some(&vec![1]));
    assert_eq!(history.undo(), Some(&Vec::<i32>::new()));
    assert_eq!(history.undo(), None);
    assert_eq!(history.redo(), Some(&vec![1]));
    history.push(vec![1, 2, 3]);
    assert!(!history.can_redo());
    assert_eq!(history.undo(), Some(&vec![1]));
    history.redo();
    history.push(vec![1, 2, 3]);
    assert!(!history.can_redo());
}

#[test]
fn export_json_round_trip() {
    let rect = box_at(0.0, 0.0, 10.0, 10.0);
    let json = scene_to_json(&[rect.clone()]);
    assert!(json.contains("\"type\": \"osidraw\""));
    let back = elements_from_json(&json).unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].id, rect.id);
    assert!(elements_from_json("not json").is_none());
    assert!(elements_from_json("{\"type\":\"other\"}").is_none());
}

#[test]
fn export_json_drops_tombstones() {
    let a = box_at(0.0, 0.0, 10.0, 10.0);
    let mut b = create_element_default(
        DrawElementType::Ellipse,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
    );
    b.is_deleted = true;
    assert_eq!(
        elements_from_json(&scene_to_json(&[a, b])).unwrap().len(),
        1
    );
}

#[test]
fn export_svg_emits_primitives() {
    let rect = box_at(0.0, 0.0, 40.0, 20.0);
    let ellipse = create_element_default(
        DrawElementType::Ellipse,
        Geometry {
            x: 100.0,
            y: 0.0,
            width: 40.0,
            height: 20.0,
        },
    );
    let svg = scene_to_svg(
        &[rect, ellipse],
        WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 140.0,
            max_y: 20.0,
        },
        8.0,
        "#ffffff",
    );
    assert!(svg.starts_with("<svg "));
    assert!(svg.contains("<rect "));
    assert!(svg.contains("<ellipse "));
    assert!(svg.contains("</svg>"));
}

#[test]
fn engine_draft_rectangle_commits() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(10.0, 10.0, false, false);
    engine.move_pointer(50.0, 40.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_tool(), DrawTool::Select);
    assert_eq!(engine.get_selected_elements().len(), 1);
    let el = &engine.get_selected_elements()[0];
    assert_eq!(el.kind, DrawElementType::Rectangle);
    assert!((el.width - 40.0).abs() < 1e-9);
    assert!((el.height - 30.0).abs() < 1e-9);
}
