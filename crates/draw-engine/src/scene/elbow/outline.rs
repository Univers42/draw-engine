//! A shape's outline as the oracle's elbow router measures it.
//!
//! The router's decisions turn on a handful of measurements — how far an end is from a
//! shape, where a ray leaves it, the box around it turned — and a route is only the
//! oracle's route when every one of them is the oracle's number. So this is a
//! transcription, operation for operation, of the functions the router reaches at the
//! pin: `getElementBounds`/`aabbForElement`/`elementCenterPoint` (`bounds.ts@1118751f`),
//! `distanceToElement` (`distance.ts`), `intersectElementWithLineSegment`
//! (`collision.ts:488-830`), `deconstructRectanguloidElement`/`deconstructDiamondElement`
//! (`utils.ts:245-508`), and the curve and segment math under them
//! (`packages/math/src`). It is kept apart from `outline_distance.rs`, which is the exact
//! distance the rest of the engine binds with: the oracle approximates its rounded corners
//! with sampled Béziers, and an exact answer here would be a different route.

use crate::scene::element::{DrawElement, DrawElementType};

pub type Pt = [f64; 2];
/// `[min_x, min_y, max_x, max_y]`, the oracle's `Bounds`.
pub type Bounds = [f64; 4];
pub type Curve = [Pt; 4];
pub type Segment = [Pt; 2];

/// `Math.hypot` as V8 computes it (`src/builtins/math.tq`): each term scaled by the
/// largest and Kahan-summed. Not libm's `hypot`, which can differ in the last place — and
/// a last place is a different side of `DEDUP_TRESHOLD` often enough to matter.
pub fn hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a == f64::INFINITY || b == f64::INFINITY {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let max = if b > a { b } else { a };
    if max == 0.0 {
        return 0.0;
    }
    let (mut sum, mut compensation) = (0.0_f64, 0.0_f64);
    for value in [a, b] {
        let n = value / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `pointDistance`.
pub fn distance(a: Pt, b: Pt) -> f64 {
    hypot(b[0] - a[0], b[1] - a[1])
}

/// `pointDistanceSq`.
pub fn distance_sq(a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    dx * dx + dy * dy
}

/// `pointRotateRads`: unchanged, not merely unmoved, at no angle.
pub fn rotate(p: Pt, center: Pt, angle: f64) -> Pt {
    if angle == 0.0 || angle.is_nan() {
        return p;
    }
    let ([x, y], [cx, cy]) = (p, center);
    [
        (x - cx) * angle.cos() - (y - cy) * angle.sin() + cx,
        (x - cx) * angle.sin() + (y - cy) * angle.cos() + cy,
    ]
}

/// `pointScaleFromOrigin`.
pub fn scale_from(p: Pt, mid: Pt, multiplier: f64) -> Pt {
    [
        mid[0] + (p[0] - mid[0]) * multiplier,
        mid[1] + (p[1] - mid[1]) * multiplier,
    ]
}

/// `pointsEqual` with its `PRECISION` tolerance.
pub fn points_equal(a: Pt, b: Pt) -> bool {
    const PRECISION: f64 = 10e-5;
    (a[0] - b[0]).abs() < PRECISION && (a[1] - b[1]).abs() < PRECISION
}

/// `vectorNormalize`.
pub fn normalize(v: Pt) -> Pt {
    let m = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if m == 0.0 {
        return [0.0, 0.0];
    }
    [v[0] / m, v[1] / m]
}

/// `vectorCross`.
pub fn cross(a: Pt, b: Pt) -> f64 {
    a[0] * b[1] - b[0] * a[1]
}

/// `getCenterForBounds`.
pub fn bounds_center(b: Bounds) -> Pt {
    [b[0] + (b[2] - b[0]) / 2.0, b[1] + (b[3] - b[1]) / 2.0]
}

/// `pointInsideBounds`: strictly inside.
pub fn inside_bounds(p: Pt, b: Bounds) -> bool {
    p[0] > b[0] && p[0] < b[2] && p[1] > b[1] && p[1] < b[3]
}

/// `doBoundsIntersect`: bounds that only touch do not.
pub fn bounds_intersect(a: Bounds, b: Bounds) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

/// `Math.min` over a list. `f64::min` differs on `-0.0`/`0.0` and NaN, and a signed zero
/// can reach a division later.
pub fn js_min(values: &[f64]) -> f64 {
    let mut out = f64::INFINITY;
    for &v in values {
        if v.is_nan() {
            return f64::NAN;
        }
        if v < out || (v == 0.0 && out == 0.0 && v.is_sign_negative()) {
            out = v;
        }
    }
    out
}

/// `Math.max` over a list; see [`js_min`].
pub fn js_max(values: &[f64]) -> f64 {
    let mut out = f64::NEG_INFINITY;
    for &v in values {
        if v.is_nan() {
            return f64::NAN;
        }
        if v > out || (v == 0.0 && out == 0.0 && out.is_sign_negative()) {
            out = v;
        }
    }
    out
}

/// Which of the oracle's three outlines a shape is measured as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outline {
    /// `isRectanguloidElement`: rectangles, notes, pictures, embeds, frames, text — and
    /// the engine's own figures, which the oracle has no outline for.
    Rectanguloid,
    Diamond,
    Ellipse,
}

/// A shape an elbow arrow can bind to, with the fields the oracle's geometry reads.
///
/// The box is normalised: the oracle never holds a negative width, and an engine element
/// drawn right to left does.
#[derive(Clone, Copy, Debug)]
pub struct Target<'a> {
    pub element: &'a DrawElement,
    pub outline: Outline,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub stroke_width: f64,
}

