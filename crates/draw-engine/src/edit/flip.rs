use std::collections::HashSet;

use crate::render::default_arrowhead;
use crate::scene::binding::{anchor, is_binding_element, is_linear_element, set_anchor, End};
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::{element_rotated_bounds, normalize_rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipAxis {
    Horizontal,
    Vertical,
}

impl FlipAxis {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

/// Mirrors one element about the line `(lo + hi) / 2` on the chosen axis, and turns it
/// the other way — Excalidraw's `resizeMultipleElements` with `flipByX | flipByY` at a
/// scale of 1 (`packages/element/src/resizeElements.ts:1409-1497`).
///
/// - **Boxes** — rectangles, ellipses, diamonds, frames, embeds, text — land on their
///   mirror image with the same width and height. What is drawn inside them is not
///   mirrored, as in the oracle: a flipped rectangle keeps its hand-drawn stroke and its
///   hatching, a text still reads forwards, and an embedded page is never shown backwards
///   (Excalidraw only turns its iframe, `App.tsx:2046-2051`).
/// - **Pictures** negate their extent on that axis. The sign is the mirror, which the
///   painter and the SVG export apply as a scale of -1: the job the oracle's `scale`
///   field does (`resizeElements.ts:1484-1489`, painted by `renderElement.ts:824-841`).
/// - **Lines, arrows and freehand strokes** mirror their points, where their shape lives.
///
/// Reflection is an involution, and so is each of these. Applied twice, the box's
/// `x -> lo + hi - (x + w)`, the picture's `x -> lo + hi - x` with `w -> -w`, and the
/// points' `p -> span - p` all return the original numbers.
fn flip_one(element: &DrawElement, axis: FlipAxis, lo: f64, hi: f64) -> DrawElement {
    let horizontal = axis == FlipAxis::Horizontal;
    let mut next = element.clone();
    // A mirror reverses the direction of rotation with it — text included
    // (`resizeElements.ts:1417-1419`).
    next.angle = if element.angle != 0.0 {
        -element.angle
    } else {
        0.0
    };

    match element.kind {
        DrawElementType::Line | DrawElementType::Arrow => {
            next.points = Some(mirror_points(element, horizontal, 0.0, 0.0));
            mirror_extent(&mut next, element, horizontal, lo, hi);
        }
        DrawElementType::Freedraw => {
            // The points carry the shape, so they mirror within the element's own box and
            // the box itself keeps a positive extent.
            let rect = normalize_rect(element.x, element.y, element.width, element.height);
            next.points = Some(mirror_points(element, horizontal, rect.width, rect.height));
            next.x = rect.x;
            next.y = rect.y;
            next.width = rect.width;
            next.height = rect.height;
            if horizontal {
                next.x = lo + hi - (rect.x + rect.width);
            } else {
                next.y = lo + hi - (rect.y + rect.height);
            }
        }
        DrawElementType::Image => mirror_extent(&mut next, element, horizontal, lo, hi),
        _ => {
            // `x + width` is the edge across from `x` whichever sign the extent has — a
            // negative one is a box resized past its own corner, anchored on its far
            // edge — so this reflects the box and keeps the sign it had.
            if horizontal {
                next.x = lo + hi - (element.x + element.width);
            } else {
                next.y = lo + hi - (element.y + element.height);
            }
        }
    }
    next
}

/// Reflects the anchor across the midline and negates the extent on that axis.
fn mirror_extent(
    next: &mut DrawElement,
    element: &DrawElement,
    horizontal: bool,
    lo: f64,
    hi: f64,
) {
    if horizontal {
        next.x = lo + hi - element.x;
        next.width = -element.width;
    } else {
        next.y = lo + hi - element.y;
        next.height = -element.height;
    }
}

/// Mirrors a point list about `span / 2`, or about the origin when `span` is zero.
fn mirror_points(
    element: &DrawElement,
    horizontal: bool,
    span_x: f64,
    span_y: f64,
) -> Vec<[f64; 2]> {
    element
        .points
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|&[px, py]| {
            if horizontal {
                [span_x - px, py]
            } else {
                [px, span_y - py]
            }
        })
        .collect()
}

/// An arrow's anchors, carried through the mirror: an end bound to a shape flipped with
/// it is mirrored in that shape's own frame, and an end bound to a shape that stayed lets
/// go (`resizeElements.ts:1558-1569`).
///
/// Excalidraw mirrors the anchors of elbow arrows only (`resizeElements.ts:1446-1474`).
/// A straight or curved one keeps its old anchors, so its mirrored points are right only
/// until something re-resolves them — on excalidraw.com it jumps back to the old sides
/// the moment a bound shape is nudged. Here every bound arrow is re-resolved straight
/// after the flip, so the anchors have to be right at once.
fn reanchor(arrow: &mut DrawElement, horizontal: bool, flipped: &HashSet<&str>) {
    for end in [End::Start, End::End] {
        let Some(mut bound) = anchor(arrow, end) else {
            continue;
        };
        if flipped.contains(bound.element_id.as_str()) {
            let [fx, fy] = bound.fixed_point;
            bound.fixed_point = if horizontal {
                [1.0 - fx, fy]
            } else {
                [fx, 1.0 - fy]
            };
            set_anchor(arrow, end, Some(bound));
        } else {
            set_anchor(arrow, end, None);
        }
    }
}

/// An arrow turned round without moving: its resolved heads trade ends, so one drawn with
/// the implicit end head visibly points the other way.
fn swap_heads(arrow: &DrawElement) -> DrawElement {
    let mut next = arrow.clone();
    next.start_arrowhead = Some(default_arrowhead(arrow, "end"));
    next.end_arrowhead = Some(default_arrowhead(arrow, "start"));
    next
}

/// The patches that flip `ids` about the middle of their box.
///
/// `ids` is the set a drag would move — a frame's children and a group's locked members
/// included, as Excalidraw flips them (`actionFlip.ts:87-94`, `groups.ts:94-132`). Labels
/// are left out and follow their container when the bindings are refreshed.
///
/// Excalidraw moves the selection back onto its old middle afterwards
/// (`actionFlip.ts:158-192`), because it measures a curved arrow by its rendered curve,
/// which can bump the box by a pixel. Here the box is measured from points, which mirror
/// exactly, so there is no drift to take back.
pub fn flip_elements(
    elements: &[DrawElement],
    ids: &HashSet<String>,
    axis: FlipAxis,
) -> Vec<DrawElement> {
    let targets: Vec<&DrawElement> = elements
        .iter()
        .filter(|el| ids.contains(&el.id) && !el.is_deleted && el.container_id.is_none())
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }
    // Only bound arrows: they turn round and nothing moves (`actionFlip.ts:116-129`).
    // Labels are not counted — an arrow with words on it is still only an arrow — where
    // Excalidraw counts them, and mirrors a labelled arrow off its shapes instead.
    if targets.iter().all(|el| {
        is_binding_element(el) && (el.start_binding.is_some() || el.end_binding.is_some())
    }) {
        return targets.into_iter().map(swap_heads).collect();
    }

    let horizontal = axis == FlipAxis::Horizontal;
    // The middle of the turned boxes, as the selection is seen on the board
    // (`getCommonBoundingBox`, `actionFlip.ts:131`), with the words on an arrow, which can
    // reach past its ends (`resizeElements.ts:1280-1310`).
    let words: HashSet<&str> = targets
        .iter()
        .filter(|el| is_linear_element(el))
        .filter_map(|el| el.bound_text_id.as_deref())
        .collect();
    let (lo, hi) = targets
        .iter()
        .copied()
        .chain(
            elements
                .iter()
                .filter(|el| !el.is_deleted && words.contains(el.id.as_str())),
        )
        .map(element_rotated_bounds)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), b| {
            if horizontal {
                (lo.min(b.min_x), hi.max(b.max_x))
            } else {
                (lo.min(b.min_y), hi.max(b.max_y))
            }
        });

    let flipped: HashSet<&str> = targets.iter().map(|el| el.id.as_str()).collect();
    targets
        .iter()
        .map(|element| {
            let mut next = flip_one(element, axis, lo, hi);
            if is_binding_element(element) {
                reanchor(&mut next, horizontal, &flipped);
            }
            next
        })
        .collect()
}
