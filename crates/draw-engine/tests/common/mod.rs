#![allow(dead_code)]

use std::collections::HashSet;

use draw_engine::*;

/// A second implementation of the PNG container, written for the tests rather than for the
/// engine. See `png.rs` for why there are two.
///
/// Reached as `common::png::*` and **not** re-exported here: a glob this module exports is
/// an import every one of the eighty-odd test binaries would have to use, and the ones
/// that do not are an `unused_imports` error under `clippy -D warnings`.
pub mod png;

/// A second implementation of the SVG document, written for the tests rather than for the
/// engine. See `svg.rs` for why there are two.
pub mod svg;

pub const EPS: f64 = 1e-9;

pub fn assert_close(a: f64, b: f64) {
    assert!((a - b).abs() < EPS, "expected {a} ~= {b}");
}

/// [`assert_close`] with a note of what was being compared.
///
/// For the cases where a bare "expected a ~= b" would not say **which** half of a pair
/// failed, or which of a few hundred swept cases. The note comes last so a call reads as
/// the two numbers it compares and then what they mean.
pub fn assert_close_msg(a: f64, b: f64, why: impl std::fmt::Display) {
    assert!((a - b).abs() < EPS, "expected {a} ~= {b} ({why})");
}

pub fn assert_point_close(a: Point, b: Point) {
    assert_close(a.x, b.x);
    assert_close(a.y, b.y);
}

pub fn assert_rect_close(a: Rect, b: Rect) {
    assert_close(a.x, b.x);
    assert_close(a.y, b.y);
    assert_close(a.width, b.width);
    assert_close(a.height, b.height);
}

/// The oracle's eighth turn, a quarter of the circle.
///
/// A square turned through it spans `100 * √2`; a quarter turn would land back on the box
/// it started in and prove nothing.
pub const EIGHTH_TURN: f64 = std::f64::consts::FRAC_PI_4;

/// A frame of `size` at the origin, named so it is not a blank rename.
pub fn frame_at_origin(size: f64) -> DrawElement {
    let mut frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: size,
            height: size,
        },
    );
    frame.name = Some("Frame 1".to_string());
    frame
}

/// A `name="…"` attribute of an SVG's root element.
///
/// The root is the first tag in the document (`export/svg.rs` writes the root tag before
/// any element's own geometry), so its `width` and `height` are the export's own box and
/// not a shape's. Parsed rather than compared as a string: the value is an `f64` and its
/// printed form is Rust's to decide, not a test's to pin.
pub fn root_attribute(svg: &str, name: &str) -> f64 {
    let tag = svg.split_once('>').expect("an svg root element").0;
    let marker = format!("{name}=\"");
    let start = tag
        .find(&marker)
        .unwrap_or_else(|| panic!("the svg root should carry a {name}"))
        + marker.len();
    let rest = &tag[start..];
    let end = rest
        .find('"')
        .unwrap_or_else(|| panic!("the {name} attribute should be closed"));
    rest[..end]
        .parse()
        .unwrap_or_else(|_| panic!("the {name} attribute should be a number"))
}

/// The `width` attribute of an SVG's root element — the export's own box. See
/// [`root_attribute`].
pub fn svg_width(svg: &str) -> f64 {
    root_attribute(svg, "width")
}

/// The `height` attribute of an SVG's root element. See [`svg_width`].
pub fn svg_height(svg: &str) -> f64 {
    root_attribute(svg, "height")
}

/// How many times an element's stroke colour appears in an SVG — how many drawn elements
/// carry that colour.
///
/// **Colours, not ids or tag counts**, and both alternatives are wrong. The SVG carries no
/// element ids: every `id` in it belongs to the export's own machinery, the
/// outline-arrowhead `mask` (`export/svg.rs:173`) and the frame `clipPath` (`:390`). And
/// counting shape tags counts the background `<rect>` (`export.ts@1118751f`'s paper, drawn
/// first) as an element, so "three rects" does not mean "three elements".
///
/// Give each element its own stroke colour with [`tinted`] and this reads the picture
/// directly: which elements reached it, and how many times.
pub fn svg_uses(svg: &str, stroke_color: &str) -> usize {
    svg.matches(&format!("stroke=\"{stroke_color}\"")).count()
}

/// `element` with its own stroke colour, so a test can ask which elements a picture holds.
pub fn tinted(element: DrawElement, stroke_color: &str) -> DrawElement {
    let mut element = element;
    element.stroke_color = stroke_color.to_string();
    element
}

/// A shape with a background, so its whole interior is a hit target.
///
/// The default style is a transparent fill, and a transparent shape is hit only on its
/// outline — so a test about *shape* maths (is this point inside the ellipse or merely
/// inside its box?) has to say it means a solid shape, or it ends up asserting the fill
/// rule by accident.
pub fn filled(mut element: DrawElement) -> DrawElement {
    element.background_color = "#ffec99".into();
    element
}

pub fn box_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x,
            y,
            width,
            height,
        },
    )
}

pub fn ellipse_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Ellipse,
        Geometry {
            x,
            y,
            width,
            height,
        },
    )
}

pub fn diamond_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Diamond,
        Geometry {
            x,
            y,
            width,
            height,
        },
    )
}

