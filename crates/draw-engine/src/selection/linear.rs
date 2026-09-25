//! Handles for editing a line or arrow point by point.
//!
//! # Why linear elements need their own handles
//!
//! A line or an arrow was given the same eight box handles as a rectangle, and that is
//! the wrong model for it. Scaling a bounding box cannot express "point this end
//! somewhere else", which is the only thing anyone wants to do to an arrow — and for a
//! dead-horizontal or dead-vertical one the box is *degenerate*, so every handle
//! collapses onto the same line and dragging any of them produces nonsense: a flat
//! arrow acquires a phantom 1px height because the resize clamps to a minimum size.
//!
//! Instead, a linear element gets a handle **on each of its points**, plus a midpoint
//! handle between each pair for adding one. Dragging an endpoint moves that end and
//! nothing else, which works identically whatever direction the element runs in.

use crate::camera::Point;
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::is_valid_polygon;

/// Which part of a linear element a pointer grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearHandle {
    /// An existing point, by index.
    Point(usize),
    /// The gap between point `i` and `i + 1`, where a new point would be inserted.
    Midpoint(usize),
}

/// A handle in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearHandlePoint {
    pub handle: LinearHandle,
    pub x: f64,
    pub y: f64,
}

/// Whether this element is edited by points rather than by a bounding box.
///
/// A **count**, not a kind. Two points make a segment, and a box around a segment is
/// degenerate — for a dead-horizontal arrow it has no height at all, so every handle
/// lands on the same spot — and a box cannot express "point this end somewhere else"
/// anyway. Three or more points enclose an area, and an area is resized and turned by its
/// box like any other shape.
///
/// Excalidraw draws the line the same place: `hasBoundingBox` is
/// `element.points.length > 2` for a linear element (`transformHandles.ts@1118751f:352`), and the
/// point circles appear only while the line editor is open or the line has exactly two
/// points (`interactiveScene.ts@1118751f:1198`).
///
/// This used to be a kind alone, so every line and arrow was grabbed by its points
/// however many it had. The element that made it matter is the bucket fill — a closed
/// line of five points or more — which selected to a circle on every vertex of the region
/// and no box at all: it could not be resized, could not be turned, and its circles
/// dragged single corners of the paint away from the outline it was traced from.
///
/// The points of a longer path are not lost, only put behind a double click; the engine
/// adds that to this rule in [`crate::DrawEngine::shows_point_handles`].
pub fn is_point_edited(element: &DrawElement) -> bool {
    matches!(
        element.kind,
        crate::scene::element::DrawElementType::Line
            | crate::scene::element::DrawElementType::Arrow
    ) && element
        .points
        .as_deref()
        .is_none_or(|points| points.len() <= 2)
}

/// The element's points in world space.
///
/// Points are stored relative to the element's origin, so this is where they actually
/// are. Rotation is applied about the centre of the element's extent, matching the
/// painter's transform.
pub fn world_points(element: &DrawElement) -> Vec<Point> {
    let points = match element.points.as_ref() {
        Some(p) if !p.is_empty() => p,
        _ => return Vec::new(),
    };

    if element.angle == 0.0 {
        return points
            .iter()
            .map(|p| Point {
                x: element.x + p[0],
                y: element.y + p[1],
            })
            .collect();
    }

    // The pivot is the centre of the *points*, which is what the painter turns about.
    // `x + width / 2` sits outside a leftward arrow entirely.
    let c = crate::scene::geometry::rotation_center(element);
    let (cx, cy) = (c.x, c.y);
    let (sin, cos) = element.angle.sin_cos();
    points
        .iter()
        .map(|p| {
            let wx = element.x + p[0];
            let wy = element.y + p[1];
            let dx = wx - cx;
            let dy = wy - cy;
            Point {
                x: cx + dx * cos - dy * sin,
                y: cy + dx * sin + dy * cos,
            }
        })
        .collect()
}

