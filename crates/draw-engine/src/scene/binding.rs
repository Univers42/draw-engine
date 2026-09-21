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

/// Recomputes bound geometry **in place**, touching only the elements that change.
///
/// [`refresh_bindings`] rebuilds the whole scene: it clones every element into a map,
/// clones them all again into a result vector, and the caller then writes every one
/// back. That runs on every pointer move during a drag, so its cost was proportional to
/// the size of the document rather than to the one shape being moved — the reason
/// dragging a single rectangle got slower as a board filled up.
///
/// This walks the same logic but writes only what actually moved. A scene with no
/// bindings costs one pass and no allocation at all.
pub fn refresh_bindings_in_place(scene: &mut crate::scene::store::Scene) {
    // Pass 1: linear elements with a bound endpoint.
    //
    // Collected before writing because the reads borrow the scene immutably; only the
    // elements that genuinely changed are cloned.
    let mut moved: Vec<DrawElement> = Vec::new();
    for element in scene.iter_ordered() {
        if !is_linear_element(element) {
            continue;
        }
        let start_shape = element
            .start_binding
            .as_deref()
            .and_then(|id| scene.get(id))
            .filter(|shape| !shape.is_deleted);
        let end_shape = element
            .end_binding
            .as_deref()
            .and_then(|id| scene.get(id))
            .filter(|shape| !shape.is_deleted);

        if start_shape.is_none() && end_shape.is_none() {
            continue;
        }

        let (start, end) = linear_endpoints(element);
        let start_target = end_shape.map(element_center).unwrap_or(end);
        let end_target = start_shape.map(element_center).unwrap_or(start);
        let next_start = start_shape
            .map(|shape| attach_point(shape, start_target, BINDING_GAP))
            .unwrap_or(start);
        let next_end = end_shape
            .map(|shape| attach_point(shape, end_target, BINDING_GAP))
            .unwrap_or(end);

        let next = linear_from_endpoints(element.clone(), next_start, next_end);
        if &next != element {
            moved.push(next);
        }
    }
    for element in moved {
        scene.put(element);
    }

    // Pass 2: bound labels follow their container. Runs after the linear pass because a
    // label on an arrow has to follow the arrow's new endpoints.
    let mut relaid: Vec<DrawElement> = Vec::new();
    for element in scene.iter_ordered() {
        if element.kind != DrawElementType::Text {
            continue;
        }
        let Some(container_id) = element.container_id.as_deref() else {
            continue;
        };
        let Some(container) = scene.get(container_id).filter(|c| !c.is_deleted) else {
            continue;
        };
        let laid = layout_label(element.clone(), container);
        if &laid != element {
            relaid.push(laid);
        }
    }
    for element in relaid {
        scene.put(element);
    }
}

/// Recomputes bound geometry over a detached element list.
///
/// Retained for callers that hold a plain slice — chiefly the tests, which assert on the
/// returned list. Per-frame and per-event code should use [`refresh_bindings_in_place`],
/// which does not clone the scene.
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