impl<'a> Target<'a> {
    /// `isBindableElement`: `None` for anything an arrow cannot bind to.
    pub fn of(element: &'a DrawElement) -> Option<Self> {
        let outline = match element.kind {
            DrawElementType::Diamond => Outline::Diamond,
            DrawElementType::Ellipse => Outline::Ellipse,
            DrawElementType::Rectangle
            | DrawElementType::StickyNote
            | DrawElementType::Image
            | DrawElementType::Embed
            | DrawElementType::Frame
            | DrawElementType::Figure => Outline::Rectanguloid,
            DrawElementType::Text if element.container_id.is_none() => Outline::Rectanguloid,
            _ => return None,
        };
        let rect = crate::scene::geometry::normalize_rect(
            element.x,
            element.y,
            element.width,
            element.height,
        );
        Some(Self {
            element,
            outline,
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            angle: element.angle,
            stroke_width: element.stroke_width,
        })
    }

    /// `getCornerRadius(x, element)`, through the engine's own corner rule — the same
    /// numbers for every rounding the oracle has.
    fn corner_radius(&self, x: f64) -> f64 {
        // Text has no roundness, and neither do the oracle's frames (`FRAME_STYLE`); ours
        // carry one for painting alone, as `outline_distance.rs` notes.
        if matches!(
            self.element.kind,
            DrawElementType::Text | DrawElementType::Frame
        ) {
            return 0.0;
        }
        crate::render::shape::corner_radius(x, self.element)
    }

    fn rounded(&self) -> bool {
        self.element.roundness.is_some()
    }
}

/// `getElementBounds`: the box around the shape as turned, tight to a diamond's corners
/// and an ellipse's curve.
pub fn element_bounds(t: &Target) -> Bounds {
    let (x1, y1, x2, y2) = (t.x, t.y, t.x + t.width, t.y + t.height);
    let (cx, cy) = (t.x + t.width / 2.0, t.y + t.height / 2.0);
    let c = [cx, cy];
    match t.outline {
        Outline::Diamond => {
            let a = rotate([cx, y1], c, t.angle);
            let b = rotate([cx, y2], c, t.angle);
            let d = rotate([x1, cy], c, t.angle);
            let e = rotate([x2, cy], c, t.angle);
            [
                js_min(&[a[0], b[0], d[0], e[0]]),
                js_min(&[a[1], b[1], d[1], e[1]]),
                js_max(&[a[0], b[0], d[0], e[0]]),
                js_max(&[a[1], b[1], d[1], e[1]]),
            ]
        }
        Outline::Ellipse => {
            let (w, h) = ((x2 - x1) / 2.0, (y2 - y1) / 2.0);
            let (cos, sin) = (t.angle.cos(), t.angle.sin());
            let ww = hypot(w * cos, h * sin);
            let hh = hypot(h * cos, w * sin);
            [cx - ww, cy - hh, cx + ww, cy + hh]
        }
        Outline::Rectanguloid => {
            let a = rotate([x1, y1], c, t.angle);
            let b = rotate([x1, y2], c, t.angle);
            let d = rotate([x2, y2], c, t.angle);
            let e = rotate([x2, y1], c, t.angle);
            [
                js_min(&[a[0], b[0], d[0], e[0]]),
                js_min(&[a[1], b[1], d[1], e[1]]),
                js_max(&[a[0], b[0], d[0], e[0]]),
                js_max(&[a[1], b[1], d[1], e[1]]),
            ]
        }
    }
}

