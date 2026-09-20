use std::f64::consts::PI;

#[derive(Clone, Debug, PartialEq)]
pub struct LinearDrag {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub points: Vec<[f64; 2]>,
}

pub fn constrain_to_angle(dx: f64, dy: f64) -> (f64, f64) {
    let length = dx.hypot(dy);
    if length < 1e-6 {
        return (0.0, 0.0);
    }
    let step = PI / 4.0;
    let angle = (dy.atan2(dx) / step).round() * step;
    (angle.cos() * length, angle.sin() * length)
}

pub fn linear_from_drag(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    constrain: bool,
) -> LinearDrag {
    let mut dx = end_x - start_x;
    let mut dy = end_y - start_y;
    if constrain {
        (dx, dy) = constrain_to_angle(dx, dy);
    }
    LinearDrag {
        x: start_x,
        y: start_y,
        width: dx,
        height: dy,
        points: vec![[0.0, 0.0], [dx, dy]],
    }
}

pub fn is_degenerate_linear(width: f64, height: f64, min: f64) -> bool {
    width.hypot(height) < min
}
