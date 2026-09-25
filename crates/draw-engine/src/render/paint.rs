use crate::scene::element::{DrawElement, DrawElementType, TextAlign};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawTheme {
    /// The colour a bindable element's outline is traced with while an arrow endpoint
    /// hovers it. Excalidraw's BINDING_HIGHLIGHT_RGB, verified by sampling their
    /// interactive canvas.
    // The host writes camelCase. Without the alias its value was dropped unread and the
    // light default drawn on both themes.
    #[serde(default = "default_binding_highlight", alias = "bindingHighlight")]
    pub binding_highlight: String,
    /// A side midpoint an arrow end is near but would not snap to yet. Excalidraw's
    /// `BINDING_MIDPOINT_COLOR` (`interactiveScene.ts:128-131`).
    #[serde(default = "default_binding_midpoint", alias = "bindingMidpoint")]
    pub binding_midpoint: String,
    pub background: String,
    pub grid: String,
    pub accent: String,
    /// The colour a frame's name is written in.
    ///
    /// Part of the theme rather than a constant because it is the one piece of frame
    /// chrome that sits on the canvas background instead of on the frame, so it is the
    /// one piece that has to change when the background does.
    #[serde(default = "default_frame_name_color", alias = "frameName")]
    pub frame_name: String,
}

/// Excalidraw's `FRAME_STYLE.nameColorLightTheme`.
fn default_frame_name_color() -> String {
    crate::scene::FRAME_NAME_COLOR_LIGHT.into()
}

/// Excalidraw's `BINDING_HIGHLIGHT_RGB`, light theme.
///
/// Taken from their source and confirmed by sampling excalidraw.com's interactive
/// canvas while an arrow endpoint hovered a shape — the pixels came back exactly
/// rgb(106, 189, 252).
fn default_binding_highlight() -> String {
    "rgb(106, 189, 252)".into()
}

fn default_binding_midpoint() -> String {
    "rgba(65, 65, 65, 0.5)".into()
}

pub fn light_theme() -> DrawTheme {
    DrawTheme {
        background: "#ffffff".into(),
        grid: "rgba(17, 17, 17, 0.06)".into(),
        // Excalidraw's primary. The chrome already used this; the engine did not, so
        // the selection frame was a different violet from the panels around it.
        accent: "#6965db".into(),
        binding_highlight: default_binding_highlight(),
        binding_midpoint: default_binding_midpoint(),
        frame_name: crate::scene::FRAME_NAME_COLOR_LIGHT.into(),
    }
}

pub fn dark_theme() -> DrawTheme {
    DrawTheme {
        background: "#191919".into(),
        grid: "rgba(255, 255, 255, 0.06)".into(),
        accent: "#a8a5ff".into(),
        binding_highlight: "rgb(104, 182, 240)".into(),
        binding_midpoint: "rgba(237, 237, 237, 0.8)".into(),
        frame_name: crate::scene::FRAME_NAME_COLOR_DARK.into(),
    }
}

/// Whether the canvas draws a grid, how coarse it is, and how often a line is emphasised.
///
/// The grid used to be unconditional and hard-coded to a 40px step, with every line the
/// same weight and nothing snapping to it — a grid you cannot turn off, cannot resize and
/// cannot align to is decoration rather than a tool, which is the wrong idea of what a
/// grid is for.
///
/// The defaults are Excalidraw's: off, 20 units, and every 5th line emphasised.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridSettings {
    /// Off by default. A grid is a mode you opt into, not the normal appearance of paper.
    pub enabled: bool,
    /// Spacing in **world** units, so the grid is a property of the drawing rather than
    /// of how far you happen to be zoomed in.
    pub size: f64,
    /// Every `step`-th line is drawn heavier, which is what makes a fine grid readable
    /// instead of a wash of identical lines. `1` disables the emphasis.
    pub step: u32,
    /// Whether drawing and dragging land on grid intersections.
    ///
    /// Separate from `enabled`, because seeing a grid and being held to it are different
    /// requests: a grid can be a visual reference you draw freely over.
    pub snap: bool,
}

/// Excalidraw's `DEFAULT_GRID_SIZE`.
pub const DEFAULT_GRID_SIZE: f64 = 20.0;
/// Excalidraw's `DEFAULT_GRID_STEP`.
pub const DEFAULT_GRID_STEP: u32 = 5;

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            size: DEFAULT_GRID_SIZE,
            step: DEFAULT_GRID_STEP,
            snap: true,
        }
    }
}

impl GridSettings {
    /// The spacing actually used, guarding against a zero or negative size reaching the
    /// renderer as an infinite loop or the snapper as a division by zero.
    pub fn effective_size(&self) -> f64 {
        if self.size.is_finite() && self.size >= 1.0 {
            self.size
        } else {
            DEFAULT_GRID_SIZE
        }
    }

