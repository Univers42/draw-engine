//! The colours one export paints with: the paper behind it, and the dark-theme filter.
//!
//! Excalidraw used to invert a dark canvas with a CSS `filter: invert(93%)
//! hue-rotate(180deg)` and then moved the arithmetic into JavaScript, because a browser
//! compositing in software cannot afford it
//! (`renderer/interactiveScene.ts@1118751f:116-120`). The same filter is what
//! `exportWithDarkMode` puts over an export: not a second palette of dark values, but every
//! colour the renderer writes put through one function
//! (`staticSvgScene.ts@1118751f:207-212, 528-534, 764-769, 821-826`; the canvas at
//! `renderElement.ts@1118751f:447-450, 462-465` and `renderer/helpers.ts@1118751f:115-119`).
//!
//! **So there is exactly one thing to port, and it is a colour function.** The dark theme
//! itself is [`crate::render::dark_theme`] and has always been here; what an *export* does
//! with it is [`ExportPalette`], and that is the only place an export's colours are
//! resolved — the SVG writer and the canvas painter both go through it, so a second
//! definition of "the dark colour of this stroke" cannot appear without this file changing.

use std::borrow::Cow;

use crate::render::arrowheads::{arrowhead_fill_color, FillRole};
use crate::render::DrawTheme;

/// Excalidraw's `DARK_MODE_FILTER_INVERT_PERCENT` (`colors.ts@1118751f:16`).
const INVERT_PERCENT: f64 = 93.0;

/// Excalidraw's `DARK_MODE_FILTER_HUE_ROTATE_DEGREES` (`colors.ts@1118751f:17`).
const HUE_ROTATE_DEGREES: f64 = 180.0;

/// The colours this export paints with: the paper behind everything, and whether every
/// colour goes through the dark filter.
///
/// Built from the theme — which is the one definition of a background colour there is — and
/// from the options, so a caller cannot invent a colour of its own and cannot leave the dark
/// filter off in one writer and on in another.
#[derive(Clone, Copy, Debug)]
pub struct ExportPalette<'a> {
    theme: &'a DrawTheme,
    /// The oracle's `exportBackground`: whether there is paper at all.
    background: bool,
    /// The oracle's `exportWithDarkMode`, whose default is **false**
    /// (`appState.ts@1118751f:72`).
    dark: bool,
}

impl<'a> ExportPalette<'a> {
    /// The palette an export painted in `theme` is drawn with.
    pub fn of(theme: &'a DrawTheme, options: &crate::export::ExportOptions) -> Self {
        Self {
            theme,
            background: options.background,
            dark: options.dark_mode,
        }
    }

    /// The paper behind everything, or `None` when the export has none.
    ///
    /// `exportBackground && viewBackgroundColor`
    /// (`packages/excalidraw/scene/export.ts@1118751f:458`), both halves: a colour nobody
    /// chose is not a colour, and a `<rect fill="">` is a rect that renders as black in some
    /// readers and as nothing in others.
    pub fn paper(&self) -> Option<String> {
        self.paper_color()
            .map(|paper| self.color(paper).into_owned())
    }

    /// The theme's own background colour, whether or not it ends up on the page.
    ///
    /// Separate from [`Self::paper`] because an outline arrowhead is punched through with
    /// the colour that will be behind it, and that question is asked even when nothing is
    /// behind it.
    pub fn paper_color(&self) -> Option<&str> {
        let background = self.theme.background.trim();
        if !self.background || background.is_empty() {
            None
        } else {
            Some(background)
        }
    }

    /// One colour as this export paints it: the oracle's
    /// `applyDarkModeFilter(color, theme === THEME.DARK)`.
    ///
    /// Borrowed and unchanged with the filter off, which is what keeps the on-screen canvas
    /// from paying for a string it will not use.
    pub fn color<'c>(&self, color: &'c str) -> Cow<'c, str> {
        if self.dark {
            Cow::Owned(dark_mode_filter(color))
        } else {
            Cow::Borrowed(color)
        }
    }

    /// [`Self::color`] for a fill, where "no fill" stays no fill.
    ///
    /// A shape with no fill carries `transparent`, and the export writes `fill="none"` for
    /// it. `transparent` does have a dark value (`#ededed00`, which the oracle's own tests
    /// pin at `colors.test.ts:1118751f:74-76`) but it is not a colour to put on a shape, so
    /// the two spellings of "nothing" are answered before the filter rather than after.
    pub fn fill<'c>(&self, color: &'c str) -> Cow<'c, str> {
        if color.is_empty() || color == "transparent" {
            Cow::Borrowed("none")
        } else {
            self.color(color)
        }
    }

    /// The colour an arrowhead is filled or punched with, in this export's colours.
    ///
    /// [`arrowhead_fill_color`] is a *selector* — solid heads take the stroke, outline heads
    /// the paper they are punched through — so both candidates are resolved here and the
    /// choice is still that function's.
    pub fn arrowhead_fill(&self, role: FillRole, stroke: &str) -> String {
        let stroke = self.color(stroke).into_owned();
        let paper = self.color(self.paper_color().unwrap_or("")).into_owned();
        arrowhead_fill_color(role, &stroke, &paper).to_string()
    }
}

