#![allow(dead_code)]

use std::collections::HashSet;

use draw_engine::*;

pub const EPS: f64 = 1e-9;

pub fn assert_close(a: f64, b: f64) {
    assert!((a - b).abs() < EPS, "expected {a} ~= {b}");
}

pub fn assert_point_close(a: Point, b: Point) {
    assert_close(a.x, b.x);
    assert_close(a.y, b.y);
}

pub fn assert_rect_close(a: Rect, b: Rect) {
    assert_close(a.x, b.x);
    assert_close(a.y, b.y);
    assert_close(a.width, b.width);
    assert_close(a.height, b.height);
}

pub fn box_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
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

pub fn ellipse_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
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

pub fn diamond_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
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

pub fn text_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
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

pub fn connector(x1: f64, y1: f64, x2: f64, y2: f64, kind: DrawElementType) -> DrawElement {
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

pub fn ids(elements: &[DrawElement]) -> HashSet<String> {
    elements.iter().map(|element| element.id.clone()).collect()
}

pub fn opposite_handle(handle: HandleKind) -> HandleKind {
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

pub fn handle_world(element: &DrawElement, handle: HandleKind) -> Point {
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

pub fn engine_with_scene(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_scene(Scene::new(elements));
    engine
}

pub fn engine_with_measure(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = engine_with_scene(elements);
    engine.set_measure_text(measure_text);
    engine
}

pub fn measure_text(text: &str, font_size: f64) -> (f64, f64) {
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

pub fn stroke_patch(color: &str) -> DrawElementStylePatch {
    DrawElementStylePatch {
        stroke_color: Some(color.to_string()),
        ..Default::default()
    }
}

pub fn deleted_count(elements: &[DrawElement]) -> usize {
    elements.iter().filter(|element| element.is_deleted).count()
}
