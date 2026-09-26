//! How far a point is from a shape's real outline: positive inside, negative outside.
//!
//! The one geometric fact arrow binding turns on. Excalidraw picks the shape whose
//! outline is nearest an arrow end (`distanceToElement`, `packages/element/src/distance.ts`
//! @1118751f) and decides between binding inside and in orbit with `isPointInElement`
//! (`collision.ts@1118751f:823-878`). Both follow the outline as it is painted, rounded
//! corners included, so a point in the cut-away corner of a rounded square is outside it.
//!
//! Each shape is symmetric about both of its own axes, so the point is turned into the
//! shape's unrotated frame and folded into one quadrant: the nearest outline point is
//! always in the same quadrant as the point. A point exactly on the outline is outside,
//! as `isPointInElement` has it (measured on excalidraw.com: a point on a square's edge
//! binds it in orbit).

use crate::camera::Point;
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::figure;
use crate::scene::geometry::{
    distance_to_segment, normalize_rect, polygon_includes_point_non_zero, to_element_local,
};

/// The distance from `p` to `shape`'s outline: positive inside, negative outside, and
/// `-0.0` exactly on it.
pub fn signed_outline_distance(shape: &DrawElement, p: Point) -> f64 {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    let (lx, ly) = to_element_local(shape, p.x, p.y);
    let (hx, hy) = (rect.width / 2.0, rect.height / 2.0);
    let (sx, sy) = (lx - rect.x - hx, ly - rect.y - hy);
    let (qx, qy) = (sx.abs(), sy.abs());
    let (distance, inside) = match shape.kind {
        DrawElementType::Ellipse => ellipse(qx, qy, hx, hy),
        // ponytail: the painted diamond's vertices are rounded (`rounded_diamond_segments`,
        // the oracle's `deconstructDiamondElement`); here they are sharp, which moves the
        // outline by at most w/32 at the tip. Upgrade: distance to those cubic corners.
        DrawElementType::Diamond => diamond(qx, qy, hx, hy),
        // Not every figure is axis-symmetric (a parallelogram, a document's wavy bottom),
        // so this one works in the full local frame instead of the folded quadrant the
        // others share.
        DrawElementType::Figure => figure_distance(shape, sx, sy, hx, hy),
        _ => rounded_box(
            qx,
            qy,
            hx,
            hy,
            corner_radius(shape, rect.width.min(rect.height)),
        ),
    };
    if inside {
        distance
    } else {
        -distance
    }
}

/// The exact distance to a figure's own outline and whether the point is inside it — a
/// direct polygon test rather than a quadrant-folded formula, since a figure need not be
/// symmetric about either of its own axes.
fn figure_distance(shape: &DrawElement, sx: f64, sy: f64, hx: f64, hy: f64) -> (f64, bool) {
    let params = shape.figure.clone().unwrap_or_default();
    let poly: Vec<Point> =
        figure::centered_vertices(params.kind, params.sides, params.ratio, hx, hy)
            .into_iter()
            .map(|(x, y)| Point { x, y })
            .collect();
    let inside = polygon_includes_point_non_zero(Point { x: sx, y: sy }, &poly);
    let n = poly.len();
    let distance = (0..n)
        .map(|i| {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            distance_to_segment(sx, sy, a.x, a.y, b.x, b.y)
        })
        .fold(f64::INFINITY, f64::min);
    (distance, inside)
}

/// The painted corner radius, capped so opposite corners cannot overlap. A line of text
/// paints no outline and its box is square — Excalidraw's text has no roundness. Nor do
/// its frames (`FRAME_STYLE.roundness: null`, `constants.ts@1118751f:209`), painted with
/// rounded corners but measured square; ours carry a roundness for the painting alone.
fn corner_radius(shape: &DrawElement, side: f64) -> f64 {
    if matches!(shape.kind, DrawElementType::Text | DrawElementType::Frame) {
        return 0.0;
    }
    crate::render::shape::corner_radius(side, shape).clamp(0.0, side / 2.0)
}

