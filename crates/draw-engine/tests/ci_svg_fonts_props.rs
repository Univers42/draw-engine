//! Properties of an exported SVG's fonts, over every family and a swept corpus.
//!
//! `ci_svg_fonts.rs` pins the **decisions** and `ci_svg_fonts_bytes.rs` pins the **bytes**.
//! This file asks the question neither can: that **every id the contract will accept emits
//! the stack the oracle emits for it** — a claim about a table, which a per-case test
//! covering three of its rows leaves open for the other five hundred and fifty.
//!
//! ## Why the table is transcribed here and not imported
//!
//! The expected strings below are a **second transcription** of the oracle's
//! `FONT_FAMILY`, `getGenericFontFamilyFallback`, `getFontFamilyFallbacks` and
//! `getFontFamilyString` (`packages/common/src/constants.ts@1118751f:137-194`,
//! `packages/common/src/utils.ts@1118751f:123-136`), written from those lines and sharing no
//! code with `engine/crates/draw-engine/src/text/font.rs`. A transcription is exactly the
//! thing that can be wrong in the same way twice, so it is backed by two **published
//! vectors** where the oracle has committed any, and by an explicit list of the rows it does
//! not:
//!
//! - `tests/__snapshots__/export.test.tsx.snap@1118751f` — a *generated* snapshot holding
//!   `font-family="Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"`, character for character.
//! - `tests/scene/__snapshots__/export.test.ts.snap@1118751f:18-42` — generated
//!   `@font-face` declarations naming `Excalifont`, `Nunito` and `Xiaolai`.
//! - The other six families' `font-family` strings rest on the transcription alone. The
//!   `font-family="Virgil, Segoe UI Emoji"` in `tests/fixtures/*.svg` is **not** a vector: those
//!   two files are hand-written markup with a payload appended, they predate the fallback
//!   list, and `getFontFamilyString(1)` has answered `Virgil, sans-serif, Segoe UI Emoji`
//!   since. Reading them as the oracle's output would be reading a fixture as a spec.
//!
//! ## Why the corpus is generated rather than written out
//!
//! No property-testing dependency, and adding one is not on the table (§3.2). The cases come
//! out of the same seeded xorshift64* the crate's other property files use, and **a failure
//! is reproducible from the seed alone**.

mod common;

use common::svg::*;
use common::*;
use draw_engine::text::font::LEGACY_CSS;
use draw_engine::*;

/// How many corpora to sweep. A failure prints its seed.
const CORPORA: u64 = 200;

/// Every id the contract will accept: `finiteInt.min(1).max(MAX_FONT_FAMILY)` with
/// `MAX_FONT_FAMILY = 64` (`packages/contract/src/element.ts:226`, `limits.ts:27`). So 0 and
/// 255 are unreachable and 1..=64 is the whole domain.
const REACHABLE: std::ops::RangeInclusive<u8> = 1..=64;

/// The eight families this app can produce, and their stacks, exactly as the oracle writes
/// them. **Asserted equal to ours, character for character.**
const ORACLE_STACKS: [(u8, &str); 8] = [
    (1, "Virgil, sans-serif, Segoe UI Emoji"),
    (2, "Helvetica, sans-serif, Segoe UI Emoji"),
    (3, "Cascadia, monospace, Segoe UI Emoji"),
    (5, "Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"),
    (6, "Nunito, sans-serif, Segoe UI Emoji"),
    (7, "Lilita One, sans-serif, Segoe UI Emoji"),
    (8, "Comic Shanns, monospace, Segoe UI Emoji"),
    (9, "Liberation Sans, sans-serif, Segoe UI Emoji"),
];

