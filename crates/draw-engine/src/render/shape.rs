//! Turns a [`DrawElement`] into a rough [`Drawable`], transcribed from Excalidraw's
//! `_generateElementShape` (`packages/element/src/shape.ts`).
//!
//! # Element-local space
//!
//! Every shape is generated at the origin, spanning `0..width` by `0..height` — exactly
//! as Excalidraw does. The element's position and rotation are applied by the painter
//! as a transform, never baked into the geometry. That is what makes **translation free**:
//! dragging an element never regenerates a single op, only resizing does. The shape
//! cache depends on it.

use draw_rough::renderer::Segment;
use draw_rough::{generator, Drawable, Options};

use crate::render::opts::generate_rough_options;
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::figure;

/// `DEFAULT_PROPORTIONAL_RADIUS` / `DEFAULT_ADAPTIVE_RADIUS`
/// (`packages/common/src/constants.ts`).
const PROPORTIONAL_RADIUS: f64 = 0.25;
const ADAPTIVE_RADIUS: f64 = 32.0;

/// `getCornerRadius(x, element)` for an adaptive-radius element.
///
/// Below the cutoff the radius is proportional, above it fixed — so a small rectangle
/// looks like a rounded rectangle rather than a lozenge, and a large one does not end
/// up with absurdly round corners.
///
/// Our element model still carries `roundness` as a bare `Option<f64>` rather than
/// Excalidraw's `{type, value}`. `Some(_)` is read as "rounded" and given Excalidraw's
/// adaptive rule, which is what its default rectangle uses; the stored radius is
/// ignored. Replacing the field with the typed form is a schema change that has to
/// carry a migration, and is tracked separately.
pub fn corner_radius(x: f64, element: &DrawElement) -> f64 {
    if element.roundness.is_none() {
        return 0.0;
    }
    // A note has a rule of its own, from its short side (`getStickyNoteCornerRadius`,
    // `utils.ts@1118751f:256-258`), and no corner handle to give it another.
    if element.kind == DrawElementType::StickyNote {
        return crate::scene::sticky::sticky_corner_radius(element);
    }
    // An explicit radius, from a corner-radius handle. Clamped to half the span, which is
    // a pill: any more and opposite corners would overlap.
    //
    // Deliberately past the oracle's own cap. `getCornerRadius` limits every radius to a
    // quarter of the short side, which would make a handle stop a quarter of the way in
    // and feel broken.
    if let Some(explicit) = element.corner_radius {
        return explicit.clamp(0.0, x / 2.0);
    }
    match element.kind {
        // Excalidraw gives rectangles (and rectangle-likes) the adaptive rule.
        DrawElementType::Rectangle
        | DrawElementType::Image
        | DrawElementType::Frame
        | DrawElementType::Embed => {
            let cutoff = ADAPTIVE_RADIUS / PROPORTIONAL_RADIUS;
            if x <= cutoff {
                x * PROPORTIONAL_RADIUS
            } else {
                ADAPTIVE_RADIUS
            }
        }
        // Diamonds and linear elements use the proportional rule.
        _ => x * PROPORTIONAL_RADIUS,
    }
}

/// The segments of Excalidraw's rounded-rectangle path.
///
/// Excalidraw writes it as an SVG string and lets rough parse it; we build the
/// normalized segments directly, which is both faster and removes a parser from the
/// trust chain. The quadratics are elevated to cubics exactly as `normalize()` does.
///
/// The path deliberately does **not** close — Excalidraw's `d` has no `Z`, and adding
/// one would emit an extra doubled line along the top edge.
pub fn rounded_rect_segments(w: f64, h: f64, r: f64) -> Vec<Segment> {
    // Each quadratic is elevated relative to the point the preceding line ended at, so
    // the current point is tracked explicitly rather than inferred.
    vec![
        Segment::MoveTo([r, 0.0]),
        Segment::LineTo([w - r, 0.0]),
        Segment::quad_to_cubic([w - r, 0.0], w, 0.0, w, r),
        Segment::LineTo([w, h - r]),
        Segment::quad_to_cubic([w, h - r], w, h, w - r, h),
        Segment::LineTo([r, h]),
        Segment::quad_to_cubic([r, h], 0.0, h, 0.0, h - r),
        Segment::LineTo([0.0, r]),
        Segment::quad_to_cubic([0.0, r], 0.0, 0.0, r, 0.0),
    ]
}

