//! Frames: a named region that owns what is inside it.
//!
//! A frame is not a group. A group is a set you chose; a frame is an *area*, and
//! membership follows from where things are. Drop a shape inside a frame and it belongs
//! to the frame; drag the frame and the shape goes with it; drag the shape out and it
//! stops belonging. Nothing has to be added to anything.
//!
//! Transcribed from Excalidraw's `element/frame.ts` at the SHA pinned in
//! `scripts/oracle-sha.txt`. All of it is geometry and all of it lives here: which
//! elements a frame captures is a question every host must answer identically, and a host
//! that answered it slightly differently would produce boards that disagree about what
//! moves when a frame moves.
//!
//! A group is judged as one element: it is inside a frame when all of it is, so it joins
//! or leaves whole and never straddles the border as two halves ([`frame_for_element`]).
//!
//! What this deliberately does not cover yet, and why it is safe to add later: frames
//! inside frames. It changes only which elements are captured, not how capture is
//! decided.

use std::collections::HashMap;

use crate::camera::{Point, WorldBounds};
use crate::scene::element::{DrawElement, DrawElementType};
use crate::scene::geometry::{
    element_outline, element_rotated_bounds, outline_edges, outline_is_closed, segments_intersect,
};

/// Excalidraw's `FRAME_STYLE`. A frame is chrome, not a drawing: it is always this grey,
/// always this weight, and takes none of the element style controls.
pub const FRAME_STROKE: &str = "#bbbbbb";
pub const FRAME_STROKE_WIDTH: f64 = 2.0;
pub const FRAME_RADIUS: f64 = 8.0;
/// Gap between the frame's top edge and the baseline of its name.
pub const FRAME_NAME_OFFSET_Y: f64 = 3.0;
pub const FRAME_NAME_FONT_SIZE: f64 = 14.0;
pub const FRAME_NAME_LINE_HEIGHT: f64 = 1.25;
pub const FRAME_NAME_COLOR_LIGHT: &str = "#999999";
pub const FRAME_NAME_COLOR_DARK: &str = "#7a7a7a";

/// Smallest frame worth keeping, in world units.
///
/// A frame is drawn by dragging, and a click that moves a pixel or two is a click. Below
/// this the frame would capture nothing and be almost impossible to grab again.
pub const FRAME_MIN_SIZE: f64 = 16.0;

pub fn is_frame(element: &DrawElement) -> bool {
    element.kind == DrawElementType::Frame
}

/// The fixed appearance of every frame.
///
/// Not merged with the current element style, and not editable: Excalidraw's shape
/// actions exclude frames entirely. A frame tinted with whatever colour you last used
/// would read as a drawn rectangle rather than as a boundary.
pub fn frame_style() -> crate::scene::element::DrawElementStyle {
    crate::scene::element::DrawElementStyle {
        stroke_color: FRAME_STROKE.to_string(),
        background_color: "transparent".to_string(),
        fill_style: crate::scene::element::FillStyle::Solid,
        stroke_width: FRAME_STROKE_WIDTH,
        stroke_style: crate::scene::element::StrokeStyle::Solid,
        // Deliberately not hand-drawn. A wobbly boundary looks like a shape someone drew.
        roughness: 0.0,
        opacity: 100.0,
        roundness: Some(FRAME_RADIUS),
    }
}

/// Whether the element lies wholly within the frame.
///
/// Excalidraw's `elementsAreInFrameBounds`: compared as boxes, and as the element's
/// *rotated* box, so a turned rectangle is judged by the room it actually occupies.
pub fn element_in_frame_bounds(element: &DrawElement, frame: &DrawElement) -> bool {
    within(
        element_rotated_bounds(element),
        element_rotated_bounds(frame),
    )
}

fn within(inner: WorldBounds, outer: WorldBounds) -> bool {
    outer.min_x <= inner.min_x
        && outer.min_y <= inner.min_y
        && outer.max_x >= inner.max_x
        && outer.max_y >= inner.max_y
}