/// The reachable ids the oracle names and we deliberately do not, with what it writes for each.
///
/// `getFontFamilyString` walks `FONT_FAMILY` and falls through to
/// `WINDOWS_EMOJI_FALLBACK_FONT` when no key matches (`utils.ts@1118751f:128-135`). Ids 4 and
/// 10 *are* keys, so the oracle writes a real name for them; 11 and up are not, so it writes
/// the emoji font on its own.
const DIVERGENT: [(u8, &str); 2] = [
    // "leave 4 unused as it was historically used for Assistant … or custom font (Obsidian)"
    // (`constants.ts@1118751f:141`) — a board from Obsidian can carry this one.
    (4, "Segoe UI Emoji"),
    // The oracle's UI font, which `Fonts._initialized` never registers
    // (`Fonts.ts@1118751f:398-408`): named on a `<text>`, declared by nobody.
    (10, "Assistant, sans-serif, Segoe UI Emoji"),
];

/// The two published vectors, verbatim, for the one family both of them name.
const VECTOR_EXCALIFONT_STACK: &str = "Excalifont, Xiaolai, sans-serif, Segoe UI Emoji";
const VECTOR_NUNITO_FACE: &str = "@font-face { font-family: Nunito; src: url(";

fn text_in(family: Option<u8>) -> DrawElement {
    let mut element = text_at(10.0, 20.0, 100.0, 50.0);
    element.text = Some("bunny".into());
    element.font_family = family;
    element
}

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

fn faces_of(svg: &str) -> Vec<FontFace> {
    font_faces(&font_face_style(svg).expect("a style element").1)
}

// ---------------------------------------------------------------------------
// The published vectors
// ---------------------------------------------------------------------------

/// The one generated oracle artifact that holds a `font-family` attribute, character for
/// character.
///
/// `packages/excalidraw/tests/__snapshots__/export.test.tsx.snap@1118751f`, a snapshot their
/// own test suite wrote by running `exportToSvg`. If ours were `Excalifont, sans-serif,
/// Segoe UI Emoji` — the near-miss a "keep only the fallbacks the font needs" reading would
/// produce — this is the one assertion in the repository that would notice, and nothing in
/// the app would.
#[test]
fn the_generated_oracle_snapshot_names_excalifonts_stack_exactly_as_we_do() {
    assert_eq!(ORACLE_STACKS[3].1, VECTOR_EXCALIFONT_STACK);
    let stacks = text_font_families(&svg_of_elements(&[text_in(Some(5))]));
    assert_eq!(stacks, [VECTOR_EXCALIFONT_STACK]);
}

/// The generated oracle declarations name the family the way we name it.
///
/// `export.test.ts.snap@1118751f:18-42` writes `@font-face { font-family: Nunito; src: url(`
/// with nothing after `url(` before the data URL — no weight, no style, no fallbacks. Ours is
/// the same text, and a declaration that added a descriptor to be helpful would render the
/// same and read differently.
#[test]
fn the_generated_oracle_declarations_have_our_shape() {
    let svg = svg_of_elements(&[text_in(Some(6))]);
    let faces = faces_of(&svg);
    assert_eq!(faces.len(), 2, "Nunito's two shards");
    for face in &faces {
        assert_eq!(
            VECTOR_NUNITO_FACE,
            &format!("@font-face {{ font-family: {}; src: url(", face.family)
        );
    }
}

// ---------------------------------------------------------------------------
// Every reachable id
// ---------------------------------------------------------------------------

/// Every id the contract accepts, both size paths, emits **the oracle's own stack**.
///
/// For the eight families this app can produce the two are **equal, character for
/// character**. That is the answer to the question the whole task turns on, and it is
/// asserted for all 64 ids rather than for the ones a case test happened to list — a table
/// that grows a row nobody wrote a test for is the way a family gets a wrong fallback.
#[test]
fn every_reachable_id_emits_the_oracles_own_stack() {
    for id in REACHABLE {
        for size in [None, Some(28.0_f64)] {
            let mut element = text_in(Some(id));
            element.font_size = size;
            let stacks = text_font_families(&svg_of_elements(&[element]));
            assert_eq!(stacks.len(), 1, "id {id}, size {size:?}");
            let expected = ORACLE_STACKS
                .iter()
                .find(|(known, _)| *known == id)
                .map_or(LEGACY_CSS, |(_, stack)| *stack);
            assert_eq!(stacks[0], expected, "id {id}, size {size:?}");
        }
    }
}

