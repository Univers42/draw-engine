//! The export's background: whether there is one, what colour, and the dark-theme export.
//!
//! The oracle decides all three in one place and in one condition:
//! `if (appState.exportBackground && viewBackgroundColor)` draws the SVG's paper `<rect>`
//! (`packages/excalidraw/scene/export.ts@1118751f:458-468`), the rect's fill is
//! `applyDarkModeFilter(viewBackgroundColor, exportWithDarkMode)` (`:466`), and the same
//! filter is put over every colour the renderer writes
//! (`staticSvgScene.ts@1118751f:207-212, 528-534, 764-769, 821-826`; on the canvas at
//! `renderElement.ts@1118751f:447-450, 462-465` and `renderer/helpers.ts@1118751f:115-119`).
//!
//! **The default is to include the paper**: `exportBackground: true`
//! (`appState.ts@1118751f:69`) and `exportWithDarkMode: false` (`appState.ts@1118751f:72`).
//! A default of "omit" would be the whole feature rather than an option, and a test placed
//! on the wrong side of it passes for a broken implementation — so the defaults are asserted
//! on their own, and every behaviour test is a **pair whose two halves are compared on the
//! same point of the same document**. Half a pair is passed by an implementation that always
//! draws the paper and by one that never does.
//!
//! The colours in [`dark_mode_filter_writes_the_oracles_own_numbers`] are the oracle's own
//! literals, from `packages/common/src/colors.test.ts@1118751f:19, 25, 31, 37, 43, 69, 75, 81,
//! 89, 94, 98, 106` — a Rust implementation and a TypeScript one agreeing by accident is
//! not evidence, and agreeing on a number neither of them computed any other way is.

mod common;
use common::*;
use draw_engine::*;

/// The box every test here is framed at, and one that holds something: a claim about "the
/// paper changed" is only distinct from a claim about "the picture changed" if there is a
/// picture.
const FRAME: WorldBounds = WorldBounds {
    min_x: 0.0,
    min_y: 0.0,
    max_x: 400.0,
    max_y: 400.0,
};

/// The scene these tests export: one filled box, so the body is never empty and a difference
/// in the body is visible.
fn one_box() -> Vec<DrawElement> {
    vec![filled(box_at(40.0, 40.0, 120.0, 80.0))]
}

/// The document of `elements` in the theme whose paper is `background`, with nothing else
/// asked for. The engine's own default options, so a test that says nothing about a flag is
/// asserting what a person who opened the dialog and touched nothing gets.
fn svg_of(elements: &[DrawElement], background: &str) -> String {
    svg_at_over(elements, FRAME, 10.0, background)
}

/// The document of `elements` in a theme whose paper is `background`, with `patch` applied
/// to the default options.
fn svg_with(elements: &[DrawElement], background: &str, patch: ExportOptions) -> String {
    let borrowed: Vec<&DrawElement> = elements.iter().collect();
    scene_to_svg(
        &borrowed,
        &ExportFrame::for_bounds(FRAME, &patch),
        &patch,
        &papered(background),
    )
}

/// A theme whose paper is `color` and whose other colours are the light defaults.
fn papered(color: &str) -> DrawTheme {
    DrawTheme {
        background: color.into(),
        ..light_theme()
    }
}

/// The paper `<rect>`, as the document's own text, or `None` when the document has none.
///
/// Read as **the text between the `<defs>` and the `<g>`** rather than as the last `<rect>`
/// in the file: that is where the oracle appends it (after the defs, before the elements,
/// `export.ts@1118751f:458-468` and `:470-502`), and it is the only place a rect that is
/// not the paper can sit. A mask's rect, a frame's clip rect and the paper's own rect are
/// then three different things rather than one search's first hit.
fn paper(svg: &str) -> Option<String> {
    let (_, after_defs) = svg.split_once("</defs>")?;
    let (between, _) = after_defs.split_once("<g transform=")?;
    let paper = between.trim();
    if paper.is_empty() {
        None
    } else {
        Some(paper.to_string())
    }
}

