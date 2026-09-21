//! Recognising the shape someone meant to draw.
//!
//! A freehand stroke goes in; a rectangle, diamond, ellipse, line or arrow comes out, or
//! nothing when the stroke is not enough like any of them. This is the purest possible
//! case for the motor owning the mathematics: it is a classifier over a point set, with
//! no rendering, no host and no state — and if two frontends disagreed about what a
//! stroke was, the same gesture would produce different drawings depending on who made
//! it.
//!
//! Transcribed from Excalidraw's `element/convertToShape.ts` and `math/pca.ts` at the
//! SHA pinned in `scripts/oracle-sha.txt`.
//!
//! The approach is moment-based rather than template-matching. A stroke is resampled to
//! a fixed number of points spread along its path, reduced to a handful of statistics
//! that do not
//! care where, how large or (mostly) how rotated it was drawn, and compared against one
//! prototype per shape. Two of those statistics are worth understanding, because they are
//! what make it work at all:
//!
//! * **`corner_turn_share`** — how much of the stroke's total turning happens at its four
//!   sharpest points. About 1 for any quadrilateral, however tapered or slanted, because
//!   all its turning is at the corners; about 0.55 for an ellipse, whose turning is spread
//!   evenly. This is what tells a sloppy trapezoid from an ellipse when their hull fill
//!   ratios happen to match.
//! * **`kurtosis_product`** — kurtosis along x times kurtosis along y. Each factor varies
//!   with the aspect ratio; their product barely does, so one number separates rectangle
//!   (1.83) from ellipse (2.25) from diamond (3.24) at any shape.
//!
//! Rectangle versus diamond is deliberately **not** rotation invariant, because a square
//! turned 45 degrees *is* a diamond — Excalidraw's own note says the same.

use crate::camera::Point;

/// Every stroke is resampled to this many evenly spaced points before measuring.
///
/// Features are moments of the point set, so raw samples would be biased by pointer
/// speed: a slow corner crowds points into it and weights the statistics towards
/// wherever the hand hesitated.
pub const RESAMPLE_N: usize = 64;

/// Smallest on-screen size, in pixels, for recognition to run at all.
///
/// Compared against the stroke's larger dimension times the zoom, so the same physical
/// gesture behaves the same however far in or out the board is. Below this a stroke reads
/// as an accidental scribble, and turning those into rectangles is worse than doing
/// nothing.
pub const RECOGNITION_MIN_SCREEN_SIZE: f64 = 25.0;

/// A stroke whose ends are further apart than this share of its own path length is open.
pub const CLOSED_GAP_MAX_RATIO: f64 = 0.15;
/// How much spread an open stroke may have across its major axis and still be straight.
pub const LINEAR_MAX_ELONGATION: f64 = 0.25;
/// The share of the start-to-tip distance around the tip where an arrowhead may live.
pub const ARROWHEAD_ZONE_RATIO: f64 = 0.5;
/// How far a straight stroke may stray from its own chord, outside the arrowhead zone.
pub const LINEAR_MAX_SHAFT_DEVIATION: f64 = 0.15;
/// How lopsided an open stroke must be before its heavy end counts as an arrowhead.
pub const ARROW_MIN_SKEW: f64 = 0.3;
/// How far from the nearest prototype a closed stroke may sit and still be recognised.
pub const CLOSED_SHAPE_MAX_DISTANCE: f64 = 1.5;
/// Half-width, in resampled points, of the window local turning is measured over.
///
/// Wide enough to smooth out pen wobble, narrow enough that two corners of a small
/// quadrilateral stay separate peaks instead of merging into one.
pub const TURN_WINDOW: usize = 3;

/// What a stroke was recognised as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecognizedShape {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    /// Not enough like anything. The stroke is left exactly as it was drawn.
    Freedraw,
}

/// The statistics a stroke is judged by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeFeatures {
    /// Distance between the endpoints over the stroke's own length. Near zero when the
    /// stroke came back to where it started.
    pub gap_ratio: f64,
    /// Spread across the minor axis over spread along the major one. Zero is a perfectly
    /// straight stroke; one has no preferred direction at all.
    pub elongation: f64,
    /// Lopsidedness along the major axis. An arrowhead adds mass at one end.
    pub major_skew: f64,
    /// Convex hull area over bounding box area: about 1 for a rectangle, `PI/4` for an
    /// ellipse, about a half for a diamond.
    pub hull_fill_ratio: f64,
    /// How much of the total turning is concentrated in four corners.
    pub corner_turn_share: f64,
    /// Kurtosis along x times kurtosis along y.
    pub kurtosis_product: f64,
    /// How far the stroke strays from the chord between its start and its furthest point.
    pub shaft_deviation_ratio: f64,
}

