//! The font families a text can be drawn in: what CSS names them, and the metrics its
//! lines are placed with.
//!
//! One table, read by the measurer, the painter, the SVG exporter and the editor, so a
//! text is measured in the face it is drawn in. The ids, names and fallbacks are
//! Excalidraw's `FONT_FAMILY` / `getFontFamilyFallbacks`
//! (`packages/common/src/constants.ts@1118751f:137-194`), the metrics its `FONT_METADATA`
//! (`packages/common/src/font-metadata.ts@1118751f:35-134`).
//!
//! A text with no family — every text saved before families existed — is drawn with
//! [`LEGACY_CSS`], the system stack it always was.

use super::FontKey;

/// One family: its id, its CSS stack (name first, then the oracle's fallbacks) and the
/// head/hhea metrics its baseline is placed with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Family {
    pub id: u8,
    pub css: &'static str,
    /// Unitless, the family's default line height (`getLineHeight`).
    pub line_height: f64,
    pub units_per_em: f64,
    pub ascender: f64,
    pub descender: f64,
}

/// Excalifont, the family new text is written in (`DEFAULT_FONT_FAMILY`,
/// `constants.ts@1118751f:265`).
pub const DEFAULT_FONT_FAMILY: u8 = 5;

/// The system stack text was drawn with before families existed.
pub const LEGACY_CSS: &str = "system-ui, -apple-system, Segoe UI, Roboto, sans-serif";

/// Every family the engine draws with. Id 4 is unused, as in the oracle; 10 (Assistant)
/// is the oracle's UI font, never a text's.
pub const FAMILIES: [Family; 8] = [
    Family {
        id: 1,
        css: "Virgil, sans-serif, Segoe UI Emoji",
        line_height: 1.25,
        units_per_em: 1000.0,
        ascender: 886.0,
        descender: -374.0,
    },
    Family {
        id: 2,
        css: "Helvetica, sans-serif, Segoe UI Emoji",
        line_height: 1.15,
        units_per_em: 2048.0,
        ascender: 1577.0,
        descender: -471.0,
    },
    Family {
        id: 3,
        css: "Cascadia, monospace, Segoe UI Emoji",
        line_height: 1.2,
        units_per_em: 2048.0,
        ascender: 1900.0,
        descender: -480.0,
    },
    Family {
        id: 5,
        css: "Excalifont, Xiaolai, sans-serif, Segoe UI Emoji",
        line_height: 1.25,
        units_per_em: 1000.0,
        ascender: 886.0,
        descender: -374.0,
    },
    Family {
        id: 6,
        css: "Nunito, sans-serif, Segoe UI Emoji",
        line_height: 1.25,
        units_per_em: 1000.0,
        ascender: 1011.0,
        descender: -353.0,
    },
    Family {
        id: 7,
        css: "Lilita One, sans-serif, Segoe UI Emoji",
        line_height: 1.15,
        units_per_em: 1000.0,
        ascender: 923.0,
        descender: -220.0,
    },
    Family {
        id: 8,
        css: "Comic Shanns, monospace, Segoe UI Emoji",
        line_height: 1.25,
        units_per_em: 1000.0,
        ascender: 750.0,
        descender: -250.0,
    },
    Family {
        id: 9,
        css: "Liberation Sans, sans-serif, Segoe UI Emoji",
        line_height: 1.15,
        units_per_em: 2048.0,
        ascender: 1854.0,
        descender: -434.0,
    },
];

/// The family with this id, if the engine draws with it.
pub fn family(id: u8) -> Option<&'static Family> {
    FAMILIES.iter().find(|family| family.id == id)
}

/// The CSS `font-family` value for a resolved family — `None` is the legacy stack.
pub fn css_stack(family_id: Option<u8>) -> &'static str {
    family_id
        .and_then(family)
        .map_or(LEGACY_CSS, |family| family.css)
}

/// The Canvas2D / CSS `font` shorthand for a measured font (`getFontString`,
/// `packages/common/src/utils.ts@1118751f:141-147`).
pub fn font_string(font: FontKey) -> String {
    let family = (font.family() != FontKey::LEGACY).then_some(font.family());
    format!("{}px {}", font.size(), css_stack(family))
}

/// Where the first line's alphabetic baseline sits below the top of its line box
/// (`getVerticalOffset`, `font-metadata.ts@1118751f:155-170`): half the line gap above
/// the ascender. A family the table does not hold is placed with Excalifont's metrics,
/// as the oracle does.
pub fn vertical_offset(family_id: u8, font_size: f64, line_height_px: f64) -> f64 {
    let metrics = family(family_id)
        .or_else(|| family(DEFAULT_FONT_FAMILY))
        .copied()
        .unwrap_or(FAMILIES[3]);
    let em = font_size / metrics.units_per_em;
    let line_gap = (line_height_px - em * metrics.ascender + em * metrics.descender) / 2.0;
    em * metrics.ascender + line_gap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_excalifont_and_every_id_is_known_once() {
        assert_eq!(family(DEFAULT_FONT_FAMILY).map(|f| f.id), Some(5));
        for (i, a) in FAMILIES.iter().enumerate() {
            assert!(FAMILIES[i + 1..].iter().all(|b| b.id != a.id));
        }
    }

    #[test]
    fn the_baseline_is_the_oracles() {
        // Excalifont at 20px, line height 1.25: em 0.02, ascent 17.72, descent 7.48,
        // gap (25 - 17.72 - 7.48) / 2 = -0.1 → 17.62.
        assert!((vertical_offset(5, 20.0, 25.0) - 17.62).abs() < 1e-9);
        // An unknown family falls back to Excalifont's metrics.
        assert_eq!(
            vertical_offset(42, 20.0, 25.0),
            vertical_offset(5, 20.0, 25.0)
        );
    }

    #[test]
    fn a_font_string_names_the_stack() {
        assert_eq!(
            font_string(FontKey::new(5, 20.0)),
            "20px Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"
        );
        assert_eq!(
            font_string(FontKey::legacy(16.0)),
            format!("16px {LEGACY_CSS}")
        );
    }
}