/// A box with half-extents `(hx, hy)` and corners of radius `r`, painted as Excalidraw
/// paints them: a quadratic from the end of one side to the start of the next with its
/// control point on the box corner (`rounded_rect_segments`, the oracle's
/// `deconstructRectanguloidElement`).
fn rounded_box(qx: f64, qy: f64, hx: f64, hy: f64, r: f64) -> (f64, bool) {
    let top = distance_to_segment(qx, qy, 0.0, hy, hx - r, hy);
    let side = distance_to_segment(qx, qy, hx, 0.0, hx, hy - r);
    // Measured inward from the box corner.
    let (u, v) = (hx - qx, hy - qy);
    let mut distance = top.min(side);
    let mut inside = u > 0.0 && v > 0.0;
    if r > 0.0 {
        distance = distance.min(corner_distance(u, v, r));
        // The corner curve is the parabola √u + √v = √r; between it and the box corner
        // is outside.
        if inside && u < r && v < r {
            inside = u.sqrt() + v.sqrt() > r.sqrt();
        }
    }
    (distance, inside)
}

/// The distance from `(u, v)` to the corner curve `C(t) = (r(1−t)², r·t²)`, `t ∈ [0, 1]`.
///
/// Exact: the nearest point is where `(C(t) − p)·C′(t) = 0`, a cubic in `t`. Substituting
/// `t = s + ½` removes its square term and leaves `s³ + a·s + b = 0`, solved in closed
/// form. The ends of the curve are the ends of the sides, measured by the caller.
fn corner_distance(u: f64, v: f64, r: f64) -> f64 {
    let a = (1.5 * r - u - v) / (2.0 * r);
    let b = (u - v) / (4.0 * r);
    let at = |s: f64| {
        let t = (s + 0.5).clamp(0.0, 1.0);
        let (cu, cv) = (r * (1.0 - t) * (1.0 - t), r * t * t);
        (u - cu).hypot(v - cv)
    };
    let disc = (b / 2.0).powi(2) + (a / 3.0).powi(3);
    if disc > 0.0 {
        let root = disc.sqrt();
        return at((-b / 2.0 + root).cbrt() + (-b / 2.0 - root).cbrt());
    }
    // Three real roots (`a ≤ 0`): the trigonometric form.
    let m = 2.0 * (-a / 3.0).sqrt();
    if m <= f64::EPSILON {
        return at(0.0);
    }
    let phi = (3.0 * b / (a * m)).clamp(-1.0, 1.0).acos() / 3.0;
    (0..3)
        .map(|k| at(m * (phi - 2.0 * std::f64::consts::PI * k as f64 / 3.0).cos()))
        .fold(f64::INFINITY, f64::min)
}

/// `ellipseDistanceFromPoint` (`packages/math/src/ellipse.ts@1118751f:88-150`): exact for a
/// circle, three iterations of the trig-free nearest-point method otherwise.
fn ellipse(qx: f64, qy: f64, a: f64, b: f64) -> (f64, bool) {
    if a <= f64::EPSILON || b <= f64::EPSILON {
        // Flat: a segment along whichever axis is left, with no inside.
        let (ex, ey) = if b <= f64::EPSILON {
            (a, 0.0)
        } else {
            (0.0, b)
        };
        return (distance_to_segment(qx, qy, 0.0, 0.0, ex, ey), false);
    }
    let inside = (qx / a).powi(2) + (qy / b).powi(2) < 1.0;
    if a == b {
        return ((qx.hypot(qy) - a).abs(), inside);
    }
    let (mut tx, mut ty) = (0.707_f64, 0.707_f64);
    for _ in 0..3 {
        let (x, y) = (a * tx, b * ty);
        let ex = (a * a - b * b) * tx.powi(3) / a;
        let ey = (b * b - a * a) * ty.powi(3) / b;
        let (rx, ry) = (x - ex, y - ey);
        let (px, py) = (qx - ex, qy - ey);
        let r = rx.hypot(ry);
        let q = px.hypot(py);
        if q == 0.0 {
            break;
        }
        tx = ((px * r / q + ex) / a).clamp(0.0, 1.0);
        ty = ((py * r / q + ey) / b).clamp(0.0, 1.0);
        let t = tx.hypot(ty);
        tx /= t;
        ty /= t;
    }
    ((qx - a * tx).hypot(qy - b * ty), inside)
}

fn diamond(qx: f64, qy: f64, hx: f64, hy: f64) -> (f64, bool) {
    let inside = qx * hy + qy * hx < hx * hy;
    (distance_to_segment(qx, qy, hx, 0.0, 0.0, hy), inside)
}
