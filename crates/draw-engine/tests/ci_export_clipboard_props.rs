//! Properties of a copy, over a swept corpus rather than one picture.
//!
//! `ci_export_clipboard.rs` pins the **decisions**: the type, the predicate, the scope, the
//! failure. Those are four questions with four answers, and the right way to test them is
//! one case each, read off the oracle.
//!
//! This file asks the question a per-case comparison cannot: that **whatever reaches the
//! clipboard is exactly what the export of the same scope would have produced**. The type
//! system cannot see that. A second `if selected.is_empty() { … }` inside the clipboard
//! path is valid Rust, compiles, passes every case test above, and is wrong the moment the
//! two paths disagree about a case the tests happened not to write down — a bound label, a
//! nested frame, a turned element. That is the only way Law 3 breaks in this feature
//! without anything looking wrong, so it is the thing worth sweeping.
//!
//! ## Why the comparison is against the *file* path and not against itself
//!
//! [`clipboard_copy`] builds its payload with [`DrawEngine::export_svg_of`] over a scope it
//! built with [`DrawEngine::export_scope`]. Comparing it to those two calls is therefore
//! checking that the clipboard took the same road, and it is the road the file export
//! walks. Comparing the clipboard's output to a second call into the clipboard's own
//! function would be true by construction and would prove nothing.
//!
//! ## Why the corpus is generated rather than written out
//!
//! No property-testing dependency, and adding one is not on the table (BUNNY.md §3.2,
//! library-first: `proptest` is not installed). So the cases come out of a seeded xorshift
//! and **a failure is reproducible from the seed alone** — the same generator this crate's
//! other property files use (`ci_secondary_pan.rs`, `ci_cardinality_props.rs`).

mod common;
use common::*;
use draw_engine::*;

/// How many corpora to sweep. A failure prints its seed.
const CORPORA: u64 = 200;

// ---------------------------------------------------------------------------
// A seeded generator: xorshift64*, so a case is reproducible from its seed and
// nothing else — no dependency, and no `--nocapture`-and-guess.
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

    /// A whole number in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A coordinate on a coarse grid, so boxes are exact and turns land on whole degrees.
    fn coordinate(&mut self) -> f64 {
        (self.below(60) as f64) * 10.0
    }

    /// A size that is never degenerate: a zero-width box is not a shape, and letting one in
    /// would test `ExportFrame`'s empty-bounds path rather than a copy.
    fn size(&mut self) -> f64 {
        10.0 + (self.below(40) as f64) * 5.0
    }
}

/// The three shapes a corpus is built from, plus a turned one: a turned element is the
/// case 4.2's bug was about (`clipboard.rs:693` measured the SVG with the unrotated union
/// while the PNG used the turned one), so a corpus without turns would miss its ghost.
fn corpus(rng: &mut Rng) -> Vec<DrawElement> {
    let kinds = 2 + rng.below(3);
    let mut elements: Vec<DrawElement> = Vec::new();
    let mut frame: Option<DrawElement> = None;
    for index in 0..kinds {
        let x = rng.coordinate();
        let y = rng.coordinate();
        let size = rng.size();
        let mut element = match rng.below(4) {
            0 => ellipse_at(x, y, size, size),
            1 => diamond_at(x, y, size, size),
            2 => box_at(x, y, size * 2.0, size),
            _ => box_at(x, y, size, size),
        };
        element.id = format!("e{index}");
        if rng.below(4) == 0 {
            element.angle = (rng.below(8) as f64) * std::f64::consts::PI / 4.0;
        }
        // Every fourth element is parented to the corpus's frame, when it has one, so
        // `includeElementsInFrames` and `getElementsOverlappingFrame` are both reached
        // with a populated set rather than only with the single-frame case.
        if let Some(holder) = frame.as_ref() {
            if rng.below(4) == 0 {
                element.frame_id = Some(holder.id.clone());
            }
        }
        elements.push(element);
        if frame.is_none() && rng.below(3) == 0 {
            let mut made = frame_at_origin(size * 4.0);
            made.id = "frame".to_string();
            frame = Some(made);
        }
    }
    if let Some(made) = frame {
        elements.push(made);
    }
    elements
}