/// `elementCenterPoint`: the middle of [`element_bounds`], not `x + width / 2`.
pub fn center(t: &Target) -> Pt {
    bounds_center(element_bounds(t))
}

/// `aabbForElement`, with its optional `[top, right, down, left]` growth.
pub fn aabb(t: &Target, offset: Option<[f64; 4]>) -> Bounds {
    let c = center(t);
    let corners = [
        rotate([t.x, t.y], c, t.angle),
        rotate([t.x + t.width, t.y], c, t.angle),
        rotate([t.x + t.width, t.y + t.height], c, t.angle),
        rotate([t.x, t.y + t.height], c, t.angle),
    ];
    let xs = corners.map(|p| p[0]);
    let ys = corners.map(|p| p[1]);
    let b = [js_min(&xs), js_min(&ys), js_max(&xs), js_max(&ys)];
    match offset {
        Some([top, right, down, left]) => [b[0] - left, b[1] - top, b[2] + right, b[3] + down],
        None => b,
    }
}

// ------------------------------------------------------------------------ curve math

/// `x ** 2` as V8 evaluates it: fdlibm's `pow` returns `x * x` for an exponent of two.
pub fn squared(x: f64) -> f64 {
    x * x
}

/// `x ** 3`: no shortcut in fdlibm, a full `pow`.
pub fn cubed(x: f64) -> f64 {
    x.powf(3.0)
}

/// `bezierEquation`.
pub fn bezier(c: &Curve, t: f64) -> Pt {
    let u = 1.0 - t;
    let at = |i: usize| {
        cubed(u) * c[0][i]
            + 3.0 * squared(u) * t * c[1][i]
            + 3.0 * u * squared(t) * c[2][i]
            + cubed(t) * c[3][i]
    };
    [at(0), at(1)]
}

/// `curveTangent`.
fn tangent(c: &Curve, t: f64) -> Pt {
    let at = |i: usize| {
        -3.0 * (1.0 - t) * (1.0 - t) * c[0][i] + 3.0 * (1.0 - t) * (1.0 - t) * c[1][i]
            - 6.0 * t * (1.0 - t) * c[1][i]
            - 3.0 * t * t * c[2][i]
            + 6.0 * t * (1.0 - t) * c[2][i]
            + 3.0 * t * t * c[3][i]
    };
    [at(0), at(1)]
}

/// `curveOffsetPoints(c, offset, 50)`.
fn offset_points(c: &Curve, offset: f64) -> Vec<Pt> {
    const STEPS: usize = 50;
    (0..=STEPS)
        .map(|i| {
            let t = i as f64 / STEPS as f64;
            let p = bezier(c, t);
            let d = normalize(tangent(c, t));
            let normal = [d[1], -d[0]];
            [p[0] + normal[0] * offset, p[1] + normal[1] * offset]
        })
        .collect()
}

