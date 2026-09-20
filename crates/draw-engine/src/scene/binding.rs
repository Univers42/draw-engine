use crate::camera::Point;
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::normalize_rect;

pub const BINDING_GAP: f64 = 6.0;
pub const LABEL_PADDING: f64 = 8.0;

pub fn is_linear_element(element: &DrawElement) -> bool {
    matches!(element.kind, DrawElementType::Line | DrawElementType::Arrow)
}

pub fn is_bindable_element(element: &DrawElement) -> bool {
    matches!(
        element.kind,
        DrawElementType::Rectangle | DrawElementType::Diamond | DrawElementType::Ellipse
    )
}

pub fn element_center(element: &DrawElement) -> Point {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }
}

pub fn attach_point(shape: &DrawElement, toward: Point, gap: f64) -> Point {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let rx = (rect.width / 2.0).max(0.5);
    let ry = (rect.height / 2.0).max(0.5);
    let mut dx = toward.x - cx;
    let mut dy = toward.y - cy;
    let length = dx.hypot(dy);
    if length < 1e-6 {
        return Point { x: cx, y: cy };
    }
    dx /= length;
    dy /= length;
    let t = match shape.kind {
        DrawElementType::Ellipse => 1.0 / (dx / rx).hypot(dy / ry),
        DrawElementType::Diamond => 1.0 / (dx.abs() / rx + dy.abs() / ry),
        _ => {
            let tx = if dx.abs() < 1e-6 {
                f64::INFINITY
            } else {
                rx / dx.abs()
            };
            let ty = if dy.abs() < 1e-6 {
                f64::INFINITY
            } else {
                ry / dy.abs()
            };
            tx.min(ty)
        }
    };
    let reach = t + gap;
    Point {
        x: cx + dx * reach,
        y: cy + dy * reach,
    }
}

pub fn bindable_at<'a>(
    elements: &'a [DrawElement],
    x: f64,
    y: f64,
    tolerance: f64,
    exclude_id: Option<&str>,
) -> Option<&'a DrawElement> {
    for element in elements.iter().rev() {
        if element.is_deleted
            || Some(element.id.as_str()) == exclude_id
            || !is_bindable_element(element)
        {
            continue;
        }
        let rect = normalize_rect(element.x, element.y, element.width, element.height);
        if x >= rect.x - tolerance
            && x <= rect.x + rect.width + tolerance
            && y >= rect.y - tolerance
            && y <= rect.y + rect.height + tolerance
        {
            return Some(element);
        }
    }
    None
}

pub fn linear_endpoints(element: &DrawElement) -> (Point, Point) {
    let points = element
        .points
        .as_deref()
        .unwrap_or(&[[0.0, 0.0], [0.0, 0.0]]);
    let first = points.first().copied().unwrap_or([0.0, 0.0]);
    let last = points.last().copied().unwrap_or([0.0, 0.0]);
    (
        Point {
            x: element.x + first[0],
            y: element.y + first[1],
        },
        Point {
            x: element.x + last[0],
            y: element.y + last[1],
        },
    )
}

pub fn linear_from_endpoints(mut element: DrawElement, start: Point, end: Point) -> DrawElement {
    element.x = start.x;
    element.y = start.y;
    element.width = end.x - start.x;
    element.height = end.y - start.y;
    element.points = Some(vec![[0.0, 0.0], [end.x - start.x, end.y - start.y]]);
    element
}

pub fn layout_label(mut label: DrawElement, container: &DrawElement) -> DrawElement {
    if is_linear_element(container) {
        let (start, end) = linear_endpoints(container);
        label.x = (start.x + end.x) / 2.0 - label.width / 2.0;
        label.y = (start.y + end.y) / 2.0 - label.height / 2.0;
        label.angle = 0.0;
        return label;
    }
    let rect = normalize_rect(container.x, container.y, container.width, container.height);
    let width = (rect.width - LABEL_PADDING * 2.0).max(8.0);
    label.x = rect.x + LABEL_PADDING;
    label.y = rect.y + rect.height / 2.0 - label.height / 2.0;
    label.width = width;
    label.angle = container.angle;
    label
}

fn live<'a>(
    by_id: &'a std::collections::HashMap<String, DrawElement>,
    id: Option<&str>,
) -> Option<&'a DrawElement> {
    let id = id?;
    let found = by_id.get(id)?;
    if found.is_deleted {
        None
    } else {
        Some(found)
    }
}

pub fn refresh_bindings(elements: &[DrawElement]) -> Vec<DrawElement> {
    let mut by_id: std::collections::HashMap<String, DrawElement> = elements
        .iter()
        .cloned()
        .map(|el| (el.id.clone(), el))
        .collect();
    let mut linears_done = Vec::with_capacity(elements.len());
    for element in elements {
        if element.is_deleted || !is_linear_element(element) {
            linears_done.push(element.clone());
            continue;
        }
        let start_shape = live(&by_id, element.start_binding.as_deref()).cloned();
        let end_shape = live(&by_id, element.end_binding.as_deref()).cloned();
        if start_shape.is_none() && end_shape.is_none() {
            linears_done.push(element.clone());
            continue;
        }
        let (start, end) = linear_endpoints(element);
        let start_target = end_shape.as_ref().map(element_center).unwrap_or(end);
        let end_target = start_shape.as_ref().map(element_center).unwrap_or(start);
        let next_start = start_shape
            .as_ref()
            .map(|shape| attach_point(shape, start_target, BINDING_GAP))
            .unwrap_or(start);
        let next_end = end_shape
            .as_ref()
            .map(|shape| attach_point(shape, end_target, BINDING_GAP))
            .unwrap_or(end);
        let next = linear_from_endpoints(element.clone(), next_start, next_end);
        by_id.insert(next.id.clone(), next.clone());
        linears_done.push(next);
    }
    linears_done
        .into_iter()
        .map(|element| {
            if element.is_deleted || element.kind != DrawElementType::Text {
                return element;
            }
            let Some(container_id) = element.container_id.as_deref() else {
                return element;
            };
            match live(&by_id, Some(container_id)).cloned() {
                Some(container) => layout_label(element, &container),
                None => element,
            }
        })
        .collect()
}