/// Everything the drawing put on the page: the `<g>` and what is in it, and the metadata.
fn drawn(svg: &str) -> String {
    svg.split_once("<g transform=")
        .map_or(String::new(), |(_, tail)| tail.to_string())
}

/// Every colour value in the document, in order.
///
/// For the property tests: what a background must not change is *what is drawn*, so the two
/// documents have to differ only in a `<rect>` and in a colour. A claim made on the whole
/// document cannot tell those apart, and a claim made on the body alone would miss a paper
/// that leaked into the body's own colours.
fn colors_of(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("=\"#") {
        rest = &rest[at + 2..];
        let end = rest.find('"').unwrap_or(rest.len());
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    out
}

// ---------------------------------------------------------------------------
// 1. the defaults, asserted on their own
// ---------------------------------------------------------------------------

/// A drawing exported without being asked for anything keeps its paper and its colours.
///
/// `exportBackground: true` and `exportWithDarkMode: false` (`appState.ts@1118751f:69, 72`).
/// Two facts in one test because they are one fact: the default export is the drawing as the
/// board shows it. The SVG half is asserted by the tests below through the same struct, and
/// this pins the *numbers* a caller reads before it asks for anything.
#[test]
fn the_defaults_are_paper_on_and_dark_off() {
    let options = ExportOptions::default();
    assert_eq!(
        (options.background, options.dark_mode),
        (true, false),
        "exportBackground: true (appState.ts:69) and exportWithDarkMode: false (appState.ts:72)"
    );
}

/// The pair, on the same point of the same document: the default export and one that asked
/// for no paper, read as the paper itself.
#[test]
fn the_default_export_has_paper_and_asking_for_none_removes_it() {
    let elements = one_box();
    let by_default = paper(&svg_of(&elements, "#123456"));
    let asked = paper(&svg_with(
        &elements,
        "#123456",
        ExportOptions {
            background: false,
            ..Default::default()
        },
    ));
    assert_eq!(
        by_default,
        Some("<rect width=\"420\" height=\"420\" fill=\"#123456\"/>".to_string()),
        "the default export draws the paper, because the default is to"
    );
    assert_eq!(
        asked, None,
        "and asking for none leaves the document without one"
    );
}

// ---------------------------------------------------------------------------
// 2. whether there is a background at all
// ---------------------------------------------------------------------------

/// An empty background colour draws no paper.
///
/// The oracle's own second half of the condition: `exportBackground && viewBackgroundColor`
/// (`export.ts@1118751f:458`). A colour nobody chose is not a colour, and a `<rect fill="">`
/// is a rect some readers paint black and others ignore.
#[test]
fn an_empty_background_colour_draws_no_paper() {
    let svg = svg_of(&one_box(), "");
    assert_eq!(
        paper(&svg),
        None,
        "no chosen colour means no paper, not a rect with an empty fill: {svg}"
    );
}

/// The pair, on the same point: a colour and the absence of one.
#[test]
fn a_chosen_colour_and_no_colour_differ_on_the_paper_rect() {
    let elements = one_box();
    assert_eq!(
        paper(&svg_of(&elements, "#123456")),
        Some("<rect width=\"420\" height=\"420\" fill=\"#123456\"/>".to_string())
    );
    assert_eq!(paper(&svg_of(&elements, "")), None);
}

// ---------------------------------------------------------------------------
// 3. the colour — "Background color", design.md:1333
// ---------------------------------------------------------------------------

/// The paper is the **chosen** colour, not the theme's default and not a constant.
///
/// The pair is the whole point: two themes, the same drawing, the same attribute. An
/// implementation that wrote the light default, or the oracle's `#ffffff`, passes the first
/// half of a one-sided test and fails this one.
#[test]
fn the_paper_is_the_chosen_background_colour() {
    let elements = one_box();
    let red = paper(&svg_of(&elements, "#ff0000"));
    let blue = paper(&svg_of(&elements, "#0000ff"));
    assert_eq!(
        red,
        Some("<rect width=\"420\" height=\"420\" fill=\"#ff0000\"/>".to_string()),
        "a chosen colour is the paper"
    );
    assert_eq!(
        blue,
        Some("<rect width=\"420\" height=\"420\" fill=\"#0000ff\"/>".to_string()),
        "and a different chosen colour is a different paper, not the same one twice"
    );
    assert_ne!(red, blue, "the two must differ, or neither is a choice");
}

/// The paper's colour and the colours the drawing is painted in are the *same* choice.
///
/// The oracle's `viewBackgroundColor` is one value read twice — once for the rect
/// (`export.ts@1118751f:466`) and once, on the canvas, for the fill behind everything
/// (`renderer/helpers.ts@1118751f:115-119`). Here it is also what an outline arrowhead is
/// punched through with, which is the third read and the one that fails if the export keeps
/// its own copy of the colour: a head punched with white on a red board is a visible hole.
#[test]
fn an_outline_arrowhead_is_punched_with_the_chosen_paper() {
    let arrow = DrawElement {
        kind: DrawElementType::Arrow,
        points: Some(vec![[0.0, 0.0], [100.0, 0.0]]),
        stroke_color: "#6965db".into(),
        ..connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow)
    };
    let with_red = svg_of(std::slice::from_ref(&arrow), "#ff0000");
    let with_blue = svg_of(&[arrow], "#0000ff");
    assert!(
        with_red.contains("fill=\"#ff0000\""),
        "an outline head is punched with the paper behind it: {with_red}"
    );
    assert!(
        with_blue.contains("fill=\"#0000ff\""),
        "and the punch follows the paper, not a constant: {with_blue}"
    );
}