    /// Rounds a point onto the nearest grid intersection.
    ///
    /// Excalidraw's `getGridPoint`: `round(v / size) * size`. Returns the point unchanged
    /// when the grid is off or not snapping, so callers can apply it unconditionally.
    pub fn snap_point(&self, x: f64, y: f64) -> (f64, f64) {
        if !self.enabled || !self.snap {
            return (x, y);
        }
        let size = self.effective_size();
        ((x / size).round() * size, (y / size).round() * size)
    }
}

pub const TEXT_LINE_HEIGHT: f64 = 1.25;

/// The font stack a text with no family is drawn and measured with — every text saved
/// before families existed. A text with one uses its family's stack
/// ([`crate::text::font`]); both go through [`crate::text::font::font_string`], so the
/// measurer, the painter, the SVG exporter and the editor resolve the same face.
pub const FONT_FAMILY: &str = crate::text::font::LEGACY_CSS;

/// The CSS font shorthand for a given size in the legacy stack, as Canvas2D wants it.
pub fn font_string(size: f64) -> String {
    crate::text::font::font_string(crate::text::FontKey::legacy(size))
}

/// What Canvas2D's `textAlign` must be set to for a given alignment.
///
/// Paired with [`text_anchor_x`], and only meaningful together: `fill_text(line, x)`
/// places the line's *anchor* at `x`, and which end of the line is the anchor is decided
/// by this setting. An x computed for one and drawn under another lands a whole
/// text-width out.
pub fn canvas_text_align(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

/// Where in the element's own box the anchor of a line goes, for a given alignment.
///
/// In element-local coordinates, so the caller has already applied the element
/// transform: 0 is its left edge and `width` its right.
pub fn text_anchor_x(align: TextAlign, width: f64) -> f64 {
    match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => width / 2.0,
        TextAlign::Right => width,
    }
}

/// Where the first line's baseline sits below the top of a text's box, and how far apart
/// its lines are — both in scene units, for [`text_baseline`]'s baseline setting.
///
/// A text in a family is placed as the oracle places it: `alphabetic`, the first
/// baseline half the line gap below the ascender (`getVerticalOffset`,
/// [`crate::text::font::vertical_offset`]). A text with none keeps the placement it has
/// always had — the top of the line on the top of the box — so a board saved before
/// families existed draws exactly as it did.
pub fn text_line_placement(element: &DrawElement) -> (f64, f64) {
    let font_size = crate::text::layout::font_size_of(element);
    let line_height = font_size * crate::scene::resolved_line_height(element);
    let first = crate::scene::resolved_font_family(element).map_or(0.0, |family| {
        crate::text::font::vertical_offset(family, font_size, line_height)
    });
    (first, line_height)
}

/// The Canvas2D `textBaseline` [`text_line_placement`] measures from.
pub fn text_baseline(element: &DrawElement) -> &'static str {
    if crate::scene::resolved_font_family(element).is_some() {
        "alphabetic"
    } else {
        "top"
    }
}

/// A line or arrow's label, when it has one: its stroke is cut away underneath it.
pub fn linear_label<'a>(
    linear: &DrawElement,
    lookup: impl Fn(&str) -> Option<&'a DrawElement>,
) -> Option<&'a DrawElement> {
    if !crate::scene::is_linear_element(linear) {
        return None;
    }
    lookup(linear.bound_text_id.as_deref()?)
        .filter(|label| !label.is_deleted && label.kind == DrawElementType::Text)
}

/// The hole an arrow's stroke leaves under its label: the label's box and
/// `BOUND_TEXT_PADDING` around it, as the oracle clips it out of the canvas
/// (`renderElement.ts@1118751f:784-812`) and masks it out of an SVG
/// (`staticSvgScene.ts@1118751f:404-470`). In scene units; an arrow's label never turns.
pub fn label_hole(label: &DrawElement) -> crate::scene::geometry::Rect {
    let pad = crate::text::layout::BOUND_TEXT_PADDING;
    crate::scene::geometry::Rect {
        x: label.x - pad,
        y: label.y - pad,
        width: label.width + pad * 2.0,
        height: label.height + pad * 2.0,
    }
}

/// The region [`label_hole`] is cut out of: a box that generously covers the line and its
/// heads at any size — its largest extent, plus 100, plus ten stroke widths, on every side
/// — as the oracle's clip and mask do (`renderElement.ts@1118751f:784-812`,
/// `staticSvgScene.ts@1118751f:404-470`).
pub fn label_cut_reach(linear: &DrawElement) -> crate::scene::geometry::Rect {
    let bounds = crate::scene::element_bounds(linear);
    let (width, height) = (bounds.max_x - bounds.min_x, bounds.max_y - bounds.min_y);
    let reach = width.max(height) + 100.0 + linear.stroke_width * 10.0;
    crate::scene::geometry::Rect {
        x: bounds.min_x - reach,
        y: bounds.min_y - reach,
        width: width + reach * 2.0,
        height: height + reach * 2.0,
    }
}

pub fn is_roughable(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle | DrawElementType::Diamond | DrawElementType::Ellipse
    )
}