/// Every subset of `elements`' ids, as a selection.
///
/// **Every one, not a sample**: with at most four elements that is sixteen selections, and
/// the whole point is the corners — no selection, each single element (which is the
/// one-frame case for the frame), each pair (which takes the `else if` at
/// `data/index.ts@1118751f:80` instead of the frame arm), and everything. A random sample
/// of sixteen subsets of sixteen would miss the interesting ones often enough to matter.
fn every_selection(elements: &[DrawElement]) -> Vec<Vec<String>> {
    let ids: Vec<String> = elements.iter().map(|el| el.id.clone()).collect();
    let mut out = vec![Vec::new()];
    for id in &ids {
        let mut grown = out.clone();
        for selection in &out {
            let mut with = selection.clone();
            with.push(id.clone());
            grown.push(with);
        }
        out = grown;
    }
    out
}

/// A browser that can take everything, so nothing here is about the predicate — that is
/// `ci_export_clipboard.rs`. What is left is the payload and the scope.
fn browser() -> ClipboardHost {
    ClipboardHost {
        can_write_blob: true,
        can_write_text: true,
    }
}

// ---------------------------------------------------------------------------
// The property.
// ---------------------------------------------------------------------------

/// **The clipboard carries the file export's own bytes, for every scene and every
/// selection.**
///
/// The invariant, in the form that catches a scope decided in the front: over a swept
/// corpus, for **every** selection — none, one, two, all, and the frame's own combinations
/// — the text a copy offers is byte-identical to what the same engine exports to a file
/// with the same options, and the copy says the same thing about what it copied.
///
/// Both halves matter. The string catches a different element list; the `scope` catches a
/// right picture described wrongly, which is the failure a toast makes: "Copied canvas to
/// clipboard" above a picture of two selected shapes.
#[test]
fn every_copy_is_byte_for_byte_the_file_export_of_the_same_scope() {
    let options = ExportOptions::default();

    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let elements = corpus(&mut rng);
        let mut engine = engine_with_scene(elements.clone());

        for selected in every_selection(&elements) {
            engine.select(selected.clone());

            let file_scope = engine.export_scope(true, &options);
            let file = engine.export_svg_of(&file_scope);
            let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &options);

            assert!(
                copy.supported,
                "seed {seed}, selection {selected:?}: nothing to copy"
            );
            assert_eq!(
                copy.text, file,
                "seed {seed}, selection {selected:?}: the clipboard's picture is not the file's"
            );
            // **Not** `copy.scope == file_scope.kind`. Both come from the same field, so
            // that comparison holds by construction and a `kind` that lies about every
            // scope sails through it — a mutation proved that before this line existed.
            // The expectation is restated from the oracle's own predicate instead, out of
            // the selection and the scene, so it is an independent witness.
            assert_eq!(
                copy.scope.kind,
                if names_a_live_element(&elements, &selected) {
                    ExportScopeKind::Selection
                } else {
                    ExportScopeKind::Scene
                },
                "seed {seed}, selection {selected:?}: the copy describes itself wrongly"
            );
        }
    }
}

/// **The frame is measured by its own box, on the clipboard as on a file.**
///
/// The other half of 4.2's finding, carried to the new path: a lone selected frame is
/// measured by the *frame element's* box at padding 0 and not by the union of what is
/// inside it (`export.ts@1118751f:228-233`). A clipboard that re-derived the box — or that
/// framed the contents instead of the frame — would produce a picture with a different
/// aspect ratio from the file's, and a pasted PNG that does not match the export.
///
/// Asserted on the picture's own `width`/`height` rather than on the frame, because the
/// picture is what reaches the clipboard.
#[test]
fn a_copied_frame_is_measured_by_its_own_box() {
    let options = ExportOptions::default();

    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let elements = corpus(&mut rng);
        let Some(frame) = elements.iter().find(|el| frame_id_is(el, "frame")) else {
            continue;
        };
        let mut engine = engine_with_scene(elements.clone());
        engine.select(vec![frame.id.clone()]);

        let file = engine
            .export_svg_of(&engine.export_scope(true, &options))
            .expect("a frame exports");
        let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &options);
        let copied = copy.text.as_deref().expect("a copy carries a picture");

        assert_eq!(copy.scope.kind, ExportScopeKind::Selection, "seed {seed}");
        assert_close(svg_width(copied), svg_width(&file));
        assert_close(svg_height(copied), svg_height(&file));
    }
}

