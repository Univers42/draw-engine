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
//! (`actionProperties.tsx@1118751f:273-292`). What a resize needs on top — the smallest
//! a text or a shape holding one may become, and a label laid out again in a shape being
//! resized — is `getMinTextElementWidth`, `getApproxMinLineWidth`,
//! `getApproxMinLineHeight` (`textMeasurements.ts@1118751f:32-104`) and
//! `handleBindTextResize` (`textElement.ts@1118751f:155-247`).
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
use crate::scene::figure::{self, FigureKind, FigureParams};

use crate::scene::sticky::{
    label_ceiling, normalize_sticky_font_size, position_after_height_change, StickyLayoutOpts,
    STICKY_NOTE_BODY_INSET_Y, STICKY_NOTE_FONT_STEP, STICKY_NOTE_MIN_FONT_SIZE,
    STICKY_NOTE_MIN_SIZE, STICKY_NOTE_PADDING,
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
pub(crate) fn js_round(value: f64) -> f64 {
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

/// Extra inset added to the base padding for a label's box in a `Figure`, along each
/// axis — the ellipse and diamond cases beside this function's callers, generalized to a
/// figure's own kinds.
///
/// Not exact for every one — a triangle's safe area really tapers to a point, which no
/// single rectangular inset follows — but chosen so a centred label's box stays inside
/// the silhouette everywhere it actually reaches: a many-sided polygon is close enough to
/// a circle to earn the ellipse's own inset, a triangle or a star's points get the
/// diamond's tighter quarter, and a parallelogram, trapezoid, cylinder or document is
/// only distorted along one axis, so only that axis is inset at all.
fn figure_text_inset(params: &FigureParams, width: f64, height: f64) -> (f64, f64) {
    (
        figure_text_inset_fraction(params, true) * width,
        figure_text_inset_fraction(params, false) * height,
    )
}

/// [`figure_text_inset`], as a fraction of the axis's own size rather than a pixel
/// amount — what [`container_dimension_for_bound_text`] inverts to grow a figure to fit
/// a label, since that has a dimension to solve *for* rather than one to inset.
fn figure_text_inset_fraction(params: &FigureParams, axis_is_width: bool) -> f64 {
    match params.kind {
        FigureKind::Polygon if figure::resolved_sides(params.kind, params.sides) >= 5 => {
            0.5 * (1.0 - std::f64::consts::FRAC_1_SQRT_2)
        }
        FigureKind::Polygon | FigureKind::Star => 0.25,
        FigureKind::Parallelogram | FigureKind::Trapezoid => {
            if axis_is_width {
                figure::resolved_ratio(params.kind, params.ratio)
            } else {
                0.0
            }
        }
        FigureKind::Cylinder | FigureKind::Document => {
            if axis_is_width {
                0.0
            } else {
                figure::resolved_ratio(params.kind, params.ratio)
            }
        }
    }
}

/// `getContainerCoords`: the top-left corner of the box a label is laid out in.
pub fn container_coords(container: &DrawElement) -> Point {
    let (x, y, width, height) = container_box(container);
    let padding = if container.kind == DrawElementType::StickyNote {
        STICKY_NOTE_PADDING
    } else {
        BOUND_TEXT_PADDING
    };
    let (mut dx, mut dy) = (padding, padding);
    match container.kind {
        DrawElementType::Ellipse => {
            dx += width / 2.0 * (1.0 - std::f64::consts::SQRT_2 / 2.0);
            dy += height / 2.0 * (1.0 - std::f64::consts::SQRT_2 / 2.0);
        }
        DrawElementType::Diamond => {
            dx += width / 4.0;
            dy += height / 4.0;
        }
        DrawElementType::Figure => {
            let params = container.figure.clone().unwrap_or_default();
            let (ix, iy) = figure_text_inset(&params, width, height);
            dx += ix;
            dy += iy;
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
    let (_, _, width, height) = container_box(container);
    if crate::scene::is_linear_element(container) {
        return (ARROW_LABEL_WIDTH_FRACTION * width)
            .max(font_size * ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO);
    }
    match container.kind {
        DrawElementType::Ellipse => {
            js_round(width / 2.0 * std::f64::consts::SQRT_2) - BOUND_TEXT_PADDING * 2.0
        }
        DrawElementType::Diamond => js_round(width / 2.0) - BOUND_TEXT_PADDING * 2.0,
        DrawElementType::StickyNote => width - STICKY_NOTE_PADDING * 2.0,
        DrawElementType::Figure => {
            let params = container.figure.clone().unwrap_or_default();
            let (ix, _) = figure_text_inset(&params, width, height);
            width - 2.0 * (BOUND_TEXT_PADDING + ix)
        }
        _ => width - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `getBoundTextMaxHeight`: how tall a label may be in `container` before it grows.
pub fn bound_text_max_height(container: &DrawElement, label_height: f64) -> f64 {
    let (_, _, width, height) = container_box(container);
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
        // The label's body ends above the date's footer.
        DrawElementType::StickyNote => (height - STICKY_NOTE_BODY_INSET_Y).max(0.0),
        DrawElementType::Figure => {
            let params = container.figure.clone().unwrap_or_default();
            let (_, iy) = figure_text_inset(&params, width, height);
            height - 2.0 * (BOUND_TEXT_PADDING + iy)
        }
        _ => height - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `computeContainerDimensionForBoundText`: the width or height a container of `kind`
/// needs to hold a label `dimension` across.
///
/// `figure`/`axis_is_width` matter only for [`DrawElementType::Figure`], whose inset is
/// not the same fraction on both axes; every other kind ignores them.
pub fn container_dimension_for_bound_text(
    dimension: f64,
    kind: DrawElementType,
    figure: Option<&FigureParams>,
    axis_is_width: bool,
) -> f64 {
    let dimension = dimension.ceil();
    let padding = BOUND_TEXT_PADDING * 2.0;
    match kind {
        DrawElementType::Ellipse => {
            js_round((dimension + padding) / std::f64::consts::SQRT_2 * 2.0)
        }
        DrawElementType::Arrow | DrawElementType::Line => dimension + padding * 8.0,
        DrawElementType::Diamond => 2.0 * (dimension + padding),
        DrawElementType::Figure => {
            let params = figure.cloned().unwrap_or_default();
            let k = figure_text_inset_fraction(&params, axis_is_width);
            // ponytail: a parallelogram/trapezoid ratio near the contract's own 0.95 cap
            // pushes `k` past a half, where the exact inverse blows up or goes negative;
            // floored so the container only ever comes out generous, never nonsensical.
            (dimension + padding) / (1.0 - 2.0 * k).max(0.1)
        }
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
    let sticky = container.kind == DrawElementType::StickyNote;
    let y = match resolved_vertical_align(label) {
        VerticalAlign::Top => coords.y,
        VerticalAlign::Bottom => coords.y + (max_height - label.height),
        // Centred in the body above the footer, a note's label sits visibly high: it is
        // centred in the whole padded note while it stays clear of the footer, and pushed
        // up against the body's bottom only once it would overlap
        // (`textElement.ts@1118751f:271-283`).
        VerticalAlign::Middle if sticky => {
            let padded = container_box(container).3 - STICKY_NOTE_PADDING * 2.0;
            coords.y + ((padded - label.height) / 2.0).min(max_height - label.height)
        }
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
    // Turned about the middle of the content box, as the shape is turned about its own —
    // a note's about its own centre: its footer makes the body lopsided
    // (`textElement.ts@1118751f:297-310`).
    let content = if sticky {
        let (bx, by, bw, bh) = container_box(container);
        Point {
            x: bx + bw / 2.0,
            y: by + bh / 2.0,
        }
    } else {
        Point {
            x: coords.x + max_width / 2.0,
            y: coords.y + max_height / 2.0,
        }
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
    // A note's fit owns both halves: the label's size and the note's height.
    if let Some(note) = container.filter(|c| c.kind == DrawElementType::StickyNote) {
        let laid = sticky_layout(note, Some(text), &StickyLayoutOpts::default(), measure);
        return Laid {
            text: laid.text.unwrap_or_else(|| text.clone()),
            container: (laid.container != *note).then_some(laid.container),
        };
    }
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
                Some(container_dimension_for_bound_text(
                    height,
                    container.kind,
                    container.figure.as_ref(),
                    false,
                )),
            );
        }
        if width > max_width {
            grow(
                &mut target,
                Some(container_dimension_for_bound_text(
                    width,
                    container.kind,
                    container.figure.as_ref(),
                    true,
                )),
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

/// `getMinTextElementWidth` (`textMeasurements.ts@1118751f:46-51`): the narrowest a text's
/// side can make it — a space, and the padding either side.
pub fn min_text_width(text: &DrawElement, measure: &Measure) -> f64 {
    measure
        .size("", font_of(text), resolved_line_height(text))
        .0
        + BOUND_TEXT_PADDING * 2.0
}

/// The chars `getApproxMinLineWidth` measures when it has none cached for the font
/// (`textMeasurements.ts@1118751f:29`).
const DUMMY_TEXT: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// The smallest a shape holding `label` may be resized to, and what a new label's shape
/// grows to (`App.tsx@1118751f:6974-7006`), width by height: its widest char and one line,
/// each with the padding either side (`getApproxMinLineWidth`, `getApproxMinLineHeight`,
/// `textMeasurements.ts@1118751f:32-44`, `:99-104`).
///
/// The chars are measured through the char widths the wrap caches, so a resize, which
/// asks this on every move, measures them once per font.
///
/// Divergence: the oracle's widest char is the widest it has measured in that font so
/// far (its `charWidth` cache), and the alphabet above only when it has measured none;
/// here it is always the alphabet's, so the minimum does not depend on what was typed
/// before.
pub fn min_container_size(label: &DrawElement, measure: &Measure) -> (f64, f64) {
    use super::TextMetrics;
    let font = font_of(label);
    let line_width = |line: &str| (measure.line_width)(line, font);
    let metrics = measure.cache.metrics(font, &line_width);
    let widest = DUMMY_TEXT
        .chars()
        .map(|c| metrics.char_width(c))
        .fold(0.0_f64, f64::max);
    (
        widest + BOUND_TEXT_PADDING * 2.0,
        font.size() * resolved_line_height(label) + BOUND_TEXT_PADDING * 2.0,
    )
}

/// Where a box that was `prev`, turned by `angle`, goes when it becomes `width` ×
/// `height` and the point `keep` of it — fractions of its width and height from its
/// top-left, turned with it — stays where it was: `getPositionAfterHeightChange`
/// (`sizeHelpers.ts@1118751f:28-54`) for either axis, and `getResizedOrigin`
/// (`resizeElements.ts@1118751f:621-727`) with the corner its anchor names. Extents
/// positive.
pub fn keep_point(
    prev: &crate::selection::Geometry,
    angle: f64,
    width: f64,
    height: f64,
    keep: (f64, f64),
) -> Point {
    let (sin, cos) = angle.sin_cos();
    // From the centre to the kept point, before and after, in the box's own frame.
    let before = ((keep.0 - 0.5) * prev.width, (keep.1 - 0.5) * prev.height);
    let after = ((keep.0 - 0.5) * width, (keep.1 - 0.5) * height);
    let (dx, dy) = (before.0 - after.0, before.1 - after.1);
    let centre = Point {
        x: prev.x + prev.width / 2.0 + dx * cos - dy * sin,
        y: prev.y + prev.height / 2.0 + dx * sin + dy * cos,
    };
    Point {
        x: centre.x - width / 2.0,
        y: centre.y - height / 2.0,
    }
}

/// `handleBindTextResize` (`textElement.ts@1118751f:155-247`): `label` laid out again in
/// `container`, which a resize has just changed — wrapped from what was typed at its new
/// room, and the container grown back to hold it from the side the drag holds: `keep`
/// is the point of it that stays, as [`keep_point`] reads it (the oracle's `"top"` is
/// `(0.5, 0.0)`, its `"bottom"` `(0.5, 1.0)`).
///
/// Through [`layout_text`], so a resize and an edit agree on every line. It differs from
/// the oracle's in two places:
/// - lines are wrapped again on every handle, where the oracle keeps them for a plain
///   north or south one. The room did not change there, so neither do the lines — the
///   wrap memo answers;
/// - a label wider than its room widens the container, as [`layout_text`] does — a label
///   that does not wrap, or a char wider than a narrow ellipse's room. The oracle lets it
///   hang out.
pub fn bound_text_resize(
    label: &DrawElement,
    container: &DrawElement,
    keep: (f64, f64),
    measure: &Measure,
) -> Laid {
    if container.kind == DrawElementType::StickyNote {
        // A note is resized with its own intents (`engine/sticky.rs`); a caller that has
        // none keeps its base height and ceiling, as the oracle's fallback does
        // (`handleBindTextResize`, `textElement.ts@1118751f:163-168`).
        return layout_text(label, Some(container), measure);
    }
    let mut laid = layout_text(label, Some(container), measure);
    let Some(grown) = laid.container.as_mut() else {
        return laid;
    };
    // `layout_text` grows it from its top-left.
    let rect =
        crate::scene::normalize_rect(container.x, container.y, container.width, container.height);
    let before = crate::selection::Geometry {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    };
    let at = keep_point(&before, container.angle, grown.width, grown.height, keep);
    grown.x = at.x;
    grown.y = at.y;
    let placed = bound_text_position(grown, &laid.text);
    laid.text.x = placed.x;
    laid.text.y = placed.y;
    laid
}

/// A note and its label laid out together.
#[derive(Clone, Debug, PartialEq)]
pub struct StickyLaid {
    pub container: DrawElement,
    pub text: Option<DrawElement>,
}

/// One size tried by the fit: the lines at it and their box.
#[derive(Clone, Debug)]
struct FontFit {
    text: String,
    font_size: f64,
    width: f64,
    height: f64,
}

/// `fitStickyNoteFont` (`stickyNote.ts@1118751f:587-661`): the largest size on the grid
/// `{ceiling − k·STEP} ∪ {min}` whose lines fit the note, `min` when none does (the note
/// then grows). Anchored at the ceiling, so the answer never depends on earlier edits, and
/// started from the size the label has now: a keystroke costs one or two measures, a cold
/// search at most log2(steps) + 2.
fn fit_sticky_font(
    fit: &dyn Fn(f64) -> FontFit,
    ceiling: f64,
    min: f64,
    (max_width, max_height): (f64, f64),
    warm_start: f64,
) -> FontFit {
    let steps = ((ceiling - min) / STICKY_NOTE_FONT_STEP).ceil().max(0.0) as usize;
    let size_at = |index: usize| {
        if index >= steps {
            min
        } else {
            ceiling - index as f64 * STICKY_NOTE_FONT_STEP
        }
    };
    let fits = std::cell::RefCell::new(std::collections::HashMap::<usize, FontFit>::new());
    let at = |index: usize| -> FontFit {
        if let Some(found) = fits.borrow().get(&index) {
            return found.clone();
        }
        let tried = fit(size_at(index));
        fits.borrow_mut().insert(index, tried.clone());
        tried
    };
    let does_fit = |index: usize| {
        let tried = at(index);
        tried.width <= max_width && tried.height <= max_height
    };
    if steps == 0 {
        return at(0);
    }
    // The last size, snapped onto the grid and into the interval: a lowered ceiling must
    // not keep the old, larger size.
    let warm =
        js_round((ceiling - warm_start) / STICKY_NOTE_FONT_STEP).clamp(0.0, steps as f64) as usize;
    let (mut lo, mut hi);
    if does_fit(warm) {
        if warm == 0 || !does_fit(warm - 1) {
            return at(warm);
        }
        lo = 0;
        hi = warm - 1;
    } else {
        if warm == steps {
            return at(steps);
        }
        lo = warm + 1;
        hi = steps;
    }
    while lo < hi {
        let mid = (lo + hi) / 2;
        if does_fit(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    at(lo)
}

/// `getStickyNoteLayout` (`stickyNote.ts@1118751f:669-762`), the one source of a note's
/// geometry: its label wrapped at the note's width, its font fitted under the ceiling, the
/// note grown past its base height only when the text still overflows at the smallest
/// size, and the label placed in it. An empty note, or a blank label, sits at its base
/// height with the label at its ceiling.
pub fn sticky_layout(
    note: &DrawElement,
    label: Option<&DrawElement>,
    opts: &StickyLayoutOpts,
    measure: &Measure,
) -> StickyLaid {
    let base_width = note.width.abs().max(STICKY_NOTE_MIN_SIZE);
    // `container.baseHeight || container.height`: a note from before the field — or one
    // whose base is 0 — takes its height as its base.
    let own_base = note
        .base_height
        .filter(|base| *base != 0.0)
        .unwrap_or(note.height.abs());
    let base_height = opts
        .base_height
        .unwrap_or(own_base)
        .max(STICKY_NOTE_MIN_SIZE);
    let place = |height: f64| {
        let at = position_after_height_change(note, height, opts.anchor);
        let mut next = note.clone();
        next.x = at.x;
        next.y = at.y;
        next.width = base_width;
        next.height = height;
        next.base_height = Some(base_height);
        next
    };
    let Some(label) = label else {
        return StickyLaid {
            container: place(base_height),
            text: None,
        };
    };
    let source = opts
        .original_text
        .clone()
        .unwrap_or_else(|| source_text(label).to_owned());
    let ceiling = normalize_sticky_font_size(
        opts.base_font_size
            .unwrap_or_else(|| label_ceiling(label, Some(note))),
    );
    let min = STICKY_NOTE_MIN_FONT_SIZE.min(ceiling);
    let max_width = (base_width - STICKY_NOTE_PADDING * 2.0).max(1.0);
    let max_height = (base_height - STICKY_NOTE_BODY_INSET_Y).max(0.0);
    let family = resolved_font_family(label).unwrap_or(FontKey::LEGACY);
    let line_height = resolved_line_height(label);
    let fit = |size: f64| {
        let font = FontKey::new(family, size);
        let text = measure.wrap(&source, max_width, font);
        let (width, height) = measure.size(&text, font, line_height);
        FontFit {
            text,
            font_size: size,
            width,
            height,
        }
    };
    let blank = source.trim().is_empty();
    let fitted = if blank {
        let (width, height) = measure.size("", FontKey::new(family, ceiling), line_height);
        FontFit {
            text: String::new(),
            font_size: ceiling,
            width,
            height,
        }
    } else {
        fit_sticky_font(
            &fit,
            ceiling,
            min,
            (max_width, max_height),
            font_size_of(label),
        )
    };
    let height = if blank {
        base_height
    } else {
        base_height.max(fitted.height + STICKY_NOTE_BODY_INSET_Y)
    };
    let container = place(height);
    let mut text = label.clone();
    text.original_text = Some(source);
    text.text = Some(fitted.text);
    text.font_size = Some(fitted.font_size);
    text.base_font_size = Some(ceiling);
    text.width = fitted.width;
    text.height = fitted.height;
    text.angle = container.angle;
    let at = bound_text_position(&container, &text);
    text.x = at.x;
    text.y = at.y;
    StickyLaid {
        container,
        text: Some(text),
    }
}
