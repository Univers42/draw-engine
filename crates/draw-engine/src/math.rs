//! Tiny pure math helpers. No I/O, no host.

pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn smoothstep(t: f64) -> f64 {
    let x = clamp(t, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// JS-compatible string hash (`Math.imul(31, hash) + codeUnit`).
pub fn hash_string(value: &str) -> u32 {
    let mut hash: i32 = 0;
    for unit in value.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash.unsigned_abs()
}

pub fn round_px(value: f64) -> f64 {
    value.round()
}

/// One cubic Bézier: start, two controls, end.
pub type Cubic = [[f64; 2]; 4];

/// `curve_tightness` as the painter leaves it — rough's default, `options.rs:60`.
///
/// Named rather than inlined because the curve a handle is placed on and the curve the
/// painter draws have to be the *same* curve, and a second hardcoded zero is how those
/// two quietly stop agreeing.
pub const CURVE_TIGHTNESS: f64 = 0.0;

/// The cubics a rounded path is drawn as, one per segment.
///
/// This is `draw-rough`'s `curve_points` (`crates/draw-rough/src/renderer.rs:454-503`)
/// with the jitter left out: the sketchy offsets are what make a stroke look hand-drawn,
/// and a handle that followed them would wander off the line every time the seed changed.
/// The geometry underneath is a Catmull-Rom spline, converted segment by segment to a
/// cubic, with **both endpoints duplicated** — that duplication is what makes the curve
/// start and end at the path's real ends instead of overshooting them.
///
/// Returns one cubic per segment, so `cubics[i]` runs from `points[i]` to `points[i + 1]`.
/// Fewer than three points is a single straight segment, and rough draws it as one too.
pub fn catmull_rom_cubics(points: &[[f64; 2]], tightness: f64) -> Vec<Cubic> {
    if points.len() < 2 {
        return Vec::new();
    }
    if points.len() == 2 {
        // A straight run. Given as a cubic with its controls on the line so that callers
        // get the same shape of answer whatever the path looks like.
        let (a, b) = (points[0], points[1]);
        let third = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        return vec![[a, third(1.0 / 3.0), third(2.0 / 3.0), b]];
    }

    // The duplicated ends, exactly as rough builds them.
    let mut ps: Vec<[f64; 2]> = Vec::with_capacity(points.len() + 2);
    ps.push(points[0]);
    ps.extend_from_slice(points);
    ps.push(points[points.len() - 1]);

    let s = 1.0 - tightness;
    let mut out = Vec::with_capacity(points.len() - 1);
    for i in 1..ps.len() - 2 {
        let start = ps[i];
        let end = ps[i + 1];
        let b1 = [
            start[0] + (s * ps[i + 1][0] - s * ps[i - 1][0]) / 6.0,
            start[1] + (s * ps[i + 1][1] - s * ps[i - 1][1]) / 6.0,
        ];
        let b2 = [
            end[0] + (s * ps[i][0] - s * ps[i + 2][0]) / 6.0,
            end[1] + (s * ps[i][1] - s * ps[i + 2][1]) / 6.0,
        ];
        out.push([start, b1, b2, end]);
    }
    out
}

/// A point on a cubic at parameter `t`.
pub fn bezier_point(curve: &Cubic, t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * curve[0][0] + b * curve[1][0] + c * curve[2][0] + d * curve[3][0],
        a * curve[0][1] + b * curve[1][1] + c * curve[2][1] + d * curve[3][1],
    ]
}

/// How many pieces a cubic is chopped into when its length is measured.
///
/// A cubic this short — one segment of a drawn path — is close enough to its own chord
/// that 64 pieces put the answer well inside a tenth of a pixel, which is far finer than
/// anything a handle is aimed at.
const ARC_STEPS: usize = 64;

/// The point a given fraction of the way **along** a cubic, by arc length.
///
/// Not the same as `bezier_point(curve, fraction)`, and the difference is the point of
/// having this at all: a cubic covers more ground in one half of `t` than the other
/// wherever it is asymmetric, so `t = 0.5` on a bent segment sits away from the middle a
/// person sees. Excalidraw asks the same question the same way —
/// `curvePointAtLength(segment, 0.5)`, `packages/math/src/curve.ts:516`.
///
/// They integrate with 24-point Legendre-Gauss and then binary-search the parameter;
/// this walks a polyline and interpolates. The answers agree to far better than a pixel
/// at these sizes, and the tests assert the property — *the handle is on the path, half
/// way along it* — rather than the method, so neither is locked in.
pub fn bezier_point_at_fraction(curve: &Cubic, fraction: f64) -> [f64; 2] {
    let fraction = clamp(fraction, 0.0, 1.0);
    let mut samples = Vec::with_capacity(ARC_STEPS + 1);
    let mut lengths = Vec::with_capacity(ARC_STEPS + 1);
    let mut total = 0.0;
    for step in 0..=ARC_STEPS {
        let point = bezier_point(curve, step as f64 / ARC_STEPS as f64);
        if step > 0 {
            let previous: [f64; 2] = samples[step - 1];
            total += (point[0] - previous[0]).hypot(point[1] - previous[1]);
        }
        samples.push(point);
        lengths.push(total);
    }
    if total <= f64::EPSILON {
        return curve[0];
    }

    let target = total * fraction;
    let index = lengths
        .iter()
        .position(|&length| length >= target)
        .unwrap_or(ARC_STEPS);
    if index == 0 {
        return samples[0];
    }
    // Where in this piece the target falls. The piece is a straight line by construction,
    // so a plain interpolation along it is exact.
    let span = lengths[index] - lengths[index - 1];
    let t = if span <= f64::EPSILON {
        0.0
    } else {
        (target - lengths[index - 1]) / span
    };
    [
        lerp(samples[index - 1][0], samples[index][0], t),
        lerp(samples[index - 1][1], samples[index][1], t),
    ]
}