/// `getDiamondPoints(element)` — top, right, bottom, left.
pub fn diamond_points(w: f64, h: f64) -> [[f64; 2]; 4] {
    [[w / 2.0, 0.0], [w, h / 2.0], [w / 2.0, h], [0.0, h / 2.0]]
}

/// The segments of Excalidraw's rounded-diamond path.
///
/// Unlike the rectangle this one is written with explicit cubics whose two control
/// points are both the corner vertex, so the elevation helper is not involved.
pub fn rounded_diamond_segments(w: f64, h: f64, vr: f64, hr: f64) -> Vec<Segment> {
    let [[top_x, top_y], [right_x, right_y], [bottom_x, bottom_y], [left_x, left_y]] =
        diamond_points(w, h);

    vec![
        Segment::MoveTo([top_x + vr, top_y + hr]),
        Segment::LineTo([right_x - vr, right_y - hr]),
        Segment::CurveTo([
            right_x,
            right_y,
            right_x,
            right_y,
            right_x - vr,
            right_y + hr,
        ]),
        Segment::LineTo([bottom_x + vr, bottom_y - hr]),
        Segment::CurveTo([
            bottom_x,
            bottom_y,
            bottom_x,
            bottom_y,
            bottom_x - vr,
            bottom_y - hr,
        ]),
        Segment::LineTo([left_x + vr, left_y + hr]),
        Segment::CurveTo([left_x, left_y, left_x, left_y, left_x + vr, left_y - hr]),
        Segment::LineTo([top_x - vr, top_y + hr]),
        Segment::CurveTo([top_x, top_y, top_x, top_y, top_x + vr, top_y + hr]),
    ]
}

/// The points a linear element is drawn through, in element-local space.
///
/// An element that has just been created can have an empty point list; Excalidraw
/// substitutes a single origin point rather than producing nothing.
fn linear_points(element: &DrawElement) -> Vec<[f64; 2]> {
    match element.points.as_ref() {
        Some(points) if !points.is_empty() => points.clone(),
        _ => vec![[0.0, 0.0]],
    }
}

/// An elbow arrow's path, `generateElbowArrowShape(points, 16)`: its runs, each corner
/// a quadratic — elevated here as rough's `normalize()` elevates the oracle's `Q`.
fn elbow_segments(points: &[[f64; 2]]) -> Vec<Segment> {
    let corners = crate::scene::elbow::rounded_corners(points, crate::scene::elbow::CORNER_RADIUS);
    let mut segments = vec![Segment::MoveTo(points[0])];
    for [before, corner, after] in corners {
        segments.push(Segment::LineTo(before));
        segments.push(Segment::quad_to_cubic(
            before, corner[0], corner[1], after[0], after[1],
        ));
    }
    segments.push(Segment::LineTo(points[points.len() - 1]));
    segments
}

