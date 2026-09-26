//! The parametric outline of a `figure` element.
//!
//! A figure is not a free path: its outline is generated from a small parameter set —
//! [`FigureKind`], an optional `sides` and an optional `ratio` — inside the element's own
//! box. [`outline`] is the one pure function that turns those into geometry, in
//! element-local **unit-box** coordinates (`x`/`y` each `0..=1`, top-left origin, exactly
//! the box `scene::geometry::local_box` already describes for every other shape).
//! Everything that needs a figure's shape — paint, hit test, bounds, binding, export —
//! scales this ring (and any extra open strokes) by the element's own width and height and
//! consumes it; nothing else defines a figure's geometry.
//!
//! Excalidraw has no such element: every shape of theirs is either a fixed primitive
//! (rectangle, diamond, ellipse) or a free-drawn path. This one is deliberately in
//! between — a handful of named silhouettes, each with the one number that changes its
//! character (`docs/reference/figure.md`).

use serde::{Deserialize, Serialize};

/// The six parametric outlines a figure can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FigureKind {
    /// A regular polygon, stretched to the box, one vertex at the top. `sides` 3 is the
    /// triangle, 6 the hexagon.
    Polygon,
    /// `sides` points, `ratio` the inner radius over the outer.
    Star,
    /// `ratio` the horizontal skew over the width.
    Parallelogram,
    /// `ratio` the top inset over the width.
    Trapezoid,
    /// `ratio` the cap height over the height. The outline is the silhouette; it also
    /// has one extra open stroke, the front arc of the top ellipse.
    Cylinder,
    /// `ratio` the wave amplitude over the height: a wavy bottom edge.
    Document,
}

/// A figure's parameters, carried on the element only when its `kind` is `"figure"`.
///
/// Absent `sides`/`ratio` mean the kind's own default — see [`resolved_sides`] and
/// [`resolved_ratio`]. Validated at the contract boundary (`packages/contract/src/element.ts`):
/// `sides` 3..=12, `ratio` 0.05..=0.95.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FigureParams {
    pub kind: FigureKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sides: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
}

impl Default for FigureParams {
    /// A hexagon — a shape with a visible `sides` and no `ratio`, so the inspector's two
    /// controls are exercised by picking nothing in particular.
    fn default() -> Self {
        Self {
            kind: FigureKind::Polygon,
            sides: Some(6),
            ratio: None,
        }
    }
}

/// The contract's `sides` range (`packages/contract/src/element.ts`).
pub const SIDES_RANGE: std::ops::RangeInclusive<u8> = 3..=12;
/// The contract's `ratio` range.
pub const RATIO_RANGE: std::ops::RangeInclusive<f64> = 0.05..=0.95;

const DEFAULT_SIDES: u8 = 6;
const DEFAULT_STAR_POINTS: u8 = 5;

/// The sides (a star's points) a figure draws with: the given value clamped to the
/// contract's range, or the kind's own default when none was given.
pub fn resolved_sides(kind: FigureKind, sides: Option<u8>) -> u8 {
    let default = if kind == FigureKind::Star {
        DEFAULT_STAR_POINTS
    } else {
        DEFAULT_SIDES
    };
    sides
        .unwrap_or(default)
        .clamp(*SIDES_RANGE.start(), *SIDES_RANGE.end())
}

/// The ratio a figure draws with: the given value clamped to what keeps *this* kind's
/// outline simple — never self-crossing — which for some kinds is narrower than the
/// contract's own 0.05..=0.95 (a trapezoid at 0.95 would have its top edge cross itself).
/// Absent is the kind's own default. Meaningless for [`FigureKind::Polygon`], which has
/// none.
pub fn resolved_ratio(kind: FigureKind, ratio: Option<f64>) -> f64 {
    let (default, lo, hi): (f64, f64, f64) = match kind {
        FigureKind::Polygon => return 0.0,
        FigureKind::Star => (0.5, 0.05, 0.95),
        FigureKind::Parallelogram => (0.25, 0.05, 0.95),
        FigureKind::Trapezoid => (0.25, 0.05, 0.49),
        FigureKind::Cylinder => (0.2, 0.05, 0.45),
        FigureKind::Document => (0.14, 0.04, 0.4),
    };
    ratio.unwrap_or(default).clamp(lo, hi)
}

/// Whether this kind has a `ratio` at all — the inspector's slider row.
pub fn has_ratio(kind: FigureKind) -> bool {
    !matches!(kind, FigureKind::Polygon)
}

