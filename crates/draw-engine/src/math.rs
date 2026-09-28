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

/// The step Shift snaps a drag or a rotation to: **15 degrees**.
///
/// Excalidraw's `SHIFT_LOCKING_ANGLE` (`packages/common/src/constants.ts@1118751f:31`,
/// `Math.PI / 12`). One constant, and below it **two** functions, because the oracle
/// rounds its two locks differently — they are not one rule with a flag, and folding them
/// together would have to pick one and call it parity.
pub const SHIFT_LOCKING_ANGLE: f64 = std::f64::consts::PI / 12.0;

/// `Math.round`, which breaks a tie **toward +infinity** (`Math.round(-0.5) === -0`).
/// `f64::round` breaks it away from zero, which is a whole step out on every negative tie.
/// The same trap `getGridPoint` has, and the same fix: `Math.round` is `floor(x + 0.5)`
/// for every input.
///
/// Public because it is the one rounding both Shift locks are built on, and because a tie
/// in the *composed* result cannot be held to a number in floating point — the halves never
/// line up in radians — so the rule is tested here, where it is exact.
pub fn round_half_up(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// The delta a Shift-locked drag leaves from its anchor.
///
/// Excalidraw's `getLockedLinearCursorAlignSize` (`sizeHelpers.ts@1118751f:187-254`): the
/// drag's angle is rounded to a step, and the endpoint is then the pointer's **orthogonal
/// projection onto the ray that step names** — the intersection of the locked ray with the
/// line through the cursor across it (`:236-250`) — with the flat and square cases written
/// out as branches of their own (`:229-234`).
///
/// The projection is the part that is easy to miss. Rotating the delta onto the locked
/// angle also yields a delta *at* the locked angle, so an assertion about the angle alone
/// cannot tell the two apart. The lengths differ: the projection stands
/// `|drag| * cos(delta)` from the anchor where a rotation stands `|drag|`, and the two
/// agree only once the drag was already on a step.
pub fn shift_locked_delta(dx: f64, dy: f64) -> (f64, f64) {
    let steps = round_half_up(dy.atan2(dx) / SHIFT_LOCKING_ANGLE);
    // The oracle's two branches, decided on the step count rather than on the angle that
    // count multiplies out to. `k * (PI / 12)` is not reliably exactly `FRAC_PI_2` in
    // binary, so the oracle's own `lockedAngle === Math.PI / 2` is a test that can miss —
    // and a miss lands in the general case, where `1 / tan(locked)` is a very large number
    // and the two axes are then right by luck rather than by construction.
    if steps.rem_euclid(12.0) == 0.0 {
        return (dx, 0.0);
    }
    if steps.rem_euclid(12.0) == 6.0 {
        return (0.0, dy);
    }
    let (sin, cos) = (steps * SHIFT_LOCKING_ANGLE).sin_cos();
    let along = dx * cos + dy * sin;
    (along * cos, along * sin)
}

/// The angle a Shift-locked rotation lands on, in `[0, 2*PI)`.
///
/// Excalidraw's two rotation locks, `rotateSingleElement`
/// (`resizeElements.ts@1118751f:229-233`) and `rotateMultipleElements` (`:424-427`), round
/// with `angle += step / 2; angle -= angle % step` and then `normalizeRadians`
/// (`packages/math/src/angle.ts@1118751f:11-14`).
///
/// **The input is normalised before it is floored, and that is load-bearing.** The oracle
/// builds the raw angle as `5 * PI / 2 + atan2(..)` (`:227`) — two and a half turns, not
/// one — so the number it floors always sits in `(3PI/2, 7PI/2]` and is never negative.
/// Ours is built as `PI / 2 + atan2(..)` (`selection/transform.rs:293`), the same angle a
/// whole turn smaller, which does go negative. `%` truncates toward zero, so a negative
/// operand lands in a cell **twice as wide**: everything within 7.5 degrees either side of
/// straight up folds onto 0. Handed a raw -20 degrees the oracle answers 345 and this
/// answered 0, and the whole lower-left quadrant of a turn with it.
///
/// Normalising first puts the input back in the range the oracle's own is in, which makes
/// the lock an ordinary round-to-nearest over 15 degrees — the same answer
/// [`shift_locked_delta`] gives for the same raw angle, as it should, since they are the
/// same rounding. `2 * PI` is exactly 24 steps, so folding the angle into `[0, 2PI)` moves
/// it by a whole number of steps and cannot change which cell it is in.
pub fn shift_locked_angle(angle: f64) -> f64 {
    let shifted = angle.rem_euclid(std::f64::consts::TAU) + SHIFT_LOCKING_ANGLE / 2.0;
    let locked = shifted - shifted % SHIFT_LOCKING_ANGLE;
    locked.rem_euclid(std::f64::consts::TAU)
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
        return vec![chord_cubic(points[0], points[1])];
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

/// A straight run as a cubic: the two ends and two controls evenly spaced between them.
///
/// The four control points are collinear and evenly spaced, so the Bezier is exactly
/// `a + t (b - a)` — this *is* the chord, and gives a path of straight segments the same
/// shape of answer as one of curves, which is what lets a caller measure a fraction of the
/// way along a line without branching on whether the line is round.
pub fn chord_cubic(a: [f64; 2], b: [f64; 2]) -> Cubic {
    let third = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    [a, third(1.0 / 3.0), third(2.0 / 3.0), b]
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
/// `curvePointAtLength(segment, 0.5)`, `packages/math/src/curve.ts@1118751f:516`.
///
/// They integrate with 24-point Legendre-Gauss and then binary-search the parameter;
/// this walks a polyline and interpolates. The answers agree to far better than a pixel
/// at these sizes, and the tests assert the property — *the handle is on the path, half
/// way along it* — rather than the method, so neither is locked in.
pub fn bezier_point_at_fraction(curve: &Cubic, fraction: f64) -> [f64; 2] {
    let (samples, lengths) = arc_table(curve);
    point_at_length(
        &samples,
        &lengths,
        clamp(fraction, 0.0, 1.0) * *lengths.last().unwrap(),
    )
}

/// A cubic's arc length, by the same walk [`bezier_point_at_fraction`] uses.
pub fn bezier_length(curve: &Cubic) -> f64 {
    let (_, lengths) = arc_table(curve);
    *lengths.last().unwrap_or(&0.0)
}

/// The arc length of a cubic from its start to parameter `t`.
///
/// What the oracle asks of a curve when a drag puts a label somewhere on it:
/// `curveLengthAtParameter` (`packages/math/src/curve.ts@1118751f:477-506`). The nearest
/// place on a curve is not at half its parameter, so a gesture that wants the closest point
/// on a segment has to be able to ask how far along *this* `t` is.
pub fn bezier_length_to(curve: &Cubic, t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return bezier_length(curve);
    }
    let (_, lengths) = arc_table(curve);
    // The step the target falls in, and how far into it, as a fraction of the step.
    let at = t * ARC_STEPS as f64;
    let index = (at.floor() as usize).min(ARC_STEPS - 1);
    let span = lengths[index + 1] - lengths[index];
    lengths[index] + span * (at - index as f64).clamp(0.0, 1.0)
}

/// The walk: a cubic sampled evenly, and the ground covered to each sample.
///
/// One walk, three readers — the total, a length to a point, a length to a parameter. They
/// have to agree with each other exactly, or a fraction of a whole path would be measured by
/// one rule and located by another, so they read this one table.
fn arc_table(curve: &Cubic) -> (Vec<[f64; 2]>, Vec<f64>) {
    let mut samples = Vec::with_capacity(ARC_STEPS + 1);
    let mut lengths = Vec::with_capacity(ARC_STEPS + 1);
    let mut total = 0.0;
    for step in 0..=ARC_STEPS {
        let point = bezier_point(curve, step as f64 / ARC_STEPS as f64);
        if let Some(previous) = samples.last() {
            let previous: [f64; 2] = *previous;
            total += (point[0] - previous[0]).hypot(point[1] - previous[1]);
        }
        samples.push(point);
        lengths.push(total);
    }
    (samples, lengths)
}

/// The parameter of a cubic nearest `at`, by walking to it.
///
/// A coarse pass finds the nearest of 32 steps and a bisection narrows inside it, which is
/// sound because the distance to a point is unimodal over a step this small. Excalidraw
/// solves the cubic that `d/dt |B(t) - p|² = 0` gives it
/// (`curveClosestParameter`, `packages/math/src/curve.ts@1118751f:223-`); this arrives at
/// the same place by a different road, and what a caller wants from either is *which point
/// of the curve is nearest*, not a parameter.
pub fn cubic_closest_parameter(curve: &Cubic, at: [f64; 2]) -> f64 {
    const STEPS: usize = 32;
    let away = |t: f64| {
        let p = bezier_point(curve, t);
        (p[0] - at[0]).hypot(p[1] - at[1])
    };
    let mut nearest = 0;
    for step in 0..=STEPS {
        if away(step as f64 / STEPS as f64) < away(nearest as f64 / STEPS as f64) {
            nearest = step;
        }
    }
    let mut lo = nearest.saturating_sub(1) as f64 / STEPS as f64;
    let mut hi = (nearest + 1).min(STEPS) as f64 / STEPS as f64;
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if away(mid) < away(lo) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// The point `length` along a walk, interpolating inside the step that reaches it.
fn point_at_length(samples: &[[f64; 2]], lengths: &[f64], length: f64) -> [f64; 2] {
    let Some(index) = lengths.iter().position(|&walked| walked >= length) else {
        return *samples.last().unwrap_or(&[0.0, 0.0]);
    };
    if index == 0 {
        return samples[0];
    }
    // The step is a straight line by construction, so a plain interpolation along it is
    // exact rather than another approximation.
    let span = lengths[index] - lengths[index - 1];
    let t = if span <= f64::EPSILON {
        0.0
    } else {
        (length - lengths[index - 1]) / span
    };
    [
        lerp(samples[index - 1][0], samples[index][0], t),
        lerp(samples[index - 1][1], samples[index][1], t),
    ]
}
