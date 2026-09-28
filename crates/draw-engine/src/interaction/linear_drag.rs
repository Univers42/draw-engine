#[derive(Clone, Debug, PartialEq)]
pub struct LinearDrag {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub points: Vec<[f64; 2]>,
}

/// The single place a Shift-locked drag is worked out, for every caller: a dragged
/// endpoint, a preview point, a line drawn by dragging, and an elbow's end.
///
/// The geometry is [`crate::math::shift_locked_delta`]'s — the oracle's ray-intersection,
/// not a rotation of the delta — and the step is the oracle's
/// `SHIFT_LOCKING_ANGLE` (`packages/common/src/constants.ts@1118751f:31`). It was `PI / 4`
/// and a rotation, which put the length a rotated delta puts it.
///
/// A drag too short to have a direction is dropped rather than snapped, so a press that
/// has not yet moved does not leave a stub pointing at 0 degrees.
pub fn constrain_to_angle(dx: f64, dy: f64) -> (f64, f64) {
    if dx.hypot(dy) < 1e-6 {
        return (0.0, 0.0);
    }
    crate::math::shift_locked_delta(dx, dy)
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
