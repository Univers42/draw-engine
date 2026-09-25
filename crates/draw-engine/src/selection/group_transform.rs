//! Resizing and rotating a multi-element selection.
//!
//! A single element is transformed by rewriting its own geometry. A group cannot be:
//! the selection has a frame, and every member has to move *within* that frame as it
//! changes, keeping its relative position and proportion. Otherwise dragging a corner
//! either moves the elements without scaling them, or scales each about its own centre
//! so the group flies apart.
//!
//! # What does and does not scale
//!
//! Position and size scale with the frame. **Stroke width does not** — scaling a
//! selection up should not thicken every line in it, and Excalidraw does not do that
//! either. Font size *does*, because text that stays the same size while its box grows
//! stops fitting.
//!
//! # When it scales as one
//!
//! As `resizeMultipleElements` (`packages/element/src/resizeElements.ts@1118751f:
//! 1370-1382`): the selection keeps its proportions — both axes take the larger scale —
//! with Shift, or when any element in it is turned, is a text, or is in a group. A text's
//! font scales with it (`:1491-1497`). A label is not scaled with the rest: its font
//! scales with the selection only when the selection keeps its proportions, and the
//! engine lays it out again in its resized shape (`:1499-1514`, `:1571-1589`).

use crate::camera::{Point, WorldBounds};
use crate::scene::element::DrawElement;
use crate::scene::geometry::{is_point_based, rotation_center};
use crate::selection::linear::{from_world_points, world_points};
use crate::selection::HandleKind;

/// The frame a group is transformed within, captured when the gesture starts.
///
/// Held for the duration of the drag rather than recomputed per move: recomputing it
/// from the elements as they change compounds rounding on every pointer event, and a
/// group slowly drifts.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupFrame {
    pub bounds: WorldBounds,
    /// Each member's geometry at the moment the drag began.
    pub origins: Vec<(String, GroupOrigin)>,
}

/// One member's geometry at the moment the drag began.
///
/// Deliberately not `Copy`, and the `points` field is why. It used to be, and the
/// consequence was subtle: a `Vec` cannot be `Copy`, so the ring had nowhere to live
/// here, so `resize_group` scaled the box from this captured state but the *points* from
/// the live element — which every earlier move of the same gesture had already scaled.
/// The box grew by `sx` and the ring by `sx` per move, and after a real drag of twenty
/// frames the drawing was far outside its own bounds.
///
/// A single-move drag cannot see that, which is how it survived having tests.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupOrigin {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub font_size: Option<f64>,
    /// The ring as it was when the drag began, for a point-based element.
    ///
    /// `None` for a shape, which is generated into its box and has no ring to keep up.
    pub points: Option<Vec<[f64; 2]>>,
}

impl GroupOrigin {
    /// `element` as it was when the drag began.
    fn restore(&self, element: &DrawElement) -> DrawElement {
        let mut at = element.clone();
        at.x = self.x;
        at.y = self.y;
        at.width = self.width;
        at.height = self.height;
        at.angle = self.angle;
        at.points.clone_from(&self.points);
        at
    }
}

/// A line, an arrow or a stroke with every point put through `map`, in the world.
///
/// Such an element *is* its points: its `x` and `y` are its first point and the sign of
/// its extent means nothing (`scene::geometry::mirror_signs`). Treating it as a box
/// moved a leftward line a whole width on a flip, and pivoted it about a point outside
/// it — so it is transformed through its points instead, which is exact.
fn map_points(from: &DrawElement, map: impl Fn(Point) -> Point) -> DrawElement {
    let world: Vec<Point> = world_points(from).into_iter().map(map).collect();
    from_world_points(from, &world)
}

impl GroupFrame {
    /// Captures the frame and each member's starting geometry.
    pub fn capture<'a>(elements: impl IntoIterator<Item = &'a DrawElement>) -> Option<Self> {
        let elements: Vec<&DrawElement> = elements.into_iter().collect();
        let bounds = crate::scene::geometry::scene_bounds(elements.iter().copied())?;
        let origins = elements
            .iter()
            .map(|e| {
                (
                    e.id.clone(),
                    GroupOrigin {
                        x: e.x,
                        y: e.y,
                        width: e.width,
                        height: e.height,
                        angle: e.angle,
                        font_size: e.font_size,
                        points: e.points.clone(),
                    },
                )
            })
            .collect();
        Some(Self { bounds, origins })
    }
}