/// Whether the element's outline crosses the frame's border.
///
/// Boxes are not enough here. A long diagonal arrow has a box that overlaps a frame it
/// passes nowhere near, and judging by boxes would capture it.
pub fn element_intersects_frame(element: &DrawElement, frame: &DrawElement) -> bool {
    let frame_edges = outline_edges(&element_outline(frame), true);
    let element_shape = element_outline(element);
    let element_edges = outline_edges(&element_shape, outline_is_closed(element));

    element_edges.iter().any(|(a, b)| {
        frame_edges
            .iter()
            .any(|(c, d)| segments_intersect(*a, *b, *c, *d))
    })
}

/// Whether the element is large enough to swallow the frame whole.
///
/// Excalidraw keeps this case because a big background rectangle behind a frame contains
/// it without ever crossing its border, and should still be clipped by it.
pub fn element_contains_frame(element: &DrawElement, frame: &DrawElement) -> bool {
    within(
        element_rotated_bounds(frame),
        element_rotated_bounds(element),
    )
}

/// Excalidraw's `elementOverlapsWithFrame`: inside it, crossing it, or swallowing it.
pub fn element_overlaps_frame(element: &DrawElement, frame: &DrawElement) -> bool {
    element_in_frame_bounds(element, frame)
        || element_intersects_frame(element, frame)
        || element_contains_frame(element, frame)
}

/// Whether this element is eligible to be owned by a frame at all.
///
/// A frame cannot hold another frame — nested frames are out of scope — and a label
/// belongs to its container, which carries the membership for both.
fn can_belong_to_frame(element: &DrawElement) -> bool {
    !element.is_deleted && !is_frame(element) && element.container_id.is_none()
}

/// The elements a frame captures: those lying wholly inside it, a group only whole.
///
/// Wholly inside, not merely overlapping. Excalidraw captures on containment because
/// capture is silent — nothing asks first — and a rule that swept in everything a frame
/// merely touched would take neighbouring diagrams with it the moment you drew one.
pub fn elements_captured_by<'a, I>(elements: I, frame: &DrawElement) -> Vec<String>
where
    I: Iterator<Item = &'a DrawElement> + Clone,
{
    let groups = group_boxes(elements.clone());
    let area = element_rotated_bounds(frame);
    elements
        .filter(|element| can_belong_to_frame(element) && element.id != frame.id)
        .filter(|element| judged_box(element, &groups).is_some_and(|b| within(b, area)))
        .map(|element| element.id.clone())
        .collect()
}

/// Every group's box taken whole, by group id — at any depth, so an outer group's box
/// covers the groups nested in it. `None` for a group holding a frame, which no frame
/// can hold (`omitGroupsContainingFrameLikes`, `packages/element/src/frame.ts:747-788`).
fn group_boxes<'a>(
    elements: impl Iterator<Item = &'a DrawElement>,
) -> HashMap<&'a str, Option<WorldBounds>> {
    let mut boxes: HashMap<&str, Option<WorldBounds>> = HashMap::new();
    for member in elements.filter(|el| !el.is_deleted) {
        let own = (!is_frame(member)).then(|| element_rotated_bounds(member));
        for id in &member.group_ids {
            let so_far = boxes.entry(id.as_str()).or_insert(own);
            *so_far = so_far.zip(own).map(|(a, b)| union(a, b));
        }
    }
    boxes
}

/// The box a frame judges `element` by: its own, or — for a grouped element — its
/// outermost group's, taken whole.
///
/// So a group joins a frame or leaves it as one, as the oracle's does: a new frame takes
/// a group only when all of it is inside (`omitPartialGroups`, `packages/element/src/
/// frame.ts:395-434`), and a dragged member is decided by its group, never on its own
/// (`isElementInFrame`, `frame.ts:857-906`). Judged per element, a group straddling the
/// border was split in two, half of it clipped and carried by a frame the rest ignored.
///
/// The element's own box is folded in, so one not yet in the scene — a bucket fill
/// joining its walls' group — is judged by where it will be.
fn judged_box(
    element: &DrawElement,
    groups: &HashMap<&str, Option<WorldBounds>>,
) -> Option<WorldBounds> {
    let own = element_rotated_bounds(element);
    match element
        .group_ids
        .last()
        .and_then(|outermost| groups.get(outermost.as_str()))
    {
        Some(group) => group.map(|group| union(group, own)),
        None => Some(own),
    }
}

