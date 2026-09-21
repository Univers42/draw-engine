//! Maps a [`DrawElement`] onto rough.js options, transcribed from Excalidraw's
//! `generateRoughOptions` (`packages/element/src/shape.ts`).
//!
//! This is the join between our element model and the rough port. Getting it wrong
//! produces shapes that are *valid* rough output and still look nothing like
//! Excalidraw, which is why it is transcribed rather than approximated.

use draw_rough::{FillStyle as RoughFill, Options};

use crate::scene::element::{DrawElement, DrawElementType, FillStyle, StrokeStyle};

/// `ROUGHNESS` from `packages/common/src/constants.ts`.
pub const ROUGHNESS_ARCHITECT: f64 = 0.0;
pub const ROUGHNESS_ARTIST: f64 = 1.0;
pub const ROUGHNESS_CARTOONIST: f64 = 2.0;

/// `getDashArrayDashed(strokeWidth)`
pub fn dash_array_dashed(stroke_width: f64) -> [f64; 2] {
    [8.0, 8.0 + stroke_width]
}

/// `getDashArrayDotted(strokeWidth)`
pub fn dash_array_dotted(stroke_width: f64) -> [f64; 2] {
    [1.5, 6.0 + stroke_width]
}

/// The dash pattern a stroke style implies, in the units Canvas2D's `setLineDash` wants.
pub fn dash_array(element: &DrawElement) -> Option<[f64; 2]> {
    match element.stroke_style {
        StrokeStyle::Solid => None,
        StrokeStyle::Dashed => Some(dash_array_dashed(element.stroke_width)),
        StrokeStyle::Dotted => Some(dash_array_dotted(element.stroke_width)),
    }
}

fn is_linear(kind: DrawElementType) -> bool {
    matches!(kind, DrawElementType::Line | DrawElementType::Arrow)
}

/// `canChangeRoundness(type)` — which element types have a roundness control at all.
fn can_change_roundness(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle
            | DrawElementType::Diamond
            | DrawElementType::Image
            | DrawElementType::Embed
    )
}

/// `adjustRoughness(element)`
///
/// Small shapes get their roughness scaled **down**. Without this a 12px square comes
/// out as an unreadable scribble, because rough's jitter is an absolute offset and does
/// not shrink with the shape. This is one of the least obvious reasons a naive port
/// looks wrong.
pub fn adjust_roughness(element: &DrawElement) -> f64 {
    let roughness = element.roughness;
    let max_size = element.width.max(element.height);
    let min_size = element.width.min(element.height);

    let both_sides_big = min_size >= 20.0 && max_size >= 50.0;
    let round_and_not_tiny =
        min_size >= 15.0 && element.roundness.is_some() && can_change_roundness(element.kind);
    let long_linear = is_linear(element.kind) && max_size >= 50.0;

    if both_sides_big || round_and_not_tiny || long_linear {
        return roughness;
    }

    (roughness / if max_size < 10.0 { 3.0 } else { 2.0 }).min(2.5)
}

fn rough_fill_style(style: FillStyle) -> RoughFill {
    match style {
        FillStyle::Hachure => RoughFill::Hachure,
        FillStyle::CrossHatch => RoughFill::CrossHatch,
        FillStyle::Solid => RoughFill::Solid,
        FillStyle::Zigzag => RoughFill::ZigZag,
    }
}

/// `isTransparent(color)` — Excalidraw treats these three spellings as "no fill".
pub fn is_transparent(color: &str) -> bool {
    color == "transparent" || color == "#00000000" || color.is_empty()
}