/// The principal axes of a point set: its own frame, found from its covariance.
#[derive(Clone, Copy, Debug)]
pub struct PrincipalAxes {
    pub centroid: Point,
    /// Unit vector along the direction the points spread furthest.
    pub major: (f64, f64),
    /// Unit vector at right angles to `major`.
    pub minor: (f64, f64),
    pub major_variance: f64,
    pub minor_variance: f64,
}

fn centroid(points: &[Point]) -> Point {
    let n = points.len() as f64;
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p.x, y + p.y));
    Point {
        x: sx / n,
        y: sy / n,
    }
}

/// Eigen decomposition of the point set's covariance matrix.
///
/// Together with the centroid this is a canonical frame: expressing the points in it
/// removes where the stroke was drawn and which way up, which is what makes the features
/// comparable between one person's rectangle and another's.
pub fn principal_axes(points: &[Point]) -> PrincipalAxes {
    let c = centroid(points);
    let n = points.len() as f64;

    let (mut m20, mut m02, mut m11) = (0.0, 0.0, 0.0);
    for p in points {
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        m20 += dx * dx;
        m02 += dy * dy;
        m11 += dx * dy;
    }
    m20 /= n;
    m02 /= n;
    m11 /= n;

    let trace = m20 + m02;
    let diff = (m20 - m02).hypot(2.0 * m11);
    let major_variance = (trace + diff) / 2.0;
    let minor_variance = (trace - diff) / 2.0;

    // When the covariance is already diagonal the axes *are* the coordinate axes, and
    // the eigenvector formula would be `0/0`.
    let major = if m11.abs() > f64::EPSILON {
        let (x, y) = (major_variance - m02, m11);
        let len = x.hypot(y);
        if len > 0.0 {
            (x / len, y / len)
        } else {
            (1.0, 0.0)
        }
    } else if m20 >= m02 {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };

    PrincipalAxes {
        centroid: c,
        major,
        minor: (-major.1, major.0),
        major_variance,
        minor_variance,
    }
}

/// The points expressed in the axes' own frame, as `(along major, along minor)`.
pub fn principal_coords(points: &[Point], axes: &PrincipalAxes) -> Vec<(f64, f64)> {
    points
        .iter()
        .map(|p| {
            let (dx, dy) = (p.x - axes.centroid.x, p.y - axes.centroid.y);
            (
                dx * axes.major.0 + dy * axes.major.1,
                dx * axes.minor.0 + dy * axes.minor.1,
            )
        })
        .collect()
}

/// Point the major axis at the denser end of the stroke.
///
/// An eigenvector has no inherent direction — `major` and `-major` are equally valid —
/// so the sign of the skew along it would otherwise be a coin toss, and the skew is
/// exactly what distinguishes an arrow from a line.
pub fn orient_principal_axes(points: &[Point], axes: PrincipalAxes) -> PrincipalAxes {
    let along: Vec<f64> = principal_coords(points, &axes)
        .iter()
        .map(|c| c.0)
        .collect();
    if skewness(&along) <= 0.0 {
        return axes;
    }
    let major = (-axes.major.0, -axes.major.1);
    PrincipalAxes {
        major,
        minor: (-major.1, major.0),
        ..axes
    }
}

/// Minor variance over major variance, in `0..=1`.
pub fn elongation(axes: &PrincipalAxes) -> f64 {
    if axes.major_variance > 0.0 {
        axes.minor_variance / axes.major_variance
    } else {
        1.0
    }
}

/// The `order`-th standardized moment: the moment divided by sigma to that power.
///
/// Standardising is what makes it scale-free, so a small rectangle and a large one give
/// the same number. Zero for a sample with no spread, which has no shape to describe.
pub fn standardized_moment(values: &[f64], order: i32) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;

    let (mut variance, mut moment) = (0.0, 0.0);
    for v in values {
        let d = v - mean;
        variance += d * d;
        moment += d.powi(order);
    }
    variance /= n;
    moment /= n;

    let sigma = variance.sqrt();
    if sigma > f64::EPSILON {
        moment / sigma.powi(order)
    } else {
        0.0
    }
}