/// **A turned element on the clipboard is measured by what it draws.**
///
/// The bug 4.2 fixed in the engine's own clipboard path, pinned on the new one because a
/// regression here is silent: a turned square copied with the *unrotated* box comes out
/// with its corners cropped, which looks like a drawing decision rather than a bug. The
/// eighth turn is the oracle's own probe — a quarter turn would land back on the box it
/// started in and prove nothing.
#[test]
fn a_turned_element_on_the_clipboard_is_measured_by_what_it_draws() {
    let mut square = box_at(0.0, 0.0, 100.0, 100.0);
    square.angle = std::f64::consts::FRAC_PI_4;
    let engine = engine_with_scene(vec![square]);
    let options = ExportOptions::default();

    let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &options);
    let svg = copy.text.as_deref().expect("a copy carries a picture");

    // `100 * √2` is the diagonal of the box the square draws.
    let expected = 100.0 * 2f64.sqrt() + 2.0 * DEFAULT_EXPORT_PADDING;
    assert_close(svg_width(svg), expected);
    assert_close(svg_height(svg), expected);
}

/// **A copy is never an empty picture, and a declined copy is never a copy.**
///
/// The degenerate answers are the ones a person cannot see coming: a 0×0 picture, or an
/// empty string, pasted into another application as an invisible object; and — the quieter
/// one — a host that asked and was handed a payload it cannot deliver, which is how "copy
/// as SVG" becomes a menu item that does nothing at all.
///
/// Both are asserted over the same corpus **and the same corpus read by a browser that can
/// take nothing**. That second half was missing at first, and a mutation found it: a
/// declined copy that still reported itself as supported and carried an empty payload
/// passed this file untouched, because the corpus has elements on it and the sweep only
/// ever asked a capable browser. A property that cannot see a branch is not covering it.
#[test]
fn a_copy_is_never_an_empty_picture() {
    let options = ExportOptions::default();
    let blind = ClipboardHost {
        can_write_blob: false,
        can_write_text: false,
    };

    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let elements = corpus(&mut rng);
        let mut engine = engine_with_scene(elements.clone());

        for selected in every_selection(&elements) {
            engine.select(selected.clone());
            let copy = engine.clipboard_copy(ClipboardFormat::Svg, &browser(), &options);
            if let Some(svg) = &copy.text {
                assert!(
                    svg_width(svg) > 0.0 && svg_height(svg) > 0.0,
                    "seed {seed}, selection {selected:?}: a picture with no extent"
                );
            } else {
                assert!(
                    !copy.supported,
                    "seed {seed}, selection {selected:?}: no picture, but reported as copied"
                );
            }

            // The same scene, asked by a browser that can take nothing. A declined copy
            // carries neither a payload nor a type: that is what makes "the engine said
            // no" something a host can see.
            let refused = engine.clipboard_copy(ClipboardFormat::Svg, &blind, &options);
            assert!(!refused.supported, "seed {seed}, selection {selected:?}");
            assert!(
                refused.text.is_none(),
                "seed {seed}, selection {selected:?}"
            );
            assert!(
                refused.mime.is_empty(),
                "seed {seed}, selection {selected:?}"
            );
        }
    }
}

/// Whether `element` is the corpus's frame.
///
/// The generator gives it a fixed id, which is the one thing about it this file relies on
/// and the reason it is asked through a function rather than inlined: a named predicate is
/// what a reader can check, and `el.id == "frame"` in four places is not.
fn frame_id_is(element: &DrawElement, id: &str) -> bool {
    element.id == id
}

/// Whether `selected` names an element that is on the board — `isSomeElementSelected`,
/// restated from the oracle (`packages/element/src/selection.ts@1118751f:141-143`).
///
/// The point of writing it out here is that it is **independent** of the engine's own
/// answer. A witness built from the code under test checks nothing: both sides move
/// together. This one is the oracle's sentence, over the test's own scene.
fn names_a_live_element(elements: &[DrawElement], selected: &[String]) -> bool {
    elements
        .iter()
        .any(|element| !element.is_deleted && selected.contains(&element.id))
}
