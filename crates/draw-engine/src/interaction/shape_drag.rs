use crate::scene::geometry::{normalize_rect, Rect};

pub fn rect_from_drag(sx: f64, sy: f64, cx: f64, cy: f64, square: bool) -> Rect {
    let mut dx = cx - sx;
    let mut dy = cy - sy;
    if square {
        let side = dx.abs().max(dy.abs());
        dx = if dx < 0.0 { -1.0 } else { 1.0 } * side;
        dy = if dy < 0.0 { -1.0 } else { 1.0 } * side;
    }
    normalize_rect(sx, sy, dx, dy)
}

pub fn is_degenerate_rect(rect: Rect, min: f64) -> bool {
    rect.width < min && rect.height < min
}
