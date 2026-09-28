//! What an exported SVG says about its fonts, character for character.
//!
//! The claim under test is a **claim to a program that is not us**. A `font-family`
//! attribute and an `@font-face` declaration are addresses written for a renderer, and
//! every one of them is a chance to name a family nobody has: a space turned into a `+`, a
//! fallback list where one name belongs, a `font-weight` copied out of the app's stylesheet
//! that the file format has no field for. None of that shows up in a screenshot of the app
//! — the app has the font — and all of it is total in a file that has left the building.
//!
//! So **nothing here asserts that an attribute exists.** `assert!(svg.contains("font-family="))`
//! passes for every possible implementation, including one that writes the empty string.
//! Every assertion below is the whole string, and each is paired with a near-miss case that
//! the wrong answer would produce.
//!
//! The oracle, for every line below:
//! - `packages/excalidraw/scene/export.ts@1118751f:435-451` — the `<defs>` and the `<style>`
//!   that carries the declarations, and the `"\n      "` delimiter;
//! - `packages/excalidraw/fonts/ExcalidrawFontFace.ts@1118751f:37-51` — `toCSS`, and the exact
//!   declaration text at `:49`;
//! - `packages/excalidraw/fonts/Fonts.ts@1118751f:182-217, 286-330, 421-432` — which
//!   families, in which order, and which of them are skipped;
//! - `packages/excalidraw/renderer/staticSvgScene.ts@1118751f:817` and
//!   `packages/common/src/utils.ts@1118751f:123-136` — the `font-family` attribute on each
//!   `<text>`.
//!
//! The bytes are pinned in `ci_svg_fonts_bytes.rs`; the two files answer different questions.

mod common;

use common::svg::*;
use common::*;
use draw_engine::text::font::LEGACY_CSS;
use draw_engine::*;

fn text_in(family: Option<u8>, at: f64) -> DrawElement {
    let mut element = text_at(at, 0.0, 120.0, 40.0);
    element.text = Some("bunny".into());
    element.font_family = family;
    element
}

/// The exported SVG of `elements`.
fn svg_of_elements(elements: &[DrawElement]) -> String {
    let borrowed: Vec<&DrawElement> = elements.iter().collect();
    let options = ExportOptions {
        // No payload: these are the tests about the fonts, and a base64 scene would be a
        // wall of text in every failure message. It has its own file.
        embed_scene: false,
        ..ExportOptions::default()
    };
    scene_to_svg(
        &borrowed,
        &ExportFrame::for_bounds(
            WorldBounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 400.0,
                max_y: 400.0,
            },
            &options,
        ),
        &options,
        // The paper is the theme's, so these tests ask for white the way the board would.
        &light_theme(),
    )
}

/// The declarations in `svg`, read by `tests/common/svg.rs` and not by the writer.
fn faces_of(svg: &str) -> Vec<FontFace> {
    font_faces(&font_face_style(svg).expect("a style element").1)
}

/// The family names in `svg`'s declarations, in order.
fn family_names(svg: &str) -> Vec<String> {
    faces_of(svg).into_iter().map(|face| face.family).collect()
}

// ---------------------------------------------------------------------------
// The declaration itself
// ---------------------------------------------------------------------------

/// The whole `<defs>`, byte for byte.
///
/// The oracle builds it as `${delimiter}${fontFaces.join(delimiter)}` where `delimiter` is
/// `"\n      "` (`export.ts@1118751f:443-449`) and appends it to the `<defs>` that
/// `exportToSvg` has already put between the `<metadata>` and the background `<rect>`
/// (`:361`, `:370`, `:451`, `:458-468`). Ours is one line where the oracle's is a DOM, but
/// those six spaces and that newline are **character data** and travel in the file.
#[test]
fn a_family_is_declared_exactly_as_the_oracle_declares_it() {
    // Virgil, because it is the one shipped family that is a single shard: Excalifont has
    // two, and this test is about the shape of a declaration, not about how many there are.
    let svg = svg_of_elements(&[text_in(Some(1), 10.0)]);
    let payload = &faces_of(&svg)[0].payload;
    let expected = format!(
        "<defs><style class=\"style-fonts\">\n      @font-face {{ font-family: Virgil; src: url({payload}); }}</style></defs>"
    );
    assert!(
        payload.starts_with("data:font/woff2;base64,"),
        "the oracle's `data:` URL and its media type, whole: {payload:.40}"
    );
    assert!(
        svg.contains(&expected),
        "the document should carry exactly\n{expected}\ngot\n{}",
        clipped(&svg)
    );
    // And the parts of the oracle's text that a paraphrase would lose: the spaces, the
    // semicolons, the `data:` URL's own media type, and the single closing brace.
    let (defs, style) = font_face_style(&svg).expect("a style element");
    assert!(style.starts_with("\n      @font-face { "), "{style:?}");
    assert!(style.ends_with("); }"), "{style:?}");
    assert!(
        defs.starts_with("<defs><style class=\"style-fonts\">"),
        "{defs:?}"
    );
    assert!(defs.ends_with("</style></defs>"), "{defs:?}");
}