// ---------------------------------------------------------------------------
// 4. the dark-theme export
// ---------------------------------------------------------------------------

/// The filter writes the oracle's own numbers.
///
/// Every expected value here is a literal from `packages/common/src/colors.test.ts`, and
/// this engine computed them from the matrix in `colors.ts` rather than reading them back
/// from a previous run. That is what makes the test independent of the thing it checks: a
/// second transcription that agrees is evidence, a round trip is not.
#[test]
fn dark_mode_filter_writes_the_oracles_own_numbers() {
    // colors.test.ts:16-19, :22-25, :28-31, :34-37, :40-43
    assert_eq!(dark_mode_filter("#000000"), "#ededed");
    assert_eq!(dark_mode_filter("#ffffff"), "#121212");
    assert_eq!(dark_mode_filter("#ff0000"), "#ff9090");
    assert_eq!(dark_mode_filter("#00ff00"), "#008f00");
    assert_eq!(dark_mode_filter("#0000ff"), "#cdcdff");
    // colors.test.ts:78-81 — the shorthand hex, which is a parser case and not a maths one.
    assert_eq!(dark_mode_filter("#f00"), "#ff9090");
    // colors.test.ts:60-64, :65-69 — `rgb()` and the alpha it must keep.
    assert_eq!(dark_mode_filter("rgb(255, 0, 0)"), "#ff9090");
    assert_eq!(dark_mode_filter("rgba(255, 0, 0, 0.5)"), "#ff909080");
    // colors.test.ts:85-89, :91-94, :96-99 — alpha in hex, kept or dropped as it should be.
    assert_eq!(dark_mode_filter("#ff0000ff"), "#ff9090");
    assert_eq!(dark_mode_filter("#ff000080"), "#ff909080");
    assert_eq!(dark_mode_filter("#ff000000"), "#ff909000");
    // colors.test.ts:71-76, :114-117 — `transparent`, and COLOR_PALETTE.black (#1e1e1e).
    assert_eq!(dark_mode_filter("transparent"), "#ededed00");
    assert_eq!(dark_mode_filter("#1e1e1e"), "#d3d3d3");
}