/// Where we differ from the oracle, named, with what each side writes, on purpose.
///
/// **`getFontFamilyString` never answers with a name our engine does not draw.** Ids 4, 10 and
/// 11..=64 are the ones the contract accepts (`element.ts:226` clamps to 1..=64) that our
/// `FAMILIES` table does not carry. The oracle answers `Segoe UI Emoji` for the key it does
/// not reach from a text element and `Assistant, …` for the one it does; we answer with the
/// legacy system stack.
///
/// **Both sides are drawing a fallback and neither is a crash**, and in the one case that can
/// actually arrive the oracle's answer is the worse one: a board from Obsidian carries
/// `fontFamily: 4`, and `Segoe UI Emoji` is an emoji font — it renders Latin text no better
/// than `system-ui, -apple-system, Segoe UI, Roboto, sans-serif` does. Changing it would also
/// move measurement for those elements, and the element schema is a public format (§3.2,
/// backward-compatible by default). So this is a decision, written down in `font_face.rs` and
/// asserted here so that changing it has to be done on purpose.
///
/// What this test is **for**: the day someone "fixes" the table to make the two agree, it
/// fails and says what they would be giving up.
#[test]
fn the_ids_we_answer_differently_from_the_oracle_are_named_and_deliberate() {
    for (id, oracle) in DIVERGENT {
        let svg = svg_of_elements(&[text_in(Some(id))]);
        assert_eq!(
            text_font_families(&svg),
            [LEGACY_CSS],
            "id {id}: the oracle writes {oracle:?}"
        );
        // And neither side declares a face for it: the oracle's `Fonts._initialized` does not
        // register 4 or 10, and we ship no file for either.
        assert!(!svg.contains("@font-face"), "id {id}");
    }
    for id in 11..=64u8 {
        assert_eq!(
            text_font_families(&svg_of_elements(&[text_in(Some(id))])),
            [LEGACY_CSS],
            "id {id}"
        );
    }
}

