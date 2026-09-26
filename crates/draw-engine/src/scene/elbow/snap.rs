//! Where an elbow arrow's end lands on a shape: the oracle's elbow branch of
//! `bindPointToSnapToElementOutline` and the fixed-point arithmetic around it
//! (`binding.ts@1118751f:1550-1830, 2106-2170, 2646-2760`, `utils.ts@1118751f:640-790`).

use super::heading::{heading_for_point_from_element, vector_to_heading, Heading};
use super::outline::{
    aabb, bounds_center, center, distance, distance_sq, distance_to, intersect, normalize, rotate,
    Outline, Pt, Target,
};
use crate::scene::binding::max_binding_distance;

/// `BASE_BINDING_GAP`.
pub const BASE_BINDING_GAP: f64 = 5.0;

/// `getBindingGap`: uncapped, unlike the engine's own [`crate::scene::binding::binding_gap`],
/// which shrinks with a shape under 24 units — the router is held to the oracle's number.
pub fn binding_gap(t: &Target) -> f64 {
    BASE_BINDING_GAP + t.stroke_width / 2.0
}

/// `Math.min(Math.max(value, min), max)`.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

/// `normalizeFixedPoint`: bounded to ±10, and never exactly a half — a ratio within 1e-4
/// of one becomes 0.5001, "to avoid jumping arrow heading due to floating point
/// imprecision".
pub fn normalize_fixed_point(fixed: [f64; 2]) -> [f64; 2] {
    const BOUND: f64 = 10.0;
    const EPSILON: f64 = 0.0001;
    if !fixed.iter().all(|r| r.is_finite()) {
        return [0.5001, 0.5001];
    }
    let clamped = fixed.map(|r| clamp(r, -BOUND, BOUND));
    if clamped.iter().any(|r| (r - 0.5).abs() < EPSILON) {
        return clamped.map(|r| if (r - 0.5).abs() < EPSILON { 0.5001 } else { r });
    }
    clamped
}

/// `getGlobalFixedPointForBindableElement`.
pub fn global_fixed_point(fixed: [f64; 2], t: &Target) -> Pt {
    let [fx, fy] = normalize_fixed_point(fixed);
    rotate(
        [t.x + t.width * fx, t.y + t.height * fy],
        center(t),
        t.angle,
    )
}

/// `getHeadingForElbowArrowSnap`: the far end's direction when there is no shape, the
/// direction from the shape's middle when the end is off its outline (or exactly on it),
/// and otherwise the cone the end is in.
pub fn heading_for_snap(p: Pt, other: Pt, target: Option<&Target>, orig: Pt, zoom: f64) -> Heading {
    let other_heading = vector_to_heading([other[0] - p[0], other[1] - p[1]]);
    let Some(t) = target else {
        return other_heading;
    };
    // `getBindPointHeading`: the box grown by how far the end is from the outline.
    let grown = aabb(t, Some([distance_to(t, p); 4]));
    let distance = distance_to(t, orig);
    if distance > max_binding_distance(zoom) || distance == 0.0 || distance.is_nan() {
        let c = center(t);
        return vector_to_heading([p[0] - c[0], p[1] - c[1]]);
    }
    heading_for_point_from_element(t, grown, p)
}

/// `getAllMidpoints`: right, bottom, left, top.
fn midpoints(t: &Target) -> [Pt; 4] {
    let c = center(t);
    if t.outline == Outline::Diamond {
        return super::outline::diamond_base_corners(t)
            .map(|curve| rotate(super::outline::bezier(&curve, 0.5), c, t.angle));
    }
    [
        [t.width, t.height / 2.0],
        [t.width / 2.0, t.height],
        [0.0, t.height / 2.0],
        [t.width / 2.0, 0.0],
    ]
    .map(|[x, y]| rotate([t.x + x, t.y + y], c, t.angle))
}

/// `getElbowArrowSnapMidPoint`: the side midpoint a point near a shape's axis snaps to
/// (`onAxis`), or on a diamond the middle of the edge it is beside.
pub fn snap_midpoint(point: Pt, t: &Target, zoom: f64) -> Option<(Pt, bool)> {
    const TOLERANCE: f64 = 0.05;
    let max_distance = max_binding_distance(zoom) + t.stroke_width / 2.0;
    let horizontal = clamp(TOLERANCE * t.width, 5.0, max_distance);
    let vertical = clamp(TOLERANCE * t.height, 5.0, max_distance);
    let c = center(t);
    let p = rotate(point, c, -t.angle);
    let gap = binding_gap(t);
    if distance(c, p) < gap {
        return None;
    }
    let [right, bottom, left, top] = midpoints(t);
    let (x, y, w, h) = (t.x, t.y, t.width, t.height);
    let in_row = p[1] > c[1] - vertical && p[1] < c[1] + vertical;
    let in_column = p[0] > c[0] - horizontal && p[0] < c[0] + horizontal;
    if p[0] <= x + w / 2.0 && in_row {
        return Some((left, true));
    }
    if p[1] <= y + h / 2.0 && in_column {
        return Some((top, true));
    }
    if p[0] >= x + w / 2.0 && in_row {
        return Some((right, true));
    }
    if p[1] >= y + h / 2.0 && in_column {
        return Some((bottom, true));
    }
    if t.outline == Outline::Diamond {
        let threshold = horizontal.max(vertical);
        let edges = [
            [x + w / 4.0, y + h / 4.0, -1.0, -1.0],
            [x + (3.0 * w) / 4.0, y + h / 4.0, 1.0, -1.0],
            [x + w / 4.0, y + (3.0 * h) / 4.0, -1.0, 1.0],
            [x + (3.0 * w) / 4.0, y + (3.0 * h) / 4.0, 1.0, 1.0],
        ];
        for [ex, ey, dx, dy] in edges {
            let zone = [ex + dx * gap, ey + dy * gap];
            if distance(zone, p) < threshold {
                return Some((rotate([ex, ey], c, t.angle), false));
            }
        }
    }
    None
}