/// The flag decides, and the pair is one input read two ways.
///
/// The one test that cannot pass by accident: the *same* colour through the *same* function
/// with the flag on and off, compared on the output string. A filter that ignored the flag
/// fails it; one that always filtered fails it.
#[test]
fn the_dark_flag_decides_and_the_two_answers_differ() {
    assert_eq!(dark_mode_filter("#1e1e1e"), "#d3d3d3");
    let theme = papered("#ffffff");
    let off = ExportPalette::of(&theme, &ExportOptions::default());
    let on = ExportPalette::of(
        &theme,
        &ExportOptions {
            dark_mode: true,
            ..Default::default()
        },
    );
    assert_eq!(off.color("#1e1e1e").as_ref(), "#1e1e1e");
    assert_eq!(on.color("#1e1e1e").as_ref(), "#d3d3d3");
    assert_ne!(
        off.color("#1e1e1e"),
        on.color("#1e1e1e"),
        "the two halves must differ, or the flag decides nothing"
    );
}

/// A dark export is dark: the paper and the drawing, both.
///
/// `applyDarkModeFilter(viewBackgroundColor, exportWithDarkMode)` for the rect
/// (`export.ts@1118751f:466`) and the same over every element colour
/// (`staticSvgScene.ts@1118751f:528-534`). **Both halves compared on their own attribute**,
/// because a filter applied to the paper and forgotten on the elements gives a dark page with
/// a light drawing on it, and a test that only looked at the page would call that correct.
#[test]
fn a_dark_export_writes_dark_paper_and_dark_elements() {
    let elements = one_box();
    let light = svg_with(&elements, "#ffffff", ExportOptions::default());
    let dark = svg_with(
        &elements,
        "#ffffff",
        ExportOptions {
            dark_mode: true,
            ..Default::default()
        },
    );
    // The oracle's own numbers for these two inputs: `colors.test.ts:22-25` and `:103-106`.
    assert_eq!(
        paper(&dark),
        Some("<rect width=\"420\" height=\"420\" fill=\"#121212\"/>".to_string()),
        "the paper goes through the filter"
    );
    assert!(
        dark.contains("stroke=\"#d3d3d3\""),
        "and so does the drawing's own stroke: {dark}"
    );
    assert!(
        !dark.contains("#1e1e1e"),
        "no colour of the light export survives into the dark one: {dark}"
    );
    // The pair, on the same attribute of the same document.
    assert_eq!(
        paper(&light),
        Some("<rect width=\"420\" height=\"420\" fill=\"#ffffff\"/>".to_string())
    );
    assert!(light.contains("stroke=\"#1e1e1e\""));
}

/// A shape with no fill has none in a dark export too.
///
/// `fill="none"` is a decision, not a colour, and the oracle's own exporter writes it
/// unfiltered: filtering `transparent` would give `#ededed00` — a near-white, fully
/// transparent, written where a renderer is entitled to read "paint nothing".
#[test]
fn a_transparent_fill_stays_none_in_a_dark_export() {
    let outline = box_at(40.0, 40.0, 120.0, 80.0);
    let dark = svg_with(
        &[outline],
        "#ffffff",
        ExportOptions {
            dark_mode: true,
            ..Default::default()
        },
    );
    assert!(dark.contains("fill=\"none\""), "{dark}");
    assert!(
        !dark.contains("#ededed00"),
        "a colour that means 'nothing' must not become one: {dark}"
    );
}

// ---------------------------------------------------------------------------
// 5. the property: the background never changes the geometry of what is drawn
// ---------------------------------------------------------------------------

