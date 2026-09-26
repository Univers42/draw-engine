//! Transcription of **points-on-curve 0.2.0** — the version roughjs 4.6.4 depends on
//! (`^0.2.0`), `lib/curve-to-bezier.js` and `lib/index.js`.
//!
//! rough uses it to turn a curve into the polygon a pattern fill is hatched inside:
//! `pointsOnBezierCurves(curveToBezier(points), 10, (1 + roughness) / 2)`. Without it
//! the hachure follows the points the curve was drawn *through* instead of the curve,
//! and a rounded closed line shows a straight-edged fill inside a bowed outline.
//!
//! Upstream is MIT, © 2020 Preet Shihn. See `NOTICE`.

/// `curveToBezier(pointsIn, curveTightness)`
///
/// A Catmull-Rom spline through the points, as one chain of cubic Béziers:
/// `[start, c1, c2, end, c1, c2, end, …]`. Both ends are duplicated first, which is what
/// makes the curve start and end at the path's real ends.
///
/// Three points is special-cased upstream into a single cubic that uses the middle point
/// and the end as its controls — not a spline through all three. Kept as written.
///
/// # Panics
///
/// On fewer than three points, where the JS throws. Every caller checks the length.
pub fn curve_to_bezier(points_in: &[[f64; 2]], curve_tightness: f64) -> Vec<[f64; 2]> {
    let len = points_in.len();
    assert!(len >= 3, "A curve must have at least three points.");
    let mut out = Vec::new();
    if len == 3 {
        out.extend_from_slice(&[points_in[0], points_in[1], points_in[2], points_in[2]]);
        return out;
    }

    let mut points = Vec::with_capacity(len + 2);
    points.push(points_in[0]);
    points.push(points_in[0]);
    points.extend_from_slice(&points_in[1..]);
    points.push(points_in[len - 1]);

    let s = 1.0 - curve_tightness;
    out.push(points[0]);
    let mut i = 1;
    while i + 2 < points.len() {
        let cached = points[i];
        let b1 = [
            cached[0] + (s * points[i + 1][0] - s * points[i - 1][0]) / 6.0,
            cached[1] + (s * points[i + 1][1] - s * points[i - 1][1]) / 6.0,
        ];
        let b2 = [
            points[i + 1][0] + (s * points[i][0] - s * points[i + 2][0]) / 6.0,
            points[i + 1][1] + (s * points[i][1] - s * points[i + 2][1]) / 6.0,
        ];
        let b3 = [points[i + 1][0], points[i + 1][1]];
        out.extend_from_slice(&[b1, b2, b3]);
        i += 1;
    }
    out
}

/// `pointsOnBezierCurves(points, tolerance, distance)`
///
/// Flattens a Bézier chain by adaptive subdivision: a cubic flatter than `tolerance` is
/// replaced by its chord, anything else is split in half and tried again. With a
/// `distance`, the result is then thinned by Ramer–Douglas–Peucker.
pub fn points_on_bezier_curves(
    points: &[[f64; 2]],
    tolerance: f64,
    distance: Option<f64>,
) -> Vec<[f64; 2]> {
    let mut new_points = Vec::new();
    let num_segments = (points.len() - 1) / 3;
    for i in 0..num_segments {
        let offset = i * 3;
        points_on_bezier_curve_with_splitting(points, offset, tolerance, &mut new_points);
    }
    match distance {
        Some(distance) if distance > 0.0 => {
            let mut out = Vec::new();
            simplify_points(&new_points, 0, new_points.len(), distance, &mut out);
            out
        }
        _ => new_points,
    }
}

/// `distanceSq(p1, p2)`. The JS squares with `Math.pow(d, 2)`, exactly equal to `d * d`.
fn distance_sq(p1: [f64; 2], p2: [f64; 2]) -> f64 {
    (p1[0] - p2[0]) * (p1[0] - p2[0]) + (p1[1] - p2[1]) * (p1[1] - p2[1])
}

fn distance(p1: [f64; 2], p2: [f64; 2]) -> f64 {
    distance_sq(p1, p2).sqrt()
}

