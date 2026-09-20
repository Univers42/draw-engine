#![allow(clippy::cloned_ref_to_slice_refs)]

use std::collections::HashSet;

use draw_engine::*;

const EPS: f64 = 1e-9;

fn assert_close(a: f64, b: f64) {
    assert!((a - b).abs() < EPS, "expected {a} ~= {b}");
}

fn assert_point_close(a: Point, b: Point) {
    assert_close(a.x, b.x);
    assert_close(a.y, b.y);
}

fn assert_rect_close(a: Rect, b: Rect) {
    assert_close(a.x, b.x);
    assert_close(a.y, b.y);
    assert_close(a.width, b.width);
    assert_close(a.height, b.height);
}

fn box_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x,
            y,
            width,
            height,
        },
    )
}

fn ellipse_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Ellipse,
        Geometry {
            x,
            y,
            width,
            height,
        },
    )
}

fn diamond_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Diamond,
        Geometry {
            x,
            y,
            width,
            height,
        },
    )
}

fn text_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    let mut element = create_element_default(
        DrawElementType::Text,
        Geometry {
            x,
            y,
            width,
            height,
        },
    );
    element.text = Some(String::new());
    element.font_size = Some(20.0);
    element
}

fn connector(x1: f64, y1: f64, x2: f64, y2: f64, kind: DrawElementType) -> DrawElement {
    let mut element = create_element_default(
        kind,
        Geometry {
            x: x1,
            y: y1,
            width: x2 - x1,
            height: y2 - y1,
        },
    );
    element.points = Some(vec![[0.0, 0.0], [x2 - x1, y2 - y1]]);
    element
}

fn ids(elements: &[DrawElement]) -> HashSet<String> {
    elements.iter().map(|element| element.id.clone()).collect()
}

fn opposite_handle(handle: HandleKind) -> HandleKind {
    match handle {
        HandleKind::Nw => HandleKind::Se,
        HandleKind::N => HandleKind::S,
        HandleKind::Ne => HandleKind::Sw,
        HandleKind::E => HandleKind::W,
        HandleKind::Se => HandleKind::Nw,
        HandleKind::S => HandleKind::N,
        HandleKind::Sw => HandleKind::Ne,
        HandleKind::W => HandleKind::E,
        HandleKind::Rotate => HandleKind::Rotate,
    }
}

fn handle_world(element: &DrawElement, handle: HandleKind) -> Point {
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let local = handle_local_point(handle, element.width / 2.0, element.height / 2.0);
    let cos = element.angle.cos();
    let sin = element.angle.sin();
    Point {
        x: cx + (local.x * cos - local.y * sin),
        y: cy + (local.x * sin + local.y * cos),
    }
}

fn engine_with_scene(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_scene(Scene::new(elements));
    engine
}

fn engine_with_measure(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = engine_with_scene(elements);
    engine.set_measure_text(measure_text);
    engine
}

fn measure_text(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.len() as f64 * font_size * 0.5 + 4.0)
        .fold(0.0, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * TEXT_LINE_HEIGHT).max(font_size),
    )
}

fn stroke_patch(color: &str) -> DrawElementStylePatch {
    DrawElementStylePatch {
        stroke_color: Some(color.to_string()),
        ..Default::default()
    }
}

fn deleted_count(elements: &[DrawElement]) -> usize {
    elements.iter().filter(|element| element.is_deleted).count()
}

macro_rules! case_suite {
    ($runner:ident) => {
        #[test]
        fn case_00() {
            $runner(0);
        }
        #[test]
        fn case_01() {
            $runner(1);
        }
        #[test]
        fn case_02() {
            $runner(2);
        }
        #[test]
        fn case_03() {
            $runner(3);
        }
        #[test]
        fn case_04() {
            $runner(4);
        }
        #[test]
        fn case_05() {
            $runner(5);
        }
        #[test]
        fn case_06() {
            $runner(6);
        }
        #[test]
        fn case_07() {
            $runner(7);
        }
        #[test]
        fn case_08() {
            $runner(8);
        }
        #[test]
        fn case_09() {
            $runner(9);
        }
        #[test]
        fn case_10() {
            $runner(10);
        }
        #[test]
        fn case_11() {
            $runner(11);
        }
        #[test]
        fn case_12() {
            $runner(12);
        }
        #[test]
        fn case_13() {
            $runner(13);
        }
        #[test]
        fn case_14() {
            $runner(14);
        }
        #[test]
        fn case_15() {
            $runner(15);
        }
        #[test]
        fn case_16() {
            $runner(16);
        }
        #[test]
        fn case_17() {
            $runner(17);
        }
        #[test]
        fn case_18() {
            $runner(18);
        }
        #[test]
        fn case_19() {
            $runner(19);
        }
        #[test]
        fn case_20() {
            $runner(20);
        }
        #[test]
        fn case_21() {
            $runner(21);
        }
        #[test]
        fn case_22() {
            $runner(22);
        }
        #[test]
        fn case_23() {
            $runner(23);
        }
        #[test]
        fn case_24() {
            $runner(24);
        }
        #[test]
        fn case_25() {
            $runner(25);
        }
        #[test]
        fn case_26() {
            $runner(26);
        }
        #[test]
        fn case_27() {
            $runner(27);
        }
        #[test]
        fn case_28() {
            $runner(28);
        }
        #[test]
        fn case_29() {
            $runner(29);
        }
    };
}

