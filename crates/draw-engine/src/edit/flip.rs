use std::collections::HashSet;

use crate::scene::binding::is_linear_element;
use crate::scene::element::DrawElement;
use crate::scene::geometry::{normalize_rect, scene_bounds};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipAxis {
    Horizontal,
    Vertical,
}

impl FlipAxis {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

/// Mirrors one element about the line `(lo + hi) / 2` on the chosen axis.
///
/// Three kinds of element, mirrored three ways, but all landing in the same place:
///
/// - **Rough-drawn shapes** negate their extent on that axis and move their anchor. The
///   sign *is* the mirror: the painter reads it as a negative scale, so the cached rough
///   geometry and the cached `Path2D` both stay valid and a flip costs one subtraction
///   and one sign change. This is what makes the shape visibly reverse at all — a
///   rectangle is symmetric, so repositioning it alone was a no-op and flipping one
///   appeared to do nothing.
/// - **Freehand strokes** mirror their points, because their shape lives in the point
///   list rather than in a width and height.
/// - **Text** only moves. Mirrored glyphs are not a drawing operation, they are a
///   rendering bug.
///
/// Reflection is an involution, and so is this: `x -> lo + hi - x` and `w -> -w` applied
/// twice returns exactly the original numbers.
fn flip_one(element: &DrawElement, axis: FlipAxis, lo: f64, hi: f64) -> DrawElement {
    let horizontal = axis == FlipAxis::Horizontal;
    let mut next = element.clone();
    // A mirror reverses the direction of rotation with it.
    next.angle = if element.angle != 0.0 {
        -element.angle
    } else {
        0.0
    };

    if is_linear_element(element) {
        next.points = Some(mirror_points(element, horizontal, 0.0, 0.0));
        mirror_extent(&mut next, element, horizontal, lo, hi);
        return next;
    }

    if element.kind == crate::scene::DrawElementType::Freedraw {
        // The points carry the shape, so they mirror within the element's own box and
        // the box itself keeps a positive extent.
        let rect = normalize_rect(element.x, element.y, element.width, element.height);
        next.points = Some(mirror_points(element, horizontal, rect.width, rect.height));
        next.x = rect.x;
        next.y = rect.y;
        next.width = rect.width;
        next.height = rect.height;
        if horizontal {
            next.x = lo + hi - (rect.x + rect.width);
        } else {
            next.y = lo + hi - (rect.y + rect.height);
        }
        return next;
    }

    if element.kind == crate::scene::DrawElementType::Text {
        // Moved, never mirrored: text read backwards is not a flip, it is broken text.
        let rect = normalize_rect(element.x, element.y, element.width, element.height);
        if horizontal {
            next.x = lo + hi - (rect.x + rect.width);
        } else {
            next.y = lo + hi - (rect.y + rect.height);
        }
        next.angle = element.angle;
        return next;
    }

    mirror_extent(&mut next, element, horizontal, lo, hi);
    next
}

/// Reflects the anchor across the midline and negates the extent on that axis.
fn mirror_extent(
    next: &mut DrawElement,
    element: &DrawElement,
    horizontal: bool,
    lo: f64,
    hi: f64,
) {
    if horizontal {
        next.x = lo + hi - element.x;
        next.width = -element.width;
    } else {
        next.y = lo + hi - element.y;
        next.height = -element.height;
    }
}

/// Mirrors a point list about `span / 2`, or about the origin when `span` is zero.
fn mirror_points(
    element: &DrawElement,
    horizontal: bool,
    span_x: f64,
    span_y: f64,
) -> Vec<[f64; 2]> {
    element
        .points
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|&[px, py]| {
            if horizontal {
                [span_x - px, py]
            } else {
                [px, span_y - py]
            }
        })
        .collect()
}

pub fn flip_elements(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    axis: FlipAxis,
) -> Vec<DrawElement> {
    let targets: Vec<&DrawElement> = elements
        .iter()
        .filter(|el| {
            ids.contains(&el.id) && !el.is_deleted && !el.locked() && el.container_id.is_none()
        })
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }
    let owned: Vec<DrawElement> = targets.into_iter().cloned().collect();
    let Some(bounds) = scene_bounds(&owned) else {
        return Vec::new();
    };
    let (lo, hi) = if axis == FlipAxis::Horizontal {
        (bounds.min_x, bounds.max_x)
    } else {
        (bounds.min_y, bounds.max_y)
    };
    owned
        .iter()
        .map(|element| flip_one(element, axis, lo, hi))
        .collect()
}
