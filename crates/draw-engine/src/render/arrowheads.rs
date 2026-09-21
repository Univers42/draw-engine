//! Arrowhead geometry, in element-local space.
//!
//! This used to live inside `export/svg.rs` as SVG string formatting, which meant the
//! canvas painter had no way to reach it — it called `default_arrowhead` and threw the
//! result away (`let _ = ...`), so arrows were indistinguishable from lines on screen
//! while the SVG export drew all six heads correctly. Producing points here lets both
//! consume one source, so they cannot disagree again.
//!
//! # Fidelity note
//!
//! The proportions below (`0.42`, `0.3`, `0.32`, the `PI/7` spread) are this project's
//! own, not Excalidraw's. They are consistent between canvas and export, which is what
//! this module fixes; matching Excalidraw's `getArrowheadPoints` exactly is a separate
//! step and is deliberately not claimed here.

use crate::scene::element::{Arrowhead, DrawElement};

pub fn default_arrowhead(element: &DrawElement, end: &str) -> Arrowhead {
    let explicit = if end == "start" {
        element.start_arrowhead
    } else {
        element.end_arrowhead
    };
    if let Some(kind) = explicit {
        return kind;
    }
    if element.kind == crate::scene::DrawElementType::Arrow && end == "end" {
        Arrowhead::Arrow
    } else {
        Arrowhead::None
    }
}

/// How an arrowhead is painted, as well as where.
#[derive(Clone, Debug, PartialEq)]
pub enum ArrowheadGeometry {
    /// Stroked, left open — the classic two-stroke chevron and the bar.
    Polyline(Vec<[f64; 2]>),
    /// Filled — triangle and diamond.
    Polygon(Vec<[f64; 2]>),
    /// Filled circle.
    Dot { center: [f64; 2], radius: f64 },
}

/// The size of an arrowhead for a given stroke width.
///
/// Scaling with the stroke keeps a head readable on a hairline and stops it swamping a
/// thick arrow.
pub fn arrowhead_size(stroke_width: f64) -> f64 {
    (stroke_width * 6.0).clamp(12.0, 40.0)
}