/// Rebuilds a point-based element from where its points are in the world: the inverse of
/// [`world_points`] for the whole path.
///
/// Whatever turn the element had is already in those positions, so the result carries
/// none — a transform applied to a path's points is exact, where one applied to its box
/// and angle is only right while the points happen to fill the box.
pub fn from_world_points(element: &DrawElement, world: &[Point]) -> DrawElement {
    let mut next = element.clone();
    let Some(first) = world.first() else {
        return next;
    };
    next.x = first.x;
    next.y = first.y;
    next.angle = 0.0;
    normalise_points(
        &mut next,
        world
            .iter()
            .map(|p| [p.x - first.x, p.y - first.y])
            .collect(),
    );
    next
}

/// Converts a world position back into the element's local point space.
///
/// The inverse of [`world_points`] for a single point — what a drag needs in order to
/// write the new position back onto the element.
pub fn to_local(element: &DrawElement, world: Point) -> [f64; 2] {
    if element.angle == 0.0 {
        return [world.x - element.x, world.y - element.y];
    }

    // The same pivot `world_points` turns about, or the round trip does not close.
    let c = crate::scene::geometry::rotation_center(element);
    let (sin, cos) = (-element.angle).sin_cos();
    let dx = world.x - c.x;
    let dy = world.y - c.y;
    let ux = c.x + dx * cos - dy * sin;
    let uy = c.y + dx * sin + dy * cos;
    [ux - element.x, uy - element.y]
}

/// Every handle for this element: one per point, and one between each adjacent pair.
///
/// Midpoints are omitted when the segment is too short to be worth a separate target —
/// two handles a few pixels apart are impossible to hit deliberately.
///
/// # Why the midpoints are not simply `(a + b) / 2`
///
/// Because the path is usually not drawn straight. `roundness` defaults to `Some(8.0)`
/// on every element, and a rounded linear element goes through `generator::curve` — a
/// Catmull-Rom spline through the points. The chord centre of a curved segment is not on
/// the curve, so every midpoint handle floated beside the line it belonged to, and since
/// the hit test reads these same positions, the place you had to click was not the place
/// you could see.
///
/// Excalidraw forks on exactly this, `LinearElementEditor.getSegmentMidPoint`
/// (`element/linearElementEditor.ts@1118751f:976-1017`): the chord centre for a path built of
/// straight segments, `curvePointAtLength(segment, 0.5)` for one built of curves.
///
/// The curve is rebuilt from the **world** points rather than the local ones, which is
/// safe and saves a round trip: a Catmull-Rom control point is a weighted sum of the
/// points it spans, so rotating the points and then building the curve gives the same
/// answer as building it and then rotating.
pub fn handle_points(element: &DrawElement, min_segment: f64) -> Vec<LinearHandlePoint> {
    let points = world_points(element);
    let mut out = Vec::with_capacity(points.len() * 2);

    for (i, p) in points.iter().enumerate() {
        out.push(LinearHandlePoint {
            handle: LinearHandle::Point(i),
            x: p.x,
            y: p.y,
        });
    }

    // Only for a rounded path; `None` means sharp corners, and a sharp path really is
    // its chords.
    let curves = element.roundness.map(|_| {
        let flat: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
        crate::math::catmull_rom_cubics(&flat, crate::math::CURVE_TIGHTNESS)
    });

    for i in 0..points.len().saturating_sub(1) {
        let a = points[i];
        let b = points[i + 1];
        if (b.x - a.x).hypot(b.y - a.y) < min_segment {
            continue;
        }
        // Half way *along* the segment, not half way through its parameter: a cubic
        // covers more ground in one half of `t` than the other wherever it bends, so
        // `t = 0.5` would stay on the line but sit away from the middle you can see.
        let centre = curves
            .as_ref()
            .and_then(|curves| curves.get(i))
            .map(|curve| crate::math::bezier_point_at_fraction(curve, 0.5))
            .unwrap_or([(a.x + b.x) / 2.0, (a.y + b.y) / 2.0]);

        out.push(LinearHandlePoint {
            handle: LinearHandle::Midpoint(i),
            x: centre[0],
            y: centre[1],
        });
    }

    out
}

