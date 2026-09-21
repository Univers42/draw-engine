use crate::camera::WorldBounds;
use crate::scene::element::DrawElement;
use crate::scene::geometry::element_rotated_bounds;

pub fn marquee_rect(sx: f64, sy: f64, cx: f64, cy: f64) -> WorldBounds {
    WorldBounds {
        min_x: sx.min(cx),
        min_y: sy.min(cy),
        max_x: sx.max(cx),
        max_y: sy.max(cy),
    }
}

/// Whether `inner` lies wholly within `outer`.
fn contains(outer: WorldBounds, inner: WorldBounds) -> bool {
    outer.min_x <= inner.min_x
        && outer.min_y <= inner.min_y
        && outer.max_x >= inner.max_x
        && outer.max_y >= inner.max_y
}

/// Whether the marquee takes this element.
///
/// **Containment, not overlap.** Excalidraw's `getElementsWithinSelection` requires the
/// element to sit entirely inside the rectangle, and it makes a large difference in
/// practice: on overlap, a small drag anywhere across a big background shape picks the
/// big shape up too, so a marquee intended to gather a few small items quietly grabs the
/// frame around them. Confirmed against excalidraw.com — a marquee that encloses a
/// rectangle selects it, and one that clips its corner, crosses its middle or touches its
/// edge selects nothing.
///
/// Measured against the element's **rotated** box, which is where it actually is.
fn taken_by(element: &DrawElement, rect: WorldBounds) -> bool {
    if element.is_deleted {
        return false;
    }
    // A label belongs to its container and is carried by it. Selecting one on its own
    // would let it be dragged out of the shape it labels.
    if element.container_id.is_some() {
        return false;
    }
    contains(rect, element_rotated_bounds(element))
}

/// Ids of every live element the marquee encloses.
///
/// Generic over the iterator so the selection path can walk the scene by reference
/// instead of cloning every element on the board to read four numbers off each.
pub fn elements_in_marquee_among<'a>(
    elements: impl Iterator<Item = &'a DrawElement>,
    rect: WorldBounds,
) -> Vec<String> {
    elements
        .filter(|element| taken_by(element, rect))
        .map(|element| element.id.clone())
        .collect()
}

pub fn elements_in_marquee(elements: &[DrawElement], rect: WorldBounds) -> Vec<String> {
    elements_in_marquee_among(elements.iter(), rect)
}
