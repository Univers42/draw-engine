use crate::scene::element::DrawElementType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawTheme {
    /// The colour a bindable element's outline is traced with while an arrow endpoint
    /// hovers it. Excalidraw's BINDING_HIGHLIGHT_RGB, verified by sampling their
    /// interactive canvas.
    #[serde(default = "default_binding_highlight")]
    pub binding_highlight: String,
    pub background: String,
    pub grid: String,
    pub accent: String,
}

/// Excalidraw's `BINDING_HIGHLIGHT_RGB`, light theme.
///
/// Taken from their source and confirmed by sampling excalidraw.com's interactive
/// canvas while an arrow endpoint hovered a shape — the pixels came back exactly
/// rgb(106, 189, 252).
fn default_binding_highlight() -> String {
    "rgb(106, 189, 252)".into()
}

pub fn light_theme() -> DrawTheme {
    DrawTheme {
        background: "#ffffff".into(),
        grid: "rgba(17, 17, 17, 0.06)".into(),
        // Excalidraw's primary. The chrome already used this; the engine did not, so
        // the selection frame was a different violet from the panels around it.
        accent: "#6965db".into(),
        binding_highlight: default_binding_highlight(),
    }
}

pub fn dark_theme() -> DrawTheme {
    DrawTheme {
        background: "#191919".into(),
        grid: "rgba(255, 255, 255, 0.06)".into(),
        accent: "#a8a5ff".into(),
        binding_highlight: "rgb(104, 182, 240)".into(),
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

/// The font stack every piece of text is drawn and measured with.
///
/// One definition, because measuring with a different font from the one you draw with is
/// how a text element ends up the wrong size. The painter, the measurer, the SVG exporter
/// and the host's editing overlay all resolve their font through [`font_string`].
///
/// Excalidraw ships Excalifont, Nunito and Comic Shanns and lets each element choose.
/// That needs the faces vendored, their licences checked individually, and measurement
/// gated on `document.fonts.ready` — measure before a webfont loads and every text
/// element is permanently mis-sized. Until then this is a single system stack, which at
/// least measures and draws identically.
pub const FONT_FAMILY: &str = "system-ui, -apple-system, Segoe UI, Roboto, sans-serif";

/// The CSS font shorthand for a given size, as Canvas2D wants it.
pub fn font_string(size: f64) -> String {
    format!("{size}px {FONT_FAMILY}")
}

pub fn is_roughable(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle | DrawElementType::Diamond | DrawElementType::Ellipse
    )
}