/// The handle under the pointer, if any.
///
/// Real points are tested before midpoints so that a midpoint sitting near an endpoint
/// never shadows it — moving an end is far more common than adding a point.
pub fn hit_handle(
    handles: &[LinearHandlePoint],
    x: f64,
    y: f64,
    tolerance: f64,
) -> Option<LinearHandle> {
    let mut best: Option<(f64, LinearHandle)> = None;
    for candidate in handles {
        let d = (candidate.x - x).hypot(candidate.y - y);
        if d > tolerance {
            continue;
        }
        let is_point = matches!(candidate.handle, LinearHandle::Point(_));
        let rank = if is_point { d } else { d + tolerance };
        if best.as_ref().is_none_or(|(b, _)| rank < *b) {
            best = Some((rank, candidate.handle));
        }
    }
    best.map(|(_, h)| h)
}

/// Rewrites an element's points so that `handle` now sits at `world`.
///
/// The element's origin and extent are recomputed from the resulting points, so a line
/// dragged past its own start flips cleanly instead of going negative-width. Returns
/// the element unchanged if the handle does not apply to it.
pub fn move_handle(element: &DrawElement, handle: LinearHandle, world: Point) -> DrawElement {
    let Some(points) = element.points.as_ref() else {
        return element.clone();
    };
    let mut points = points.clone();

    match handle {
        LinearHandle::Point(i) if i < points.len() => {
            let moved = to_local(element, world);
            points[i] = moved;
            // A polygon's first and last point are the same vertex twice over — dragging
            // either end has to move both, or the loop tears open. `movePoints`,
            // `packages/element/src/linearElementEditor.ts@1118751f:1663-1680`.
            if element.kind == DrawElementType::Line && element.is_polygon() {
                let last = points.len() - 1;
                if i == 0 {
                    points[last] = moved;
                } else if i == last {
                    points[0] = moved;
                }
            }
        }
        LinearHandle::Midpoint(i) if i + 1 < points.len() => {
            // Dragging a midpoint inserts a point there and starts moving it, which is
            // how a two-point arrow becomes a three-point one.
            points.insert(i + 1, to_local(element, world));
        }
        _ => return element.clone(),
    }

    let mut next = element.clone();
    normalise_points(&mut next, points);
    next
}

/// Adds a point at `world` between `i` and `i + 1`, returning its index.
pub fn insert_point(element: &DrawElement, i: usize, world: Point) -> (DrawElement, usize) {
    let moved = move_handle(element, LinearHandle::Midpoint(i), world);
    (moved, i + 1)
}

/// Removes a point, unless doing so would leave fewer than two.
pub fn remove_point(element: &DrawElement, i: usize) -> DrawElement {
    let Some(points) = element.points.as_ref() else {
        return element.clone();
    };
    if points.len() <= 2 || i >= points.len() {
        return element.clone();
    }
    let mut points = points.clone();
    points.remove(i);

    let mut next = element.clone();
    let is_polygon = element.kind == DrawElementType::Line && element.is_polygon();
    if is_polygon {
        // Keep the loop closed whichever vertex went — the start, the end, or one in
        // between — by snapping the new first point back onto the new last one.
        // `deletePoints`, `packages/element/src/linearElementEditor.ts@1118751f:1590-1602`.
        if let (Some(&last), true) = (points.last(), points.len() >= 2) {
            points[0] = last;
        }
        // A polygon broken below validity by the deletion gives up the flag, exactly as
        // it does when the drawing gesture itself closes on too short a loop
        // (`engine/multi_linear.rs`) — `actionFinalize.tsx@1118751f:336-340`.
        if !is_valid_polygon(&points) {
            next.polygon = Some(false);
        }
    }
    normalise_points(&mut next, points);
    next
}

