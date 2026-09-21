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

fn overlaps(a: WorldBounds, b: WorldBounds) -> bool {
    a.min_x <= b.max_x && a.max_x >= b.min_x && a.min_y <= b.max_y && a.max_y >= b.min_y
}

/// Ids of every live element the marquee touches.
///
/// Measured against the element's **rotated** box. Using the unrotated one meant a turned
/// shape was caught by a marquee drawn over empty space next to it, and missed by one
/// drawn over the part of it that sticks out.
pub fn elements_in_marquee(elements: &[DrawElement], rect: WorldBounds) -> Vec<String> {
    elements
        .iter()
        .filter(|element| !element.is_deleted && overlaps(element_rotated_bounds(element), rect))
        .map(|element| element.id.clone())
        .collect()
}