/// Third standardized moment: how lopsided the sample is.
pub fn skewness(values: &[f64]) -> f64 {
    standardized_moment(values, 3)
}

/// Fourth standardized moment. Not *excess* kurtosis — a normal sample gives 3.
pub fn kurtosis(values: &[f64]) -> f64 {
    standardized_moment(values, 4)
}

/// The convex hull, by Andrew's monotone chain.
pub fn convex_hull(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });

    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let half = |pts: &[Point]| {
        let mut chain: Vec<Point> = Vec::new();
        for &p in pts {
            while chain.len() >= 2
                && cross(chain[chain.len() - 2], chain[chain.len() - 1], p) <= 0.0
            {
                chain.pop();
            }
            chain.push(p);
        }
        // The last point of each half is the first of the other.
        chain.pop();
        chain
    };

    let mut reversed = sorted.clone();
    reversed.reverse();
    let mut hull = half(&sorted);
    hull.extend(half(&reversed));

    if hull.len() >= 3 {
        hull
    } else {
        points.to_vec()
    }
}

/// Area of a polygon, by the shoelace formula. Always positive.
pub fn polygon_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    (sum / 2.0).abs()
}

/// Resample a stroke to exactly `n` points spread along its path.
///
/// **Evenly spaced only for a densely sampled stroke, which is the real case.** The
/// inner loop advances `prev` to each point it emits but keeps `seg_len` at the original
/// segment's length, so a segment much longer than the interval is subdivided into
/// stretching steps rather than equal ones. Traced against their JS, a stroke with one
/// enormous segment comes out with gaps from 0 to five times the interval.
///
/// Transcribed anyway, and deliberately. Real strokes arrive at about one point per
/// frame, so every segment is far shorter than the interval and the quirk never fires —
/// and the prototype constants below were fitted against *this* sampler. Correcting it
/// would shift every feature by an unknown amount and silently re-tune a classifier
/// nobody could re-fit.
pub fn resample(points: &[Point], n: usize) -> Vec<Point> {
    if points.is_empty() || n == 0 {
        return Vec::new();
    }
    if points.len() == 1 || n == 1 {
        return vec![points[0]; n];
    }

    let total: f64 = points
        .windows(2)
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .sum();
    // A stroke of zero length has no path to walk along; every sample is the same point.
    if total <= 0.0 {
        return vec![points[0]; n];
    }

    let interval = total / (n - 1) as f64;
    let mut result = vec![points[0]];
    let mut prev = points[0];
    let mut accumulated = 0.0;

    for &curr in &points[1..] {
        let seg_len = (curr.x - prev.x).hypot(curr.y - prev.y);
        if accumulated + seg_len >= interval {
            let mut remaining = interval - accumulated;
            while remaining <= seg_len + 1e-10 {
                let t = remaining / seg_len;
                let next = Point {
                    x: prev.x + t * (curr.x - prev.x),
                    y: prev.y + t * (curr.y - prev.y),
                };
                result.push(next);
                if result.len() == n {
                    return result;
                }
                // `seg_len` is deliberately *not* recomputed against the new `prev`:
                // it stays the original segment's length, and `remaining` walks along it
                // in whole intervals. That is Excalidraw's own loop, and changing it
                // would shift every sample.
                prev = next;
                remaining += interval;
            }
            accumulated = seg_len - (remaining - interval);
        } else {
            accumulated += seg_len;
        }
        prev = curr;
    }

    // Rounding can leave the last slot or two short.
    while result.len() < n {
        result.push(points[points.len() - 1]);
    }
    result
}

/// How far the stroke strays from the chord between its start and its furthest point.
///
/// Points near the tip are exempt, so an arrowhead is not counted as straying. This is
/// the check that rejects elbows, arcs and sawtooths, whose point-cloud statistics can
/// look perfectly line-like while their geometry plainly is not.
fn shaft_deviation_ratio(points: &[Point]) -> f64 {
    let start = points[0];
    let mut tip = start;
    let mut tip_distance = 0.0;
    for &p in points {
        let d = (p.x - start.x).hypot(p.y - start.y);
        if d > tip_distance {
            tip_distance = d;
            tip = p;
        }
    }
    if tip_distance == 0.0 {
        return 0.0;
    }

    let mut worst: f64 = 0.0;
    for &p in points {
        if (p.x - tip.x).hypot(p.y - tip.y) <= ARROWHEAD_ZONE_RATIO * tip_distance {
            continue;
        }
        worst = worst.max(crate::scene::geometry::distance_to_segment(
            p.x, p.y, start.x, start.y, tip.x, tip.y,
        ));
    }
    worst / tip_distance
}