/// `curveCatmullRomCubicApproxPoints(points, 0.5)`.
fn catmull_rom(points: &[Pt]) -> Vec<Curve> {
    const TENSION: f64 = 0.5;
    let n = points.len();
    (0..n.saturating_sub(1))
        .map(|i| {
            let p0 = points[i.saturating_sub(1)];
            let p1 = points[i];
            let p2 = points[(i + 1).min(n - 1)];
            let p3 = points[(i + 2).min(n - 1)];
            let t1 = [(p2[0] - p0[0]) * TENSION, (p2[1] - p0[1]) * TENSION];
            let t2 = [(p3[0] - p1[0]) * TENSION, (p3[1] - p1[1]) * TENSION];
            [
                p1,
                [p1[0] + t1[0] / 3.0, p1[1] + t1[1] / 3.0],
                [p2[0] - t2[0] / 3.0, p2[1] - t2[1] / 3.0],
                p2,
            ]
        })
        .collect()
}

/// `solveWithAnalyticalJacobian`: Newton on bezier(t) = line(s).
fn solve(
    c: &Curve,
    l: &Segment,
    mut t0: f64,
    mut s0: f64,
    tolerance: f64,
    limit: u32,
) -> Option<Pt> {
    let mut error = f64::INFINITY;
    let mut iter = 0;
    while error >= tolerance {
        if iter >= limit {
            return None;
        }
        let bt = 1.0 - t0;
        let bt2 = bt * bt;
        let bt3 = bt2 * bt;
        let t0_2 = t0 * t0;
        let t0_3 = t0_2 * t0;
        let bezier_x =
            bt3 * c[0][0] + 3.0 * bt2 * t0 * c[1][0] + 3.0 * bt * t0_2 * c[2][0] + t0_3 * c[3][0];
        let bezier_y =
            bt3 * c[0][1] + 3.0 * bt2 * t0 * c[1][1] + 3.0 * bt * t0_2 * c[2][1] + t0_3 * c[3][1];
        let line_x = l[0][0] + s0 * (l[1][0] - l[0][0]);
        let line_y = l[0][1] + s0 * (l[1][1] - l[0][1]);
        let fx = bezier_x - line_x;
        let fy = bezier_y - line_y;
        error = fx.abs() + fy.abs();
        if error < tolerance {
            break;
        }
        let dfx_dt = -3.0 * bt2 * c[0][0] + 3.0 * bt2 * c[1][0]
            - 6.0 * bt * t0 * c[1][0]
            - 3.0 * t0_2 * c[2][0]
            + 6.0 * bt * t0 * c[2][0]
            + 3.0 * t0_2 * c[3][0];
        let dfy_dt = -3.0 * bt2 * c[0][1] + 3.0 * bt2 * c[1][1]
            - 6.0 * bt * t0 * c[1][1]
            - 3.0 * t0_2 * c[2][1]
            + 6.0 * bt * t0 * c[2][1]
            + 3.0 * t0_2 * c[3][1];
        let dfx_ds = -(l[1][0] - l[0][0]);
        let dfy_ds = -(l[1][1] - l[0][1]);
        let det = dfx_dt * dfy_ds - dfx_ds * dfy_dt;
        if det.abs() < 1e-12 {
            return None;
        }
        let inv_det = 1.0 / det;
        let dt = inv_det * (dfy_ds * -fx - dfx_ds * -fy);
        let ds = inv_det * (-dfy_dt * -fx + dfx_dt * -fy);
        t0 += dt;
        s0 += ds;
        iter += 1;
    }
    Some([t0, s0])
}

/// `curveIntersectLineSegment` with its defaults (tolerance 1e-2, four iterations), from
/// the oracle's three starting guesses in order.
fn curve_hits_segment(c: &Curve, l: &Segment) -> Option<Pt> {
    [0.5, 0.2, 0.8].into_iter().find_map(|t0| {
        let [t, s] = solve(c, l, t0, 0.0, 1e-2, 4)?;
        if t < 0.0 || t > 1.0 || s < 0.0 || s > 1.0 {
            return None;
        }
        Some(bezier(c, t))
    })
}

/// `getBezierValueForT`.
fn bezier_value(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let u = 1.0 - t;
    cubed(u) * p0 + 3.0 * squared(u) * t * p1 + 3.0 * u * squared(t) * p2 + cubed(t) * p3
}