/// Whether this kind has a `sides` stepper — a regular polygon or a star's points.
pub fn has_sides(kind: FigureKind) -> bool {
    matches!(kind, FigureKind::Polygon | FigureKind::Star)
}

/// Applies a kind change to `params`, in place: `sides` is kept only when the old and the
/// new kind both have one ([`has_sides`] — polygon and star are the only pair), since
/// anywhere else it is a leftover count that means nothing on the new kind; `ratio` always
/// resets to the new kind's own default ([`resolved_ratio`]), because it is a different
/// axis on every kind that has one (a star's inner radius is not a trapezoid's inset). A
/// no-op already at `kind`.
///
/// Shared by the two places a kind is picked: the inspector's patch
/// (`scene::element::apply_style_patch`'s `figure_kind`) and the Shapes tool's own queued
/// kind (`engine::selection_style::apply_style`), so a pick changes both the same way.
pub fn change_kind(params: &mut FigureParams, kind: FigureKind) {
    if params.kind == kind {
        return;
    }
    if !(has_sides(params.kind) && has_sides(kind)) {
        params.sides = None;
    }
    params.ratio = None;
    params.kind = kind;
}

/// One vertex of an outline, in unit-box coordinates: `(0, 0)` is the box's top-left
/// corner, `(1, 1)` its bottom-right, whatever the element's actual width and height are.
pub type Vertex = (f64, f64);

/// A figure's outline: the closed ring painted, hit-tested and bound to as the shape
/// itself, and any extra open strokes drawn on top of it — only the cylinder's front rim.
#[derive(Clone, Debug, PartialEq)]
pub struct FigureOutline {
    pub closed: Vec<Vertex>,
    pub extra: Vec<Vec<Vertex>>,
}

/// How finely a curve is sampled into a polyline.
const ARC_SAMPLES: usize = 16;

/// The one pure function a figure's shape comes from, in unit-box coordinates. Everything
/// else — paint, hit test, bounds, binding, export — scales this by the element's own box.
pub fn outline(kind: FigureKind, sides: Option<u8>, ratio: Option<f64>) -> FigureOutline {
    match kind {
        FigureKind::Polygon => FigureOutline {
            closed: fit_to_unit_box(&regular_polygon(resolved_sides(kind, sides))),
            extra: Vec::new(),
        },
        FigureKind::Star => FigureOutline {
            closed: fit_to_unit_box(&star_points(
                resolved_sides(kind, sides),
                resolved_ratio(kind, ratio),
            )),
            extra: Vec::new(),
        },
        FigureKind::Parallelogram => {
            let skew = resolved_ratio(kind, ratio);
            FigureOutline {
                closed: vec![(skew, 0.0), (1.0, 0.0), (1.0 - skew, 1.0), (0.0, 1.0)],
                extra: Vec::new(),
            }
        }
        FigureKind::Trapezoid => {
            let inset = resolved_ratio(kind, ratio);
            FigureOutline {
                closed: vec![(inset, 0.0), (1.0 - inset, 0.0), (1.0, 1.0), (0.0, 1.0)],
                extra: Vec::new(),
            }
        }
        FigureKind::Cylinder => cylinder_outline(resolved_ratio(kind, ratio)),
        FigureKind::Document => FigureOutline {
            closed: document_outline(resolved_ratio(kind, ratio)),
            extra: Vec::new(),
        },
    }
}

/// A regular polygon on the unit circle, one vertex at the top (`-90°`), evenly spaced —
/// `sides` 3 is a triangle, 6 a hexagon.
fn regular_polygon(sides: u8) -> Vec<Vertex> {
    let sides = sides.max(3) as usize;
    (0..sides)
        .map(|i| {
            let angle =
                -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / sides as f64;
            (angle.cos(), angle.sin())
        })
        .collect()
}

/// `points` outer vertices on the unit circle alternating with `points` inner ones at
/// `ratio` of the radius, the first outer vertex at the top.
fn star_points(points: u8, ratio: f64) -> Vec<Vertex> {
    let points = points.max(3) as usize;
    (0..points * 2)
        .map(|i| {
            let angle =
                -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / points as f64;
            let r = if i % 2 == 0 { 1.0 } else { ratio };
            (r * angle.cos(), r * angle.sin())
        })
        .collect()
}

/// Rescales `points`, independently per axis, so their bounding box becomes exactly
/// `(0, 0)..(1, 1)` — "stretched to the box": a shape generated on a unit circle keeps its
/// angles but fills whatever rectangle it is drawn into, exactly as the diamond and
/// ellipse already do for their own boxes.
fn fit_to_unit_box(points: &[Vertex]) -> Vec<Vertex> {
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let (dx, dy) = ((max_x - min_x).max(1e-9), (max_y - min_y).max(1e-9));
    points
        .iter()
        .map(|&(x, y)| ((x - min_x) / dx, (y - min_y) / dy))
        .collect()
}