/// `distanceToSegmentSq(p, v, w)`
fn distance_to_segment_sq(p: [f64; 2], v: [f64; 2], w: [f64; 2]) -> f64 {
    let l2 = distance_sq(v, w);
    if l2 == 0.0 {
        return distance_sq(p, v);
    }
    let mut t = ((p[0] - v[0]) * (w[0] - v[0]) + (p[1] - v[1]) * (w[1] - v[1])) / l2;
    // `Math.max(0, Math.min(1, t))`; `clamp` agrees, NaN included.
    t = t.clamp(0.0, 1.0);
    distance_sq(p, lerp(v, w, t))
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// `flatness(points, offset)`
fn flatness(points: &[[f64; 2]], offset: usize) -> f64 {
    let p1 = points[offset];
    let p2 = points[offset + 1];
    let p3 = points[offset + 2];
    let p4 = points[offset + 3];

    let mut ux = 3.0 * p2[0] - 2.0 * p1[0] - p4[0];
    ux *= ux;
    let mut uy = 3.0 * p2[1] - 2.0 * p1[1] - p4[1];
    uy *= uy;
    let mut vx = 3.0 * p3[0] - 2.0 * p4[0] - p1[0];
    vx *= vx;
    let mut vy = 3.0 * p3[1] - 2.0 * p4[1] - p1[1];
    vy *= vy;

    if ux < vx {
        ux = vx;
    }
    if uy < vy {
        uy = vy;
    }
    ux + uy
}

/// `getPointsOnBezierCurveWithSplitting(points, offset, tolerance, newPoints)`
fn points_on_bezier_curve_with_splitting(
    points: &[[f64; 2]],
    offset: usize,
    tolerance: f64,
    out_points: &mut Vec<[f64; 2]>,
) {
    if flatness(points, offset) < tolerance {
        let p0 = points[offset];
        // A start within a unit of where the previous piece ended is that end again.
        if let Some(&last) = out_points.last() {
            if distance(last, p0) > 1.0 {
                out_points.push(p0);
            }
        } else {
            out_points.push(p0);
        }
        out_points.push(points[offset + 3]);
    } else {
        let t = 0.5;
        let p1 = points[offset];
        let p2 = points[offset + 1];
        let p3 = points[offset + 2];
        let p4 = points[offset + 3];

        let q1 = lerp(p1, p2, t);
        let q2 = lerp(p2, p3, t);
        let q3 = lerp(p3, p4, t);

        let r1 = lerp(q1, q2, t);
        let r2 = lerp(q2, q3, t);

        let red = lerp(r1, r2, t);

        points_on_bezier_curve_with_splitting(&[p1, q1, r1, red], 0, tolerance, out_points);
        points_on_bezier_curve_with_splitting(&[red, r2, q3, p4], 0, tolerance, out_points);
    }
}

/// `simplifyPoints(points, start, end, epsilon, newPoints)` — Ramer–Douglas–Peucker.
fn simplify_points(
    points: &[[f64; 2]],
    start: usize,
    end: usize,
    epsilon: f64,
    out_points: &mut Vec<[f64; 2]>,
) {
    let s = points[start];
    let e = points[end - 1];
    let mut max_dist_sq = 0.0;
    // 1, not `start + 1`, as upstream. It is only read after a distance beat zero, which
    // has overwritten it.
    let mut max_ndx = 1;
    for (i, &point) in points.iter().enumerate().take(end - 1).skip(start + 1) {
        let dist_sq = distance_to_segment_sq(point, s, e);
        if dist_sq > max_dist_sq {
            max_dist_sq = dist_sq;
            max_ndx = i;
        }
    }

    if f64::sqrt(max_dist_sq) > epsilon {
        simplify_points(points, start, max_ndx + 1, epsilon, out_points);
        simplify_points(points, max_ndx, end, epsilon, out_points);
    } else {
        if out_points.is_empty() {
            out_points.push(s);
        }
        out_points.push(e);
    }
}