/// `solveQuadratic`: the curve's extremes on one axis, where they fall inside it.
fn extremes(p0: f64, p1: f64, p2: f64, p3: f64) -> Option<[Option<f64>; 2]> {
    let (i, j, k) = (p1 - p0, p2 - p1, p3 - p2);
    let a = 3.0 * i - 6.0 * j + 3.0 * k;
    let b = 6.0 * j - 6.0 * i;
    let c = 3.0 * i;
    let sqrt_part = b * b - 4.0 * a * c;
    let has_solution = sqrt_part >= 0.0;
    if !has_solution {
        return None;
    }
    let (t1, t2) = if a == 0.0 {
        (-c / b, -c / b)
    } else {
        (
            (-b + sqrt_part.sqrt()) / (2.0 * a),
            (-b - sqrt_part.sqrt()) / (2.0 * a),
        )
    };
    let at = |t: f64| {
        (0.0..=1.0)
            .contains(&t)
            .then(|| bezier_value(t, p0, p1, p2, p3))
    };
    Some([at(t1), at(t2)])
}

/// `getCubicBezierCurveBound`.
fn curve_bounds(c: &Curve) -> Bounds {
    let axis = |i: usize| {
        let mut values = vec![c[0][i], c[3][i]];
        if let Some(found) = extremes(c[0][i], c[1][i], c[2][i], c[3][i]) {
            values.extend(found.into_iter().flatten());
        }
        (js_min(&values), js_max(&values))
    };
    let ((min_x, max_x), (min_y, max_y)) = (axis(0), axis(1));
    [min_x, min_y, max_x, max_y]
}

/// `curveClosestParameter`: thirty-one samples, then a bisection to `tolerance`.
fn closest_parameter(c: &Curve, p: Pt, tolerance: f64) -> f64 {
    const MAX_STEPS: usize = 30;
    let f = |t: f64| distance(p, bezier(c, t));
    let mut min = f64::INFINITY;
    let mut closest = 0;
    for step in 0..=MAX_STEPS {
        let d = f(step as f64 / MAX_STEPS as f64);
        if d < min {
            min = d;
            closest = step;
        }
    }
    let t0 = ((closest as f64 - 1.0) / MAX_STEPS as f64).max(0.0);
    let t1 = ((closest as f64 + 1.0) / MAX_STEPS as f64).min(1.0);
    let (mut m, mut n) = (t0, t1);
    let mut k = None;
    while n - m > tolerance {
        let mid = (n + m) / 2.0;
        k = Some(mid);
        if f(mid - tolerance) < f(mid + tolerance) {
            n = mid;
        } else {
            m = mid;
        }
    }
    k.unwrap_or(closest as f64 / MAX_STEPS as f64)
}

/// `curvePointDistance` with its 1e-3 tolerance.
fn curve_distance(c: &Curve, p: Pt) -> f64 {
    distance(p, bezier(c, closest_parameter(c, p, 1e-3)))
}

/// `distanceToLineSegment`.
pub fn segment_distance(p: Pt, l: &Segment) -> f64 {
    let ([x, y], [[x1, y1], [x2, y2]]) = (p, *l);
    let (a, b, c, d) = (x - x1, y - y1, x2 - x1, y2 - y1);
    let dot = a * c + b * d;
    let len_sq = c * c + d * d;
    let param = if len_sq != 0.0 { dot / len_sq } else { 0.0 };
    let t = 1.0_f64.min(param).max(0.0);
    distance(p, [x1 + t * (x2 - x1), y1 + t * (y2 - y1)])
}

/// `lineSegmentIntersectionPoints`: the crossing of the two lines, kept when it lies on
/// both segments to within `PRECISION`.
fn segments_cross(l: &Segment, s: &Segment) -> Option<Pt> {
    const PRECISION: f64 = 10e-5;
    let a1 = l[1][1] - l[0][1];
    let b1 = l[0][0] - l[1][0];
    let a2 = s[1][1] - s[0][1];
    let b2 = s[0][0] - s[1][0];
    let d = a1 * b2 - a2 * b1;
    if d == 0.0 {
        return None;
    }
    let c1 = a1 * l[0][0] + b1 * l[0][1];
    let c2 = a2 * s[0][0] + b2 * s[0][1];
    let p = [(c1 * b2 - c2 * b1) / d, (a1 * c2 - a2 * c1) / d];
    let on = |seg: &Segment| {
        let d = segment_distance(p, seg);
        d == 0.0 || d < PRECISION
    };
    (on(s) && on(l)).then_some(p)
}