mod camera_matrix {
    use super::*;

    fn run_case(index: usize) {
        let camera = Camera {
            x: -180.0 + index as f64 * 11.0,
            y: 140.0 - index as f64 * 7.0,
            scale: 0.35 + (index % 7) as f64 * 0.45,
        };
        let world = Point {
            x: index as f64 * 1.75 - 25.0,
            y: index as f64 * -2.5 + 42.0,
        };
        let screen = world_to_screen(camera, world.x, world.y);
        assert_point_close(screen_to_world(camera, screen.x, screen.y), world);

        let factor = [0.5, 0.8, 1.25, 1.5][index % 4];
        let zoomed = zoom_at(camera, screen.x, screen.y, factor);
        assert_point_close(screen_to_world(zoomed, screen.x, screen.y), world);

        let dx = 7.0 + index as f64 * 0.5;
        let dy = -3.0 + index as f64 * 0.25;
        let panned = pan_by(camera, dx, dy);
        let panned_screen = world_to_screen(panned, world.x, world.y);
        assert_point_close(
            panned_screen,
            Point {
                x: screen.x + dx,
                y: screen.y + dy,
            },
        );

        if index.is_multiple_of(5) {
            let mut current = camera;
            for _ in 0..24 {
                current = zoom_at(current, screen.x, screen.y, 2.0);
            }
            assert!(current.scale <= MAX_ZOOM + EPS);
            for _ in 0..48 {
                current = zoom_at(current, screen.x, screen.y, 0.1);
            }
            assert!(current.scale >= MIN_ZOOM - EPS);
        }

        if index.is_multiple_of(6) {
            let bounds = WorldBounds {
                min_x: world.x - 20.0,
                min_y: world.y - 15.0,
                max_x: world.x + 30.0,
                max_y: world.y + 45.0,
            };
            let fitted = fit_bounds(bounds, 800.0, 600.0, index as f64);
            let center = world_to_screen(
                fitted,
                (bounds.min_x + bounds.max_x) / 2.0,
                (bounds.min_y + bounds.max_y) / 2.0,
            );
            assert_point_close(center, Point { x: 400.0, y: 300.0 });
        }
    }

    case_suite!(run_case);
}

mod geometry_matrix {
    use super::*;

    fn run_case(index: usize) {
        let kind = match index % 3 {
            0 => DrawElementType::Rectangle,
            1 => DrawElementType::Ellipse,
            _ => DrawElementType::Diamond,
        };
        let x = -60.0 + index as f64 * 2.0;
        let y = 80.0 - index as f64 * 1.5;
        let width = 12.0 + (index % 6) as f64 * 4.0;
        let height = 10.0 + (index % 5) as f64 * 3.0;
        let raw = if index.is_multiple_of(2) {
            normalize_rect(x, y, -width, -height)
        } else {
            normalize_rect(x, y, width, height)
        };
        let expected = if index.is_multiple_of(2) {
            Rect {
                x: x - width,
                y: y - height,
                width,
                height,
            }
        } else {
            Rect {
                x,
                y,
                width,
                height,
            }
        };
        assert_rect_close(raw, expected);

        let element = create_element_default(
            kind,
            Geometry {
                x: expected.x,
                y: expected.y,
                width: expected.width,
                height: expected.height,
            },
        );
        let bounds = element_bounds(&element);
        assert_close(bounds.min_x, expected.x);
        assert_close(bounds.min_y, expected.y);
        assert_close(bounds.max_x, expected.x + expected.width);
        assert_close(bounds.max_y, expected.y + expected.height);

        let inside = Point {
            x: expected.x + expected.width / 2.0,
            y: expected.y + expected.height / 2.0,
        };
        assert!(hit_test_element(&element, inside.x, inside.y, 0.0));
        assert!(!hit_test_element(
            &element,
            expected.x - 4.0,
            expected.y - 4.0,
            0.0
        ));

        let other = box_at(
            expected.x + 3.0,
            expected.y + 3.0,
            expected.width,
            expected.height,
        );
        assert_eq!(
            hit_test(&[element.clone(), other.clone()], inside.x, inside.y, 0.0)
                .map(|hit| hit.id.clone()),
            Some(other.id.clone())
        );

        let bounds = scene_bounds(&[element.clone(), other.clone()]).unwrap();
        assert_close(bounds.min_x, expected.x);
        assert_close(bounds.min_y, expected.y);
        assert_close(bounds.max_x, expected.x + expected.width + 3.0);
        assert_close(bounds.max_y, expected.y + expected.height + 3.0);

        if index.is_multiple_of(4) {
            assert_eq!(scene_bounds(&[]), None);
        }
    }

    case_suite!(run_case);
}

mod selection_matrix {
    use super::*;

    const HANDLES: [HandleKind; 8] = [
        HandleKind::Nw,
        HandleKind::N,
        HandleKind::Ne,
        HandleKind::E,
        HandleKind::Se,
        HandleKind::S,
        HandleKind::Sw,
        HandleKind::W,
    ];