fn rotate_about(p: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// Builds the head at `tip`, pointing away from `from`.
///
/// Returns `None` when there is no head to draw, or when the segment is too short to
/// have a direction — pointing an arrowhead along a zero-length vector yields NaN.
pub fn arrowhead_geometry(
    kind: Arrowhead,
    tip: [f64; 2],
    from: [f64; 2],
    stroke_width: f64,
) -> Option<ArrowheadGeometry> {
    if kind == Arrowhead::None {
        return None;
    }

    let dx = tip[0] - from[0];
    let dy = tip[1] - from[1];
    if dx.hypot(dy) < 1e-6 {
        return None;
    }
    let angle = dy.atan2(dx);
    let size = arrowhead_size(stroke_width);

    // Built pointing along +x at the origin, then rotated onto the segment and moved to
    // the tip — the same construction the SVG exporter expresses as a transform.
    let place = |pts: Vec<[f64; 2]>| -> Vec<[f64; 2]> {
        pts.into_iter()
            .map(|p| {
                let r = rotate_about(p, angle);
                [r[0] + tip[0], r[1] + tip[1]]
            })
            .collect()
    };

    Some(match kind {
        Arrowhead::Arrow => {
            let spread = std::f64::consts::PI / 7.0;
            let bx = -size * spread.cos();
            let by = size * spread.sin();
            ArrowheadGeometry::Polyline(place(vec![[bx, -by], [0.0, 0.0], [bx, by]]))
        }
        Arrowhead::Triangle => ArrowheadGeometry::Polygon(place(vec![
            [0.0, 0.0],
            [-size, -size * 0.42],
            [-size, size * 0.42],
        ])),
        Arrowhead::Diamond => ArrowheadGeometry::Polygon(place(vec![
            [0.0, 0.0],
            [-size * 0.5, -size * 0.42],
            [-size, 0.0],
            [-size * 0.5, size * 0.42],
        ])),
        Arrowhead::Dot => {
            let c = rotate_about([-size * 0.3, 0.0], angle);
            ArrowheadGeometry::Dot {
                center: [c[0] + tip[0], c[1] + tip[1]],
                radius: size * 0.32,
            }
        }
        Arrowhead::Bar => {
            ArrowheadGeometry::Polyline(place(vec![[0.0, -size * 0.5], [0.0, size * 0.5]]))
        }
        Arrowhead::None => unreachable!("returned above"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_head_for_a_degenerate_segment() {
        // A zero-length direction would make atan2 meaningless and the head NaN.
        assert!(arrowhead_geometry(Arrowhead::Arrow, [10.0, 10.0], [10.0, 10.0], 2.0).is_none());
        assert!(arrowhead_geometry(Arrowhead::None, [10.0, 10.0], [0.0, 0.0], 2.0).is_none());
    }

    /// The head's apex sits exactly on the tip, whatever direction the arrow points.
    #[test]
    fn the_apex_lands_on_the_tip() {
        for (from, tip) in [
            ([0.0, 0.0], [100.0, 0.0]),
            ([0.0, 0.0], [0.0, 100.0]),
            ([100.0, 100.0], [0.0, 0.0]),
            ([0.0, 0.0], [-70.0, 70.0]),
        ] {
            let g = arrowhead_geometry(Arrowhead::Arrow, tip, from, 2.0).unwrap();
            let ArrowheadGeometry::Polyline(pts) = g else {
                panic!("an arrow head is a polyline");
            };
            assert!((pts[1][0] - tip[0]).abs() < 1e-9, "apex x for {tip:?}");
            assert!((pts[1][1] - tip[1]).abs() < 1e-9, "apex y for {tip:?}");
        }
    }

    /// The barbs trail *behind* the tip — a head rotated the wrong way points backwards,
    /// which looks plausible in a screenshot and is wrong.
    #[test]
    fn the_barbs_trail_behind_the_tip() {
        let g = arrowhead_geometry(Arrowhead::Arrow, [100.0, 0.0], [0.0, 0.0], 2.0).unwrap();
        let ArrowheadGeometry::Polyline(pts) = g else {
            unreachable!()
        };
        assert!(pts[0][0] < 100.0);
        assert!(pts[2][0] < 100.0);
        // ...and symmetrically about the arrow's axis.
        assert!((pts[0][1] + pts[2][1]).abs() < 1e-9);
    }

    #[test]
    fn heads_scale_with_the_stroke_but_stay_within_bounds() {
        assert_eq!(arrowhead_size(0.5), 12.0, "clamped up for a hairline");
        assert_eq!(arrowhead_size(4.0), 24.0);
        assert_eq!(
            arrowhead_size(100.0),
            40.0,
            "clamped down for an absurd stroke"
        );
    }

    #[test]
    fn filled_heads_are_polygons_and_open_ones_are_polylines() {
        let tip = [100.0, 0.0];
        let from = [0.0, 0.0];
        assert!(matches!(
            arrowhead_geometry(Arrowhead::Triangle, tip, from, 2.0).unwrap(),
            ArrowheadGeometry::Polygon(_)
        ));
        assert!(matches!(
            arrowhead_geometry(Arrowhead::Diamond, tip, from, 2.0).unwrap(),
            ArrowheadGeometry::Polygon(_)
        ));
        assert!(matches!(
            arrowhead_geometry(Arrowhead::Bar, tip, from, 2.0).unwrap(),
            ArrowheadGeometry::Polyline(_)
        ));
        assert!(matches!(
            arrowhead_geometry(Arrowhead::Dot, tip, from, 2.0).unwrap(),
            ArrowheadGeometry::Dot { .. }
        ));
    }
}