// ------------------------------------------------------------------ deconstruction

/// `[sides, corners]`, unrotated: `ElementShape`.
struct Shape {
    sides: Vec<Segment>,
    corners: Vec<Curve>,
}

/// `getDiamondPoints`: the oracle's top and bottom sit one unit right of the middle and
/// its sides one unit below, floored — kept, since the router aims at them.
fn diamond_points(t: &Target) -> [f64; 8] {
    let top_x = (t.width / 2.0).floor() + 1.0;
    let right_y = (t.height / 2.0).floor() + 1.0;
    [top_x, 0.0, t.width, right_y, top_x, t.height, 0.0, right_y]
}

/// `getDiamondBaseCorners`.
pub fn diamond_base_corners(t: &Target) -> [Curve; 4] {
    let [top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y] = diamond_points(t);
    let (vr, hr) = if t.rounded() {
        (
            t.corner_radius((top_x - left_x).abs()),
            t.corner_radius((right_y - top_y).abs()),
        )
    } else {
        ((top_x - left_x) * 0.01, (right_y - top_y) * 0.01)
    };
    let top = [t.x + top_x, t.y + top_y];
    let right = [t.x + right_x, t.y + right_y];
    let bottom = [t.x + bottom_x, t.y + bottom_y];
    let left = [t.x + left_x, t.y + left_y];
    [
        [
            [right[0] - vr, right[1] - hr],
            right,
            right,
            [right[0] - vr, right[1] + hr],
        ],
        [
            [bottom[0] + vr, bottom[1] - hr],
            bottom,
            bottom,
            [bottom[0] - vr, bottom[1] - hr],
        ],
        [
            [left[0] + vr, left[1] + hr],
            left,
            left,
            [left[0] + vr, left[1] - hr],
        ],
        [
            [top[0] - vr, top[1] + hr],
            top,
            top,
            [top[0] + vr, top[1] + hr],
        ],
    ]
}

/// The four corners grown by `offset` — each a run of Catmull-Rom cubics through its
/// offset samples — or as they are, and the sides joining them.
fn assemble(base: [Curve; 4], offset: f64) -> Shape {
    let corners: Vec<Vec<Curve>> = if offset > 0.0 {
        base.iter()
            .map(|c| catmull_rom(&offset_points(c, offset)))
            .collect()
    } else {
        base.iter().map(|c| vec![*c]).collect()
    };
    let last = |i: usize| corners[i][corners[i].len() - 1][3];
    let sides = vec![
        [last(0), corners[1][0][0]],
        [last(1), corners[2][0][0]],
        [last(2), corners[3][0][0]],
        [last(3), corners[0][0][0]],
    ];
    Shape {
        sides,
        corners: corners.into_iter().flatten().collect(),
    }
}

/// `deconstructRectanguloidElement`.
fn rectanguloid(t: &Target, offset: f64) -> Shape {
    let mut radius = t.corner_radius(t.width.min(t.height));
    if radius == 0.0 {
        radius = 0.01;
    }
    let r = [[t.x, t.y], [t.x + t.width, t.y + t.height]];
    let top = [[r[0][0] + radius, r[0][1]], [r[1][0] - radius, r[0][1]]];
    let right = [[r[1][0], r[0][1] + radius], [r[1][0], r[1][1] - radius]];
    let bottom = [[r[0][0] + radius, r[1][1]], [r[1][0] - radius, r[1][1]]];
    let left = [[r[0][0], r[1][1] - radius], [r[0][0], r[0][1] + radius]];
    let toward = |p: Pt, cx: f64, cy: f64| {
        [
            p[0] + (2.0 / 3.0) * (cx - p[0]),
            p[1] + (2.0 / 3.0) * (cy - p[1]),
        ]
    };
    let base = [
        [
            left[1],
            toward(left[1], r[0][0], r[0][1]),
            toward(top[0], r[0][0], r[0][1]),
            top[0],
        ],
        [
            top[1],
            toward(top[1], r[1][0], r[0][1]),
            toward(right[0], r[1][0], r[0][1]),
            right[0],
        ],
        [
            right[1],
            toward(right[1], r[1][0], r[1][1]),
            toward(bottom[1], r[1][0], r[1][1]),
            bottom[1],
        ],
        [
            bottom[0],
            toward(bottom[0], r[0][0], r[1][1]),
            toward(left[0], r[0][0], r[1][1]),
            left[0],
        ],
    ];
    assemble(base, offset)
}

