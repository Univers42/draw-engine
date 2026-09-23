//! The clean outline of an element, for showing which members a selection holds.
//!
//! A multi-selection used to draw a padded box around every member. On a board of
//! overlapping shapes the boxes were the loudest thing on screen and said the least: a
//! box is the same for a circle, a diamond and a scribble, and it covers whatever sits
//! beside the shape. Tracing the shape itself shows exactly what is held.
//!
//! Clean geometry, not the element's rough strokes: the sketchy double line is how the
//! shape is *drawn*, and tracing it again in the selection colour would draw a second
//! sketch on top of the first. Everything here is in element-local space — the same
//! space the element's own geometry is generated in — so the painter places it with the
//! element's transform, mirror and rotation included.

use draw_rough::renderer::Segment;

use crate::render::shape::{
    corner_radius, diamond_points, rounded_diamond_segments, rounded_rect_segments,
};
use crate::scene::element::{DrawElement, DrawElementType};

/// What to trace for one element.
#[derive(Debug, PartialEq)]
pub enum Outline {
    /// A path, open or closed as its segments say.
    Path(Vec<Segment>),
    /// An ellipse filling the box `0..w` × `0..h`.
    Ellipse { w: f64, h: f64 },
    /// A freehand stroke: the outline of its ink, which the painter already holds.
    Freehand,
}

/// The outline of `element`, or `None` when it has nothing to trace.
pub fn element_outline(element: &DrawElement) -> Option<Outline> {
    let w = element.width.abs();
    let h = element.height.abs();
    match element.kind {
        DrawElementType::Rectangle
        | DrawElementType::Image
        | DrawElementType::Frame
        | DrawElementType::Embed
        | DrawElementType::Text => {
            if w == 0.0 && h == 0.0 {
                return None;
            }
            if element.roundness.is_some() && element.kind != DrawElementType::Text {
                let mut path = rounded_rect_segments(w, h, corner_radius(w.min(h), element));
                path.push(Segment::Close);
                Some(Outline::Path(path))
            } else {
                Some(Outline::Path(polygon(&[
                    [0.0, 0.0],
                    [w, 0.0],
                    [w, h],
                    [0.0, h],
                ])))
            }
        }
        DrawElementType::Diamond => {
            if w == 0.0 && h == 0.0 {
                return None;
            }
            let pts = diamond_points(w, h);
            Some(Outline::Path(if element.roundness.is_some() {
                let vertical = corner_radius((pts[0][0] - pts[3][0]).abs(), element);
                let horizontal = corner_radius((pts[1][1] - pts[0][1]).abs(), element);
                let mut path = rounded_diamond_segments(w, h, vertical, horizontal);
                path.push(Segment::Close);
                path
            } else {
                polygon(&pts)
            }))
        }
        DrawElementType::Ellipse => (w > 0.0 || h > 0.0).then_some(Outline::Ellipse { w, h }),
        DrawElementType::Line | DrawElementType::Arrow => {
            let points = element.points.as_deref().unwrap_or(&[]);
            if points.len() < 2 {
                return None;
            }
            Some(Outline::Path(if element.roundness.is_some() {
                curve(points)
            } else {
                polyline(points)
            }))
        }
        DrawElementType::Freedraw => {
            (element.points.as_ref().is_some_and(|p| p.len() >= 2)).then_some(Outline::Freehand)
        }
    }
}

fn polyline(points: &[[f64; 2]]) -> Vec<Segment> {
    let mut path = Vec::with_capacity(points.len());
    path.push(Segment::MoveTo(points[0]));
    path.extend(points[1..].iter().map(|p| Segment::LineTo(*p)));
    path
}

fn polygon(points: &[[f64; 2]]) -> Vec<Segment> {
    let mut path = polyline(points);
    path.push(Segment::Close);
    path
}