/// The turn angle at each point, between the chords reaching `TURN_WINDOW` points either
/// side.
///
/// Deliberately **not** treated as a closed loop. Wrapping round would let the closure of
/// a hand-drawn outline — which always overshoots or undershoots its own start —
/// manufacture a sharp corner nobody drew, and four corners is exactly the thing being
/// counted.
fn windowed_turns(points: &[Point]) -> Vec<f64> {
    if points.len() <= 2 * TURN_WINDOW {
        return Vec::new();
    }
    (TURN_WINDOW..points.len() - TURN_WINDOW)
        .map(|i| {
            let a = points[i - TURN_WINDOW];
            let b = points[i];
            let c = points[i + TURN_WINDOW];
            let (v1x, v1y) = (b.x - a.x, b.y - a.y);
            let (v2x, v2y) = (c.x - b.x, c.y - b.y);
            (v1x * v2y - v1y * v2x).atan2(v1x * v2x + v1y * v2y).abs()
        })
        .collect()
}

/// The share of total turning that happens at the four sharpest places.
///
/// Each peak absorbs its neighbourhood, so one physical corner — whose turning smears
/// across the measuring window — is counted once rather than seven times.
fn corner_turn_share(points: &[Point]) -> f64 {
    let turns = windowed_turns(points);
    let total: f64 = turns.iter().sum();
    if total == 0.0 {
        return 0.0;
    }

    let mut taken = vec![false; turns.len()];
    let mut top4 = 0.0;
    for _ in 0..4 {
        let mut peak: Option<usize> = None;
        let mut peak_turn = 0.0;
        for (i, &turn) in turns.iter().enumerate() {
            if !taken[i] && turn > peak_turn {
                peak_turn = turn;
                peak = Some(i);
            }
        }
        let Some(peak) = peak else { break };
        let lo = peak.saturating_sub(TURN_WINDOW);
        let hi = (peak + TURN_WINDOW).min(turns.len() - 1);
        for i in lo..=hi {
            if !taken[i] {
                top4 += turns[i];
                taken[i] = true;
            }
        }
    }
    top4 / total
}

/// Reduce a stroke to the statistics it is judged by.
pub fn extract_features(points: &[Point]) -> StrokeFeatures {
    let pts = resample(points, RESAMPLE_N);

    let path_length: f64 = pts
        .windows(2)
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .sum();
    let last = pts[pts.len() - 1];
    let gap = (last.x - pts[0].x).hypot(last.y - pts[0].y);

    let axes = orient_principal_axes(&pts, principal_axes(&pts));
    let along: Vec<f64> = principal_coords(&pts, &axes).iter().map(|c| c.0).collect();

    let hull = convex_hull(&pts);
    let min_x = pts.iter().fold(f64::MAX, |a, p| a.min(p.x));
    let max_x = pts.iter().fold(f64::MIN, |a, p| a.max(p.x));
    let min_y = pts.iter().fold(f64::MAX, |a, p| a.min(p.y));
    let max_y = pts.iter().fold(f64::MIN, |a, p| a.max(p.y));
    let box_area = (max_x - min_x) * (max_y - min_y);

    StrokeFeatures {
        gap_ratio: if path_length > 0.0 {
            gap / path_length
        } else {
            0.0
        },
        elongation: elongation(&axes),
        major_skew: skewness(&along),
        hull_fill_ratio: if box_area > 0.0 {
            polygon_area(&hull) / box_area
        } else {
            0.0
        },
        corner_turn_share: corner_turn_share(&pts),
        kurtosis_product: kurtosis(&pts.iter().map(|p| p.x).collect::<Vec<_>>())
            * kurtosis(&pts.iter().map(|p| p.y).collect::<Vec<_>>()),
        shaft_deviation_ratio: shaft_deviation_ratio(&pts),
    }
}

/// One prototype per closed shape, in feature space.
struct Prototype {
    shape: RecognizedShape,
    hull_fill_ratio: f64,
    corner_turn_share: f64,
    kurtosis_product: f64,
}

