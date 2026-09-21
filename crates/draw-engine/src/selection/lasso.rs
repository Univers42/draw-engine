//! Free-form selection: draw a loop, take what it encloses.
//!
//! Every part of this is geometry, so every part of it lives here rather than in a
//! frontend. A host only has to report where the pointer went and paint the path it is
//! handed back; the decision about what that loop selects is the engine's, and is
//! identical on any platform that drives the same engine.
//!
//! Transcribed from Excalidraw's `lasso/utils.ts` at the SHA pinned in
//! `scripts/oracle-sha.txt`. Their pipeline, and this one:
//!
//! 1. simplify the captured path, which is a raw pointer trail and far denser than the
//!    shape it describes;
//! 2. reject on axis-aligned boxes first, because that is cheap and most elements on a
//!    large board are nowhere near the loop;
//! 3. test the survivors properly, against the element's own outline.

use crate::camera::{Point, WorldBounds};
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::{element_rotated_bounds, normalize_rect};

/// What counts as selected.
///
/// Excalidraw's `BoxSelectionMode`, and it is a real choice rather than a detail:
/// `Contain` is predictable and is their default, `Intersect` is what people expect
/// when they scribble quickly through a diagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LassoMode {
    /// The element must lie wholly inside the loop.
    #[default]
    Contain,
    /// The loop need only cross the element.
    Intersect,
}

/// How far a point may sit from the simplified path before it is kept.
///
/// Excalidraw simplifies with `points-on-curve`'s `simplify`, which is
/// Ramer–Douglas–Peucker. A raw pointer trail is hundreds of points a second; testing
/// every element against every one of them is what makes a lasso feel heavy on a large
/// board, and the extra points describe the hand's tremor rather than the loop.
pub const LASSO_SIMPLIFY_DISTANCE: f64 = 2.0;

/// Ramer–Douglas–Peucker: drop the points that do not change the shape.
///
/// Keeps the endpoints, keeps whichever interior point is furthest from the chord
/// between them, and recurses on each side — but only while that furthest point is
/// further than `tolerance`. Iterative rather than recursive so a long path cannot
/// overflow the stack.
pub fn simplify_path(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() <= 2 || tolerance <= 0.0 {
        return points.to_vec();
    }

    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;

    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let a = points[start];
        let b = points[end];

        let mut worst = 0.0_f64;
        let mut worst_at = start;
        for (i, p) in points.iter().enumerate().take(end).skip(start + 1) {
            let d = crate::scene::geometry::distance_to_segment(p.x, p.y, a.x, a.y, b.x, b.y);
            if d > worst {
                worst = d;
                worst_at = i;
            }
        }

        if worst > tolerance {
            keep[worst_at] = true;
            stack.push((start, worst_at));
            stack.push((worst_at, end));
        }
    }

    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// Whether the polygon encloses the point, by the **non-zero winding rule**.
///
/// Excalidraw's `polygonIncludesPointNonZero`. Winding rather than even-odd because a
/// lasso is drawn by hand and crosses itself constantly: under even-odd, a loop that
/// doubles back leaves holes in its own middle, so scribbling around a diagram would
/// select some of it and not the rest. Under winding, anything the path goes around is
/// inside, however many times the path crossed itself getting there.
pub fn polygon_contains_point(polygon: &[Point], point: Point) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut winding = 0i32;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];

        if a.y <= point.y {
            if b.y > point.y && cross(a, b, point) > 0.0 {
                winding += 1;
            }
        } else if b.y <= point.y && cross(a, b, point) < 0.0 {
            winding -= 1;
        }
    }
    winding != 0
}

/// `> 0` when `p` is left of the directed line `a -> b`.
fn cross(a: Point, b: Point, p: Point) -> f64 {
    (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)
}

/// Whether two segments cross.
fn segments_cross(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    let d1 = cross(p3, p4, p1);
    let d2 = cross(p3, p4, p2);
    let d3 = cross(p1, p2, p3);
    let d4 = cross(p1, p2, p4);

    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    // Collinear touching counts, so a loop drawn exactly along an edge still catches it.
    let on = |a: Point, b: Point, p: Point, d: f64| {
        d == 0.0
            && p.x >= a.x.min(b.x)
            && p.x <= a.x.max(b.x)
            && p.y >= a.y.min(b.y)
            && p.y <= a.y.max(b.y)
    };
    on(p3, p4, p1, d1) || on(p3, p4, p2, d2) || on(p1, p2, p3, d3) || on(p1, p2, p4, d4)
}

