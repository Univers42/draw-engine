//! Corner-radius handles: where they sit, and what a pointer position means.
//!
//! Not an Excalidraw feature. Theirs offers Sharp and Round and nothing between, though
//! its model has a slot for exactly this (`roundness.value`, read as a fixed radius by
//! `getCornerRadius` and never set by any UI). A handle inside each corner writes that
//! value directly, the way design tools let you pull a corner round.
//!
//! One radius for all four corners, driven by whichever handle is dragged — the model has
//! one number, and independent corners would need four.

use crate::camera::Point;
use crate::scene::element::{DrawElement, DrawElementType};

/// The radius the renderer draws, for an element this module offers handles on.
fn effective_radius(element: &DrawElement, w: f64, h: f64) -> f64 {
    crate::render::shape::corner_radius(w.min(h), element)
}

/// Whether an element is rounded by corner-radius handles at all.
///
/// Rectangles only, for now. An image is drawn as a bitmap that does not clip to its
/// outline, so a radius on one would change nothing visible; a diamond's rounded corners
/// are built from two radii measured off different spans and need handles at its
/// vertices rather than inside a box.
pub fn supports_corner_radius(element: &DrawElement) -> bool {
    element.kind == DrawElementType::Rectangle
}

/// The inward unit direction of each corner, clockwise from the top-left, in local
/// (unrotated) space: +x is right, +y is down.
const CORNERS: [(f64, f64); 4] = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];

/// The element's box, normalised, and its centre.
fn frame(element: &DrawElement) -> (f64, f64, f64, f64, Point) {
    let r =
        crate::scene::geometry::normalize_rect(element.x, element.y, element.width, element.height);
    let centre = Point {
        x: r.x + r.width / 2.0,
        y: r.y + r.height / 2.0,
    };
    (r.x, r.y, r.width, r.height, centre)
}

fn rotate(p: Point, about: Point, angle: f64) -> Point {
    if angle == 0.0 {
        return p;
    }
    let (sin, cos) = angle.sin_cos();
    let (dx, dy) = (p.x - about.x, p.y - about.y);
    Point {
        x: about.x + dx * cos - dy * sin,
        y: about.y + dx * sin + dy * cos,
    }
}

/// The four handles in world space, clockwise from the top-left.
///
/// Each sits on its corner's inward diagonal at the drawn radius — the centre of the arc
/// it controls, so the handle and the curve move together. `min_inset` keeps a handle
/// grabbable on a sharp or barely-rounded corner, where "at the radius" would put it on
/// the corner itself, underneath the resize handle.
pub fn radius_handles(element: &DrawElement, min_inset: f64) -> [Point; 4] {
    let (x, y, w, h, centre) = frame(element);
    let inset = effective_radius(element, w, h)
        .max(min_inset)
        // Never past the middle: two handles crossing over would swap which corner
        // each one appears to belong to.
        .min(w.min(h) / 2.0);
    let corner_points = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let mut out = [Point { x: 0.0, y: 0.0 }; 4];
    for (i, ((cx, cy), (ix, iy))) in corner_points.into_iter().zip(CORNERS).enumerate() {
        out[i] = rotate(
            Point {
                x: cx + ix * inset,
                y: cy + iy * inset,
            },
            centre,
            element.angle,
        );
    }
    out
}

/// How far along `corner`'s inward diagonal a world point is, in radius units.
///
/// The average of the two inward offsets: moving along the diagonal by `(d, d)` reads as
/// exactly `d`, and moving along one edge counts for half, so a drag that is not quite
/// diagonal still behaves predictably. Measured in the element's own frame, so it works
/// on a turned shape.
///
/// Callers take a *difference* of two of these, never the value itself — see
/// `radius_after_drag`.
pub fn along_corner(element: &DrawElement, corner: usize, world: Point) -> f64 {
    let (x, y, w, h, centre) = frame(element);
    let local = rotate(world, centre, -element.angle);
    let corner_points = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let (cx, cy) = corner_points[corner % 4];
    let (ix, iy) = CORNERS[corner % 4];
    ((local.x - cx) * ix + (local.y - cy) * iy) / 2.0
}

/// The radius after dragging a handle from `grab` to `now`.
///
/// Relative to the grab rather than to the corner. The handle is drawn at
/// `max(radius, min_inset)`, so on a small radius it sits further in than the radius it
/// controls — and measuring from the corner would make the radius jump to the handle's
/// position the moment it was pressed.
pub fn radius_after_drag(
    element: &DrawElement,
    corner: usize,
    start_radius: f64,
    grab: Point,
    now: Point,
) -> f64 {
    let (_, _, w, h, _) = frame(element);
    let moved = along_corner(element, corner, now) - along_corner(element, corner, grab);
    (start_radius + moved).clamp(0.0, w.min(h) / 2.0)
}

/// The radius a rectangle is drawn with now — the starting point for a drag.
pub fn current_radius(element: &DrawElement) -> f64 {
    let (_, _, w, h, _) = frame(element);
    effective_radius(element, w, h)
}
