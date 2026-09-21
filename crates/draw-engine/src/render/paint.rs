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