/// The outline of an element, as the points a lasso is tested against.
///
/// A line or arrow is its own path; everything else is its box, turned if it is turned.
/// Sampling an ellipse as its four box corners would let a loop drawn snugly around a
/// circle miss it, so curved shapes are sampled around their perimeter.
fn outline(element: &DrawElement) -> Vec<Point> {
    if matches!(
        element.kind,
        DrawElementType::Line | DrawElementType::Arrow | DrawElementType::Freedraw
    ) {
        let points = crate::selection::linear::world_points(element);
        if !points.is_empty() {
            return points;
        }
    }

    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    let centre = crate::scene::geometry::rotation_center(element);
    let (sin, cos) = element.angle.sin_cos();
    let turn = |x: f64, y: f64| {
        let (dx, dy) = (x - centre.x, y - centre.y);
        Point {
            x: centre.x + dx * cos - dy * sin,
            y: centre.y + dx * sin + dy * cos,
        }
    };

    if element.kind == DrawElementType::Ellipse {
        // Enough samples that a loop hugging the curve cannot slip between them.
        const SAMPLES: usize = 24;
        let (rx, ry) = (rect.width / 2.0, rect.height / 2.0);
        let (cx, cy) = (rect.x + rx, rect.y + ry);
        return (0..SAMPLES)
            .map(|i| {
                let t = i as f64 / SAMPLES as f64 * std::f64::consts::TAU;
                turn(cx + rx * t.cos(), cy + ry * t.sin())
            })
            .collect();
    }

    if element.kind == DrawElementType::Diamond {
        let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        return vec![
            turn(cx, rect.y),
            turn(rect.x + rect.width, cy),
            turn(cx, rect.y + rect.height),
            turn(rect.x, cy),
        ];
    }

    vec![
        turn(rect.x, rect.y),
        turn(rect.x + rect.width, rect.y),
        turn(rect.x + rect.width, rect.y + rect.height),
        turn(rect.x, rect.y + rect.height),
    ]
}

fn bounds_of(points: &[Point]) -> Option<WorldBounds> {
    let first = points.first()?;
    let mut b = WorldBounds {
        min_x: first.x,
        min_y: first.y,
        max_x: first.x,
        max_y: first.y,
    };
    for p in points {
        b.min_x = b.min_x.min(p.x);
        b.min_y = b.min_y.min(p.y);
        b.max_x = b.max_x.max(p.x);
        b.max_y = b.max_y.max(p.y);
    }
    Some(b)
}

fn boxes_overlap(a: WorldBounds, b: WorldBounds) -> bool {
    a.min_x <= b.max_x && a.max_x >= b.min_x && a.min_y <= b.max_y && a.max_y >= b.min_y
}

fn box_contains(outer: WorldBounds, inner: WorldBounds) -> bool {
    outer.min_x <= inner.min_x
        && outer.min_y <= inner.min_y
        && outer.max_x >= inner.max_x
        && outer.max_y >= inner.max_y
}

/// Ids of the elements a lasso path selects.
///
/// `path` is the raw pointer trail in world coordinates; it is simplified here, so a
/// host never has to.
pub fn elements_in_lasso<'a>(
    elements: impl Iterator<Item = &'a DrawElement>,
    path: &[Point],
    mode: LassoMode,
) -> Vec<String> {
    let simplified = simplify_path(path, LASSO_SIMPLIFY_DISTANCE);
    if simplified.len() < 3 {
        return Vec::new();
    }
    let Some(lasso_bounds) = bounds_of(&simplified) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for element in elements {
        if element.is_deleted || element.locked() {
            continue;
        }
        // A label belongs to its container and is taken with it, never on its own.
        if element.container_id.is_some() {
            continue;
        }

        let element_bounds = element_rotated_bounds(element);
        // Cheap reject first: most of a large board is nowhere near the loop.
        if !boxes_overlap(lasso_bounds, element_bounds) {
            continue;
        }
        if mode == LassoMode::Contain && !box_contains(lasso_bounds, element_bounds) {
            continue;
        }

        let shape = outline(element);
        if shape.is_empty() {
            continue;
        }

        let taken = match mode {
            // Every point of the outline inside the loop. Checking the box would take
            // an element whose corner pokes out of a concave lasso.
            LassoMode::Contain => shape
                .iter()
                .all(|p| polygon_contains_point(&simplified, *p)),
            LassoMode::Intersect => {
                shape
                    .iter()
                    .any(|p| polygon_contains_point(&simplified, *p))
                    || crosses(&simplified, &shape, element)
            }
        };

        if taken {
            out.push(element.id.clone());
        }
    }
    out
}

/// Whether the lasso crosses the element's outline.
///
/// Only asked in `Intersect` mode, and only after the boxes have already overlapped.
fn crosses(lasso: &[Point], shape: &[Point], element: &DrawElement) -> bool {
    // A line is an open path; a shape closes back to its first point.
    let closed = !matches!(
        element.kind,
        DrawElementType::Line | DrawElementType::Arrow | DrawElementType::Freedraw
    );
    let edges = if closed { shape.len() } else { shape.len() - 1 };

    for i in 0..edges {
        let a = shape[i];
        let b = shape[(i + 1) % shape.len()];
        for j in 0..lasso.len() {
            let c = lasso[j];
            let d = lasso[(j + 1) % lasso.len()];
            if segments_cross(a, b, c, d) {
                return true;
            }
        }
    }
    false
}