/// The declaration sits between the `<metadata>` and the background `<rect>`, in that order.
///
/// Three elements in a row is a claim about the document, and a writer that appended the
/// `<defs>` at the end of the body would still pass every other test in this file.
#[test]
fn the_defs_sits_between_the_metadata_and_the_paper() {
    let svg = svg_of_elements(&[text_in(Some(1), 10.0)]);
    let (source, metadata, defs, rect) = (
        svg.find("svg-source:").expect("the oracle's comment"),
        svg.find("<metadata").expect("the metadata element"),
        svg.find("<defs>").expect("the defs element"),
        svg.find("<rect").expect("the background rect"),
    );
    assert!(
        source < metadata && metadata < defs && defs < rect,
        "{}",
        clipped(&svg)
    );
}

/// A family whose name has a space is named with its space.
///
/// The oracle interpolates the registered name raw — `` `font-family: ${this.fontFace.family}` ``
/// (`ExcalidrawFontFace.ts@1118751f:49`) — so `Lilita One` goes in unquoted and unescaped, and
/// so does the same name in the `font-family` attribute
/// (`getFontFamilyString`, `utils.ts@1118751f:130`). A `+`, a `_` or a `\"` would each be a
/// plausible-looking substitution and each names a family nobody has.
#[test]
fn a_family_with_a_space_is_named_with_its_space() {
    for (id, name) in [(7u8, "Lilita One"), (8, "Comic Shanns")] {
        let svg = svg_of_elements(&[text_in(Some(id), 10.0)]);
        let faces = faces_of(&svg);
        assert!(
            faces.iter().all(|face| face.family == name),
            "family {id} ({name}): {:?}",
            family_names(&svg)
        );
        // Reassemble each declaration the way the oracle's template builds it, and hold the
        // result against the document: that is the whole string, and a `+` or a quote anywhere
        // in the name is the only thing that could change it.
        for face in &faces {
            let declaration = format!(
                "@font-face {{ font-family: {name}; src: url({}); }}",
                face.payload
            );
            assert!(
                svg.contains(&declaration),
                "family {id}: {name} should be declared exactly as this"
            );
        }
        // The same spelling on the text itself, with the oracle's fallbacks after it.
        let stack = text_font_families(&svg);
        assert_eq!(stack.len(), 1, "{}", clipped(&svg));
        assert_eq!(stack[0].split(", ").next(), Some(name), "{stack:?}");
        // And never the spellings a CSS author reaches for instead.
        for wrong in [
            format!("{}+", name.replace(' ', "+")),
            format!("{}_", name.replace(' ', "_")),
        ] {
            assert!(!svg.contains(&wrong), "{wrong:?} in family {id}");
        }
    }
}

