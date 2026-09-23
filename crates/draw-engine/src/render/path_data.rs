//! Geometry as SVG path data, simplified to what the zoom can show.
//!
//! The painter used to build every path one call at a time — `moveTo`, `lineTo`,
//! `quadraticCurveTo` — and each call crosses from WASM into JavaScript. A freehand stroke
//! of 150 samples is an outline of 300 points, so 300 crossings; a board of 2,000 strokes
//! was 600,000 of them **per frame**, because freehand strokes were not cached at all.
//! Measured at 15% zoom on such a board, a frame took 105ms.
//!
//! Two changes live here, both pure so they are tested on the host:
//!
//! - **One string, one call.** A path is written out as SVG path data and handed to
//!   `new Path2D(d)` once. Building it is then a string in WASM memory and a single
//!   crossing, which matters for the paths that are rebuilt often — the stroke being drawn
//!   changes every frame.
//! - **Level of detail.** At 15% a 150-point stroke is a few dozen pixels long; three
//!   hundred curve segments draw it no better than thirty, and the browser rasterises every
//!   one of them. The outline is simplified to a tolerance in *device pixels*, so it is
//!   indistinguishable at the zoom it is drawn for and keeps full detail close up.

use std::fmt::Write;

/// How far a simplified outline may stray from the true one, in device pixels.
///
/// A third of a pixel is below what antialiasing can show.
pub const LOD_TOLERANCE_PX: f64 = 0.33;

/// The detail level for a zoom, as a power of two: 0 at 100% and above, -1 at 50%, -2 at
/// 25%, and so on, down to -8.
///
/// Levels rather than the exact scale, so a cached path survives a zoom within its level
/// instead of being rebuilt on every wheel tick. Rounded **down**, so a path is always
/// built for a zoom at or below the one it is drawn at, and never shows less detail than
/// the tolerance allows.
pub fn lod_level(device_scale: f64) -> i32 {
    if !device_scale.is_finite() || device_scale <= 0.0 {
        return 0;
    }
    (device_scale.log2().floor() as i32).clamp(-8, 0)
}

/// The simplification tolerance, in world units, for a detail level.
pub fn lod_tolerance(level: i32) -> f64 {
    LOD_TOLERANCE_PX / 2f64.powi(level)
}

/// Ramer-Douglas-Peucker: the fewest points that stay within `tolerance` of the line.
///
/// Iterative, so a stroke of thousands of samples cannot overflow the stack. The first
/// and last points are always kept.
pub fn simplify(points: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if points.len() <= 2 || tolerance <= 0.0 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    let limit = tolerance * tolerance;
    while let Some((first, last)) = stack.pop() {
        let (a, b) = (points[first], points[last]);
        let mut farthest = 0.0;
        let mut index = first;
        for (i, p) in points.iter().enumerate().take(last).skip(first + 1) {
            let d = squared_distance_to_segment(*p, a, b);
            if d > farthest {
                farthest = d;
                index = i;
            }
        }
        if farthest > limit {
            keep[index] = true;
            if index - first > 1 {
                stack.push((first, index));
            }
            if last - index > 1 {
                stack.push((index, last));
            }
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, kept)| kept.then_some(*p))
        .collect()
}

fn squared_distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (x, y) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
    x * x + y * y
}

/// Writes a number with two decimals, which is a hundredth of a world unit — below
/// anything that can be seen at any zoom this engine allows — without going through the
/// general float formatter, which was a visible cost in the profile.
fn number(out: &mut String, value: f64) {
    let scaled = (value * 100.0).round();
    if !scaled.is_finite() {
        out.push('0');
        return;
    }
    let scaled = scaled as i64;
    if scaled < 0 {
        out.push('-');
    }
    let magnitude = scaled.unsigned_abs();
    let _ = write!(out, "{}", magnitude / 100);
    let fraction = magnitude % 100;
    if fraction != 0 {
        let _ = write!(out, ".{:02}", fraction);
        // "1.50" and "1.5" are the same number; the shorter is less to parse.
        if out.ends_with('0') {
            out.pop();
        }
    }
}

fn point(out: &mut String, p: [f64; 2]) {
    number(out, p[0]);
    out.push(' ');
    number(out, p[1]);
}

/// A closed outline as path data, smoothed through the midpoints with quadratics — the
/// same curve the painter drew call by call: each point is the control point of a curve
/// ending half way to the next, which keeps the corners the samples would otherwise put
/// at every one of them.
pub fn smooth_closed_outline(outline: &[[f64; 2]]) -> String {
    let mut out = String::with_capacity(outline.len() * 24);
    let Some(first) = outline.first() else {
        return out;
    };
    out.push('M');
    point(&mut out, *first);
    for i in 1..outline.len() {
        let current = outline[i];
        let next = outline[(i + 1) % outline.len()];
        let mid = [(current[0] + next[0]) / 2.0, (current[1] + next[1]) / 2.0];
        out.push('Q');
        point(&mut out, current);
        out.push(' ');
        point(&mut out, mid);
    }
    out.push('Z');
    out
}

/// One move, line or cubic of rough's output.
pub enum PathOp {
    Move([f64; 2]),
    Line([f64; 2]),
    Cubic([f64; 6]),
}