/// `generateRoughOptions(element, continuousPath)`
///
/// `continuous_path` is set for the shapes drawn as a single SVG path — rounded
/// rectangles and diamonds, and elbow arrows. It forces `preserveVertices`, so the
/// segments actually meet at the corners instead of each being jittered independently
/// and leaving visible gaps.
pub fn generate_rough_options(element: &DrawElement, continuous_path: bool) -> Options {
    let non_solid = element.stroke_style != StrokeStyle::Solid;

    let mut options = Options {
        seed: element.seed as i32,
        // For non-solid strokes rough's second pass makes dashes overlay each other, so
        // Excalidraw disables it and widens the stroke slightly to compensate.
        disable_multi_stroke: non_solid,
        stroke_width: if non_solid {
            element.stroke_width + 0.5
        } else {
            element.stroke_width
        },
        // Pinned explicitly because rough derives both from strokeWidth when they are
        // left at their sentinel, and the widened stroke above would then change the
        // fill as a side effect.
        fill_weight: element.stroke_width / 2.0,
        hachure_gap: element.stroke_width * 4.0,
        roughness: adjust_roughness(element),
        preserve_vertices: continuous_path || element.roughness < ROUGHNESS_CARTOONIST,
        ..Default::default()
    };

    match element.kind {
        DrawElementType::Rectangle
        | DrawElementType::Diamond
        | DrawElementType::Ellipse
        | DrawElementType::Image
        | DrawElementType::Frame
        | DrawElementType::Embed => {
            options.fill_style = rough_fill_style(element.fill_style);
            options.filled = !is_transparent(&element.background_color);
            if element.kind == DrawElementType::Ellipse {
                // An ellipse is the one shape Excalidraw asks rough not to wobble the
                // radius of, or it stops reading as a circle.
                options.curve_fitting = 1.0;
            }
        }
        DrawElementType::Line | DrawElementType::Freedraw => {
            // Only a closed path can be filled.
            if is_path_a_loop(element) {
                options.fill_style = rough_fill_style(element.fill_style);
                options.filled = !is_transparent(&element.background_color);
            }
        }
        // Arrows are never filled, whatever the background is set to.
        DrawElementType::Arrow | DrawElementType::Text => {}
    }

    options
}

/// `isPathALoop(points)` — whether the first and last point are close enough that the
/// path reads as closed, and can therefore take a fill.
///
/// Excalidraw uses a fixed threshold of `LINE_CONFIRM_THRESHOLD` (8px) scaled by zoom;
/// the zoom term only matters while drawing, so the static form is used here.
pub fn is_path_a_loop(element: &DrawElement) -> bool {
    const LINE_CONFIRM_THRESHOLD: f64 = 8.0;

    let Some(points) = element.points.as_ref() else {
        return false;
    };
    if points.len() < 3 {
        return false;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    let dx = first[0] - last[0];
    let dy = first[1] - last[1];
    (dx * dx + dy * dy).sqrt() <= LINE_CONFIRM_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::element::{create_element, DrawElementStyle, Geometry};

    fn element(kind: DrawElementType, w: f64, h: f64) -> DrawElement {
        let mut e = create_element(
            kind,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
            },
            DrawElementStyle::default(),
            0.0,
        );
        // Pinned so shape output is reproducible across runs; `create_element` seeds
        // from a counter that the test order would otherwise leak into.
        e.seed = 12345;
        e
    }

    #[test]
    fn roughness_is_scaled_down_for_small_shapes_only() {
        // Both sides comfortably big: untouched.
        let big = element(DrawElementType::Rectangle, 100.0, 60.0);
        assert_eq!(adjust_roughness(&big), big.roughness);

        // Tiny: halved, or divided by three below 10px.
        let mut small = element(DrawElementType::Rectangle, 12.0, 12.0);
        small.roundness = None;
        small.roughness = 2.0;
        assert_eq!(adjust_roughness(&small), 1.0);

        let mut micro = element(DrawElementType::Rectangle, 8.0, 8.0);
        micro.roundness = None;
        micro.roughness = 2.0;
        assert!((adjust_roughness(&micro) - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_long_line_keeps_its_roughness_despite_being_thin() {
        let mut line = element(DrawElementType::Line, 200.0, 2.0);
        line.roughness = 2.0;
        assert_eq!(adjust_roughness(&line), 2.0);
    }

    #[test]
    fn transparent_background_means_unfilled() {
        let mut e = element(DrawElementType::Rectangle, 100.0, 60.0);
        e.background_color = "transparent".into();
        assert!(!generate_rough_options(&e, false).filled);

        e.background_color = "#ffc9c9".into();
        assert!(generate_rough_options(&e, false).filled);
    }

    #[test]
    fn arrows_never_fill() {
        let mut e = element(DrawElementType::Arrow, 100.0, 60.0);
        e.background_color = "#ffc9c9".into();
        assert!(!generate_rough_options(&e, false).filled);
    }

    /// A widened non-solid stroke must not silently change the hachure spacing.
    #[test]
    fn dashed_strokes_widen_without_disturbing_the_fill() {
        let mut e = element(DrawElementType::Rectangle, 100.0, 60.0);
        e.stroke_width = 2.0;
        e.stroke_style = StrokeStyle::Dashed;

        let o = generate_rough_options(&e, false);
        assert_eq!(o.stroke_width, 2.5);
        assert!(o.disable_multi_stroke);
        assert_eq!(
            o.hachure_gap, 8.0,
            "derived from the element width, not the widened one"
        );
        assert_eq!(o.fill_weight, 1.0);
    }
}