    fn run_case(index: usize) {
        let handle = HANDLES[index % HANDLES.len()];
        let mut element = box_at(
            20.0 + index as f64,
            10.0 + index as f64 * 0.5,
            80.0 + (index % 5) as f64 * 10.0,
            40.0 + (index % 3) as f64 * 8.0,
        );
        if index.is_multiple_of(3) {
            element.angle = std::f64::consts::PI / 6.0;
        }
        let before = handle_world(&element, opposite_handle(handle));
        let handle_point = handle_world(&element, handle);
        let offset = match handle {
            HandleKind::Nw => Point { x: -22.0, y: -16.0 },
            HandleKind::N => Point { x: 0.0, y: -18.0 },
            HandleKind::Ne => Point { x: 22.0, y: -16.0 },
            HandleKind::E => Point { x: 24.0, y: 0.0 },
            HandleKind::Se => Point { x: 22.0, y: 18.0 },
            HandleKind::S => Point { x: 0.0, y: 18.0 },
            HandleKind::Sw => Point { x: -22.0, y: 18.0 },
            HandleKind::W => Point { x: -24.0, y: 0.0 },
            HandleKind::Rotate => Point { x: 0.0, y: 0.0 },
        };
        let ratio = if index.is_multiple_of(2) {
            Some(element.width.abs() / element.height.abs())
        } else {
            None
        };
        let geometry = resize_element(
            &element,
            handle,
            handle_point.x + offset.x,
            handle_point.y + offset.y,
            4.0,
            ratio,
        );
        let mut resized = element.clone();
        resized.x = geometry.x;
        resized.y = geometry.y;
        resized.width = geometry.width;
        resized.height = geometry.height;
        let after = handle_world(&resized, opposite_handle(handle));
        assert_point_close(before, after);
        assert!(geometry.width >= 4.0);
        assert!(geometry.height >= 4.0);

        let points = selection_handle_points(&element, 20.0);
        assert_eq!(points.len(), 9);
        assert_eq!(
            hit_handle(&points, handle_point.x, handle_point.y, 6.0),
            Some(handle)
        );

        if index.is_multiple_of(4) {
            let rotate =
                rotate_element(&element, element.x + element.width / 2.0, element.y - 100.0);
            assert!(
                (rotate - 0.0).abs() < 1e-9 || (rotate - std::f64::consts::PI * 2.0).abs() < 1e-9
            );
        }
    }

    case_suite!(run_case);
}

mod binding_matrix {
    use super::*;

    fn run_case(index: usize) {
        match index % 5 {
            0 => {
                let under = box_at(0.0, 0.0, 80.0, 50.0);
                let over = box_at(10.0, 10.0, 80.0, 50.0);
                let arrow = connector(0.0, 0.0, 80.0, 50.0, DrawElementType::Arrow);
                let scene = vec![under.clone(), over.clone(), arrow];
                assert_eq!(
                    bindable_at(&scene, 20.0 + index as f64, 20.0, 0.0, None)
                        .map(|hit| hit.id.clone()),
                    Some(over.id.clone())
                );
                assert_eq!(
                    bindable_at(&scene, 20.0 + index as f64, 20.0, 0.0, Some(&over.id))
                        .map(|hit| hit.id.clone()),
                    Some(under.id.clone())
                );
            }
            1 => {
                let left = box_at(0.0, 0.0, 100.0, 60.0);
                let right = box_at(300.0, 0.0, 100.0, 60.0);
                let mut link = connector(100.0, 30.0, 300.0, 30.0, DrawElementType::Arrow);
                link.start_binding = Some(left.id.clone());
                link.end_binding = Some(right.id.clone());
                let bound = refresh_bindings(&[left.clone(), right.clone(), link.clone()]);
                let endpoints = linear_endpoints(&bound[2]);
                assert_close(endpoints.0.x, left.x + left.width + BINDING_GAP);
                assert_close(endpoints.0.y, 30.0);
                assert_close(endpoints.1.x, right.x - BINDING_GAP);
                let mut moved = right.clone();
                moved.y += 240.0 + index as f64;
                let after = refresh_bindings(&[left, moved.clone(), link]);
                let moved_endpoints = linear_endpoints(&after[2]);
                assert!(moved_endpoints.1.y > endpoints.1.y);
                assert!(moved_endpoints.0.y > endpoints.0.y);
                assert!(!hit_test_element(
                    &moved,
                    moved_endpoints.1.x,
                    moved_endpoints.1.y,
                    0.0
                ));
            }
            2 => {
                let mut shape = box_at(0.0, 0.0, 100.0, 60.0);
                shape.is_deleted = true;
                let mut link = connector(0.0, 0.0, 300.0, 0.0, DrawElementType::Arrow);
                link.start_binding = Some(shape.id.clone());
                let next = refresh_bindings(&[shape, link.clone()]);
                assert_eq!(linear_endpoints(&next[1]), linear_endpoints(&link));
            }
            3 => {
                let arrow = connector(0.0, 0.0, 10.0, 0.0, DrawElementType::Arrow);
                assert_eq!(default_arrowhead(&arrow, "end"), Arrowhead::Arrow);
                assert_eq!(default_arrowhead(&arrow, "start"), Arrowhead::None);
                let mut explicit = arrow.clone();
                explicit.end_arrowhead = Some(Arrowhead::Diamond);
                assert_eq!(default_arrowhead(&explicit, "end"), Arrowhead::Diamond);
                let line = connector(0.0, 0.0, 10.0, 0.0, DrawElementType::Line);
                assert_eq!(default_arrowhead(&line, "end"), Arrowhead::None);
            }
            _ => {
                let drag = linear_from_drag(
                    20.0,
                    30.0,
                    60.0 + index as f64,
                    90.0,
                    index.is_multiple_of(2),
                );
                assert_eq!((drag.x, drag.y), (20.0, 30.0));
                assert!(!is_degenerate_linear(drag.width, drag.height, 4.0));
                let click = linear_from_drag(20.0, 30.0, 21.0, 30.0, false);
                assert!(is_degenerate_linear(click.width, click.height, 4.0));
            }
        }
    }