/// A colour put through the oracle's dark-mode filter: `invert(93%)` then
/// `hue-rotate(180deg)`, **in that order**, keeping the alpha
/// (`packages/common/src/colors.ts@1118751f:86-119`).
///
/// ponytail: no named colours. `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, `rgb()`, `rgba()`
/// and `transparent` are what an element and a theme carry; anything else comes back
/// **unchanged**, so a colour this cannot read is a colour it does not touch. The oracle
/// reads names through tinycolor (`colors.ts:1, 99`); upgrade path is the CSS named-colour
/// table, when a named colour ever reaches here.
pub fn dark_mode_filter(color: &str) -> String {
    let Some((r, g, b, alpha)) = parse_css_color(color) else {
        return color.to_string();
    };
    let (ir, ig, ib) = invert(r, g, b);
    let (rr, rg, rb) = hue_rotate(ir, ig, ib);
    rgb_to_hex(rr, rg, rb, alpha)
}

/// `c * (1 - p) + (255 - c) * p` per channel, rounded and clamped
/// (`colors.ts@1118751f:61-81`).
fn invert(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let p = (INVERT_PERCENT / 100.0).clamp(0.0, 1.0);
    let one = |c: u8| (f64::from(c) * (1.0 - p) + (255.0 - f64::from(c)) * p).round() as u8;
    (one(r), one(g), one(b))
}

/// The CSS `hue-rotate` matrix, on channels **normalised to 0–1** first and clamped back to
/// 0–1 before the 255 (`colors.ts@1118751f:19-57`).
///
/// The normalisation is not tidiness: feeding 0–255 straight into the matrix and clamping to
/// `[0, 1]` afterwards saturates every channel above 1 to 255, and `#ff0000` comes out
/// `#ffffff` instead of the `#ff9090` the oracle's own test pins
/// (`colors.test.ts@1118751f:30-32`).
fn hue_rotate(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let (c, s) = (
        HUE_ROTATE_DEGREES.to_radians().cos(),
        HUE_ROTATE_DEGREES.to_radians().sin(),
    );
    let rows = [
        [
            0.213 + c * 0.787 - s * 0.213,
            0.715 - c * 0.715 - s * 0.715,
            0.072 - c * 0.072 + s * 0.928,
        ],
        [
            0.213 - c * 0.213 + s * 0.143,
            0.715 + c * 0.285 + s * 0.14,
            0.072 - c * 0.072 - s * 0.283,
        ],
        [
            0.213 - c * 0.213 - s * 0.787,
            0.715 - c * 0.715 + s * 0.715,
            0.072 + c * 0.928 + s * 0.072,
        ],
    ];
    let out = [rows[0], rows[1], rows[2]]
        .map(|m| ((r * m[0] + g * m[1] + b * m[2]).clamp(0.0, 1.0) * 255.0).round() as u8);
    (out[0], out[1], out[2])
}

/// `#rrggbb`, or `#rrggbbaa` for anything not fully opaque — `rgbToHex`
/// (`colors.ts@1118751f:345-361`).
fn rgb_to_hex(r: u8, g: u8, b: u8, alpha: f64) -> String {
    let hex = format!("#{r:02x}{g:02x}{b:02x}");
    if alpha >= 1.0 {
        return hex;
    }
    format!("{hex}{:02x}", (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A CSS colour as channels and an alpha, or `None` for one this cannot read.
fn parse_css_color(color: &str) -> Option<(u8, u8, u8, f64)> {
    let value = color.trim();
    if value == "transparent" {
        return Some((0, 0, 0, 0.0));
    }
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex(hex);
    }
    let rest = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = rest
        .split(&[',', '/', ' '][..])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let channel = |index: usize| -> Option<u8> {
        let part = parts.get(index)?;
        let percent = part.ends_with('%');
        let scale = if percent { 255.0 / 100.0 } else { 1.0 };
        let number: f64 = part.trim_end_matches('%').parse().ok()?;
        Some((number * scale).round().clamp(0.0, 255.0) as u8)
    };
    let alpha = match parts.get(3) {
        Some(part) => part.parse::<f64>().ok()?.clamp(0.0, 1.0),
        None => 1.0,
    };
    Some((channel(0)?, channel(1)?, channel(2)?, alpha))
}

/// `#RGB`, `#RGBA`, `#RRGGBB` and `#RRGGBBAA` — the four hex shapes the oracle's own tests
/// pin (`colors.test.ts@1118751f:78-81, 85-99`).
fn parse_hex(hex: &str) -> Option<(u8, u8, u8, f64)> {
    let nibble = |i: usize| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok();
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let alpha = |a: u8| f64::from(a) / 255.0;
    match hex.len() {
        3 => Some((nibble(0)? * 17, nibble(1)? * 17, nibble(2)? * 17, 1.0)),
        4 => Some((
            nibble(0)? * 17,
            nibble(1)? * 17,
            nibble(2)? * 17,
            alpha(nibble(3)? * 17),
        )),
        6 => Some((byte(0)?, byte(2)?, byte(4)?, 1.0)),
        8 => Some((byte(0)?, byte(2)?, byte(4)?, alpha(byte(6)?))),
        _ => None,
    }
}