/// Generates the rough geometry for one element, in element-local space.
///
/// Returns `None` for element types that are not drawn through rough at all — text is
/// laid out and filled directly, and freedraw uses a stroke outline rather than a
/// sketched path.
pub fn element_drawable(element: &DrawElement) -> Option<Drawable> {
    // Absolute: a negative extent means the element is mirrored on that axis, and the
    // mirror is a sign in the painter's transform, not a different shape. Generating from
    // the signed value would rebuild every op on a flip and — because rough's jitter is
    // derived from the coordinates it is given — would hand back a different hand-drawn
    // stroke instead of the same one reversed.
    let w = element.width.abs();
    let h = element.height.abs();

    match element.kind {
        // Painted directly, canvas and SVG alike, never through rough.js (`shape.ts@1118751f:996-999`).
        DrawElementType::StickyNote => None,
        DrawElementType::Rectangle
        | DrawElementType::Image
        | DrawElementType::Frame
        | DrawElementType::Embed => Some(if element.roundness.is_some() {
            let r = corner_radius(w.min(h), element);
            let o = generate_rough_options(element, true);
            generator::path(&rounded_rect_segments(w, h, r), o)
        } else {
            let o = generate_rough_options(element, false);
            generator::rectangle(0.0, 0.0, w, h, o)
        }),

        DrawElementType::Diamond => Some(if element.roundness.is_some() {
            let pts = diamond_points(w, h);
            // Excalidraw measures the two radii off different spans: the vertical
            // radius from the horizontal extent and vice versa.
            let vertical_radius = corner_radius((pts[0][0] - pts[3][0]).abs(), element);
            let horizontal_radius = corner_radius((pts[1][1] - pts[0][1]).abs(), element);
            let o = generate_rough_options(element, true);
            generator::path(
                &rounded_diamond_segments(w, h, vertical_radius, horizontal_radius),
                o,
            )
        } else {
            let o = generate_rough_options(element, false);
            generator::polygon(&diamond_points(w, h), o)
        }),

        DrawElementType::Ellipse => {
            let o = generate_rough_options(element, false);
            Some(generator::ellipse(w / 2.0, h / 2.0, w, h, o))
        }

        DrawElementType::Arrow if crate::scene::elbow::is_elbow(element) => {
            let points = linear_points(element);
            // The oracle draws nothing past a million out rather than a shape that size
            // (`shape.ts@1118751f:901-915`).
            if !points
                .iter()
                .all(|p| p[0].abs() <= 1e6 && p[1].abs() <= 1e6)
            {
                return None;
            }
            let o = generate_rough_options(element, true);
            Some(generator::path(&elbow_segments(&points), o))
        }

        DrawElementType::Line | DrawElementType::Arrow => {
            let points = linear_points(element);
            let o: Options = generate_rough_options(element, false);
            Some(if element.roundness.is_none() {
                if o.filled {
                    generator::polygon(&points, o)
                } else {
                    generator::linear_path(&points, o)
                }
            } else {
                generator::curve(&points, o)
            })
        }

        // Not rough-drawn: text is filled glyphs, freedraw is a stroke outline.
        DrawElementType::Text | DrawElementType::Freedraw => None,

        DrawElementType::Figure => {
            let params = element.figure.clone().unwrap_or_default();
            let outline = figure::outline(params.kind, params.sides, params.ratio);
            let scale = |&(u, v): &(f64, f64)| [u * w, v * h];
            let o = generate_rough_options(element, false);
            let mut drawable =
                generator::polygon(&outline.closed.iter().map(scale).collect::<Vec<_>>(), o);
            // Extra open strokes (the cylinder's front rim) are never filled, so they are
            // generated separately and folded into the same drawable rather than through
            // `polygon`, which would try to close and fill them too.
            for stroke in &outline.extra {
                let extra =
                    generator::linear_path(&stroke.iter().map(scale).collect::<Vec<_>>(), o);
                drawable.sets.extend(extra.sets);
            }
            Some(drawable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, Geometry};

    fn element(kind: DrawElementType, w: f64, h: f64) -> DrawElement {
        let mut e = create_element(
            kind,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
            },
            DrawElementStyle::default(),
            0.0,
        );
        // Pinned so shape output is reproducible across runs; `create_element` seeds
        // from a counter that the test order would otherwise leak into.
        e.seed = 12345;
        e
    }

    /// The whole shape cache rests on this: an element's geometry must not depend on
    /// where it is, only on how big it is.
    #[test]
    fn geometry_is_independent_of_position() {
        let mut a = element(DrawElementType::Rectangle, 100.0, 60.0);
        let mut b = a.clone();
        b.x = 4321.0;
        b.y = -987.0;
        assert_eq!(element_drawable(&a), element_drawable(&b));

        // ...and it *does* depend on size, or a resize would reuse stale ops.
        a.width = 101.0;
        assert_ne!(element_drawable(&a), element_drawable(&b));
    }

    #[test]
    fn the_same_seed_always_produces_the_same_shape() {
        let e = element(DrawElementType::Rectangle, 100.0, 60.0);
        assert_eq!(element_drawable(&e), element_drawable(&e));
    }

    #[test]
    fn a_different_seed_produces_a_different_shape() {
        let a = element(DrawElementType::Rectangle, 100.0, 60.0);
        let mut b = a.clone();
        b.seed = a.seed.wrapping_add(1);
        assert_ne!(element_drawable(&a), element_drawable(&b));
    }

    #[test]
    fn rounded_and_sharp_rectangles_take_different_paths() {
        let mut sharp = element(DrawElementType::Rectangle, 100.0, 60.0);
        sharp.roundness = None;
        let rounded = element(DrawElementType::Rectangle, 100.0, 60.0);
        assert!(
            rounded.roundness.is_some(),
            "the default is rounded, as in Excalidraw"
        );

        assert_eq!(element_drawable(&sharp).unwrap().shape, "rectangle");
        assert_eq!(element_drawable(&rounded).unwrap().shape, "path");
    }

    /// Excalidraw's `d` string has no `Z`; closing it would double the top edge.
    #[test]
    fn the_rounded_rect_path_does_not_close() {
        let segs = rounded_rect_segments(100.0, 60.0, 8.0);
        assert!(!segs.contains(&Segment::Close));
        assert_eq!(segs.len(), 9);
    }

    #[test]
    fn adaptive_corner_radius_switches_at_the_cutoff() {
        let e = element(DrawElementType::Rectangle, 100.0, 60.0);
        // Below the 128px cutoff: proportional.
        assert_eq!(corner_radius(60.0, &e), 15.0);
        // Above it: pinned to the fixed radius.
        assert_eq!(corner_radius(400.0, &e), ADAPTIVE_RADIUS);
    }

    /// `M 0 0 L 84 0 Q 100 0, 100 16 L 100 50`: a corner rounded by 16, and by no more
    /// than half the run beside it.
    #[test]
    fn an_elbow_arrow_rounds_its_corners() {
        let points = [[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [90.0, 50.0]];
        let segments = elbow_segments(&points);
        assert_eq!(segments[0], Segment::MoveTo([0.0, 0.0]));
        assert_eq!(segments[1], Segment::LineTo([84.0, 0.0]));
        assert_eq!(
            segments[2],
            Segment::quad_to_cubic([84.0, 0.0], 100.0, 0.0, 100.0, 16.0)
        );
        // Beside a 10-long run the corner is 5.
        assert_eq!(segments[3], Segment::LineTo([100.0, 45.0]));
        assert_eq!(segments.last(), Some(&Segment::LineTo([90.0, 50.0])));

        let mut arrow = element(DrawElementType::Arrow, 100.0, 50.0);
        arrow.points = Some(points.to_vec());
        let straight = element_drawable(&arrow).unwrap();
        arrow.elbowed = Some(true);
        assert_eq!(element_drawable(&arrow).unwrap().shape, "path");
        assert_ne!(element_drawable(&arrow), Some(straight));
    }

    #[test]
    fn text_and_freedraw_are_not_rough_drawn() {
        assert!(element_drawable(&element(DrawElementType::Text, 50.0, 20.0)).is_none());
        assert!(element_drawable(&element(DrawElementType::Freedraw, 50.0, 20.0)).is_none());
    }

    #[test]
    fn a_filled_shape_emits_the_fill_beneath_the_outline() {
        let mut e = element(DrawElementType::Rectangle, 100.0, 60.0);
        e.roundness = None;
        e.background_color = "#ffc9c9".into();
        let d = element_drawable(&e).unwrap();
        assert_eq!(d.sets.len(), 2);
        assert_eq!(d.sets[1].kind, draw_rough::OpSetKind::Path);
    }
}