/// `avoidRectangularCorner`: a point off a box's corner is moved onto the nearer of the
/// two sides meeting there, a gap out.
fn avoid_corner(t: &Target, p: Pt) -> Pt {
    let c = center(t);
    let q = rotate(p, c, -t.angle);
    let gap = binding_gap(t);
    let (x, y, w, h) = (t.x, t.y, t.width, t.height);
    let at = |px: f64, py: f64| rotate([px, py], c, t.angle);
    if q[0] < x && q[1] < y {
        if q[1] - y > -gap {
            return at(x - gap, y);
        }
        return at(x, y - gap);
    }
    if q[0] < x && q[1] > y + h {
        if q[0] - x > -gap {
            return at(x, y + h + gap);
        }
        return at(x - gap, y + h);
    }
    if q[0] > x + w && q[1] > y + h {
        if q[0] - x < w + gap {
            return at(x + w, y + h + gap);
        }
        return at(x + w + gap, y + h);
    }
    if q[0] > x + w && q[1] < y {
        if q[0] - x < w + gap {
            return at(x + w, y - gap);
        }
        return at(x + w + gap, y);
    }
    p
}

/// The nearest of `hits` to `to`, the first of equals: `sort(byDistance)[0]`.
fn nearest(hits: Vec<Pt>, to: Pt) -> Option<Pt> {
    let mut best: Option<(Pt, f64)> = None;
    for p in hits {
        let d = distance_sq(p, to);
        if best.is_none_or(|(_, b)| d < b) {
            best = Some((p, d));
        }
    }
    best.map(|(p, _)| p)
}

/// `bindPointToSnapToElementOutline` for an elbow arrow whose end is at `point`: onto the
/// shape's outline a gap out, along the axis through its middle that the end is on.
pub fn snap_to_outline(
    point: Pt,
    points_len: usize,
    t: &Target,
    zoom: f64,
    midpoint_snapping: bool,
) -> Pt {
    const PRECISION: f64 = 10e-5;
    if points_len < 2 {
        return point;
    }
    let edge = if t.outline == Outline::Diamond || t.outline == Outline::Rectanguloid {
        avoid_corner(t, point)
    } else {
        point
    };
    let gap = binding_gap(t);
    let bounds = aabb(t, None);
    let middle = bounds_center(bounds);
    let snap = if midpoint_snapping {
        snap_midpoint(edge, t, zoom)
    } else {
        None
    };
    let resolved = snap.map_or(point, |(p, _)| p);
    let horizontal = match snap {
        Some((p, true)) => vector_to_heading([p[0] - middle[0], p[1] - middle[1]]),
        _ => heading_for_point_from_element(t, bounds, point),
    }
    .is_horizontal();
    let reach = t.width.max(t.height) * 2.0;
    let ray = |from: Pt| {
        let d = normalize([resolved[0] - from[0], resolved[1] - from[1]]);
        [from, [from[0] + d[0] * reach, from[1] + d[1] * reach]]
    };
    let across = [
        if horizontal { middle[0] } else { resolved[0] },
        if horizontal { resolved[1] } else { middle[1] },
    ];
    let mut hit = nearest(intersect(t, ray(across), gap), resolved);
    if hit.is_none() {
        let along = [
            if horizontal { resolved[0] } else { middle[0] },
            if horizontal { middle[1] } else { resolved[1] },
        ];
        hit = nearest(intersect(t, ray(along), BASE_BINDING_GAP), resolved);
    }
    match hit {
        Some(p) if distance_sq(edge, p) >= PRECISION => p,
        _ => edge,
    }
}

/// `calculateFixedPointForElbowArrowBinding`: the snapped end as a ratio of the shape's
/// unrotated box, each axis divided by at least the gap.
pub fn fixed_point_for(
    point: Pt,
    points_len: usize,
    t: &Target,
    zoom: f64,
    snap: bool,
    midpoint_snapping: bool,
) -> [f64; 2] {
    const MIN_BINDABLE_SIZE: f64 = 1.0;
    let bounds = [t.x, t.y, t.x + t.width, t.y + t.height];
    let snapped = if snap {
        snap_to_outline(point, points_len, t, zoom, midpoint_snapping)
    } else {
        point
    };
    let middle = [
        bounds[0] + (bounds[2] - bounds[0]) / 2.0,
        bounds[1] + (bounds[3] - bounds[1]) / 2.0,
    ];
    let q = rotate(snapped, middle, -t.angle);
    if t.width < MIN_BINDABLE_SIZE || t.height < MIN_BINDABLE_SIZE {
        return normalize_fixed_point([0.5, 0.5]);
    }
    let floor = binding_gap(t);
    normalize_fixed_point([
        (q[0] - t.x) / t.width.max(floor),
        (q[1] - t.y) / t.height.max(floor),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_is_never_exactly_a_half() {
        assert_eq!(normalize_fixed_point([0.5, 0.2]), [0.5001, 0.2]);
        assert_eq!(normalize_fixed_point([0.49995, 12.0]), [0.5001, 10.0]);
        assert_eq!(normalize_fixed_point([f64::NAN, 0.0]), [0.5001, 0.5001]);
    }
}