/// The declaration carries **no descriptor** and **no fallback** — the two things a copy of
/// the app's own stylesheet would add and the file format has no field for.
///
/// `fonts.css:45-54` declares Nunito at `font-weight: 500` because that is how the app draws
/// it, and the oracle's `toCSS` writes neither weight nor style
/// (`ExcalidrawFontFace.ts@1118751f:49`) because its `FontFace` descriptors are consumed by
/// the browser, not by the file. The stack's fallbacks belong on the `<text>`, where
/// `getFontFamilyString` puts them (`utils.ts@1118751f:130-132`), and not inside the face.
#[test]
fn a_face_names_one_family_and_nothing_else() {
    let svg = svg_of_elements(&[text_in(Some(6), 10.0)]);
    let (_, style) = font_face_style(&svg).expect("a style element");
    for banned in [
        "font-weight",
        "font-style",
        "font-stretch",
        "display:",
        "format(",
        "unicode-range",
        ", sans-serif",
        ", monospace",
        "Xiaolai",
        "Segoe UI Emoji",
    ] {
        assert!(
            !style.contains(banned),
            "{banned:?} should not be in:\n{}",
            clipped(&style)
        );
    }
    let faces = faces_of(&svg);
    assert_eq!(faces.len(), 2, "Nunito ships two shards: {faces:?}");
    assert!(
        faces.iter().all(|face| face.family == "Nunito"),
        "one name, no list: {:?}",
        family_names(&svg)
    );
}

// ---------------------------------------------------------------------------
// Which families, and in which order
// ---------------------------------------------------------------------------

/// Only the families the exported text actually uses, each once, in first-appearance order.
///
/// The oracle's `getUniqueFamilies` is a `Set` filled while walking the elements
/// (`Fonts.ts@1118751f:421-432`), and `generateFontFaceDeclarations` iterates it in that
/// order (`:186`, `:211`). Six copies of Nunito's face because six elements use Nunito is a
/// size bug; Nunito before Excalifont when Excalifont comes first in the scene is an
/// ordering bug, and neither shows in a picture.
#[test]
fn the_faces_are_the_used_families_once_each_in_the_order_the_text_appears() {
    let svg = svg_of_elements(&[
        text_in(Some(6), 0.0),
        text_in(Some(5), 60.0),
        text_in(Some(6), 120.0),
        text_in(Some(1), 180.0),
    ]);
    // Six texts, three families, and Nunito's two shards are still two: the family order is
    // first appearance and the shard order is the table's.
    assert_eq!(
        family_names(&svg),
        ["Nunito", "Nunito", "Excalifont", "Excalifont", "Virgil"],
        "{}",
        clipped(&svg)
    );
}

/// A scene with no text still carries the element, empty — because the oracle writes it
/// unconditionally.
///
/// `exportToSvg` creates the `<style>` and appends it whether or not `generateFontFaceDeclarations`
/// found anything (`export.ts@1118751f:445-451`), so a shapes-only drawing has a `<defs>` with
/// an empty `<style>`. Writing nothing there would be tidier and would be a second shape of
/// document, which is the thing this file keeps refusing to let in.
#[test]
fn a_drawing_with_no_text_still_carries_an_empty_style() {
    let svg = svg_of_elements(&[box_at(0.0, 0.0, 40.0, 40.0)]);
    let (defs, style) = font_face_style(&svg).expect("an empty style element");
    assert!(defs.starts_with("<defs><style"), "{}", clipped(&defs));
    assert!(!style.contains("@font-face"), "{style:?}");
    assert!(!svg.contains("<text "), "{}", clipped(&svg));
}

// ---------------------------------------------------------------------------
// The families we cannot carry, and the ones a text does not resolve to
// ---------------------------------------------------------------------------

/// Helvetica and Liberation Sans are named by the `<text>` and declared by nobody.
///
/// Both are named on every text today and neither has a file we may ship: Helvetica is a
/// system font in the oracle too (`fonts/Helvetica/index.ts:6-8` is `LOCAL_FONT_PROTOCOL`,
/// which `fontFacesStylesGenerator` skips at `Fonts.ts@1118751f:301-304`), and Liberation
/// Sans 1.05's licence is unresolved (`apps/web/static/fonts/LICENSES.md:30`). The oracle
/// *does* inline Liberation Sans, so this is a divergence and it is a licensing one.
#[test]
fn a_family_with_no_file_is_named_on_the_text_and_declared_by_nobody() {
    for id in [2u8, 9] {
        let svg = svg_of_elements(&[text_in(Some(id), 10.0)]);
        assert!(
            !svg.contains("@font-face"),
            "family {id} has no shipped file, so nothing is declared:\n{}",
            clipped(&svg)
        );
        let stack = text_font_families(&svg);
        assert_eq!(stack.len(), 1, "{}", clipped(&svg));
        assert_eq!(
            stack[0].split(", ").next(),
            Some(if id == 2 {
                "Helvetica"
            } else {
                "Liberation Sans"
            }),
            "{stack:?}"
        );
    }
}

