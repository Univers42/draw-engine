//! Where a text's lines break, how big its box is, where a label sits in its shape, and
//! how far the shape grows to hold it — one function, [`layout_text`], for every path
//! that changes a text.
//!
//! A port of Excalidraw's `redrawTextBoundingBox` and the geometry it leans on
//! (`packages/element/src/textElement.ts@1118751f`): `getContainerCoords` (:396-417),
//! `getBoundTextMaxWidth` (:511-540), `getBoundTextMaxHeight` (:542-570),
//! `computeContainerDimensionForBoundText` (:492-509), `computeBoundTextPosition`
//! (:249-324) and `measureText` (`textMeasurements.ts@1118751f:12-27`). The anchors a
//! free text keeps when its text or its font size changes are `getAdjustedDimensions`
//! (`newElement.ts@1118751f:393-527`) and `offsetElementAfterFontResize`
//! (`actionProperties.tsx@1118751f:273-292`).
//!
//! Lines are always wrapped from what was typed ([`source_text`]), never from the lines
//! drawn last time: re-wrapping drawn lines turns every soft break into a hard one.
//!
//! Pure: everything it needs from a font comes in through [`Measure`], so the rules are
//! tested without a browser and the engine and the editor share them.

use crate::camera::Point;
use crate::scene::element::{
    is_auto_resize, resolved_font_family, resolved_line_height, resolved_text_align,
    resolved_vertical_align, source_text, DrawElement, DrawElementType, TextAlign, VerticalAlign,
};

use super::{FontKey, MeasureCache};

/// Excalidraw's `BOUND_TEXT_PADDING` (`packages/common/src/constants.ts@1118751f:417`):
/// the air between a label and its shape's outline.
pub const BOUND_TEXT_PADDING: f64 = 5.0;
/// `ARROW_LABEL_WIDTH_FRACTION` (`constants.ts@1118751f:418`): how much of an arrow's
/// width its label's lines may take.
pub const ARROW_LABEL_WIDTH_FRACTION: f64 = 0.7;
/// `ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO` (`constants.ts@1118751f:419`): however short
/// the arrow, its label may be this many font sizes wide.
pub const ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO: f64 = 11.0;
/// `DEFAULT_FONT_SIZE`, for a text that carries none.
pub const DEFAULT_FONT_SIZE: f64 = 20.0;

/// What layout needs from the fonts: a line's advance width, and the caches in front of
/// it.
#[derive(Clone, Copy)]
pub struct Measure<'a> {
    pub cache: &'a MeasureCache,
    /// The advance width of one line (never holding `\n`) in a font, kerning included.
    pub line_width: &'a dyn Fn(&str, FontKey) -> f64,
}

impl Measure<'_> {
    /// `text` soft-wrapped to `max_width` (`wrapText`), through the per-hard-line memo.
    pub fn wrap(&self, text: &str, max_width: f64, font: FontKey) -> String {
        let line_width = |line: &str| (self.line_width)(line, font);
        self.cache.wrap_text(text, max_width, font, &line_width)
    }

    /// `measureText`: the widest line, and a line height per line. An empty line
    /// measures as a space, as the oracle's does, so a text of blank lines still has a
    /// width to click.
    pub fn size(&self, text: &str, font: FontKey, line_height: f64) -> (f64, f64) {
        let line_width = |line: &str| (self.line_width)(line, font);
        let mut width = 0.0_f64;
        let mut lines = 0usize;
        for line in text.split('\n') {
            let line = if line.is_empty() { " " } else { line };
            width = width.max(self.cache.line_width(font, line, &line_width));
            lines += 1;
        }
        (width, lines as f64 * font.size() * line_height)
    }
}

/// The font a text is measured and drawn in.
pub fn font_of(text: &DrawElement) -> FontKey {
    FontKey::new(
        resolved_font_family(text).unwrap_or(FontKey::LEGACY),
        font_size_of(text),
    )
}

pub fn font_size_of(text: &DrawElement) -> f64 {
    text.font_size.unwrap_or(DEFAULT_FONT_SIZE)
}