const CLOSED_SHAPE_PROTOTYPES: [Prototype; 3] = [
    Prototype {
        shape: RecognizedShape::Rectangle,
        hull_fill_ratio: 1.0,
        corner_turn_share: 0.95,
        kurtosis_product: 1.83,
    },
    Prototype {
        shape: RecognizedShape::Diamond,
        hull_fill_ratio: 0.5,
        corner_turn_share: 0.95,
        kurtosis_product: 3.24,
    },
    Prototype {
        shape: RecognizedShape::Ellipse,
        // A circle fills exactly this much of its own box.
        hull_fill_ratio: std::f64::consts::FRAC_PI_4,
        corner_turn_share: 0.55,
        kurtosis_product: 2.25,
    },
];

/// How much slack each feature gets. Distances are measured in units of these, so a
/// feature with a wide tolerance contributes less to the total.
const HULL_FILL_RATIO_TOLERANCE: f64 = 0.2;
const CORNER_TURN_SHARE_TOLERANCE: f64 = 0.2;
const KURTOSIS_PRODUCT_TOLERANCE: f64 = 0.7;

/// The closed shape whose prototype the stroke sits nearest, or freedraw if none is near.
fn classify_closed(features: &StrokeFeatures) -> RecognizedShape {
    let mut best = RecognizedShape::Freedraw;
    let mut best_distance = CLOSED_SHAPE_MAX_DISTANCE;

    for prototype in &CLOSED_SHAPE_PROTOTYPES {
        let distance = (((features.hull_fill_ratio - prototype.hull_fill_ratio)
            / HULL_FILL_RATIO_TOLERANCE)
            .powi(2)
            + ((features.corner_turn_share - prototype.corner_turn_share)
                / CORNER_TURN_SHARE_TOLERANCE)
                .powi(2)
            + ((features.kurtosis_product - prototype.kurtosis_product)
                / KURTOSIS_PRODUCT_TOLERANCE)
                .powi(2))
        .sqrt();

        if distance < best_distance {
            best_distance = distance;
            best = prototype.shape;
        }
    }
    best
}

/// An open stroke is a line or an arrow, if it is straight enough to be either.
fn classify_open(features: &StrokeFeatures) -> RecognizedShape {
    if features.elongation > LINEAR_MAX_ELONGATION
        || features.shaft_deviation_ratio > LINEAR_MAX_SHAFT_DEVIATION
    {
        return RecognizedShape::Freedraw;
    }
    if features.major_skew.abs() >= ARROW_MIN_SKEW {
        RecognizedShape::Arrow
    } else {
        RecognizedShape::Line
    }
}

/// Which shape a stroke's features describe.
pub fn classify(features: &StrokeFeatures) -> RecognizedShape {
    if features.gap_ratio > CLOSED_GAP_MAX_RATIO {
        classify_open(features)
    } else {
        classify_closed(features)
    }
}

/// Recognise a freehand stroke.
///
/// `zoom` is the camera scale, used only to decide whether the stroke is large enough on
/// screen to be worth recognising: the same physical gesture should behave the same
/// however far in or out the board is.
pub fn recognize_shape(points: &[Point], zoom: f64) -> RecognizedShape {
    if points.len() < 3 {
        return RecognizedShape::Freedraw;
    }
    let min_x = points.iter().fold(f64::MAX, |a, p| a.min(p.x));
    let max_x = points.iter().fold(f64::MIN, |a, p| a.max(p.x));
    let min_y = points.iter().fold(f64::MAX, |a, p| a.min(p.y));
    let max_y = points.iter().fold(f64::MIN, |a, p| a.max(p.y));
    let max_dim = (max_x - min_x).max(max_y - min_y);

    // Too small to be meant. Turning an accidental scribble into a rectangle is worse
    // than leaving it alone, because it is a change nobody asked for.
    let screen_size = max_dim * zoom;
    if !screen_size.is_finite() || screen_size < RECOGNITION_MIN_SCREEN_SIZE {
        return RecognizedShape::Freedraw;
    }

    classify(&extract_features(points))
}

/// The tip of a recognised arrow.
///
/// The last point someone drew is rarely the tip — an arrowhead is drawn by going out to
/// the point and back, so the stroke ends on a barb. The tip is the point on the
/// bounding box perimeter furthest from the start, and this returns the drawn point
/// nearest to it.
pub fn arrow_endpoints(points: &[Point]) -> Option<(Point, Point)> {
    if points.len() < 2 {
        return None;
    }
    let start = points[0];
    let mut tip = points[1];
    let mut furthest = 0.0;
    for &p in &points[1..] {
        let d = (p.x - start.x).hypot(p.y - start.y);
        if d > furthest {
            furthest = d;
            tip = p;
        }
    }
    Some((start, tip))
}