/// A text with no family is the legacy stack, and carries no face either.
///
/// `resolved_font_family` answers `None` for a missing id, and `css_stack(None)` is the
/// system stack (`text/font.rs`). The oracle has no such state — its `fontFamily` is a
/// required number — so this is ours alone, and it must not invent a family to declare.
#[test]
fn a_text_with_no_family_carries_no_face_and_the_legacy_stack() {
    let svg = svg_of_elements(&[text_in(None, 10.0)]);
    assert!(!svg.contains("@font-face"), "{}", clipped(&svg));
    let stack = text_font_families(&svg);
    assert_eq!(stack.len(), 1, "{}", clipped(&svg));
    assert_eq!(stack[0], LEGACY_CSS, "{stack:?}");
}

/// A family id the engine does not know is not a family, and declares nothing.
///
/// The contract refuses it and a board can only get one by hand-editing, but the SVG is a
/// file that other programs read and "an id I did not recognise" must not turn into a name.
/// The oracle's answer is the same shape for a different reason: `Fonts.registered.get(id)` is
/// `undefined` and `fontFacesStylesGenerator` `continue`s (`Fonts.ts@1118751f:291-299`).
#[test]
fn an_unknown_family_id_declares_nothing() {
    for id in [0u8, 4, 10, 11, 200, 255] {
        let svg = svg_of_elements(&[text_in(Some(id), 10.0)]);
        assert!(!svg.contains("@font-face"), "id {id}:\n{}", clipped(&svg));
        assert_eq!(
            text_font_families(&svg),
            [LEGACY_CSS],
            "id {id}: {}",
            clipped(&svg)
        );
    }
}

// ---------------------------------------------------------------------------
// The size paths, and the list of files the engine says it is carrying
// ---------------------------------------------------------------------------

/// Both ways a text gets its size, and both carry the same face.
///
/// `font_size_of` reads the element's own `fontSize` and falls back to the default
/// (`text/layout.rs`); a face is not a size, so a text at 14px and a text at the default
/// must produce one declaration between them and not two. The sizes themselves are a
/// different file's claim and are not asserted here.
#[test]
fn both_size_paths_carry_one_face_and_two_sizes() {
    let mut small = text_in(Some(1), 0.0);
    small.font_size = Some(14.0);
    let mut big = text_in(Some(1), 60.0);
    big.font_size = None;
    let svg = svg_of_elements(&[small, big]);
    assert_eq!(faces_of(&svg).len(), 1, "{}", clipped(&svg));
    assert_eq!(
        svg.matches("font-size=\"14px\"").count(),
        1,
        "{}",
        clipped(&svg)
    );
    assert_eq!(
        svg.matches("font-size=\"20px\"").count(),
        1,
        "{}",
        clipped(&svg)
    );
}

/// The list of files the engine says it carries is the list the document declares, in order.
///
/// This is the same function the two answers come from, so it is not an independent check —
/// it is here to pin that the two are the same claim. The bytes are the independent file.
#[test]
fn the_files_the_engine_names_are_the_files_the_document_declares() {
    let scene = vec![
        text_in(Some(6), 0.0),
        text_in(Some(5), 60.0),
        text_in(Some(2), 120.0),
    ];
    let borrowed: Vec<&DrawElement> = scene.iter().collect();
    let asked = font_face_files(&borrowed);
    let svg = svg_of_elements(&scene);
    let faces = faces_of(&svg);
    assert_eq!(
        asked.len(),
        faces.len(),
        "{asked:?} vs {:?}",
        family_names(&svg)
    );
    // Nothing for Helvetica, which has no file: the engine did not claim a font nobody can
    // ship.
    assert!(
        !asked
            .iter()
            .any(|path| path.contains("Liberation") || path.contains("Helvetica")),
        "{asked:?}"
    );
    // And every named file is one the table says a shipped family has, twice over: once as
    // the table and once as the declaration's family name.
    for (path, face) in asked.iter().zip(&faces) {
        let family = SHIPPED_FONT_FACES
            .iter()
            .find(|shipped| shipped.files.contains(path))
            .unwrap_or_else(|| panic!("{path} is not in the table"));
        assert_eq!(family.name, face.family, "{path}");
    }
}