/// The corner that stays put while `handle` is dragged.
fn anchor_for(handle: HandleKind, b: &WorldBounds) -> (f64, f64) {
    match handle {
        HandleKind::Nw => (b.max_x, b.max_y),
        HandleKind::Ne => (b.min_x, b.max_y),
        HandleKind::Se => (b.min_x, b.min_y),
        HandleKind::Sw => (b.max_x, b.min_y),
        HandleKind::N => (b.min_x, b.max_y),
        HandleKind::S => (b.min_x, b.min_y),
        HandleKind::W => (b.max_x, b.min_y),
        HandleKind::E => (b.min_x, b.min_y),
        HandleKind::Rotate => ((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0),
    }
}

/// The scale factors a drag to `pointer` implies, relative to the captured frame.
///
/// A handle that only moves one axis leaves the other at 1. Factors are clamped away
/// from zero so a selection dragged onto its own anchor collapses to a sliver rather
/// than to nothing — a zero-size group can never be grabbed again.
fn scale_for(handle: HandleKind, frame: &GroupFrame, pointer: Point, uniform: bool) -> (f64, f64) {
    const MIN_SCALE: f64 = 0.01;

    let b = &frame.bounds;
    let (ax, ay) = anchor_for(handle, b);
    let w = b.max_x - b.min_x;
    let h = b.max_y - b.min_y;

    let horizontal = matches!(
        handle,
        HandleKind::Nw
            | HandleKind::Ne
            | HandleKind::Se
            | HandleKind::Sw
            | HandleKind::E
            | HandleKind::W
    );
    let vertical = matches!(
        handle,
        HandleKind::Nw
            | HandleKind::Ne
            | HandleKind::Se
            | HandleKind::Sw
            | HandleKind::N
            | HandleKind::S
    );

    let mut sx = if horizontal && w.abs() > f64::EPSILON {
        (pointer.x - ax) / (if ax == b.min_x { w } else { -w })
    } else {
        1.0
    };
    let mut sy = if vertical && h.abs() > f64::EPSILON {
        (pointer.y - ay) / (if ay == b.min_y { h } else { -h })
    } else {
        1.0
    };

    if uniform && horizontal && vertical {
        // Shift keeps the group's proportions; the larger magnitude wins so the drag
        // still follows the pointer rather than lagging it.
        let s = if sx.abs() > sy.abs() {
            sx.abs()
        } else {
            sy.abs()
        };
        sx = s * sx.signum();
        sy = s * sy.signum();
    }

    (
        if sx.abs() < MIN_SCALE {
            MIN_SCALE * sx.signum().max(0.0).max(1.0)
        } else {
            sx
        },
        if sy.abs() < MIN_SCALE {
            MIN_SCALE * sy.signum().max(0.0).max(1.0)
        } else {
            sy
        },
    )
}

/// Whether a drag of `handle` to `pointer` turns the selection through its anchor, on
/// each axis (`flipByX`, `flipByY`: `resizeElements.ts@1118751f:1177-1198`).
pub fn resize_group_flips(handle: HandleKind, frame: &GroupFrame, pointer: Point) -> (bool, bool) {
    let (sx, sy) = scale_for(handle, frame, pointer, false);
    (sx < 0.0, sy < 0.0)
}

/// Whether a resize of `elements` keeps its proportions without Shift: any of them
/// turned, a text, or in a group (`keepAspectRatio`, `resizeElements.ts@1118751f:
/// 1370-1377`). Labels are not asked: the oracle resizes the selection without them.
pub fn resize_keeps_aspect(elements: &[DrawElement]) -> bool {
    elements
        .iter()
        .filter(|e| e.container_id.is_none())
        .any(|e| {
            e.angle != 0.0
                || e.kind == crate::scene::DrawElementType::Text
                || !e.group_ids.is_empty()
        })
}

/// Scales every member of the group within the frame.
///
/// A label whose shape is in `elements` comes back with only its font changed — scaled
/// when the selection keeps its proportions, as it started otherwise — for the caller to
/// lay out in the resized shape. Nothing comes back when a font would drop below
/// [`crate::selection::MIN_FONT_SIZE`]: the oracle leaves the selection as it was
/// (`resizeElements.ts@1118751f:1491-1514`).
pub fn resize_group(
    elements: &[DrawElement],
    frame: &GroupFrame,
    handle: HandleKind,
    pointer: Point,
    uniform: bool,
) -> Vec<DrawElement> {
    let keep_aspect = uniform || resize_keeps_aspect(elements);
    let (sx, sy) = scale_for(handle, frame, pointer, keep_aspect);
    let (ax, ay) = anchor_for(handle, &frame.bounds);
    let containers: std::collections::HashSet<&str> = elements
        .iter()
        .filter(|e| e.container_id.is_none())
        .map(|e| e.id.as_str())
        .collect();
    let font_below_minimum = std::cell::Cell::new(false);

    let out = elements
        .iter()
        .filter_map(|element| {
            let origin = frame
                .origins
                .iter()
                .find(|(id, _)| id == &element.id)
                .map(|(_, o)| o)?;

            if element
                .container_id
                .as_deref()
                .is_some_and(|id| containers.contains(id))
            {
                let mut label = element.clone();
                label.font_size =
                    origin
                        .font_size
                        .map(|size| if keep_aspect { size * sx.abs() } else { size });
                if label
                    .font_size
                    .is_some_and(|size| size < super::MIN_FONT_SIZE)
                {
                    font_below_minimum.set(true);
                }
                return Some(label);
            }

            // Scaled from `origin` — the geometry as it was when the drag began — and
            // never from the live element, which every earlier move of this gesture has
            // already scaled. Reading the live ring compounded: the box ended up right
            // and the drawing `sx²` too big, bursting out of its own bounds.
            let from = origin.restore(element);
            let mut next = if from.points.is_some() && is_point_based(&from) {
                map_points(&from, |p| Point {
                    x: ax + (p.x - ax) * sx,
                    y: ay + (p.y - ay) * sy,
                })
            } else {
                let mut next = from.clone();
                next.x = ax + (origin.x - ax) * sx;
                next.y = ay + (origin.y - ay) * sy;
                next.width = origin.width * sx;
                next.height = origin.height * sy;
                // A negative scale flips the shape through the anchor. Normalising keeps
                // width and height positive, which everything downstream assumes.
                if next.width < 0.0 {
                    next.x += next.width;
                    next.width = -next.width;
                }
                if next.height < 0.0 {
                    next.y += next.height;
                    next.height = -next.height;
                }
                next
            };

            // Text has to grow with its box or it stops fitting — by its width, which with
            // a text in the selection is the scale of both axes (`measureFontSizeFromWidth`,
            // `resizeElements.ts@1118751f:292-315`). Stroke width deliberately does not:
            // scaling a selection up should not thicken every line in it.
            if let Some(size) = origin.font_size {
                let size = size * sx.abs();
                if size < super::MIN_FONT_SIZE {
                    font_below_minimum.set(true);
                }
                next.font_size = Some(size);
            }

            Some(next)
        })
        .collect();
    if font_below_minimum.get() {
        return Vec::new();
    }
    out
}

/// Rotates every member about the group's centre.
///
/// Each element's own angle advances by the same delta *and* its centre orbits the
/// group centre — rotating only the angles would spin each element in place and leave
/// the arrangement untouched. A line, an arrow or a stroke turns through its points
/// instead (see [`map_points`]), so it keeps no angle of its own: an arrow's ends are
/// then exactly where its bindings will look for them.
pub fn rotate_group(
    elements: &[DrawElement],
    frame: &GroupFrame,
    pointer: Point,
) -> Vec<DrawElement> {
    let b = &frame.bounds;
    let cx = (b.min_x + b.max_x) / 2.0;
    let cy = (b.min_y + b.max_y) / 2.0;

    // The same convention as single-element rotation: straight up is zero.
    let target = (pointer.y - cy).atan2(pointer.x - cx) + std::f64::consts::FRAC_PI_2;

    elements
        .iter()
        .filter_map(|element| {
            let origin = frame
                .origins
                .iter()
                .find(|(id, _)| id == &element.id)
                .map(|(_, o)| o)?;

            let (sin, cos) = target.sin_cos();
            let turn = |p: Point| Point {
                x: cx + (p.x - cx) * cos - (p.y - cy) * sin,
                y: cy + (p.x - cx) * sin + (p.y - cy) * cos,
            };

            let from = origin.restore(element);
            if from.points.is_some() && is_point_based(&from) {
                return Some(map_points(&from, turn));
            }
            // The pivot the element itself turns about, carried round the group's.
            let pivot = rotation_center(&from);
            let moved = turn(pivot);
            let mut next = from;
            next.angle = origin.angle + target;
            next.x += moved.x - pivot.x;
            next.y += moved.y - pivot.y;
            Some(next)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, DrawElementType, Geometry};

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> DrawElement {
        let mut e = create_element(
            DrawElementType::Rectangle,
            Geometry {
                x,
                y,
                width: w,
                height: h,
            },
            DrawElementStyle::default(),
            0.0,
        );
        e.id = id.to_string();
        e
    }

    fn trio() -> Vec<DrawElement> {
        vec![
            rect("a", 0.0, 0.0, 100.0, 100.0),
            rect("b", 200.0, 0.0, 100.0, 100.0),
            rect("c", 0.0, 200.0, 100.0, 100.0),
        ]
    }

    #[test]
    fn the_frame_spans_every_member() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();
        assert_eq!(frame.bounds.min_x, 0.0);
        assert_eq!(frame.bounds.min_y, 0.0);
        assert_eq!(frame.bounds.max_x, 300.0);
        assert_eq!(frame.bounds.max_y, 300.0);
        assert_eq!(frame.origins.len(), 3);
    }

    /// Dragging the SE corner to double the frame must double every member's size and
    /// its distance from the anchor — the arrangement scales, it does not just move.
    #[test]
    fn resizing_scales_members_and_their_spacing() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();

        let out = resize_group(
            &els,
            &frame,
            HandleKind::Se,
            Point { x: 600.0, y: 600.0 },
            false,
        );

        let a = out.iter().find(|e| e.id == "a").unwrap();
        assert_eq!((a.x, a.y), (0.0, 0.0), "the anchored corner stays put");
        assert_eq!((a.width, a.height), (200.0, 200.0), "and doubles in size");

        let b = out.iter().find(|e| e.id == "b").unwrap();
        assert_eq!(b.x, 400.0, "spacing doubles too");
    }

    /// Scaling a group up must not thicken its strokes.
    #[test]
    fn stroke_width_is_left_alone() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();
        let before = els[0].stroke_width;

        let out = resize_group(
            &els,
            &frame,
            HandleKind::Se,
            Point { x: 900.0, y: 900.0 },
            false,
        );
        assert_eq!(out[0].stroke_width, before);
    }

    /// ...but text has to grow with its box, or it stops fitting.
    #[test]
    fn font_size_scales_with_the_group() {
        let mut els = trio();
        els[0].kind = DrawElementType::Text;
        els[0].font_size = Some(20.0);

        let frame = GroupFrame::capture(els.iter()).unwrap();
        let out = resize_group(
            &els,
            &frame,
            HandleKind::Se,
            Point { x: 600.0, y: 600.0 },
            false,
        );
        assert_eq!(out[0].font_size, Some(40.0));
    }

    #[test]
    fn a_single_axis_handle_leaves_the_other_axis_alone() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();

        let out = resize_group(
            &els,
            &frame,
            HandleKind::E,
            Point { x: 600.0, y: 300.0 },
            false,
        );
        let a = out.iter().find(|e| e.id == "a").unwrap();
        assert_eq!(a.width, 200.0, "width doubled");
        assert_eq!(a.height, 100.0, "height untouched");
    }

    /// Dragging a corner past the opposite one flips the group rather than producing
    /// negative extents, which everything downstream assumes cannot happen.
    #[test]
    fn dragging_past_the_anchor_flips_instead_of_going_negative() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();

        let out = resize_group(
            &els,
            &frame,
            HandleKind::Se,
            Point {
                x: -300.0,
                y: -300.0,
            },
            false,
        );
        for e in &out {
            assert!(e.width >= 0.0, "{} went negative-width", e.id);
            assert!(e.height >= 0.0, "{} went negative-height", e.id);
        }
    }

    /// A group dragged onto its own anchor must stay grabbable.
    #[test]
    fn collapsing_a_group_leaves_something_to_grab() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();

        let out = resize_group(
            &els,
            &frame,
            HandleKind::Se,
            Point { x: 0.0, y: 0.0 },
            false,
        );
        let total: f64 = out.iter().map(|e| e.width + e.height).sum();
        assert!(
            total > 0.0,
            "the group vanished and can never be selected again"
        );
    }

    #[test]
    fn shift_keeps_the_proportions() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();

        let out = resize_group(
            &els,
            &frame,
            HandleKind::Se,
            Point { x: 900.0, y: 450.0 },
            true,
        );
        let a = out.iter().find(|e| e.id == "a").unwrap();
        assert_eq!(a.width, a.height, "a square member stayed square");
    }

    /// Rotating a group must move the members around the centre, not spin each in
    /// place — otherwise the arrangement never changes.
    #[test]
    fn rotating_orbits_members_about_the_group_centre() {
        let els = trio();
        let frame = GroupFrame::capture(els.iter()).unwrap();

        // Straight down from the centre is a half turn from the zero convention.
        let out = rotate_group(
            &els,
            &frame,
            Point {
                x: 150.0,
                y: 1000.0,
            },
        );

        let a = out.iter().find(|e| e.id == "a").unwrap();
        let b = out.iter().find(|e| e.id == "b").unwrap();

        assert!(a.angle != 0.0, "each member turned");
        // "a" started top-left; after a half turn it must be bottom-right of centre.
        assert!(
            a.x > 100.0 && a.y > 100.0,
            "member a orbited: {:?}",
            (a.x, a.y)
        );
        assert!(b.x < 150.0, "member b swapped sides: {}", b.x);
    }

    #[test]
    fn an_empty_selection_has_no_frame() {
        assert!(GroupFrame::capture(std::iter::empty()).is_none());
    }
}