    case_suite!(run_case);
}

mod edit_matrix {
    use super::*;

    fn run_case(index: usize) {
        let a = box_at(0.0, 0.0, 20.0 + index as f64, 20.0);
        let b = box_at(30.0 + index as f64, 0.0, 20.0, 20.0);
        let c = box_at(60.0 + index as f64, 0.0, 20.0, 20.0);
        let live = vec![a.clone(), b.clone(), c.clone()];

        match index % 5 {
            0 => {
                let (selected, mode, expected) = match index % 4 {
                    0 => (
                        ids(&[a.clone()]),
                        ZOrderMode::Front,
                        vec![b.id.clone(), c.id.clone(), a.id.clone()],
                    ),
                    1 => (
                        ids(&[c.clone()]),
                        ZOrderMode::Back,
                        vec![c.id.clone(), a.id.clone(), b.id.clone()],
                    ),
                    2 => (
                        ids(&[a.clone()]),
                        ZOrderMode::Forward,
                        vec![b.id.clone(), a.id.clone(), c.id.clone()],
                    ),
                    _ => (
                        ids(&[c.clone()]),
                        ZOrderMode::Backward,
                        vec![a.id.clone(), c.id.clone(), b.id.clone()],
                    ),
                };
                let reordered = reorder_elements(&live, &selected, mode);
                assert_eq!(
                    reordered
                        .iter()
                        .map(|element| element.id.clone())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
            1 => {
                let mode = match index % 4 {
                    0 => AlignMode::Left,
                    1 => AlignMode::Right,
                    2 => AlignMode::CenterX,
                    _ => AlignMode::CenterY,
                };
                let aligned = align_elements(&live, &ids(&[a.clone(), b.clone(), c.clone()]), mode);
                match mode {
                    AlignMode::Left => assert_eq!(
                        aligned.iter().map(|element| element.x).collect::<Vec<_>>(),
                        vec![0.0, 0.0, 0.0]
                    ),
                    AlignMode::Right => assert_eq!(
                        aligned
                            .iter()
                            .map(|element| element.x + element.width)
                            .collect::<Vec<_>>(),
                        vec![
                            80.0 + index as f64,
                            80.0 + index as f64,
                            80.0 + index as f64
                        ]
                    ),
                    AlignMode::CenterX => assert_close(
                        aligned[0].x + aligned[0].width / 2.0,
                        aligned[1].x + aligned[1].width / 2.0,
                    ),
                    AlignMode::CenterY => assert_close(
                        aligned[0].y + aligned[0].height / 2.0,
                        aligned[1].y + aligned[1].height / 2.0,
                    ),
                    _ => unreachable!(),
                }
            }
            2 => {
                let axis = if index.is_multiple_of(2) { 'x' } else { 'y' };
                let spread =
                    distribute_elements(&live, &ids(&[a.clone(), b.clone(), c.clone()]), axis);
                if axis == 'x' {
                    let mut centres: Vec<_> = spread
                        .iter()
                        .map(|element| element.x + element.width / 2.0)
                        .collect();
                    centres.sort_by(|left, right| left.partial_cmp(right).unwrap());
                    assert_close(centres[1] - centres[0], centres[2] - centres[1]);
                } else {
                    let mut centres: Vec<_> = spread
                        .iter()
                        .map(|element| element.y + element.height / 2.0)
                        .collect();
                    centres.sort_by(|left, right| left.partial_cmp(right).unwrap());
                    assert_close(centres[1] - centres[0], centres[2] - centres[1]);
                }
            }
            3 => {
                let connector = connector(
                    0.0,
                    0.0,
                    400.0,
                    100.0 + index as f64,
                    DrawElementType::Arrow,
                );
                let axis = if index.is_multiple_of(2) {
                    FlipAxis::Horizontal
                } else {
                    FlipAxis::Vertical
                };
                let flipped = flip_elements(
                    &[a.clone(), b.clone(), connector.clone()],
                    &ids(&[a.clone(), b.clone(), connector.clone()]),
                    axis,
                );
                let bounds = scene_bounds(&[a.clone(), b.clone(), connector.clone()]).unwrap();
                let flipped_connector = flipped
                    .iter()
                    .find(|element| element.kind == DrawElementType::Arrow)
                    .unwrap();
                let endpoints = linear_endpoints(flipped_connector);
                if matches!(axis, FlipAxis::Horizontal) {
                    let flipped_a = flipped.iter().find(|element| element.id == a.id).unwrap();
                    let flipped_b = flipped.iter().find(|element| element.id == b.id).unwrap();
                    assert_close(flipped_a.x, bounds.min_x + bounds.max_x - (a.x + a.width));
                    assert_close(flipped_b.x, bounds.min_x + bounds.max_x - (b.x + b.width));
                    assert_close(endpoints.0.x, 400.0);
                    assert_close(endpoints.1.x, 0.0);
                } else {
                    let flipped_a = flipped.iter().find(|element| element.id == a.id).unwrap();
                    let flipped_b = flipped.iter().find(|element| element.id == b.id).unwrap();
                    assert_close(flipped_a.y, bounds.min_y + bounds.max_y - (a.y + a.height));
                    assert_close(flipped_b.y, bounds.min_y + bounds.max_y - (b.y + b.height));
                    assert_close(endpoints.0.y, 100.0 + index as f64);
                    assert_close(endpoints.1.y, 0.0);
                }
            }
            _ => {
                let grouped = group_patches(
                    &[a.clone(), b.clone(), c.clone()],
                    &ids(&[a.clone(), b.clone()]),
                    "g1",
                );
                assert!(is_single_group(
                    &grouped,
                    &ids(&[grouped[0].clone(), grouped[1].clone()])
                ));
                assert!(!is_single_group(
                    &[a.clone(), b.clone(), c.clone()],
                    &ids(&[a.clone(), b.clone(), c.clone()])
                ));
                let expanded = expand_to_groups(&grouped, [a.id.clone()]);
                assert_eq!(expanded, HashSet::from([a.id.clone(), b.id.clone()]));
                let ungrouped =
                    ungroup_patches(&grouped, &ids(&[grouped[0].clone(), grouped[1].clone()]));
                assert!(ungrouped
                    .iter()
                    .take(2)
                    .all(|element| element.group_id.is_none()));
            }
        }
    }

    case_suite!(run_case);
}

mod pointer_matrix {
    use super::*;

    fn run_case(index: usize) {
        match index % 5 {
            0 => {
                let tool = match index % 3 {
                    0 => DrawTool::Rectangle,
                    1 => DrawTool::Ellipse,
                    _ => DrawTool::Diamond,
                };
                let mut engine = DrawEngine::new();
                engine.set_viewport(800.0, 600.0, 1.0);
                engine.set_tool(tool);
                engine.begin_pointer(20.0, 30.0, false, false);
                engine.move_pointer(
                    60.0 + index as f64,
                    70.0 + index as f64,
                    index.is_multiple_of(2),
                    false,
                );
                engine.end_pointer();
                let scene = engine.get_scene();
                assert_eq!(scene.len(), 1);
                assert_eq!(
                    scene[0].kind,
                    match tool {
                        DrawTool::Rectangle => DrawElementType::Rectangle,
                        DrawTool::Ellipse => DrawElementType::Ellipse,
                        DrawTool::Diamond => DrawElementType::Diamond,
                        _ => unreachable!(),
                    }
                );
                assert_eq!(engine.get_tool(), DrawTool::Select);
                assert_eq!(engine.get_selected_elements().len(), 1);
            }
            1 => {
                let left = box_at(0.0, 0.0, 100.0, 60.0);
                let right = box_at(300.0, 0.0, 100.0, 60.0);
                let mut engine = engine_with_scene(vec![left.clone(), right.clone()]);
                engine.set_tool(if index.is_multiple_of(2) {
                    DrawTool::Arrow
                } else {
                    DrawTool::Line
                });
                engine.begin_pointer(50.0, 30.0, false, false);
                if index.is_multiple_of(4) {
                    engine.move_pointer(51.0, 30.0, false, false);
                } else {
                    engine.move_pointer(350.0, 30.0, index.is_multiple_of(2), false);
                }
                engine.end_pointer();
                let scene = engine.get_scene();
                if index.is_multiple_of(4) {
                    assert_eq!(scene.len(), 2);
                } else {
                    assert_eq!(scene.len(), 3);
                    let link = scene.last().unwrap();
                    assert_eq!(link.start_binding.as_deref(), Some(left.id.as_str()));
                    assert_eq!(link.end_binding.as_deref(), Some(right.id.as_str()));
                }
                assert_eq!(engine.get_tool(), DrawTool::Select);
            }
            2 => {
                let mut engine = DrawEngine::new();
                engine.set_viewport(800.0, 600.0, 1.0);
                engine.set_tool(DrawTool::Freedraw);
                engine.begin_pointer(20.0, 20.0, false, false);
                if !index.is_multiple_of(5) {
                    for step in 1..=(2 + index % 3) {
                        engine.move_pointer(
                            20.0 + step as f64 * 10.0,
                            20.0 + step as f64 * 5.0,
                            false,
                            false,
                        );
                    }
                }
                engine.end_pointer();
                let scene = engine.get_scene();
                if index.is_multiple_of(5) {
                    assert!(scene.is_empty());
                } else {
                    assert_eq!(scene.len(), 1);
                    assert_eq!(scene[0].kind, DrawElementType::Freedraw);
                    assert!(scene[0].points.as_ref().unwrap().len() > 1);
                }
                assert_eq!(engine.get_tool(), DrawTool::Select);
            }
            3 => {
                if index.is_multiple_of(2) {
                    let mut engine = engine_with_measure(vec![]);
                    engine.set_tool(DrawTool::Text);
                    engine.begin_pointer(100.0, 120.0, false, false);
                    let events = engine.drain_events();
                    assert!(events.text_edit.is_some());
                    assert_eq!(engine.get_scene().len(), 1);
                    assert_eq!(engine.get_scene()[0].kind, DrawElementType::Text);
                    assert_eq!(engine.get_selected_elements().len(), 1);
                    assert_eq!(engine.get_tool(), DrawTool::Select);
                } else {
                    let rect = box_at(0.0, 0.0, 100.0, 60.0);
                    let mut engine = engine_with_scene(vec![rect.clone()]);
                    engine.set_tool(DrawTool::Hand);
                    engine.begin_pointer(10.0, 20.0, false, false);
                    engine.move_pointer(24.0 + index as f64, 34.0 + index as f64, false, false);
                    assert!(engine.in_motion());
                    assert!(engine.camera.x != 0.0 || engine.camera.y != 0.0);
                    engine.end_pointer();
                    assert_eq!(engine.get_scene().len(), 1);
                    assert_eq!(engine.get_scene()[0].id, rect.id);
                }
            }
            _ => {
                let top = box_at(10.0, 10.0, 100.0, 60.0);
                let under = box_at(0.0, 0.0, 120.0, 90.0);
                let mut engine = engine_with_scene(vec![under.clone(), top.clone()]);
                if index.is_multiple_of(2) {
                    engine.set_tool(DrawTool::Select);
                    engine.begin_pointer(40.0, 30.0, false, false);
                    engine.end_pointer();
                    assert_eq!(engine.get_selection().len(), 1);
                    assert_eq!(engine.get_selected_elements().len(), 1);
                    assert_eq!(engine.get_selection()[0], top.id);
                } else {
                    engine.select(vec![top.id.clone()]);
                    engine.set_tool(DrawTool::Eraser);
                    engine.begin_pointer(40.0, 30.0, false, false);
                    engine.end_pointer();
                    let scene = engine.get_scene();
                    assert!(scene
                        .iter()
                        .any(|element| element.id == top.id && element.is_deleted));
                    assert!(engine.get_selected_elements().is_empty());
                }
            }
        }
    }

    case_suite!(run_case);
}

mod style_matrix {
    use super::*;

    fn run_case(index: usize) {
        match index % 5 {
            0 => {
                let mut engine = engine_with_scene(vec![]);
                let color = format!("#{:02x}{:02x}{:02x}", index * 7, index * 11, index * 13);
                engine.apply_style(stroke_patch(&color));
                assert!(engine.get_scene().is_empty());
                assert_eq!(engine.get_next_style().stroke_color, color);
            }
            1 => {
                let rect = box_at(0.0, 0.0, 100.0, 60.0);
                let mut engine = engine_with_scene(vec![rect.clone()]);
                engine.select(vec![rect.id.clone()]);
                let updated_color = format!(
                    "#{:02x}{:02x}{:02x}",
                    200 - index * 3,
                    80 + index,
                    30 + index * 2
                );
                let now = 42.0 + index as f64;
                engine.set_now(now);
                engine.apply_style(stroke_patch(&updated_color));
                let updated = engine
                    .get_scene()
                    .into_iter()
                    .find(|element| element.id == rect.id)
                    .unwrap();
                assert_eq!(updated.stroke_color, updated_color);
                assert_eq!(updated.version, 2);
                assert_eq!(updated.updated, now);
            }
            2 => {
                let rect = box_at(0.0, 0.0, 100.0, 60.0);
                let line = connector(0.0, 0.0, 120.0, 40.0 + index as f64, DrawElementType::Arrow);
                let mut engine = engine_with_scene(vec![rect.clone(), line.clone()]);
                engine.select(vec![line.id.clone()]);
                engine.set_arrowheads(Some(Arrowhead::Diamond), Some(Arrowhead::Arrow));
                let updated_line = engine
                    .get_scene()
                    .into_iter()
                    .find(|element| element.id == line.id)
                    .unwrap();
                assert_eq!(updated_line.start_arrowhead, Some(Arrowhead::Diamond));
                assert_eq!(updated_line.end_arrowhead, Some(Arrowhead::Arrow));

                engine.select(vec![rect.id.clone()]);
                engine.set_arrowheads(Some(Arrowhead::Arrow), Some(Arrowhead::Diamond));
                let unchanged = engine
                    .get_scene()
                    .into_iter()
                    .find(|element| element.id == rect.id)
                    .unwrap();
                assert_eq!(unchanged.id, rect.id);
                assert_eq!(unchanged.kind, DrawElementType::Rectangle);
            }
            3 => {
                let text = text_at(0.0, 0.0, 8.0, 20.0);
                let mut engine = engine_with_measure(vec![text.clone()]);
                engine.select(vec![text.id.clone()]);
                let size = 12.0 + index as f64;
                engine.set_font_size(size);
                let updated = engine
                    .get_scene()
                    .into_iter()
                    .find(|element| element.id == text.id)
                    .unwrap();
                let (width, height) = measure_text("", size);
                assert_eq!(updated.font_size, Some(size));
                assert_close(updated.width, width);
                assert_close(updated.height, height);
                assert_close(engine.get_font_size(), size);
            }
            _ => {
                let rect = box_at(0.0, 0.0, 100.0, 60.0);
                let mut engine = engine_with_scene(vec![rect.clone()]);
                engine.set_next_style(stroke_patch("#ff00aa"));
                assert_eq!(engine.get_next_style().stroke_color, "#ff00aa");
                engine.apply_style(stroke_patch("#00ffaa"));
                assert_eq!(engine.get_scene().len(), 1);
                assert_eq!(engine.get_scene()[0].id, rect.id);
            }
        }
    }

    case_suite!(run_case);
}

mod text_matrix {
    use super::*;

    fn run_case(index: usize) {
        match index % 5 {
            0 => {
                let mut engine = engine_with_measure(vec![]);
                engine.set_tool(DrawTool::Select);
                engine.handle_double_click(100.0 + index as f64, 120.0);
                let events = engine.drain_events();
                assert!(events.text_edit.is_some());
                assert_eq!(engine.get_scene().len(), 1);
                assert_eq!(engine.get_scene()[0].kind, DrawElementType::Text);
                assert_eq!(engine.get_selected_elements().len(), 1);
            }
            1 => {
                let text = text_at(40.0, 50.0, 120.0, 24.0);
                let mut engine = engine_with_measure(vec![text.clone()]);
                engine.set_tool(DrawTool::Select);
                engine.handle_double_click(text.x + 4.0, text.y + 4.0);
                let events = engine.drain_events();
                assert!(events.text_edit.is_some());
                assert_eq!(engine.get_selection(), vec![text.id.clone()]);
            }
            2 => {
                let container = match index % 3 {
                    0 => box_at(0.0, 0.0, 100.0, 60.0),
                    1 => ellipse_at(0.0, 0.0, 100.0, 60.0),
                    _ => diamond_at(0.0, 0.0, 100.0, 60.0),
                };
                let mut engine = engine_with_measure(vec![container.clone()]);
                engine.handle_double_click(
                    container.x + container.width / 2.0,
                    container.y + container.height / 2.0,
                );
                let scene = engine.get_scene();
                assert_eq!(scene.len(), 2);
                let label = scene
                    .iter()
                    .find(|element| element.kind == DrawElementType::Text)
                    .unwrap();
                let host = scene
                    .iter()
                    .find(|element| element.id == container.id)
                    .unwrap();
                assert_eq!(label.container_id.as_deref(), Some(container.id.as_str()));
                assert_eq!(host.bound_text_id.as_deref(), Some(label.id.as_str()));
                assert_eq!(engine.get_selection(), vec![label.id.clone()]);
            }
            3 => {
                let text = text_at(0.0, 0.0, 10.0, 20.0);
                let mut engine = engine_with_measure(vec![text.clone()]);
                engine.select(vec![text.id.clone()]);
                assert!(engine.edit_selected_text());
                let events = engine.drain_events();
                assert!(events.text_edit.is_some());
                assert_eq!(engine.get_selection().len(), 1);
            }
            _ => {
                let container = box_at(0.0, 0.0, 120.0, 80.0);
                let mut label = text_at(0.0, 0.0, 8.0, 20.0);
                label.container_id = Some(container.id.clone());
                let mut engine = engine_with_measure(vec![container.clone(), label.clone()]);
                if index.is_multiple_of(2) {
                    engine.set_element_text(&label.id, "hello\nworld");
                    let updated = engine
                        .get_scene()
                        .into_iter()
                        .find(|element| element.id == label.id)
                        .unwrap();
                    assert!(updated.height >= 2.0 * TEXT_LINE_HEIGHT * 20.0);
                    assert_eq!(updated.text.as_deref(), Some("hello\nworld"));
                } else {
                    engine.set_element_text(&label.id, "");
                    let scene = engine.get_scene();
                    assert_eq!(scene.len(), 1);
                    let host = scene
                        .iter()
                        .find(|element| element.id == container.id)
                        .unwrap();
                    assert!(host.bound_text_id.is_none());
                }
            }
        }
    }

    case_suite!(run_case);
}

mod persistence_matrix {
    use super::*;

    fn run_case(index: usize) {
        match index % 6 {
            0 => {
                let left = box_at(0.0, 0.0, 100.0, 60.0);
                let right = box_at(300.0, 0.0, 100.0, 60.0);
                let mut link = connector(100.0, 30.0, 300.0, 30.0, DrawElementType::Arrow);
                link.start_binding = Some(left.id.clone());
                link.end_binding = Some(right.id.clone());
                let mut engine = engine_with_scene(vec![left.clone(), right.clone(), link.clone()]);
                engine.select(vec![left.id.clone(), right.id.clone(), link.id.clone()]);
                let json = engine.copy_selection().unwrap();
                assert!(json.contains("\"type\": \"osidraw\""));
                assert!(engine.paste_json(None, None));
                let scene = engine.get_scene();
                assert_eq!(scene.len(), 6);
                let old_ids = ids(&[left.clone(), right.clone(), link.clone()]);
                assert_eq!(
                    scene
                        .iter()
                        .filter(|element| old_ids.contains(&element.id))
                        .count(),
                    3
                );
                assert_eq!(
                    scene
                        .iter()
                        .map(|element| element.id.clone())
                        .collect::<HashSet<_>>()
                        .len(),
                    scene.len()
                );
            }
            1 => {
                let rect = box_at(0.0, 0.0, 100.0, 60.0);
                let mut label = text_at(0.0, 0.0, 8.0, 20.0);
                label.container_id = Some(rect.id.clone());
                let mut engine = engine_with_measure(vec![rect.clone(), label.clone()]);
                engine.select(vec![label.id.clone()]);
                let json = engine.cut_selection().unwrap();
                assert!(!json.is_empty());
                let scene = engine.get_scene();
                assert!(deleted_count(&scene) >= 1);
                assert!(engine.get_selected_elements().is_empty());
            }
            2 => {
                let left = box_at(0.0, 0.0, 100.0, 60.0);
                let right = box_at(300.0, 0.0, 100.0, 60.0);
                let mut link = connector(100.0, 30.0, 300.0, 30.0, DrawElementType::Arrow);
                link.start_binding = Some(left.id.clone());
                link.end_binding = Some(right.id.clone());
                let mut engine = engine_with_scene(vec![left.clone(), right.clone(), link.clone()]);
                engine.select(vec![left.id.clone(), right.id.clone(), link.id.clone()]);
                engine.duplicate_selection(20.0 + index as f64, 10.0);
                let scene = engine.get_scene();
                assert_eq!(scene.len(), 6);
                let copied_arrow = scene
                    .iter()
                    .find(|element| {
                        element.id != left.id
                            && element.id != right.id
                            && element.id != link.id
                            && element.kind == DrawElementType::Arrow
                    })
                    .unwrap();
                let copied_start = copied_arrow.start_binding.as_ref().unwrap();
                let copied_end = copied_arrow.end_binding.as_ref().unwrap();
                assert_ne!(copied_start, &left.id);
                assert_ne!(copied_end, &right.id);
                let copied_left = scene
                    .iter()
                    .find(|element| element.id == *copied_start)
                    .unwrap();
                let copied_right = scene
                    .iter()
                    .find(|element| element.id == *copied_end)
                    .unwrap();
                assert_close(copied_left.x, left.x + 20.0 + index as f64);
                assert_close(copied_right.x, right.x + 20.0 + index as f64);
            }
            3 => {
                let rect = box_at(0.0, 0.0, 100.0, 60.0);
                let mut label = text_at(0.0, 0.0, 8.0, 20.0);
                label.container_id = Some(rect.id.clone());
                let mut engine = engine_with_measure(vec![rect.clone(), label.clone()]);
                engine.select(vec![label.id.clone()]);
                engine.delete_selection();
                let scene = engine.get_scene();
                let host = scene.iter().find(|element| element.id == rect.id).unwrap();
                assert!(host.bound_text_id.is_none());
                assert!(
                    scene
                        .iter()
                        .find(|element| element.id == label.id)
                        .unwrap()
                        .is_deleted
                );
            }
            4 => {
                let rect = box_at(0.0, 0.0, 100.0, 60.0);
                let mut engine = engine_with_scene(vec![rect.clone()]);
                engine.select(vec![rect.id.clone()]);
                let color = format!("#{:02x}{:02x}{:02x}", 64 + index, 32 + index, 16 + index);
                engine.set_now(42.0 + index as f64);
                engine.apply_style(stroke_patch(&color));
                let changed = engine
                    .get_scene()
                    .into_iter()
                    .find(|element| element.id == rect.id)
                    .unwrap();
                assert_eq!(changed.stroke_color, color);
                engine.undo();
                let undone = engine
                    .get_scene()
                    .into_iter()
                    .find(|element| element.id == rect.id)
                    .unwrap();
                assert_ne!(undone.stroke_color, changed.stroke_color);
                engine.redo();
                let redone = engine
                    .get_scene()
                    .into_iter()
                    .find(|element| element.id == rect.id)
                    .unwrap();
                assert_eq!(redone.stroke_color, changed.stroke_color);
            }
            _ => {
                let rect = box_at(0.0, 0.0, 100.0, 60.0);
                let engine = engine_with_scene(vec![rect.clone()]);
                let json = engine.export_json();
                assert!(json.contains("\"type\": \"osidraw\""));
                let svg = engine.export_svg(8.0).unwrap();
                assert!(svg.starts_with("<svg "));
                let mut loader = DrawEngine::new();
                assert!(loader.load_scene(&json));
                assert!(loader.export_svg(8.0).is_some());
                assert!(!loader.load_scene("not json"));
                loader.clear();
                assert!(loader.get_scene().is_empty());
                assert!(loader.content_in_view());
            }
        }
    }

    case_suite!(run_case);
}