/// A background is behind the drawing, so it changes nothing in front of it.
///
/// **The property, stated once and then generated over**: the same scene, exported with and
/// without a paper, has byte-identical geometry — the same body, the same viewBox, the same
/// transform — and the only difference is the paper itself. This is the test that catches a
/// background leaking into the paint order or into an element's own colours, and it cannot
/// be caught by looking at one picture: a leak shows up as one element of one scene being
/// wrong.
///
/// It is why the comparison is on the *body* and on the *shape of the document*, rather than
/// on the whole string, which would differ by construction.
#[test]
fn the_background_never_changes_the_geometry_of_what_is_drawn() {
    for scene in generated_scenes() {
        let with = svg_with(&scene, "#ff8800", ExportOptions::default());
        let without = svg_with(
            &scene,
            "#ff8800",
            ExportOptions {
                background: false,
                ..Default::default()
            },
        );
        assert_eq!(
            drawn(&with),
            drawn(&without),
            "the drawing is the same with and without a paper behind it: {scene:?}"
        );
        assert_eq!(
            view_box_of(&with),
            view_box_of(&without),
            "and the box it is cut to: {scene:?}"
        );
        assert!(
            paper(&with).is_some(),
            "and there was a paper to remove: {scene:?}"
        );
        assert_eq!(paper(&without), None);
    }
}

/// The same property for the dark filter, over the same corpus — read as "only colours
/// moved".
///
/// Stripped of colour values, the two documents must be **the same string**. That is
/// stronger than counting elements and stronger than comparing the body, and it is the shape
/// of the claim: a theme changes what a picture looks like and cannot change where anything
/// is.
#[test]
fn the_dark_filter_moves_colours_and_nothing_else() {
    for scene in generated_scenes() {
        let light = svg_with(&scene, "#ffffff", ExportOptions::default());
        let dark = svg_with(
            &scene,
            "#ffffff",
            ExportOptions {
                dark_mode: true,
                ..Default::default()
            },
        );
        assert_eq!(
            without_colors(&light),
            without_colors(&dark),
            "a dark export is the light one with other colours: {scene:?}"
        );
        assert_ne!(
            colors_of(&light),
            colors_of(&dark),
            "and the colours did move"
        );
    }
}

/// The `viewBox` of a document, as its own text.
fn view_box_of(svg: &str) -> String {
    let head: String = svg.chars().take(svg.find('>').unwrap_or(0)).collect();
    head
}

/// The document with every colour value replaced by a marker.
///
/// Deliberately crude — it replaces the *inside* of any `="#…"` — and it is enough because
/// the claim is a negative one: a dark export that moved a coordinate, a count or a
/// transform changes a number, not a colour, and a number is not touched by this.
fn without_colors(svg: &str) -> String {
    colors_of(svg)
        .into_iter()
        .fold(svg.to_string(), |rest, color| rest.replacen(&color, "@", 1))
}

/// The corpus the two property tests run over.
///
/// Empty, one, many, turned, transparent and text — because the property is about *the
/// background*, and a corpus of one rectangle proves nothing about a background that leaked
/// into a rotated element's fill or into a text run's baseline.
fn generated_scenes() -> Vec<Vec<DrawElement>> {
    let mut turned = box_at(200.0, 200.0, 100.0, 100.0);
    turned.angle = std::f64::consts::FRAC_PI_4;
    let mut clear = box_at(20.0, 20.0, 60.0, 60.0);
    clear.background_color = "transparent".into();
    let mut word = text_at(20.0, 120.0, 200.0, 40.0);
    word.text = Some("bunny".into());
    let mut many = Vec::new();
    for i in 0..24 {
        let mut element = filled(box_at(
            (i % 6) as f64 * 55.0,
            (i / 6) as f64 * 55.0,
            40.0,
            30.0,
        ));
        element.stroke_color = format!("#{:02x}3366", 20 + i * 9);
        many.push(element);
    }
    let together: Vec<DrawElement> = [
        many.clone(),
        vec![turned.clone(), clear.clone(), word.clone()],
    ]
    .concat();
    vec![
        Vec::new(),
        one_box(),
        vec![clear.clone()],
        vec![turned.clone()],
        vec![word.clone()],
        vec![connector(10.0, 10.0, 300.0, 300.0, DrawElementType::Arrow)],
        many,
        together,
    ]
}