/// A sequence of ops as path data.
pub fn ops(ops: impl IntoIterator<Item = PathOp>) -> String {
    let mut out = String::new();
    for op in ops {
        match op {
            PathOp::Move(p) => {
                out.push('M');
                point(&mut out, p);
            }
            PathOp::Line(p) => {
                out.push('L');
                point(&mut out, p);
            }
            PathOp::Cubic([x1, y1, x2, y2, x, y]) => {
                out.push('C');
                point(&mut out, [x1, y1]);
                out.push(' ');
                point(&mut out, [x2, y2]);
                out.push(' ');
                point(&mut out, [x, y]);
            }
        }
    }
    out
}

/// A fast fingerprint of a freehand stroke's geometry at one detail level.
///
/// Word by word rather than byte by byte: the rough-shape key hashes bytes, which is fine
/// for a rectangle's handful of numbers and a real cost over a board of freehand strokes,
/// where it runs over every sample of every stroke on every frame. Everything that
/// changes the outline is in it — the samples, the width, the level — and nothing that
/// does not, so moving a stroke never rebuilds it.
pub fn freehand_fingerprint(points: &[[f64; 2]], stroke_width: f64, level: i32) -> u64 {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |word: u64| {
        h = (h.rotate_left(5) ^ word).wrapping_mul(K);
    };
    eat(points.len() as u64);
    eat(stroke_width.to_bits());
    eat(level as u64);
    for p in points {
        eat(p[0].to_bits());
        eat(p[1].to_bits());
    }
    // A final avalanche, so fingerprints that differ in one late word differ everywhere.
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_powers_of_two_rounded_down() {
        assert_eq!(lod_level(1.0), 0);
        assert_eq!(lod_level(3.0), 0, "never more detail than full");
        assert_eq!(lod_level(0.5), -1);
        assert_eq!(lod_level(0.15), -3, "15% is built for 12.5%, never for 25%");
        assert_eq!(lod_level(0.0001), -8);
        assert_eq!(lod_level(0.0), 0);
        assert_eq!(lod_level(f64::NAN), 0);
    }

    #[test]
    fn tolerance_is_a_third_of_a_pixel_at_the_level() {
        assert!((lod_tolerance(0) - LOD_TOLERANCE_PX).abs() < 1e-12);
        assert!((lod_tolerance(-3) - LOD_TOLERANCE_PX * 8.0).abs() < 1e-12);
    }

    #[test]
    fn simplifying_a_straight_run_keeps_its_ends() {
        let line: Vec<[f64; 2]> = (0..50).map(|i| [f64::from(i), 0.0]).collect();
        assert_eq!(simplify(&line, 0.1), vec![[0.0, 0.0], [49.0, 0.0]]);
    }

    #[test]
    fn simplifying_keeps_a_corner() {
        let mut path: Vec<[f64; 2]> = (0..20).map(|i| [f64::from(i), 0.0]).collect();
        path.extend((1..20).map(|i| [19.0, f64::from(i)]));
        assert_eq!(
            simplify(&path, 0.1),
            vec![[0.0, 0.0], [19.0, 0.0], [19.0, 19.0]]
        );
    }

    #[test]
    fn simplified_points_stay_within_the_tolerance() {
        // A wobbly curve: every original point must lie within the tolerance of the
        // simplified polyline, or the drawing visibly changed.
        let wobble: Vec<[f64; 2]> = (0..400)
            .map(|i| {
                let t = f64::from(i) * 0.05;
                [t * 10.0, (t * 3.0).sin() * 8.0 + (t * 17.0).sin() * 0.4]
            })
            .collect();
        let tolerance = 1.0;
        let kept = simplify(&wobble, tolerance);
        assert!(
            kept.len() < wobble.len() / 3,
            "{} of {}",
            kept.len(),
            wobble.len()
        );
        for p in &wobble {
            let nearest = kept
                .windows(2)
                .map(|w| squared_distance_to_segment(*p, w[0], w[1]))
                .fold(f64::INFINITY, f64::min);
            assert!(
                nearest.sqrt() <= tolerance + 1e-9,
                "{p:?} strayed {}",
                nearest.sqrt()
            );
        }
    }

    #[test]
    fn numbers_are_written_short_and_exact_to_a_hundredth() {
        let mut out = String::new();
        for v in [0.0, 1.0, -1.0, 1.5, 1.25, -0.004, 12.345, 1e6] {
            number(&mut out, v);
            out.push(',');
        }
        assert_eq!(out, "0,1,-1,1.5,1.25,0,12.35,1000000,");
    }

    #[test]
    fn a_closed_outline_is_curves_through_midpoints() {
        let d = smooth_closed_outline(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!(d, "M0 0Q10 0 10 5Q10 10 5 5Z");
    }

    #[test]
    fn ops_are_written_in_order() {
        let d = ops([
            PathOp::Move([0.0, 0.0]),
            PathOp::Line([1.0, 2.0]),
            PathOp::Cubic([1.0, 1.0, 2.0, 2.0, 3.0, 3.0]),
        ]);
        assert_eq!(d, "M0 0L1 2C1 1 2 2 3 3");
    }

    #[test]
    fn the_fingerprint_changes_with_the_geometry_and_the_level_only() {
        let a = [[0.0, 0.0], [1.0, 1.0], [2.0, 0.5]];
        let mut b = a;
        b[2][1] = 0.6;
        let base = freehand_fingerprint(&a, 2.0, -3);
        assert_eq!(base, freehand_fingerprint(&a, 2.0, -3));
        assert_ne!(base, freehand_fingerprint(&b, 2.0, -3));
        assert_ne!(base, freehand_fingerprint(&a, 4.0, -3));
        assert_ne!(base, freehand_fingerprint(&a, 2.0, -2));
    }
}