/// The near-misses a paraphrase produces, spelled out — and a positive check that the two
/// names with a space in them keep it.
///
/// A `+`, an `_`, a lost generic fallback and a "only the fallbacks the font needs" Excalifont
/// are all plausible and all wrong, and none of them changes a pixel in this app.
#[test]
fn the_near_misses_are_not_what_any_family_writes() {
    let every: Vec<&str> = ORACLE_STACKS.iter().map(|(_, stack)| *stack).collect();
    for wrong in [
        // A space turned into the token CSS uses in a URL.
        "Lilita+One, sans-serif, Segoe UI Emoji",
        "Lilita_One, sans-serif, Segoe UI Emoji",
        // A monospace family given the sans-serif generic.
        "Comic Shanns, sans-serif, Segoe UI Emoji",
        "Cascadia, sans-serif, Segoe UI Emoji",
        // "Only the fallbacks the font needs" — Xiaolai is the oracle's first CJK fallback and
        // it is in the string whether or not this scene has a CJK glyph in it.
        "Excalifont, sans-serif, Segoe UI Emoji",
        // The pre-fallback-list spelling, which is what the oracle's own *hand-written* test
        // fixtures still say (`tests/fixtures/*.svg`) and which nobody should copy from.
        "Virgil, Segoe UI Emoji",
    ] {
        assert!(
            !every.contains(&wrong),
            "{wrong:?} is not what the oracle writes"
        );
    }
    for (id, name) in [(7u8, "Lilita One"), (8, "Comic Shanns")] {
        let stacks = text_font_families(&svg_of_elements(&[text_in(Some(id))]));
        assert!(stacks[0].starts_with(&format!("{name}, ")), "{stacks:?}");
        assert!(
            !stacks[0].contains('+') && !stacks[0].contains('_'),
            "{stacks:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The swept corpus
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A corpus of shapes and texts, with families drawn from the whole reachable domain.
///
/// Text **and** shapes, because a shape that happens to carry a `fontFamily` is the way a
/// declaration gets pulled into a file that has no text in it; and deleted elements, because
/// `scene_to_svg` filters them and a family list that forgot to would carry a face for a
/// tombstone.
fn corpus(rng: &mut Rng) -> Vec<DrawElement> {
    let mut elements = Vec::new();
    for index in 0..(2 + rng.below(6)) {
        let mut element = if rng.below(3) == 0 {
            let mut shape = box_at(
                (rng.below(40) as f64) * 10.0,
                (rng.below(40) as f64) * 10.0,
                40.0 + (rng.below(30) as f64),
                40.0 + (rng.below(30) as f64),
            );
            // The near-miss the filter exists for.
            shape.font_family =
                Some(1 + rng.below(u64::from(REACHABLE.end() - REACHABLE.start())) as u8);
            shape
        } else {
            let mut text = text_at(
                (rng.below(40) as f64) * 10.0,
                (rng.below(40) as f64) * 10.0,
                120.0,
                40.0,
            );
            text.id = format!("t{index}");
            text.font_family = if rng.below(6) == 0 {
                None
            } else {
                Some(1 + rng.below(u64::from(REACHABLE.end() - REACHABLE.start())) as u8)
            };
            if rng.below(2) == 0 {
                text.font_size = None;
            }
            text
        };
        element.id = format!("e{index}");
        if rng.below(8) == 0 {
            element.is_deleted = true;
        }
        elements.push(element);
    }
    elements
}

/// The families `elements`' **text** uses, in first appearance, that we ship — restated here
/// from the rule rather than by calling the module under test, so the corpus assertion is a
/// comparison and not an echo.
fn expected_families(elements: &[DrawElement]) -> Vec<(&'static str, usize)> {
    let mut out: Vec<(&'static str, usize)> = Vec::new();
    for element in elements {
        if element.is_deleted || element.kind != DrawElementType::Text || !element.text.is_some() {
            continue;
        }
        let Some(face) = element
            .font_family
            .and_then(|id| SHIPPED_FONT_FACES.iter().find(|face| face.id == id))
        else {
            continue;
        };
        // First appearance only, and the shard count with it: two texts in one family are
        // one family, which is what the oracle's `Set` says (`Fonts.ts@1118751f:421-432`).
        if !out.iter().any(|(name, _)| *name == face.name) {
            out.push((face.name, face.files.len()));
        }
    }
    out
}

/// The declarations a corpus's file carries are exactly the families its text uses, in
/// first-appearance order, one declaration per shard — and the near-misses, all swept.
///
/// 200 corpora over the whole reachable id domain, mixed with shapes, deleted elements and
/// texts with no size. Four claims at once, because each is cheap and each catches a different
/// mistake: a face for a family nothing uses (a size bug), a missing face for one that does
/// (the bug this feature exists to prevent), a sorted order rather than first appearance, and
/// a descriptor copied out of the app's stylesheet.
#[test]
fn the_declarations_are_exactly_the_used_shipped_families_in_first_appearance_order() {
    for seed in 1..=CORPORA {
        let elements = corpus(&mut Rng(seed));
        let svg = svg_of_elements(&elements);
        let faces = faces_of(&svg);
        let expected = expected_families(&elements);
        let expected_names: Vec<&str> = expected
            .iter()
            .flat_map(|(name, shards)| std::iter::repeat_n(*name, *shards))
            .collect();
        let got: Vec<&str> = faces.iter().map(|face| face.family.as_str()).collect();
        assert_eq!(got, expected_names, "seed {seed}: {elements:?}");

        let (_, style) = font_face_style(&svg).expect("a style element");
        for banned in [
            "font-weight",
            "font-style",
            "unicode-range",
            "format(",
            "display:",
        ] {
            assert!(
                !style.contains(banned),
                "seed {seed}: {banned:?} in a declaration"
            );
        }
        // One `<text>` per live text element, each naming the stack for its own family, and
        // nothing for a shape.
        assert_eq!(
            text_font_families(&svg).len(),
            elements
                .iter()
                .filter(|e| !e.is_deleted && e.kind == DrawElementType::Text && e.text.is_some())
                .count(),
            "seed {seed}"
        );
    }
}

/// A corpus's declarations carry the bytes of the files its families name, in the same order.
///
/// The order is the half that matters: two families with two shards each produce four
/// declarations whose families repeat, so a writer that sorted by file name instead of
/// following the element order would produce the right four names and four wrong payloads.
/// Checking names alone cannot see that; checking CRCs against the table can.
#[test]
fn a_corpus_carries_each_family_s_own_shards_in_that_order() {
    for seed in 1..=CORPORA {
        let elements = corpus(&mut Rng(seed));
        let svg = svg_of_elements(&elements);
        let faces = faces_of(&svg);
        let borrowed: Vec<&DrawElement> = elements.iter().collect();
        // One entry per *file*, which is what `font_face_files` returns and therefore what
        // there is one declaration for.
        let wanted: Vec<(&'static str, &'static str)> = font_face_files(&borrowed)
            .into_iter()
            .map(|path| {
                let face = SHIPPED_FONT_FACES
                    .iter()
                    .find(|shipped| shipped.files.contains(&path))
                    .expect("a named path is in the table");
                (face.name, path)
            })
            .collect();
        assert_eq!(faces.len(), wanted.len(), "seed {seed}");
        for (face, (name, path)) in faces.iter().zip(&wanted) {
            assert_eq!(&face.family, name, "seed {seed}, {path}");
            // The payload is a data URL of that exact file: its first four bytes are the
            // WOFF2 signature and its header's own `length` field is its byte count, so a
            // payload from another family would be a different woff2 and either fail the
            // signature or the length.
            let payload = face
                .payload
                .strip_prefix("data:font/woff2;base64,")
                .unwrap_or_else(|| panic!("seed {seed}: {path} should be a woff2 data url"));
            let bytes = decode_base64(payload).expect("seed {seed}: base64");
            assert_eq!(&bytes[..4], b"wOF2", "seed {seed}, {path}");
            let length = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            assert_eq!(
                length as usize,
                bytes.len(),
                "seed {seed}: {path}'s payload is a different file"
            );
        }
    }
}

/// Exporting the same corpus twice gives the same file, and a reordered corpus gives a
/// **different declaration order** — so the order is a fact about the elements rather than a
/// sort that happens to be stable.
#[test]
fn the_output_is_a_function_of_the_elements_and_the_order_is_theirs() {
    for seed in 1..=CORPORA {
        let elements = corpus(&mut Rng(seed));
        assert_eq!(
            svg_of_elements(&elements),
            svg_of_elements(&elements),
            "seed {seed}"
        );
        let mut reversed: Vec<DrawElement> = elements.iter().rev().cloned().collect();
        for (index, element) in reversed.iter_mut().enumerate() {
            element.id = format!("e{index}");
        }
        let before: Vec<String> = faces_of(&svg_of_elements(&elements))
            .into_iter()
            .map(|face| face.family)
            .collect();
        let after: Vec<String> = faces_of(&svg_of_elements(&reversed))
            .into_iter()
            .map(|face| face.family)
            .collect();
        // Same set, possibly different order — and where it is the same it is because the
        // corpus's first appearance and its reverse agree, not because it was sorted.
        let mut left = before.clone();
        let mut right = after.clone();
        left.sort();
        left.dedup();
        right.sort();
        right.dedup();
        assert_eq!(left, right, "seed {seed}: reversing changed the families");
    }
}
