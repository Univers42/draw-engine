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

/// Topmost shape an endpoint at `(x, y)` would attach to.
///
/// Generic over the iterator so the hot paths can walk the scene by reference.
/// `candidates` must already run **top of the z-order first**.
pub fn bindable_among<'a>(
    candidates: impl Iterator<Item = &'a DrawElement>,
    x: f64,
    y: f64,
    tolerance: f64,
    exclude_id: Option<&str>,
) -> Option<&'a DrawElement> {
    for element in candidates {
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

pub fn bindable_at<'a>(
    elements: &'a [DrawElement],
    x: f64,
    y: f64,
    tolerance: f64,
    exclude_id: Option<&str>,
) -> Option<&'a DrawElement> {
    bindable_among(elements.iter().rev(), x, y, tolerance, exclude_id)
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

/// Replaces a linear element's geometry with a straight run from `start` to `end`.
///
/// Destroys any intermediate points, which is correct only while the line *is* two
/// points — drawing one, or rebuilding one from its ends. Anything re-anchoring an
/// existing line wants [`linear_retarget`].
pub fn linear_from_endpoints(mut element: DrawElement, start: Point, end: Point) -> DrawElement {
    element.x = start.x;
    element.y = start.y;
    element.width = end.x - start.x;
    element.height = end.y - start.y;
    element.points = Some(vec![[0.0, 0.0], [end.x - start.x, end.y - start.y]]);
    element
}

/// Moves a linear element's two ends, leaving everything between them where it is.
///
/// Binding used to go through [`linear_from_endpoints`], which rewrites the point list as
/// a straight pair. So the moment a bound shape was nudged, an arrow that had been given
/// midpoints collapsed to a straight line and the user's edits were gone — silently, and
/// unrecoverably once the history entry was folded.
///
/// Interior points keep their **world** positions: the line bends exactly as it did, only
/// its ends have moved. The origin is re-pinned to the first point, which is the invariant
/// the rest of the engine relies on, and the extent is recomputed from the points, which
/// is what [`crate::scene::geometry::element_bounds`] measures.
pub fn linear_retarget(mut element: DrawElement, start: Point, end: Point) -> DrawElement {
    let Some(points) = element.points.as_deref() else {
        return linear_from_endpoints(element, start, end);
    };
    if points.len() < 3 {
        // Two points are entirely defined by their ends; nothing to preserve.
        return linear_from_endpoints(element, start, end);
    }

    let (ox, oy) = (element.x, element.y);
    let last = points.len() - 1;
    let mut next: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            next.push([0.0, 0.0]);
        } else if i == last {
            next.push([end.x - start.x, end.y - start.y]);
        } else {
            // Same place on the board, expressed against the new origin.
            next.push([ox + p[0] - start.x, oy + p[1] - start.y]);
        }
    }

    element.x = start.x;
    element.y = start.y;
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &next {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    element.width = max_x - min_x;
    element.height = max_y - min_y;
    element.points = Some(next);
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
    // Horizontal alignment is the painter's job — the label keeps the container's full
    // inner width and the glyphs move inside it. Vertical alignment is this function's,
    // because the label is a real element with its own `y` and nothing else would move it.
    label.x = rect.x + LABEL_PADDING;
    label.y = rect.y
        + crate::render::label_offset_y(
            crate::scene::resolved_vertical_align(&label),
            rect.height,
            label.height,
        );
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

/// Where each end of a linear element should aim when it attaches.
///
/// Excalidraw aims an endpoint from its **adjacent point**, which is what keeps a bent
/// arrow attaching along the direction it actually arrives from. With only two points the
/// adjacent point *is* the other end, and when that other end is itself bound the two
/// would chase each other, so the far shape's centre is used instead — it is fixed, which
/// makes the result stable.
fn attach_targets(
    element: &DrawElement,
    start: Point,
    end: Point,
    start_shape: Option<&DrawElement>,
    end_shape: Option<&DrawElement>,
) -> (Point, Point) {
    let points = element.points.as_deref().unwrap_or(&[]);
    if points.len() > 2 {
        let world = |p: &[f64; 2]| Point {
            x: element.x + p[0],
            y: element.y + p[1],
        };
        return (world(&points[1]), world(&points[points.len() - 2]));
    }
    (
        end_shape.map(element_center).unwrap_or(end),
        start_shape.map(element_center).unwrap_or(start),
    )
}

/// Both endpoints of a bound linear element, resolved against the shapes it attaches to.
///
/// Shared by the in-place and detached refreshers so the two cannot drift apart.
///
/// # Not through the shapes
///
/// Each end is placed on its shape's outline, a gap clear of it, aimed along the line the
/// arrow arrives on. That is right while the two shapes are apart. Once they touch, the
/// two points cross over each other: the tail sits on the far side of the head, so the
/// arrow runs **backwards** and is drawn almost entirely inside both shapes.
///
/// Excalidraw does not allow that, and neither does this now. Measured on excalidraw.com
/// with one rectangle slid onto another, their arrow goes 228 long, 128, 48, then 0 and
/// stays 0 — never once entering either shape. When the endpoints would cross, the arrow
/// collapses onto its start anchor rather than turning itself inside out.
fn resolve_endpoints(
    element: &DrawElement,
    start_shape: Option<&DrawElement>,
    end_shape: Option<&DrawElement>,
) -> (Point, Point) {
    let (start, end) = linear_endpoints(element);
    let (start_target, end_target) = attach_targets(element, start, end, start_shape, end_shape);

    let next_start = start_shape
        .map(|shape| attach_point(shape, start_target, BINDING_GAP))
        .unwrap_or(start);
    let next_end = end_shape
        .map(|shape| attach_point(shape, end_target, BINDING_GAP))
        .unwrap_or(end);

    // Only a pair of bound ends can cross: a free end is where the user put it, and is
    // allowed to be anywhere, including inside a shape.
    let (Some(from), Some(to)) = (start_shape, end_shape) else {
        return (next_start, next_end);
    };

    let a = element_center(from);
    let b = element_center(to);
    let axis = (b.x - a.x, b.y - a.y);
    let run = (next_end.x - next_start.x, next_end.y - next_start.y);
    if run.0 * axis.0 + run.1 * axis.1 < 0.0 {
        // Inverted: the shapes have closed on each other far enough that no arrow fits
        // between them. Collapse onto the start anchor, which is where Excalidraw leaves
        // it, rather than drawing a reversed arrow through the middle of both.
        return (next_start, next_start);
    }
    (next_start, next_end)
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

        let (next_start, next_end) = resolve_endpoints(element, start_shape, end_shape);

        // `linear_retarget`, not `linear_from_endpoints`: the latter rewrites the point
        // list as a straight pair, so every bend a user had put in an arrow vanished the
        // moment the shape it pointed at was nudged.
        let next = linear_retarget(element.clone(), next_start, next_end);
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
        let (next_start, next_end) =
            resolve_endpoints(element, start_shape.as_ref(), end_shape.as_ref());
        let next = linear_retarget(element.clone(), next_start, next_end);
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