fn deconstruct(t: &Target, offset: f64) -> Shape {
    match t.outline {
        Outline::Diamond => assemble(diamond_base_corners(t), offset),
        _ => rectanguloid(t, offset),
    }
}

// ------------------------------------------------------------------------- queries

/// `distanceToElement` for the three outlines.
pub fn distance_to(t: &Target, p: Pt) -> f64 {
    let c = center(t);
    let q = rotate(p, c, -t.angle);
    if t.outline == Outline::Ellipse {
        return ellipse_distance(q, c, t.width / 2.0, t.height / 2.0);
    }
    let shape = deconstruct(t, 0.0);
    let sides = shape.sides.iter().map(|s| segment_distance(q, s));
    let corners = shape.corners.iter().map(|k| curve_distance(k, q));
    js_min(&sides.chain(corners).collect::<Vec<_>>())
}

/// `ellipseDistanceFromPoint`.
fn ellipse_distance(p: Pt, center: Pt, a: f64, b: f64) -> f64 {
    // `p + center × -1`, which is `p - center` to the bit.
    let tp = [p[0] - center[0], p[1] - center[1]];
    if a == b {
        return (hypot(tp[0], tp[1]) - a).abs();
    }
    let (px, py) = (tp[0].abs(), tp[1].abs());
    let (mut tx, mut ty) = (0.707_f64, 0.707_f64);
    for _ in 0..3 {
        let (x, y) = (a * tx, b * ty);
        let ex = ((a * a - b * b) * cubed(tx)) / a;
        let ey = ((b * b - a * a) * cubed(ty)) / b;
        let (rx, ry) = (x - ex, y - ey);
        let (qx, qy) = (px - ex, py - ey);
        let r = hypot(ry, rx);
        let q = hypot(qy, qx);
        if q == 0.0 {
            break;
        }
        tx = 1.0_f64.min(0.0_f64.max(((qx * r) / q + ex) / a));
        ty = 1.0_f64.min(0.0_f64.max(((qy * r) / q + ey) / b));
        let t = hypot(ty, tx);
        tx /= t;
        ty /= t;
    }
    let min = [
        a * tx * if tp[0] < 0.0 { -1.0 } else { 1.0 },
        b * ty * if tp[1] < 0.0 { -1.0 } else { 1.0 },
    ];
    distance(tp, min)
}

