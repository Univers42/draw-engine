use crate::camera::WorldBounds;
use crate::scene::element::DrawElement;
use crate::scene::geometry::element_bounds;

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

pub fn elements_in_marquee(elements: &[DrawElement], rect: WorldBounds) -> Vec<String> {
    elements
        .iter()
        .filter(|element| !element.is_deleted && overlaps(element_bounds(element), rect))
        .map(|element| element.id.clone())
        .collect()
}
