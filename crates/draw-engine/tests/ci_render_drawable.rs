//! Every element the app can produce must be drawable.
//!
//! Not *drawn correctly* — that is what the rough oracle and the op snapshots are for.
//! Only that asking the renderer for the shape does not panic. In a browser this is not
//! a subtle distinction: a panic inside WASM aborts the module, so the canvas stops
//! responding and stays that way until the page is reloaded, with no error the user can
//! act on and nothing in the scene to explain it.
//!
//! The gap this closes is structural. The engine tests paint through a `NoopPainter`
//! that never builds a shape, so every test in the suite could be green over an element
//! the renderer cannot draw — and one was. A three-point line given a background in the
//! inspector is a *filled curve*, and rough's pattern fill for a curve was a `todo!()`
//! left on the reasoning that nothing here produced one.
//!
//! `element_drawable` is pure, so this costs nothing and needs no browser.

use draw_engine::render::shape::element_drawable;
use draw_engine::*;

const FILL_STYLES: [FillStyle; 4] = [
    FillStyle::Hachure,
    FillStyle::CrossHatch,
    FillStyle::Solid,
    FillStyle::Zigzag,
];

/// Every kind, including the ones with no drawable — `None` is a fine answer.
const KINDS: [DrawElementType; 10] = [
    DrawElementType::Rectangle,
    DrawElementType::Diamond,
    DrawElementType::Ellipse,
    DrawElementType::Line,
    DrawElementType::Arrow,
    DrawElementType::Freedraw,
    DrawElementType::Text,
    DrawElementType::Image,
    DrawElementType::Frame,
    DrawElementType::Embed,
];

fn base(kind: DrawElementType, width: f64, height: f64) -> DrawElement {
    create_element_default(
        kind,
        Geometry {
            x: 0.0,
            y: 0.0,
            width,
            height,
        },
    )
}

#[test]
fn every_kind_is_drawable_at_every_fill_style_and_roundness() {
    for kind in KINDS {
        for fill_style in FILL_STYLES {
            for rounded in [true, false] {
                for background in ["transparent", "#ffec99"] {
                    let mut element = base(kind, 120.0, 90.0);
                    element.fill_style = fill_style;
                    element.background_color = background.into();
                    element.roundness = if rounded { Some(8.0) } else { None };
                    if matches!(kind, DrawElementType::Line | DrawElementType::Arrow) {
                        element.points =
                            Some(vec![[0.0, 0.0], [120.0, 0.0], [60.0, 90.0], [0.0, 0.0]]);
                    }
                    let _ = element_drawable(&element);
                }
            }
        }
    }
}

#[test]
fn a_hand_drawn_polyline_given_a_background_is_drawable() {
    // The exact gesture that froze the board: draw a line with three or more points, then
    // pick a background in the inspector. Three points make it a curve, and the default
    // fill style is hachure, so this is the one combination rough refused to build.
    let mut element = base(DrawElementType::Line, 100.0, 80.0);
    element.points = Some(vec![[0.0, 0.0], [100.0, 0.0], [50.0, 80.0], [0.0, 0.0]]);
    element.background_color = "#ffec99".into();
    assert!(
        element.roundness.is_some(),
        "a line is rounded by default, which is what makes it a curve"
    );
    assert_eq!(element.fill_style, FillStyle::Hachure);

    let drawable = element_drawable(&element).expect("a filled line has a shape");
    assert!(
        drawable.sets.len() >= 2,
        "a filled curve should carry a fill set as well as its outline"
    );
}

#[test]
fn a_degenerate_element_is_drawable() {
    // Zero extents, one point, no points. These arrive from a click that never became a
    // drag, from a paste, and from a scene written by an older version — all of which
    // reach the renderer before anything has a chance to tidy them up.
    for kind in KINDS {
        for (width, height) in [(0.0, 0.0), (0.0, 50.0), (50.0, 0.0), (-40.0, -30.0)] {
            for points in [None, Some(vec![[0.0, 0.0]]), Some(Vec::new())] {
                let mut element = base(kind, width, height);
                element.background_color = "#ffec99".into();
                element.points = points;
                let _ = element_drawable(&element);
            }
        }
    }
}