/// `Math.round`, which rounds halves up — Rust's `round` rounds them away from zero.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// A container's box as the oracle reads it: its own unrotated `x`, `y`, `width` and
/// `height`. A shape drawn right to left can carry negative extents here; the box is the
/// same one either way. A line or arrow is as wide and tall as its points.
fn container_box(container: &DrawElement) -> (f64, f64, f64, f64) {
    if crate::scene::is_linear_element(container) {
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        for (i, p) in container
            .points
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            if i == 0 {
                (min_x, min_y, max_x, max_y) = (p[0], p[1], p[0], p[1]);
            }
            min_x = min_x.min(p[0]);
            min_y = min_y.min(p[1]);
            max_x = max_x.max(p[0]);
            max_y = max_y.max(p[1]);
        }
        return (container.x, container.y, max_x - min_x, max_y - min_y);
    }
    let rect =
        crate::scene::normalize_rect(container.x, container.y, container.width, container.height);
    (rect.x, rect.y, rect.width, rect.height)
}

/// `getContainerCoords`: the top-left corner of the box a label is laid out in.
pub fn container_coords(container: &DrawElement) -> Point {
    let (x, y, width, height) = container_box(container);
    let (mut dx, mut dy) = (BOUND_TEXT_PADDING, BOUND_TEXT_PADDING);
    match container.kind {
        DrawElementType::Ellipse => {
            dx += width / 2.0 * (1.0 - std::f64::consts::SQRT_2 / 2.0);
            dy += height / 2.0 * (1.0 - std::f64::consts::SQRT_2 / 2.0);
        }
        DrawElementType::Diamond => {
            dx += width / 4.0;
            dy += height / 4.0;
        }
        _ => {}
    }
    Point {
        x: x + dx,
        y: y + dy,
    }
}