/// A point on an ellipse centred at `(cx, cy)` with radii `(rx, ry)`: `t = 0` is its
/// rightmost point, `t = PI` its leftmost, increasing through the bottom in this
/// (y-down) coordinate system.
fn ellipse_point(cx: f64, cy: f64, rx: f64, ry: f64, t: f64) -> Vertex {
    (cx + rx * t.cos(), cy + ry * t.sin())
}

/// The cylinder's silhouette and its one extra stroke.
///
/// The silhouette is the top ellipse's **back** (upper) arc, the right side straight down,
/// the bottom ellipse's **front** (lower) arc, and the left side straight back up — the
/// ring closes the last edge itself. The front arc of the *top* ellipse would otherwise be
/// inside that silhouette, so it is drawn separately, on top, as the open rim of the cap.
fn cylinder_outline(cap: f64) -> FigureOutline {
    let (cx, rx) = (0.5, 0.5);
    let mut closed = Vec::with_capacity(ARC_SAMPLES * 2 + 2);
    // Top ellipse, back (upper) arc: left, over the top, to right.
    for i in 0..=ARC_SAMPLES {
        let t = std::f64::consts::PI + (i as f64 / ARC_SAMPLES as f64) * std::f64::consts::PI;
        closed.push(ellipse_point(cx, cap, rx, cap, t));
    }
    // Bottom ellipse, front (lower) arc: right, under the bottom, to left. The straight
    // side between the two arcs is implicit: both arcs end/start at the same `x`.
    for i in 0..=ARC_SAMPLES {
        let t = (i as f64 / ARC_SAMPLES as f64) * std::f64::consts::PI;
        closed.push(ellipse_point(cx, 1.0 - cap, rx, cap, t));
    }
    // The top ellipse's front (lower) arc: right, under, to left — the visible rim of the
    // open cap, drawn as its own stroke rather than folded into the silhouette above it.
    let front_rim: Vec<Vertex> = (0..=ARC_SAMPLES)
        .map(|i| {
            let t = (i as f64 / ARC_SAMPLES as f64) * std::f64::consts::PI;
            ellipse_point(cx, cap, rx, cap, t)
        })
        .collect();
    FigureOutline {
        closed,
        extra: vec![front_rim],
    }
}

/// A rectangle whose bottom edge is a sine wave of `amplitude` (a fraction of the height),
/// sampled right to left after the top edge, so the ring closes back to the top-left
/// corner along a straight left side.
fn document_outline(amplitude: f64) -> Vec<Vertex> {
    const WAVES: f64 = 1.5;
    const SAMPLES: usize = 24;
    let baseline = 1.0 - amplitude;
    let mut points = vec![(0.0, 0.0), (1.0, 0.0)];
    for i in 0..=SAMPLES {
        let x = 1.0 - i as f64 / SAMPLES as f64;
        let y = baseline + amplitude * (x * std::f64::consts::TAU * WAVES).sin();
        points.push((x, y));
    }
    points
}

