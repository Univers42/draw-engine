//! How much room an element actually needs on screen, as opposed to how big it is.
//!
//! An element's logical bounds are not what gets painted. A rough stroke wanders
//! outside them by up to `maxRandomnessOffset * roughness`, half the stroke width sits
//! outside the path, an arrowhead extends past the last point, and a rotated element
//! sweeps a larger box than it occupies. Culling on logical bounds therefore pops
//! shapes out of existence slightly before they leave the screen — a bug that only
//! shows at the viewport edge and is easy to ship.

use crate::camera::WorldBounds;
use crate::render::arrowheads::arrowhead_size;
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::Rect;

/// rough's `maxRandomnessOffset` default, the amplitude its jitter is scaled from.
const MAX_RANDOMNESS_OFFSET: f64 = 2.0;

/// A little extra, so a rounding difference between the cull test and the paint
/// transform can never clip a stroke.
const SAFETY: f64 = 2.0;

/// The world-space box an element can paint into.
///
/// Rotation-aware: the element's corners are rotated about its centre and the axis-
/// aligned box of the result is returned, which is what the painter's transform
/// actually sweeps.
pub fn render_bounds(element: &DrawElement) -> WorldBounds {
    // Through `element_bounds`, so a line or arrow is measured by its points. Reading a
    // leftward arrow's box as `[x, x + width]` put it a full width to the right of where
    // it is drawn, which culled it from view while it was still on screen.
    let plain = crate::scene::geometry::element_bounds(element);
    let rect = Rect {
        x: plain.min_x,
        y: plain.min_y,
        width: plain.max_x - plain.min_x,
        height: plain.max_y - plain.min_y,
    };

    let mut pad =
        element.stroke_width / 2.0 + MAX_RANDOMNESS_OFFSET * element.roughness.max(0.0) + SAFETY;

    // An arrowhead reaches beyond the final point of the path.
    if element.kind == DrawElementType::Arrow {
        pad += arrowhead_size(element.stroke_width);
    }

    let (min_x, min_y, max_x, max_y) = if element.angle == 0.0 {
        (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height)
    } else {
        let centre = crate::scene::geometry::rotation_center(element);
        let (cx, cy) = (centre.x, centre.y);
        let (sin, cos) = element.angle.sin_cos();

        let corners = [
            (rect.x, rect.y),
            (rect.x + rect.width, rect.y),
            (rect.x + rect.width, rect.y + rect.height),
            (rect.x, rect.y + rect.height),
        ];

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for (x, y) in corners {
            let dx = x - cx;
            let dy = y - cy;
            let rx = cx + dx * cos - dy * sin;
            let ry = cy + dx * sin + dy * cos;
            min_x = min_x.min(rx);
            min_y = min_y.min(ry);
            max_x = max_x.max(rx);
            max_y = max_y.max(ry);
        }
        (min_x, min_y, max_x, max_y)
    };

    WorldBounds {
        min_x: min_x - pad,
        min_y: min_y - pad,
        max_x: max_x + pad,
        max_y: max_y + pad,
    }
}

/// Whether the element can paint anything inside the visible rect.
///
/// Deliberately conservative: a false positive costs one wasted replay, a false
/// negative is a missing shape. When in doubt, draw it.
pub fn intersects_viewport(element: &DrawElement, visible: &WorldBounds) -> bool {
    let b = render_bounds(element);
    // A NaN coordinate makes every comparison false, which would silently hide the
    // element. Treat a non-finite box as visible so the bug is seen, not swallowed.
    if !b.min_x.is_finite() || !b.min_y.is_finite() || !b.max_x.is_finite() || !b.max_y.is_finite()
    {
        return true;
    }
    b.min_x <= visible.max_x
        && b.max_x >= visible.min_x
        && b.min_y <= visible.max_y
        && b.max_y >= visible.min_y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, Geometry};

    fn element(kind: DrawElementType, x: f64, y: f64, w: f64, h: f64) -> DrawElement {
        let mut e = create_element(
            kind,
            Geometry {
                x,
                y,
                width: w,
                height: h,
            },
            DrawElementStyle::default(),
            0.0,
        );
        e.seed = 1;
        e
    }

    fn viewport() -> WorldBounds {
        WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1000.0,
            max_y: 800.0,
        }
    }

    #[test]
    fn an_element_in_view_is_kept() {
        let e = element(DrawElementType::Rectangle, 100.0, 100.0, 200.0, 100.0);
        assert!(intersects_viewport(&e, &viewport()));
    }

    #[test]
    fn an_element_far_off_screen_is_culled() {
        let e = element(DrawElementType::Rectangle, 50_000.0, 50_000.0, 100.0, 100.0);
        assert!(!intersects_viewport(&e, &viewport()));
    }

    /// The case culling gets wrong: an element straddling the edge must survive, and
    /// its jittered stroke must not be clipped either.
    #[test]
    fn an_element_straddling_the_edge_is_kept() {
        let e = element(DrawElementType::Rectangle, -50.0, 100.0, 100.0, 100.0);
        assert!(intersects_viewport(&e, &viewport()));

        // Just outside, but close enough that a rough stroke could still reach in.
        let mut near = element(DrawElementType::Rectangle, -100.0, 100.0, 99.0, 100.0);
        near.roughness = 2.0;
        assert!(
            intersects_viewport(&near, &viewport()),
            "stroke jitter reaches in"
        );
    }

    #[test]
    fn bounds_are_padded_for_stroke_and_jitter() {
        let mut e = element(DrawElementType::Rectangle, 0.0, 0.0, 100.0, 100.0);
        e.stroke_width = 4.0;
        e.roughness = 2.0;
        let b = render_bounds(&e);
        // 4/2 stroke + 2*2 jitter + 2 safety = 8
        assert_eq!(b.min_x, -8.0);
        assert_eq!(b.max_x, 108.0);
    }

    /// A rotated square sweeps a box wider than itself — culling on the unrotated one
    /// clips it near the edge.
    #[test]
    fn rotation_widens_the_box() {
        let mut e = element(DrawElementType::Rectangle, 0.0, 0.0, 100.0, 100.0);
        e.roughness = 0.0;
        e.stroke_width = 0.0;
        let square = render_bounds(&e);

        e.angle = std::f64::consts::FRAC_PI_4;
        let turned = render_bounds(&e);

        assert!(turned.max_x - turned.min_x > square.max_x - square.min_x);
        // A square turned 45 degrees spans its diagonal.
        let expected = 100.0 * std::f64::consts::SQRT_2 + 2.0 * SAFETY;
        assert!((turned.max_x - turned.min_x - expected).abs() < 1e-9);
    }

    #[test]
    fn an_arrow_reserves_room_for_its_head() {
        let mut arrow = element(DrawElementType::Arrow, 0.0, 0.0, 100.0, 0.0);
        arrow.stroke_width = 2.0;
        let mut line = arrow.clone();
        line.kind = DrawElementType::Line;

        let a = render_bounds(&arrow);
        let l = render_bounds(&line);
        assert!(a.max_x > l.max_x, "the head extends past the last point");
    }

    /// A NaN must not silently hide an element — a missing shape is far harder to
    /// diagnose than one that is drawn when it need not be.
    #[test]
    fn a_non_finite_element_is_drawn_rather_than_hidden() {
        let mut e = element(DrawElementType::Rectangle, f64::NAN, 0.0, 100.0, 100.0);
        e.roughness = 0.0;
        assert!(intersects_viewport(&e, &viewport()));
    }
}
