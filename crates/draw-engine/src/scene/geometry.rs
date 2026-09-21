use crate::camera::WorldBounds;
use crate::scene::element::{DrawElement, DrawElementType};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn normalize_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x: if width < 0.0 { x + width } else { x },
        y: if height < 0.0 { y + height } else { y },
        width: width.abs(),
        height: height.abs(),
    }
}

pub fn element_bounds(element: &DrawElement) -> WorldBounds {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    WorldBounds {
        min_x: rect.x,
        min_y: rect.y,
        max_x: rect.x + rect.width,
        max_y: rect.y + rect.height,
    }
}

/// The union of every live element's bounds.
///
/// Generic over the iterable so it accepts both an owned slice and a list of borrowed
/// elements — the render path holds `&DrawElement` to avoid cloning the scene, and
/// should not have to clone it back just to measure it.
pub fn scene_bounds<'a>(
    elements: impl IntoIterator<Item = &'a DrawElement>,
) -> Option<WorldBounds> {
    let mut bounds: Option<WorldBounds> = None;
    for element in elements {
        if element.is_deleted {
            continue;
        }
        let next = element_bounds(element);
        bounds = Some(match bounds {
            None => next,
            Some(cur) => WorldBounds {
                min_x: cur.min_x.min(next.min_x),
                min_y: cur.min_y.min(next.min_y),
                max_x: cur.max_x.max(next.max_x),
                max_y: cur.max_y.max(next.max_y),
            },
        });
    }
    bounds
}

pub fn distance_to_segment(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let length_sq = dx * dx + dy * dy;
    if length_sq < 1e-9 {
        return (px - ax).hypot(py - ay);
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / length_sq;
    let t = t.clamp(0.0, 1.0);
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// The pointer expressed in the element's own frame.
///
/// Every shape is defined unrotated and then turned about its centre by `angle`, so a hit
/// test only has to turn the *query* back by the same amount and can then work in the
/// simple axis-aligned space the shape is described in. Without this a rotated element is
/// tested against the box it would occupy if it had never been turned: a square rotated
/// 45 degrees is selectable from the empty space beyond its flat corners and dead along
/// its actual points.
pub fn to_element_local(element: &DrawElement, wx: f64, wy: f64) -> (f64, f64) {
    if element.angle == 0.0 {
        return (wx, wy);
    }
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let (sin, cos) = (-element.angle).sin_cos();
    let dx = wx - cx;
    let dy = wy - cy;
    (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
}

/// The axis-aligned box an element occupies once its rotation is taken into account.
///
/// [`element_bounds`] deliberately returns the *unrotated* box, which is what resize and
/// the stored geometry are expressed in. Anything asking "where is this on the board" —
/// a marquee, a fit-to-content — wants this one instead.
pub fn element_rotated_bounds(element: &DrawElement) -> WorldBounds {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    if element.angle == 0.0 {
        return WorldBounds {
            min_x: rect.x,
            min_y: rect.y,
            max_x: rect.x + rect.width,
            max_y: rect.y + rect.height,
        };
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let (sin, cos) = element.angle.sin_cos();
    let hw = rect.width / 2.0;
    let hh = rect.height / 2.0;
    // For an axis-aligned box turned by `angle`, the half-extent of the result has this
    // closed form — no need to walk the four corners.
    let ex = hw * cos.abs() + hh * sin.abs();
    let ey = hw * sin.abs() + hh * cos.abs();
    WorldBounds {
        min_x: cx - ex,
        min_y: cy - ey,
        max_x: cx + ex,
        max_y: cy + ey,
    }
}

pub fn hit_test_element(element: &DrawElement, wx: f64, wy: f64, tolerance: f64) -> bool {
    let (wx, wy) = to_element_local(element, wx, wy);
    if matches!(element.kind, DrawElementType::Line | DrawElementType::Arrow) {
        return hit_linear(element, wx, wy, tolerance);
    }
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    let t = tolerance;
    if wx < rect.x - t
        || wx > rect.x + rect.width + t
        || wy < rect.y - t
        || wy > rect.y + rect.height + t
    {
        return false;
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let rx = rect.width / 2.0 + t;
    let ry = rect.height / 2.0 + t;
    if rx <= 0.0 || ry <= 0.0 {
        return true;
    }
    let nx = (wx - cx) / rx;
    let ny = (wy - cy) / ry;
    match element.kind {
        DrawElementType::Ellipse => nx * nx + ny * ny <= 1.0,
        DrawElementType::Diamond => nx.abs() + ny.abs() <= 1.0,
        _ => true,
    }
}

fn hit_linear(element: &DrawElement, wx: f64, wy: f64, tolerance: f64) -> bool {
    let points = match &element.points {
        Some(points) if points.len() >= 2 => points,
        _ => return false,
    };
    let reach = tolerance.max(element.stroke_width) + 4.0;
    for window in points.windows(2) {
        let ax = element.x + window[0][0];
        let ay = element.y + window[0][1];
        let bx = element.x + window[1][0];
        let by = element.y + window[1][1];
        if distance_to_segment(wx, wy, ax, ay, bx, by) <= reach {
            return true;
        }
    }
    false
}

pub fn hit_test(
    elements: &[DrawElement],
    wx: f64,
    wy: f64,
    tolerance: f64,
) -> Option<&DrawElement> {
    for element in elements.iter().rev() {
        if element.is_deleted {
            continue;
        }
        if hit_test_element(element, wx, wy, tolerance) {
            return Some(element);
        }
    }
    None
}