/// [`outline`]'s closed ring, scaled and centred: `(0, 0)` maps to `(-hx, -hy)` and
/// `(1, 1)` to `(hx, hy)`. What [`crate::scene::outline_distance`] and
/// [`crate::scene::binding`] work in — a shape-centred, half-extent frame shared with the
/// rectangle, diamond and ellipse formulas already there.
pub fn centered_vertices(
    kind: FigureKind,
    sides: Option<u8>,
    ratio: Option<f64>,
    hx: f64,
    hy: f64,
) -> Vec<Vertex> {
    outline(kind, sides, ratio)
        .closed
        .into_iter()
        .map(|(u, v)| ((u * 2.0 - 1.0) * hx, (v * 2.0 - 1.0) * hy))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(points: &[Vertex]) -> (f64, f64, f64, f64) {
        let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
        let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(x, y) in points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        (min_x, min_y, max_x, max_y)
    }

    #[test]
    fn a_triangle_is_three_points() {
        let outline = outline(FigureKind::Polygon, Some(3), None);
        assert_eq!(outline.closed.len(), 3);
        assert!(outline.extra.is_empty());
    }

    #[test]
    fn a_hexagon_is_six_points() {
        let outline = outline(FigureKind::Polygon, Some(6), None);
        assert_eq!(outline.closed.len(), 6);
    }

    #[test]
    fn every_polygon_fills_the_unit_box() {
        for sides in 3..=12 {
            let outline = outline(FigureKind::Polygon, Some(sides), None);
            let (min_x, min_y, max_x, max_y) = bbox(&outline.closed);
            assert!((min_x - 0.0).abs() < 1e-9, "sides={sides}");
            assert!((min_y - 0.0).abs() < 1e-9, "sides={sides}");
            assert!((max_x - 1.0).abs() < 1e-9, "sides={sides}");
            assert!((max_y - 1.0).abs() < 1e-9, "sides={sides}");
        }
    }

    #[test]
    fn a_star_has_twice_its_points_as_vertices() {
        let outline = outline(FigureKind::Star, Some(5), Some(0.5));
        assert_eq!(outline.closed.len(), 10);
    }

    #[test]
    fn sides_out_of_range_is_clamped_not_rejected() {
        assert_eq!(resolved_sides(FigureKind::Polygon, Some(2)), 3);
        assert_eq!(resolved_sides(FigureKind::Polygon, Some(99)), 12);
    }

    #[test]
    fn absent_params_are_the_kinds_defaults() {
        assert_eq!(resolved_sides(FigureKind::Polygon, None), DEFAULT_SIDES);
        assert_eq!(resolved_sides(FigureKind::Star, None), DEFAULT_STAR_POINTS);
        assert!(resolved_ratio(FigureKind::Star, None) > 0.0);
    }

    #[test]
    fn ratio_out_of_this_kinds_safe_range_is_clamped() {
        // 0.95 is within the contract's range but would self-cross a trapezoid's top edge.
        assert!(resolved_ratio(FigureKind::Trapezoid, Some(0.95)) < 0.5);
    }

    #[test]
    fn parallelogram_and_trapezoid_already_fill_the_box() {
        for kind in [FigureKind::Parallelogram, FigureKind::Trapezoid] {
            let (min_x, min_y, max_x, max_y) = bbox(&outline(kind, None, None).closed);
            assert!((min_x - 0.0).abs() < 1e-9);
            assert!((min_y - 0.0).abs() < 1e-9);
            assert!((max_x - 1.0).abs() < 1e-9);
            assert!((max_y - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn cylinder_has_a_silhouette_and_one_extra_stroke() {
        let outline = outline(FigureKind::Cylinder, None, None);
        assert_eq!(outline.extra.len(), 1);
        let (min_x, min_y, max_x, max_y) = bbox(&outline.closed);
        assert!((min_x - 0.0).abs() < 1e-9);
        assert!((min_y - 0.0).abs() < 1e-9);
        assert!((max_x - 1.0).abs() < 1e-9);
        assert!((max_y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn document_has_a_flat_top_and_a_wavy_bottom() {
        let outline = outline(FigureKind::Document, None, Some(0.2)).closed;
        assert_eq!(outline[0], (0.0, 0.0));
        assert_eq!(outline[1], (1.0, 0.0));
        // The wave dips below the flat top's own baseline.
        assert!(outline.iter().any(|&(_, y)| y > 0.5));
    }

    #[test]
    fn kind_change_keeps_sides_only_between_polygon_and_star() {
        let mut params = FigureParams {
            kind: FigureKind::Polygon,
            sides: Some(7),
            ratio: None,
        };
        change_kind(&mut params, FigureKind::Star);
        assert_eq!(params.kind, FigureKind::Star);
        assert_eq!(params.sides, Some(7), "polygon <-> star keeps sides");

        change_kind(&mut params, FigureKind::Cylinder);
        assert_eq!(
            params.sides, None,
            "sides drop leaving the polygon/star pair"
        );
    }

    #[test]
    fn kind_change_always_resets_ratio() {
        let mut params = FigureParams {
            kind: FigureKind::Trapezoid,
            sides: None,
            ratio: Some(0.4),
        };
        change_kind(&mut params, FigureKind::Cylinder);
        assert_eq!(params.ratio, None);
    }

    #[test]
    fn centered_vertices_span_the_given_half_extents() {
        let vertices = centered_vertices(FigureKind::Trapezoid, None, None, 50.0, 20.0);
        let (min_x, min_y, max_x, max_y) = bbox(&vertices);
        assert!((min_x - -50.0).abs() < 1e-9);
        assert!((min_y - -20.0).abs() < 1e-9);
        assert!((max_x - 50.0).abs() < 1e-9);
        assert!((max_y - 20.0).abs() < 1e-9);
    }
}