/// A figure element with the given kind/sides/ratio, at the given box. `sides`/`ratio`
/// absent means the kind's own default, exactly as [`FigureParams`] documents.
pub fn figure_at(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    kind: FigureKind,
    sides: Option<u8>,
    ratio: Option<f64>,
) -> DrawElement {
    let mut element = create_element_default(
        DrawElementType::Figure,
        Geometry {
            x,
            y,
            width,
            height,
        },
    );
    element.figure = Some(FigureParams { kind, sides, ratio });
    element
}

pub fn text_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    let mut element = create_element_default(
        DrawElementType::Text,
        Geometry {
            x,
            y,
            width,
            height,
        },
    );
    element.text = Some(String::new());
    element.font_size = Some(20.0);
    element
}

pub fn connector(x1: f64, y1: f64, x2: f64, y2: f64, kind: DrawElementType) -> DrawElement {
    let mut element = create_element_default(
        kind,
        Geometry {
            x: x1,
            y: y1,
            width: x2 - x1,
            height: y2 - y1,
        },
    );
    element.points = Some(vec![[0.0, 0.0], [x2 - x1, y2 - y1]]);
    element
}

pub fn ids(elements: &[DrawElement]) -> HashSet<String> {
    elements.iter().map(|element| element.id.clone()).collect()
}

pub fn opposite_handle(handle: HandleKind) -> HandleKind {
    match handle {
        HandleKind::Nw => HandleKind::Se,
        HandleKind::N => HandleKind::S,
        HandleKind::Ne => HandleKind::Sw,
        HandleKind::E => HandleKind::W,
        HandleKind::Se => HandleKind::Nw,
        HandleKind::S => HandleKind::N,
        HandleKind::Sw => HandleKind::Ne,
        HandleKind::W => HandleKind::E,
        HandleKind::Rotate => HandleKind::Rotate,
    }
}

/// Where a handle sits, matching what the engine's `selection_handles` computes.
///
/// The half-extents are **absolute**. A negative extent means the element is mirrored,
/// not that it is inside out: it occupies the same box and its north-west handle is still
/// visually north-west. Using the signed value here put the handles on mirrored positions
/// and made this helper disagree with the code it is used to check.
pub fn handle_world(element: &DrawElement, handle: HandleKind) -> Point {
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let local = handle_local_point(
        handle,
        element.width.abs() / 2.0,
        element.height.abs() / 2.0,
    );
    let cos = element.angle.cos();
    let sin = element.angle.sin();
    Point {
        x: cx + (local.x * cos - local.y * sin),
        y: cy + (local.x * sin + local.y * cos),
    }
}

pub fn engine_with_scene(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_scene(Scene::new(elements));
    engine
}

pub fn engine_with_measure(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = engine_with_scene(elements);
    engine.set_measure_text(measure_text);
    engine
}

pub fn measure_text(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.len() as f64 * font_size * 0.5 + 4.0)
        .fold(0.0, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * TEXT_LINE_HEIGHT).max(font_size),
    )
}

pub fn stroke_patch(color: &str) -> DrawElementStylePatch {
    DrawElementStylePatch {
        stroke_color: Some(color.to_string()),
        ..Default::default()
    }
}

pub fn deleted_count(elements: &[DrawElement]) -> usize {
    elements.iter().filter(|element| element.is_deleted).count()
}

/// `scene_to_svg` over a borrowed slice, with the framing built from a bounds and a
/// padding.
///
/// The tests that care about *markup* — a fill, a path, a text run — do not care about how
/// the box was decided, and writing `ExportFrame::for_bounds` at every one of them says
/// `for_bounds` is the thing under test. This is the spelling for "these bounds, this
/// padding"; a test about the framing itself should use `ExportFrame` directly.
pub fn svg_at(elements: &[DrawElement], bounds: WorldBounds, padding: f64) -> String {
    svg_at_over(elements, bounds, padding, "#ffffff")
}

/// [`svg_at`] with the background colour spelled out, for the tests that are about it.
///
/// A separate function rather than a fourth argument on `svg_at`: the background is what the
/// `<rect>` paints and what an outline arrowhead is punched through with, so the tests that
/// care pass their own and the ones that do not should not have to name one.
pub fn svg_at_over(
    elements: &[DrawElement],
    bounds: WorldBounds,
    padding: f64,
    background: &str,
) -> String {
    let borrowed: Vec<&DrawElement> = elements.iter().collect();
    draw_engine::scene_to_svg(
        &borrowed,
        &ExportFrame::for_bounds(
            bounds,
            &ExportOptions {
                padding,
                ..Default::default()
            },
        ),
        background,
        // `None`: these are the tests about *markup*, and a payload would be a wall of
        // base64 in every failure message. The payload has its own file.
        None,
    )
}

/// `export_svg` with an explicit padding, for the tests that are about the markup.
///
/// Goes through [`DrawEngine::export_scope`] because that is where the scope lives now; a
/// test about which elements or which box an export covers belongs in `ci_export_scope.rs`,
/// which asserts on the scope itself rather than on the string.
pub fn svg_of(engine: &DrawEngine, padding: f64) -> String {
    let scope = engine.export_scope(
        false,
        &ExportOptions {
            padding,
            ..Default::default()
        },
    );
    let options = ExportOptions::default();
    engine.export_svg_of(&scope, &options).unwrap_or_default()
}
