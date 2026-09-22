//! Turning a stream of pointer samples into a stroke someone would call a line.
//!
//! The raw stream is not one. A pointer reports where it was, at whatever rate the
//! platform feels like, including every tremor of the hand holding it — so a stroke drawn
//! straight from the samples is a polyline of hard corners that shakes, and that is what
//! this engine drew before. Excalidraw does not draw the samples either; it runs them
//! through `perfect-freehand`, and the three things that library does are the three
//! things below.
//!
//! 1. **Streamline** — each sample is pulled toward the one before it, which is a
//!    one-pole low-pass filter on the input. This is what removes the tremble, and it is
//!    the single biggest difference between a drawn line and a recorded one.
//! 2. **Thinning** — the faster the pointer is moving, the thinner the line, because that
//!    is what a real nib does under a hand that is moving fast. A constant width reads as
//!    a cable rather than a stroke.
//! 3. **An outline, not a path** — a variable width cannot be stroked, only filled, so
//!    the stroke becomes a closed polygon: down one side and back up the other.
//!
//! All pure functions over slices, so they are tested on the host. The painter is
//! `wasm/paint.rs` and needs a canvas, which is why none of this lives there.

/// How hard each sample is pulled toward the previous one, 0 to 1.
///
/// `perfect-freehand`'s own default is 0.5, which Excalidraw keeps for freedraw. Higher
/// smooths more and lags more: at 1.0 the line never reaches the pointer at all.
pub const STREAMLINE: f64 = 0.5;

/// How much speed is allowed to take off the width, 0 to 1.
///
/// At 0.6 a fast stroke is 40% of the width of a slow one — clearly tapered, without the
/// thin parts breaking up.
pub const THINNING: f64 = 0.6;

/// The speed, in world units per sample, at which thinning is fully applied.
///
/// Per sample rather than per second on purpose: the engine is handed samples, not
/// timestamps, and a rate that varied with the platform's event frequency would make the
/// same gesture draw differently on two machines.
const FULL_SPEED: f64 = 14.0;

/// Pulls a raw sample toward the previous smoothed one.
///
/// The whole of the tremble fix. `strength` 0 returns the sample untouched.
pub fn streamline(previous: [f64; 2], raw: [f64; 2], strength: f64) -> [f64; 2] {
    let keep = 1.0 - strength.clamp(0.0, 1.0);
    [
        previous[0] + (raw[0] - previous[0]) * keep,
        previous[1] + (raw[1] - previous[1]) * keep,
    ]
}

/// The half-width of the stroke at a point, given how fast the pointer was moving.
///
/// Clamped to a quarter of the nominal radius at the fast end. Letting it reach zero
/// makes a quick flick vanish mid-stroke, which reads as a dropped input rather than as
/// a taper.
pub fn radius_at(size: f64, speed: f64, thinning: f64) -> f64 {
    let nominal = size / 2.0;
    let fast = (speed / FULL_SPEED).clamp(0.0, 1.0);
    let scale = 1.0 - thinning.clamp(0.0, 1.0) * fast;
    nominal * scale.max(0.25)
}

/// The outline of a stroke, as one closed polygon.
///
/// Walks the points laying a radius either side of the direction of travel, then returns
/// down the far side. A variable width cannot be stroked — `lineWidth` is one number for
/// the whole path — so the only way to draw it is to fill the region between the two
/// sides.
///
/// Returns an empty vector for fewer than two points: a single sample has no direction,
/// and guessing one puts a dash on the canvas pointing somewhere arbitrary. The painter
/// draws a dot for that case, which is what a single tap means.
pub fn stroke_outline(points: &[[f64; 2]], size: f64, thinning: f64) -> Vec<[f64; 2]> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut left: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    let mut right: Vec<[f64; 2]> = Vec::with_capacity(points.len());

    for (i, &[x, y]) in points.iter().enumerate() {
        // The direction through this point, taken from its neighbours so a corner gets
        // the average of the two segments meeting there rather than one of them.
        let previous = points[i.saturating_sub(1)];
        let next = points[(i + 1).min(points.len() - 1)];
        let (dx, dy) = (next[0] - previous[0], next[1] - previous[1]);
        let length = dx.hypot(dy);
        if length < f64::EPSILON {
            continue;
        }
        // Speed is measured over the step actually taken, not over the two-sample span
        // used for direction, or every point would report the average of its neighbours.
        let speed = (x - previous[0]).hypot(y - previous[1]);
        let radius = radius_at(size, speed, thinning);
        let (nx, ny) = (-dy / length * radius, dx / length * radius);
        left.push([x + nx, y + ny]);
        right.push([x - nx, y - ny]);
    }

    if left.is_empty() {
        return Vec::new();
    }
    right.reverse();
    left.extend(right);
    left
}

pub fn points_bounds(points: &[[f64; 2]]) -> [f64; 4] {
    if points.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &[x, y] in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    [min_x, min_y, max_x, max_y]
}