/// `intersectElementWithLineSegment(element, map, line, offset)`: where `line` crosses
/// the outline grown by `offset`, in the oracle's order — sides, then corners.
pub fn intersect(t: &Target, line: Segment, offset: f64) -> Vec<Pt> {
    let reach = [
        js_min(&[line[0][0] - offset, line[1][0] - offset]),
        js_min(&[line[0][1] - offset, line[1][1] - offset]),
        js_max(&[line[0][0] + offset, line[1][0] + offset]),
        js_max(&[line[0][1] + offset, line[1][1] + offset]),
    ];
    if !bounds_intersect(reach, element_bounds(t)) {
        return Vec::new();
    }
    let c = center(t);
    let turned = [rotate(line[0], c, -t.angle), rotate(line[1], c, -t.angle)];
    if t.outline == Outline::Ellipse {
        return ellipse_hits(c, t.width / 2.0 + offset, t.height / 2.0 + offset, &turned)
            .into_iter()
            .map(|p| rotate(p, c, t.angle))
            .collect();
    }
    let shape = deconstruct(t, offset);
    let mut hits: Vec<Pt> = shape
        .sides
        .iter()
        .filter_map(|side| segments_cross(side, &turned))
        .map(|p| rotate(p, c, t.angle))
        .collect();
    const CURVE_BOUNDS_EPSILON: f64 = 1e-6;
    let padded = [
        js_min(&[turned[0][0], turned[1][0]]) - CURVE_BOUNDS_EPSILON,
        js_min(&[turned[0][1], turned[1][1]]) - CURVE_BOUNDS_EPSILON,
        js_max(&[turned[0][0], turned[1][0]]) + CURVE_BOUNDS_EPSILON,
        js_max(&[turned[0][1], turned[1][1]]) + CURVE_BOUNDS_EPSILON,
    ];
    for corner in &shape.corners {
        if !bounds_intersect(curve_bounds(corner), padded) {
            continue;
        }
        if let Some(p) = curve_hits_segment(corner, &turned) {
            hits.push(rotate(p, c, t.angle));
        }
    }
    hits
}

/// `isPointInElement` for a bindable shape: inside its box, and an odd number of distinct
/// crossings on a ray from the point out through the far side (`collision.ts@1118751f:823-877`).
pub fn contains(t: &Target, p: Pt) -> bool {
    let [x1, y1, x2, y2] = element_bounds(t);
    if !(p[0] <= x1.max(x2) && p[0] >= x1.min(x2) && p[1] <= y1.max(y2) && p[1] >= y1.min(y2)) {
        return false;
    }
    let c = [(x1 + x2) / 2.0, (y1 + y2) / 2.0];
    // `vectorFromPoint(point, center, 0.1)`: straight down when the point is the middle.
    let away = [p[0] - c[0], p[1] - c[1]];
    let away = if away[0] * away[0] + away[1] * away[1] < 0.1 * 0.1 {
        [0.0, 1.0]
    } else {
        away
    };
    let reach = t.width.max(t.height) * 2.0;
    let d = normalize(away);
    let hits = intersect(t, [p, [c[0] + d[0] * reach, c[1] + d[1] * reach]], 0.0);
    let distinct = hits
        .iter()
        .enumerate()
        .filter(|&(i, q)| hits.iter().position(|r| points_equal(*r, *q)) == Some(i))
        .count();
    distinct % 2 == 1
}

/// `ellipseSegmentInterceptPoints`.
fn ellipse_hits(center: Pt, rx: f64, ry: f64, s: &Segment) -> Vec<Pt> {
    let dir = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
    let diff = [s[0][0] - center[0], s[0][1] - center[1]];
    let m_dir = [dir[0] / (rx * rx), dir[1] / (ry * ry)];
    let m_diff = [diff[0] / (rx * rx), diff[1] / (ry * ry)];
    let a = dir[0] * m_dir[0] + dir[1] * m_dir[1];
    let b = dir[0] * m_diff[0] + dir[1] * m_diff[1];
    let c = (diff[0] * m_diff[0] + diff[1] * m_diff[1]) - 1.0;
    let d = b * b - a * c;
    let at = |t: f64| {
        [
            s[0][0] + (s[1][0] - s[0][0]) * t,
            s[0][1] + (s[1][1] - s[0][1]) * t,
        ]
    };
    let mut out = Vec::new();
    if d > 0.0 {
        let t_a = (-b - d.sqrt()) / a;
        let t_b = (-b + d.sqrt()) / a;
        if (0.0..=1.0).contains(&t_a) {
            out.push(at(t_a));
        }
        if (0.0..=1.0).contains(&t_b) {
            out.push(at(t_b));
        }
    } else if d == 0.0 {
        let t = -b / a;
        if (0.0..=1.0).contains(&t) {
            out.push(at(t));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypot_is_v8s() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, -0.0), 0.0);
        // An infinity wins over a NaN, as `Math.hypot(NaN, Infinity)` does.
        assert_eq!(hypot(f64::NAN, f64::INFINITY), f64::INFINITY);
        assert!(hypot(f64::NAN, 1.0).is_nan());
    }
}