/// `getBoundTextMaxWidth`: how wide a label's lines may be in `container`.
pub fn bound_text_max_width(container: &DrawElement, font_size: f64) -> f64 {
    let (_, _, width, _) = container_box(container);
    if crate::scene::is_linear_element(container) {
        return (ARROW_LABEL_WIDTH_FRACTION * width)
            .max(font_size * ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO);
    }
    match container.kind {
        DrawElementType::Ellipse => {
            js_round(width / 2.0 * std::f64::consts::SQRT_2) - BOUND_TEXT_PADDING * 2.0
        }
        DrawElementType::Diamond => js_round(width / 2.0) - BOUND_TEXT_PADDING * 2.0,
        _ => width - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `getBoundTextMaxHeight`: how tall a label may be in `container` before it grows.
pub fn bound_text_max_height(container: &DrawElement, label_height: f64) -> f64 {
    let (_, _, _, height) = container_box(container);
    if crate::scene::is_linear_element(container) {
        return if height - BOUND_TEXT_PADDING * 8.0 * 2.0 <= 0.0 {
            label_height
        } else {
            height
        };
    }
    match container.kind {
        DrawElementType::Ellipse => {
            js_round(height / 2.0 * std::f64::consts::SQRT_2) - BOUND_TEXT_PADDING * 2.0
        }
        DrawElementType::Diamond => js_round(height / 2.0) - BOUND_TEXT_PADDING * 2.0,
        _ => height - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `computeContainerDimensionForBoundText`: the width or height a container of `kind`
/// needs to hold a label `dimension` across.
pub fn container_dimension_for_bound_text(dimension: f64, kind: DrawElementType) -> f64 {
    let dimension = dimension.ceil();
    let padding = BOUND_TEXT_PADDING * 2.0;
    match kind {
        DrawElementType::Ellipse => {
            js_round((dimension + padding) / std::f64::consts::SQRT_2 * 2.0)
        }
        DrawElementType::Arrow | DrawElementType::Line => dimension + padding * 8.0,
        DrawElementType::Diamond => 2.0 * (dimension + padding),
        _ => dimension + padding,
    }
}

/// Where an arrow's label is centred (`getBoundTextElementCenter`,
/// `linearElementEditor.ts@1118751f:1942-1960`): on the middle point of an odd count, or
/// half way along the middle segment of an even one — along its curve when it is round.
pub fn linear_label_center(container: &DrawElement) -> Option<Point> {
    let count = container.points.as_ref().map_or(0, Vec::len);
    if count < 2 {
        return None;
    }
    let wanted = if count % 2 == 1 {
        crate::selection::linear::LinearHandle::Point(count / 2)
    } else {
        crate::selection::linear::LinearHandle::Midpoint(count / 2 - 1)
    };
    crate::selection::linear::handle_points(container, 0.0)
        .into_iter()
        .find(|handle| handle.handle == wanted)
        .map(|handle| Point {
            x: handle.x,
            y: handle.y,
        })
}

/// `computeBoundTextPosition`: where `label`, at its current size, goes in `container`.
pub fn bound_text_position(container: &DrawElement, label: &DrawElement) -> Point {
    if crate::scene::is_linear_element(container) {
        return linear_label_center(container).map_or(
            Point {
                x: label.x,
                y: label.y,
            },
            |center| Point {
                x: center.x - label.width / 2.0,
                y: center.y - label.height / 2.0,
            },
        );
    }
    let coords = container_coords(container);
    let max_height = bound_text_max_height(container, label.height);
    let max_width = bound_text_max_width(container, font_size_of(label));
    let y = match resolved_vertical_align(label) {
        VerticalAlign::Top => coords.y,
        VerticalAlign::Bottom => coords.y + (max_height - label.height),
        VerticalAlign::Middle => coords.y + (max_height / 2.0 - label.height / 2.0),
    };
    let x = match resolved_text_align(label) {
        TextAlign::Left => coords.x,
        TextAlign::Right => coords.x + (max_width - label.width),
        TextAlign::Center => coords.x + (max_width / 2.0 - label.width / 2.0),
    };
    if container.angle == 0.0 {
        return Point { x, y };
    }
    // Turned about the middle of the content box, as the shape is turned about its own.
    let content = Point {
        x: coords.x + max_width / 2.0,
        y: coords.y + max_height / 2.0,
    };
    let (sin, cos) = container.angle.sin_cos();
    let (dx, dy) = (
        x + label.width / 2.0 - content.x,
        y + label.height / 2.0 - content.y,
    );
    Point {
        x: content.x + dx * cos - dy * sin - label.width / 2.0,
        y: content.y + dx * sin + dy * cos - label.height / 2.0,
    }
}

/// The width a text's lines are wrapped at, or `None` when they are not wrapped: a
/// label that wraps takes its shape's [`bound_text_max_width`], a fixed-width text its
/// own width. An auto-sizing text, and a label told not to wrap, keep their hard lines.
pub fn wrap_width(text: &DrawElement, container: Option<&DrawElement>) -> Option<f64> {
    match container {
        Some(container) if text.wrap != Some(false) => {
            Some(bound_text_max_width(container, font_size_of(text)))
        }
        Some(_) => None,
        None if !is_auto_resize(text) => Some(text.width.abs()),
        None => None,
    }
}

/// A text laid out, and its container if laying it out grew it.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub text: DrawElement,
    pub container: Option<DrawElement>,
}

/// `redrawTextBoundingBox`: `text` re-wrapped from what was typed, measured, and — for a
/// label — placed in `container`, which grows to hold it.
///
/// - Lines wrap at [`wrap_width`].
/// - The box takes the measured height, and the measured width unless it is a
///   fixed-width text, which keeps the width it was given.
/// - A label taller than its shape allows grows the shape's height; one wider (a label
///   that does not wrap, or a word no break fits) grows its width. A line or arrow never
///   grows: its extent is its points'. Excalidraw writes the new width onto an arrow
///   (`textElement.ts@1118751f:127-133`), which its points then contradict.
/// - A label turns with its shape; an arrow's never turns.
///
/// A free text stays where it is: which edge it keeps as it grows is the caller's to say
/// ([`edit_anchor`], [`font_resize_anchor`]).
pub fn layout_text(text: &DrawElement, container: Option<&DrawElement>, measure: &Measure) -> Laid {
    let font = font_of(text);
    let line_height = resolved_line_height(text);
    let source = source_text(text);
    let lines = match wrap_width(text, container) {
        Some(max_width) => measure.wrap(source, max_width, font),
        None => source.to_owned(),
    };
    let (width, height) = measure.size(&lines, font, line_height);

    let mut next = text.clone();
    next.original_text = Some(source.to_owned());
    next.text = Some(lines);
    if container.is_some() || is_auto_resize(text) {
        next.width = width;
    }
    next.height = height;
    let Some(container) = container else {
        return Laid {
            text: next,
            container: None,
        };
    };
    next.angle = if crate::scene::is_linear_element(container) {
        0.0
    } else {
        container.angle
    };

    let mut grown = None;
    if !crate::scene::is_linear_element(container) {
        let max_height = bound_text_max_height(container, height);
        let max_width = bound_text_max_width(container, font_size_of(text));
        let mut target = container.clone();
        if height > max_height {
            grow(
                &mut target,
                None,
                Some(container_dimension_for_bound_text(height, container.kind)),
            );
        }
        if width > max_width {
            grow(
                &mut target,
                Some(container_dimension_for_bound_text(width, container.kind)),
                None,
            );
        }
        if &target != container {
            grown = Some(target);
        }
    }
    let placed_in = grown.as_ref().unwrap_or(container);
    let at = bound_text_position(placed_in, &next);
    next.x = at.x;
    next.y = at.y;
    Laid {
        text: next,
        container: grown,
    }
}

/// Sets a container's width or height, from the corner the oracle keeps: its `x`, `y`.
/// A negative extent is made positive first, so the shape grows away from the same
/// corner it is drawn from.
fn grow(container: &mut DrawElement, width: Option<f64>, height: Option<f64>) {
    let rect =
        crate::scene::normalize_rect(container.x, container.y, container.width, container.height);
    container.x = rect.x;
    container.y = rect.y;
    container.width = width.unwrap_or(rect.width);
    container.height = height.unwrap_or(rect.height);
}

/// Where a free text goes when its text changes and it becomes `next_width` ×
/// `next_height` (`getAdjustedDimensions`): the edges its alignment names stay put, so a
/// right-aligned text grows to the left and a centred one both ways — turned or not.
///
/// `prev_measured` is the old text as measured now, which the oracle uses in place of
/// the old box for a centred, middle, auto-sizing text.
pub fn edit_anchor(
    prev: &DrawElement,
    prev_measured: (f64, f64),
    next_width: f64,
    next_height: f64,
) -> Point {
    let align = resolved_text_align(prev);
    let valign = resolved_vertical_align(prev);
    if align == TextAlign::Center
        && valign == VerticalAlign::Middle
        && prev.container_id.is_none()
        && is_auto_resize(prev)
    {
        return Point {
            x: prev.x - (next_width - prev_measured.0) / 2.0,
            y: prev.y - (next_height - prev_measured.1) / 2.0,
        };
    }
    // `getResizedElementAbsoluteCoords` keeps x1/y1, so only the far edges move.
    let (dx2, dy2) = (
        (prev.width - next_width) / 2.0,
        (prev.height - next_height) / 2.0,
    );
    let e = matches!(align, TextAlign::Center | TextAlign::Left);
    let w = matches!(align, TextAlign::Center | TextAlign::Right);
    let n = matches!(valign, VerticalAlign::Middle | VerticalAlign::Bottom);
    let s = matches!(valign, VerticalAlign::Middle | VerticalAlign::Top);
    let (sin, cos) = prev.angle.sin_cos();
    let (mut x, mut y) = (prev.x, prev.y);
    // `adjustXYWithRotation` (`newElement.ts@1118751f:482-527`), with deltaX1 = deltaY1 = 0.
    if e && w {
        x += dx2;
    } else if e {
        x += dx2 * (1.0 - cos);
        y += dx2 * -sin;
    } else if w {
        x += dx2 * (1.0 + cos);
        y += dx2 * sin;
    }
    if n && s {
        y += dy2;
    } else if n {
        x += dy2 * -sin;
        y += dy2 * (1.0 + cos);
    } else if s {
        x += dy2 * sin;
        y += dy2 * (1.0 - cos);
    }
    if x.is_finite() && y.is_finite() {
        Point { x, y }
    } else {
        Point {
            x: prev.x,
            y: prev.y,
        }
    }
}

/// Where a free, auto-sizing text goes when its font size changes
/// (`offsetElementAfterFontResize`): its aligned edge stays, and it grows about its
/// vertical middle. `None` for a label or a fixed-width text, which do not move.
pub fn font_resize_anchor(prev: &DrawElement, next: &DrawElement) -> Option<Point> {
    if next.container_id.is_some() || !is_auto_resize(next) {
        return None;
    }
    let x = match resolved_text_align(prev) {
        TextAlign::Left => prev.x,
        TextAlign::Center => prev.x + (prev.width - next.width) / 2.0,
        TextAlign::Right => prev.x + (prev.width - next.width),
    };
    Some(Point {
        x,
        y: prev.y + (prev.height - next.height) / 2.0,
    })
}

/// Where a fixed-width text goes when it becomes auto-sizing again and measures
/// `next_width` × `next_height` (`actionTextAutoResize`,
/// `packages/excalidraw/actions/actionTextAutoResize.ts@1118751f`): the point its
/// alignment anchors stays put (`getTextAnchorRatios`).
pub fn auto_resize_anchor(prev: &DrawElement, next_width: f64, next_height: f64) -> Point {
    let ax = match resolved_text_align(prev) {
        TextAlign::Left => 0.0,
        TextAlign::Center => 0.5,
        TextAlign::Right => 1.0,
    };
    let ay = match resolved_vertical_align(prev) {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => 0.5,
        VerticalAlign::Bottom => 1.0,
    };
    Point {
        x: prev.x + (prev.width - next_width) * ax,
        y: prev.y + (prev.height - next_height) * ay,
    }
}