fn union(a: WorldBounds, b: WorldBounds) -> WorldBounds {
    WorldBounds {
        min_x: a.min_x.min(b.min_x),
        min_y: a.min_y.min(b.min_y),
        max_x: a.max_x.max(b.max_x),
        max_y: a.max_y.max(b.max_y),
    }
}

/// The ids currently claiming membership of this frame.
pub fn frame_children<'a>(
    elements: impl Iterator<Item = &'a DrawElement>,
    frame_id: &str,
) -> Vec<String> {
    elements
        .filter(|element| !element.is_deleted)
        .filter(|element| element.frame_id.as_deref() == Some(frame_id))
        .map(|element| element.id.clone())
        .collect()
}

/// The frame that should own this element, if any: see [`FrameOwners::of`].
pub fn frame_for_element<'a, I>(elements: I, element: &DrawElement) -> Option<String>
where
    I: DoubleEndedIterator<Item = &'a DrawElement> + Clone,
{
    FrameOwners::new(elements).of(element)
}

/// What frame membership is judged against, read from the scene once: its frames,
/// topmost first, and every group's box taken whole.
///
/// Once, because membership is re-derived across the whole scene after a gesture, and
/// reading the frames and each group's members again for every element is quadratic in
/// the size of the board.
pub(crate) struct FrameOwners<'a> {
    frames: Vec<&'a DrawElement>,
    groups: HashMap<&'a str, Option<WorldBounds>>,
}

impl<'a> FrameOwners<'a> {
    pub(crate) fn new<I>(elements: I) -> Self
    where
        I: DoubleEndedIterator<Item = &'a DrawElement> + Clone,
    {
        Self {
            frames: elements
                .clone()
                .rev()
                .filter(|candidate| is_frame(candidate) && !candidate.is_deleted)
                .collect(),
            groups: group_boxes(elements),
        }
    }

    /// The topmost frame `element` lies wholly within — with its whole group, if it has
    /// one. Topmost because frames are drawn on top of one another freely, and the one
    /// you can see is the one you meant.
    pub(crate) fn of(&self, element: &DrawElement) -> Option<String> {
        if !can_belong_to_frame(element) {
            return None;
        }
        let judged = judged_box(element, &self.groups)?;
        self.frames
            .iter()
            .find(|frame| within(judged, element_rotated_bounds(frame)))
            .map(|frame| frame.id.clone())
    }
}

/// Whether a child should be clipped to its frame when painted.
///
/// Excalidraw's `shouldApplyFrameClip`, reduced to the part that is geometry: a child
/// wholly inside needs no clip because nothing of it sticks out, and clipping it anyway
/// would cost a path per element for no visible difference.
pub fn needs_frame_clip(element: &DrawElement, frame: &DrawElement) -> bool {
    element_intersects_frame(element, frame) || element_contains_frame(element, frame)
}

/// The next unused default name, as Excalidraw numbers them.
///
/// Numbered by the highest existing default rather than by the count, so deleting
/// "Frame 2" of three does not produce a second "Frame 3".
pub fn default_frame_name<'a>(elements: impl Iterator<Item = &'a DrawElement>) -> String {
    let highest = elements
        .filter(|element| is_frame(element) && !element.is_deleted)
        .filter_map(|element| element.name.as_deref())
        .filter_map(|name| name.strip_prefix("Frame "))
        .filter_map(|n| n.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("Frame {}", highest + 1)
}

/// Where the frame's name sits: above its top-left corner, outside the frame.
///
/// Outside, because a name drawn inside would sit on top of whatever the frame contains
/// and would be clipped along with it.
pub fn frame_name_anchor(frame: &DrawElement) -> Point {
    let bounds = element_rotated_bounds(frame);
    Point {
        x: bounds.min_x,
        y: bounds.min_y - FRAME_NAME_OFFSET_Y,
    }
}

/// The box a frame clips its children to.
pub fn frame_clip_bounds(frame: &DrawElement) -> WorldBounds {
    element_rotated_bounds(frame)
}