/// The curve a rounded line is drawn along: rough's Catmull-Rom with the jitter left
/// out, so the trace follows the drawn line instead of cutting its bends.
///
/// Rough doubles the first and last points before fitting, which is what makes the
/// curve start and end exactly on them; this does the same.
fn curve(points: &[[f64; 2]]) -> Vec<Segment> {
    let first = points[0];
    let last = points[points.len() - 1];
    let mut ps = Vec::with_capacity(points.len() + 2);
    ps.push(first);
    ps.extend_from_slice(points);
    ps.push(last);

    let mut path = vec![Segment::MoveTo(ps[1])];
    for i in 1..ps.len() - 2 {
        let b1 = [
            ps[i][0] + (ps[i + 1][0] - ps[i - 1][0]) / 6.0,
            ps[i][1] + (ps[i + 1][1] - ps[i - 1][1]) / 6.0,
        ];
        let b2 = [
            ps[i + 1][0] + (ps[i][0] - ps[i + 2][0]) / 6.0,
            ps[i + 1][1] + (ps[i][1] - ps[i + 2][1]) / 6.0,
        ];
        path.push(Segment::CurveTo([
            b1[0],
            b1[1],
            b2[0],
            b2[1],
            ps[i + 1][0],
            ps[i + 1][1],
        ]));
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, Geometry};

    fn element(kind: DrawElementType, width: f64, height: f64) -> DrawElement {
        create_element(
            kind,
            Geometry {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            DrawElementStyle::default(),
            0.0,
        )
    }

    fn ends(path: &[Segment]) -> Vec<[f64; 2]> {
        path.iter()
            .filter_map(|s| match s {
                Segment::MoveTo(p) | Segment::LineTo(p) => Some(*p),
                Segment::CurveTo(c) => Some([c[4], c[5]]),
                Segment::Close => None,
            })
            .collect()
    }

    #[test]
    fn a_rectangle_is_traced_on_its_own_edges() {
        let mut rect = element(DrawElementType::Rectangle, 80.0, 40.0);
        rect.roundness = None;
        let Some(Outline::Path(path)) = element_outline(&rect) else {
            panic!("a rectangle has a path");
        };
        assert_eq!(
            ends(&path),
            vec![[0.0, 0.0], [80.0, 0.0], [80.0, 40.0], [0.0, 40.0]]
        );
        assert_eq!(path.last(), Some(&Segment::Close));
    }

    #[test]
    fn a_mirrored_shape_is_traced_from_its_size() {
        // The mirror is the painter's transform; the outline is built from the extent,
        // exactly as the shape's own geometry is.
        let mut rect = element(DrawElementType::Rectangle, -80.0, -40.0);
        rect.roundness = None;
        let Some(Outline::Path(path)) = element_outline(&rect) else {
            panic!("a rectangle has a path");
        };
        assert_eq!(ends(&path)[2], [80.0, 40.0]);
    }

    #[test]
    fn a_rounded_rectangle_keeps_its_corners() {
        let mut rect = element(DrawElementType::Rectangle, 100.0, 100.0);
        rect.roundness = Some(8.0);
        let Some(Outline::Path(path)) = element_outline(&rect) else {
            panic!("a rectangle has a path");
        };
        assert!(path.iter().any(|s| matches!(s, Segment::CurveTo(_))));
        assert!(
            !ends(&path).contains(&[0.0, 0.0]),
            "the corner is cut, not squared"
        );
    }

    #[test]
    fn a_diamond_is_its_four_points() {
        let mut diamond = element(DrawElementType::Diamond, 60.0, 40.0);
        diamond.roundness = None;
        let Some(Outline::Path(path)) = element_outline(&diamond) else {
            panic!("a diamond has a path");
        };
        assert_eq!(
            ends(&path),
            vec![[30.0, 0.0], [60.0, 20.0], [30.0, 40.0], [0.0, 20.0]]
        );
    }

    #[test]
    fn an_ellipse_is_an_ellipse() {
        let ellipse = element(DrawElementType::Ellipse, 60.0, 40.0);
        assert_eq!(
            element_outline(&ellipse),
            Some(Outline::Ellipse { w: 60.0, h: 40.0 })
        );
    }

    #[test]
    fn a_sharp_line_is_traced_through_its_points_and_left_open() {
        let mut line = element(DrawElementType::Line, 0.0, 0.0);
        line.roundness = None;
        line.points = Some(vec![[0.0, 0.0], [50.0, 10.0], [100.0, 0.0]]);
        let Some(Outline::Path(path)) = element_outline(&line) else {
            panic!("a line has a path");
        };
        assert_eq!(ends(&path), vec![[0.0, 0.0], [50.0, 10.0], [100.0, 0.0]]);
        assert!(!path.contains(&Segment::Close));
    }

    #[test]
    fn a_curved_line_passes_through_every_point() {
        let mut line = element(DrawElementType::Arrow, 0.0, 0.0);
        line.roundness = Some(8.0);
        line.points = Some(vec![[0.0, 0.0], [50.0, 30.0], [100.0, 0.0]]);
        let Some(Outline::Path(path)) = element_outline(&line) else {
            panic!("a line has a path");
        };
        assert_eq!(ends(&path), vec![[0.0, 0.0], [50.0, 30.0], [100.0, 0.0]]);
        assert!(path[1..].iter().all(|s| matches!(s, Segment::CurveTo(_))));
    }

    #[test]
    fn nothing_to_trace_is_nothing() {
        let mut line = element(DrawElementType::Line, 0.0, 0.0);
        line.points = Some(vec![[0.0, 0.0]]);
        assert_eq!(element_outline(&line), None);
        assert_eq!(
            element_outline(&element(DrawElementType::Rectangle, 0.0, 0.0)),
            None
        );
        let mut stroke = element(DrawElementType::Freedraw, 0.0, 0.0);
        stroke.points = Some(vec![[0.0, 0.0], [4.0, 4.0]]);
        assert_eq!(element_outline(&stroke), Some(Outline::Freehand));
    }
}