/// Re-origins an element so its first point is at `(0, 0)` and its extent matches its
/// points.
///
/// Keeping the origin pinned to the first point is what the rest of the engine assumes,
/// and recomputing width and height keeps the bounding box honest — otherwise hit
/// testing, culling and the selection frame all drift away from the visible line.
fn normalise_points(element: &mut DrawElement, mut points: Vec<[f64; 2]>) {
    if points.is_empty() {
        return;
    }

    let shift = points[0];
    if shift != [0.0, 0.0] {
        for p in points.iter_mut() {
            p[0] -= shift[0];
            p[1] -= shift[1];
        }
        element.x += shift[0];
        element.y += shift[1];
    }

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }

    element.width = max_x - min_x;
    element.height = max_y - min_y;
    element.points = Some(points);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, DrawElementType, Geometry};

    fn arrow(points: Vec<[f64; 2]>) -> DrawElement {
        let mut e = create_element(
            DrawElementType::Arrow,
            Geometry {
                x: 100.0,
                y: 100.0,
                width: 0.0,
                height: 0.0,
            },
            DrawElementStyle::default(),
            0.0,
        );
        e.points = Some(points.clone());
        normalise_points(&mut e, points);
        e
    }

    #[test]
    fn points_are_reported_in_world_space() {
        let a = arrow(vec![[0.0, 0.0], [200.0, 50.0]]);
        let w = world_points(&a);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].x, w[0].y), (100.0, 100.0));
        assert_eq!((w[1].x, w[1].y), (300.0, 150.0));
    }

    /// `to_local` is the inverse of the local-to-world mapping `world_points` performs.
    ///
    /// The probe keeps the element's whole point list, because the pivot is the centre of
    /// the points: handing a one-point element to `world_points` would turn it about a
    /// different centre and the round trip would drift for that reason alone, testing the
    /// harness rather than the code.
    #[test]
    fn local_and_world_round_trip_under_rotation() {
        let mut a = arrow(vec![[0.0, 0.0], [200.0, 50.0]]);
        a.angle = 0.7;

        for (i, world) in world_points(&a).into_iter().enumerate() {
            let local = to_local(&a, world);
            let mut probe = a.clone();
            let mut points = a.points.clone().unwrap();
            points[i] = local;
            probe.points = Some(points);

            let back = world_points(&probe)[i];
            assert!((back.x - world.x).abs() < 1e-9, "x drifted");
            assert!((back.y - world.y).abs() < 1e-9, "y drifted");
        }
    }

    /// A leftward arrow turns about the middle of its own points.
    ///
    /// Reading the pivot as `x + width / 2` put it a full width past the arrow's own tip,
    /// so rotating one swung it round a point in empty space beside it.
    #[test]
    fn the_pivot_is_the_middle_of_the_points() {
        let leftward = arrow(vec![[0.0, 0.0], [-200.0, 0.0]]);
        let centre = crate::scene::geometry::rotation_center(&leftward);
        // The arrow runs from x=100 back to x=-100, so its middle is x=0.
        assert!((centre.x - 0.0).abs() < 1e-9, "got {}", centre.x);
        assert!((centre.y - 100.0).abs() < 1e-9);
    }

    /// The case box handles cannot express: a dead-horizontal arrow.
    #[test]
    fn a_flat_arrow_still_has_usable_handles() {
        let a = arrow(vec![[0.0, 0.0], [400.0, 0.0]]);
        assert_eq!(
            a.height, 0.0,
            "the bounding box is degenerate, as it must be"
        );

        let handles = handle_points(&a, 20.0);
        // Two endpoints and one midpoint, all distinct.
        assert_eq!(handles.len(), 3);
        assert_eq!(handles[0].x, 100.0);
        assert_eq!(handles[1].x, 500.0);

        // Dragging the far end up must move that end and leave the near one alone.
        let moved = move_handle(&a, LinearHandle::Point(1), Point { x: 500.0, y: 300.0 });
        let w = world_points(&moved);
        assert_eq!(
            (w[0].x, w[0].y),
            (100.0, 100.0),
            "the near end did not move"
        );
        assert_eq!(
            (w[1].x, w[1].y),
            (500.0, 300.0),
            "the far end went where asked"
        );
        assert_eq!(moved.height, 200.0, "and the bounding box followed");
    }

    #[test]
    fn dragging_an_end_past_the_other_flips_cleanly() {
        let a = arrow(vec![[0.0, 0.0], [200.0, 0.0]]);
        let moved = move_handle(
            &a,
            LinearHandle::Point(1),
            Point {
                x: -100.0,
                y: 100.0,
            },
        );

        assert!(moved.width >= 0.0, "width must not go negative");
        assert!(moved.height >= 0.0, "height must not go negative");
        let w = world_points(&moved);
        assert_eq!((w[1].x, w[1].y), (-100.0, 100.0));
    }

    #[test]
    fn the_origin_follows_the_first_point() {
        let a = arrow(vec![[0.0, 0.0], [200.0, 0.0]]);
        let moved = move_handle(&a, LinearHandle::Point(0), Point { x: 0.0, y: 0.0 });

        assert_eq!(
            moved.points.as_ref().unwrap()[0],
            [0.0, 0.0],
            "still origin-anchored"
        );
        let w = world_points(&moved);
        assert_eq!((w[0].x, w[0].y), (0.0, 0.0), "and it is where we put it");
        assert_eq!((w[1].x, w[1].y), (300.0, 100.0), "the other end held still");
    }

    #[test]
    fn a_midpoint_drag_adds_a_point() {
        let a = arrow(vec![[0.0, 0.0], [200.0, 0.0]]);
        let bent = move_handle(&a, LinearHandle::Midpoint(0), Point { x: 200.0, y: 200.0 });

        assert_eq!(bent.points.as_ref().unwrap().len(), 3);
        let w = world_points(&bent);
        assert_eq!(
            (w[1].x, w[1].y),
            (200.0, 200.0),
            "the new point is where dragged"
        );
    }

    #[test]
    fn midpoints_are_omitted_on_segments_too_short_to_aim_at() {
        let tiny = arrow(vec![[0.0, 0.0], [5.0, 0.0]]);
        let handles = handle_points(&tiny, 20.0);
        assert_eq!(handles.len(), 2, "endpoints only");
        assert!(handles
            .iter()
            .all(|h| matches!(h.handle, LinearHandle::Point(_))));
    }

    /// An endpoint must win over a midpoint that happens to be within tolerance, or a
    /// short segment becomes impossible to re-aim.
    #[test]
    fn an_endpoint_beats_a_nearby_midpoint() {
        let a = arrow(vec![[0.0, 0.0], [60.0, 0.0]]);
        let handles = handle_points(&a, 20.0);
        let hit = hit_handle(&handles, 100.0, 100.0, 40.0);
        assert_eq!(hit, Some(LinearHandle::Point(0)));
    }

    #[test]
    fn nothing_is_hit_outside_the_tolerance() {
        let a = arrow(vec![[0.0, 0.0], [200.0, 0.0]]);
        let handles = handle_points(&a, 20.0);
        assert_eq!(hit_handle(&handles, 100.0, 400.0, 10.0), None);
    }

    #[test]
    fn a_two_point_line_refuses_to_lose_a_point() {
        let a = arrow(vec![[0.0, 0.0], [200.0, 0.0]]);
        let same = remove_point(&a, 1);
        assert_eq!(
            same.points.as_ref().unwrap().len(),
            2,
            "a line needs two ends"
        );
    }

    #[test]
    fn a_middle_point_can_be_removed() {
        let a = arrow(vec![[0.0, 0.0], [100.0, 100.0], [200.0, 0.0]]);
        let straightened = remove_point(&a, 1);
        assert_eq!(straightened.points.as_ref().unwrap().len(), 2);
        assert_eq!(straightened.height, 0.0, "the box shrank with the points");
    }
}
