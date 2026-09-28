//! Properties of a round trip, over a swept corpus rather than one file.
//!
//! `ci_export_roundtrip.rs` pins the **decisions** — the key, the generation, and the five
//! failure answers — and `ci_export_roundtrip_bytes.rs` pins the **framing**, against a
//! second implementation, RFC 4648's vectors and two committed fixtures. This file asks the
//! question neither of them can: not *is this case right* but *is the rule right for cases
//! nobody wrote down*.
//!
//! ## Why these and not more
//!
//! Two invariants, and they are the two that actually matter:
//!
//! 1. **Any byte sequence either restores a scene or is refused — never an empty scene.**
//!    A restore that "works" by yielding nothing is indistinguishable from a fresh board.
//!    The owner finds that by opening a file and finding nothing there, which is the
//!    worst possible way to find it. Every generator below feeds `restore`, and the only
//!    thing the test ever asserts is that it does not answer with an empty scene.
//! 2. **The scene restored equals the scene saved, field for field, every id included.**
//!    `DrawElement` is `PartialEq` over forty-odd fields, so one `assert_eq!` on the whole
//!    vector is the whole claim and cannot be satisfied by a comparison that skipped
//!    something.
//!
//! ## Why the corpus is generated
//!
//! No property-testing dependency, and adding one is not on the table (BUNNY.md §3.2,
//! library-first: `proptest` is not installed). So the cases come out of a seeded
//! xorshift64* and **a failure is reproducible from the seed alone** — the same generator
//! this crate's other property files use (`ci_export_clipboard_props.rs`,
//! `ci_cardinality_props.rs`).

mod common;
use common::png::*;
use common::*;
use draw_engine::*;

/// How many corpora to sweep. A failure prints its seed.
const CORPORA: u64 = 240;

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

    fn coordinate(&mut self) -> f64 {
        (self.below(60) as f64) * 10.0
    }

    /// A size that is never degenerate: a zero-width box is not a shape, and letting one in
    /// would test `ExportFrame`'s empty-bounds path rather than a round trip.
    fn size(&mut self) -> f64 {
        10.0 + (self.below(40) as f64) * 5.0
    }

    /// A byte, for the corpora that are nothing but bytes.
    fn byte(&mut self) -> u8 {
        (self.next() & 0xFF) as u8
    }
}

/// One corpus's worth of elements: 0, 1 or many, and never all the same kind.
///
/// Zero is in there on purpose even though a restore refuses it — the generator producing a
/// scene the writer cannot write is the boundary the refusal is about, and a corpus that
/// stopped at one would never reach it.
fn a_scene(rng: &mut Rng) -> Vec<DrawElement> {
    let count = match rng.below(10) {
        0 => 0,
        1 | 2 => 1,
        other => other,
    };
    (0..count).map(|index| an_element(rng, index)).collect()
}

/// One element of a kind the generator picks, with an id it mints and text it chooses.
fn an_element(rng: &mut Rng, index: u64) -> DrawElement {
    let x = rng.coordinate();
    let y = rng.coordinate();
    let width = rng.size();
    let height = rng.size();
    // Ids are minted here rather than read off a clock, so a corpus is a pure function of
    // its seed and the same id comes back every run.
    let id = format!("prop-{index}-{}", rng.next() & 0xFFFF_FFFF);
    let mut element = match rng.below(4) {
        0 => box_at(x, y, width, height),
        1 => ellipse_at(x, y, width, height),
        2 => connector(x, y, x + width, y + height, DrawElementType::Arrow),
        _ => text_at(x, y, width, height),
    };
    element.id = id;
    if element.kind == DrawElementType::Text {
        element.text = Some(the_text(rng));
    }
    if rng.below(4) == 0 {
        element.group_ids = vec![format!("g-{}", rng.next() & 0xFFFF)];
    }
    if rng.below(5) == 0 {
        element.angle = (rng.below(4) as f64) * std::f64::consts::FRAC_PI_2;
    }
    element
}

/// Text that is not ASCII, and sometimes not even valid on one line.
///
/// The set is fixed rather than random because each entry is a *class* of thing the
/// payload has to survive: an accent (two UTF-8 bytes), an emoji (four, and above U+FFFF),
/// an em dash, a `--` (which closes an XML comment), and a NUL (which a `tEXt` text may not
/// contain at all).
fn the_text(rng: &mut Rng) -> String {
    const PIECES: [&str; 7] = [
        "plain",
        "héllo",
        "🐰",
        "a--b",
        "— em dash",
        "日本語",
        "nul\0inside",
    ];
    let count = 1 + rng.below(3) as usize;
    (0..count)
        .map(|_| PIECES[rng.below(PIECES.len() as u64) as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

/// The payload for a scene, through the engine's own writer.
fn payload_of(scene: &[DrawElement]) -> String {
    let borrowed: Vec<&DrawElement> = scene.iter().collect();
    scene_payload(&borrowed)
}

/// A PNG carrying `payload` under `keyword`.
fn embedded(keyword: &str, payload: &str) -> Vec<u8> {
    png_with_text(keyword, payload)
}

/// An SVG of `scene`, with the payload in it or not.
fn svg_of(scene: &[DrawElement], embed_scene: bool) -> String {
    let engine = engine_with_scene(scene.to_vec());
    let options = ExportOptions {
        embed_scene,
        ..Default::default()
    };
    engine
        .export_svg_of(&engine.export_scope(false, &options), &options)
        .unwrap_or_default()
}

/// The scene `bytes` restored to, or `None` for any refusal. A `Some` is never empty: that
/// is the invariant both files below are about, and the helper is where it is enforced so
/// neither test can forget to check.
fn restored(bytes: &[u8]) -> Option<Vec<DrawElement>> {
    match restore(bytes) {
        Restore::Payload(json) => {
            let elements = elements_from_json(&json).expect("a payload that parsed");
            assert!(
                !elements.is_empty(),
                "a restore answered with a scene of no elements, which looks like a fresh \
                 board: {json}"
            );
            Some(elements)
        }
        // Any of the three refusals is an answer; which one is `ci_export_roundtrip.rs`'s
        // question and is pinned there, case by case.
        Restore::Refused(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Property 1: nothing restores to an empty scene, whatever the bytes are.
// ---------------------------------------------------------------------------

#[test]
fn no_byte_sequence_ever_restores_to_an_empty_scene() {
    // The whole property in one sweep, and the sweep is the point: five families of input
    // a case-by-case reading would not have written down, each fed to the one function a
    // host can call.
    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let scene = a_scene(&mut rng);
        let payload = payload_of(&scene);
        let good = embedded(SCENE_FORMAT, &payload);
        let svg = svg_of(&scene, true);
        for bytes in corpus(&mut rng, &payload, &good, svg.as_bytes()) {
            restored(&bytes);
        }
    }
}

/// The input families, in one place so the list is auditable.
///
/// Split in two by what they are: the files a person could have, and the files nobody
/// would. A single list of twenty entries is a list nobody reads twice, and the point of
/// writing it down is that it can be.
fn corpus(rng: &mut Rng, payload: &str, good: &[u8], svg: &[u8]) -> Vec<Vec<u8>> {
    let mut cases = files_a_person_could_have(payload, good, svg);
    cases.extend(files_nobody_would(rng, good));
    cases
}

/// The half of the corpus that is a file somebody could plausibly have saved.
fn files_a_person_could_have(payload: &str, good: &[u8], svg: &[u8]) -> Vec<Vec<u8>> {
    vec![
        // Nothing at all, and something else entirely.
        Vec::new(),
        b"not a png".to_vec(),
        PNG_SIGNATURE.to_vec(),
        // A picture with no chunk, and a chunk under a key that is not ours — the two the
        // oracle cannot tell apart either (`image.ts@1118751f:51`).
        plain_png(),
        embedded("Comment", payload),
        embedded("application/vnd.excalidraw+json", payload),
        // Ours, and unreadable: a generation we have not written, and garbage.
        embedded(SCENE_FORMAT, ""),
        embedded(SCENE_FORMAT, "0:e30="),
        embedded(SCENE_FORMAT, "2:e30="),
        embedded(SCENE_FORMAT, "1:!!!!"),
        // Ours, and carrying nothing: the silent failure.
        embedded(
            SCENE_FORMAT,
            &format!(
                "1:{}",
                base64_encode(br#"{"type":"osidraw","version":1,"elements":[]}"#)
            ),
        ),
        // A valid file, cut at a random length. Every length is covered by
        // `a_png_cut_short_anywhere_is_never_silently_a_scene`; this is the version that
        // also lands in the middle of the payload rather than only at the end.
        good[..good.len() / 2].to_vec(),
        svg.to_vec(),
        svg[..svg.len() / 2].to_vec(),
        b"<svg/>".to_vec(),
        b"<html><body>no</body></html>".to_vec(),
    ]
}

/// The half that is a file nobody would save: two chunks, random bytes, one byte changed.
///
/// Whether the answer changes is not the claim about any of these; that it never becomes an
/// empty scene is.
fn files_nobody_would(rng: &mut Rng, good: &[u8]) -> Vec<Vec<u8>> {
    let mut cases = vec![flipped(rng, good), random(rng, 64), {
        // Random bytes wearing the signature: the two halves of a fuzzer that knows
        // what it is looking for, where half of them walk a chunk table at all.
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend(random(rng, 48));
        bytes
    }];
    // Two `tEXt` chunks, all three ways round: ours second behind someone else's, ours
    // first carrying garbage, and ours first carrying a real scene — the oracle's `find`
    // (`data/index.ts:111`… `:18`) takes the first and never sees the second in any of them.
    cases.push(png_with_two_text(
        ("Comment", "no scene here"),
        (SCENE_FORMAT, "1:e30="),
    ));
    cases.push(png_with_two_text(
        (SCENE_FORMAT, "1:!!!!"),
        (SCENE_FORMAT, "1:e30="),
    ));
    cases.push(png_with_two_text(
        (SCENE_FORMAT, "1:e30="),
        ("Comment", "no scene here"),
    ));
    cases
}

/// `bytes` with one byte changed at a random place.
fn flipped(rng: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if out.is_empty() {
        return out;
    }
    let at = rng.below(out.len() as u64) as usize;
    out[at] = rng.byte();
    out
}

/// `count` random bytes.
fn random(rng: &mut Rng, count: usize) -> Vec<u8> {
    (0..count).map(|_| rng.byte()).collect()
}

// ---------------------------------------------------------------------------
// Property 2: what comes back is what went in, field for field.
// ---------------------------------------------------------------------------

#[test]
fn every_scene_in_the_corpus_comes_back_itself() {
    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
        let scene = a_scene(&mut rng);
        let payload = payload_of(&scene);
        // A scene of no elements is not one this engine writes (`data/index.ts:1118751f:
        // 120-122`), so there is nothing to round-trip and the writer's own boundary is the
        // answer. Asserted rather than skipped: a corpus that quietly dropped its empties
        // would be a corpus that stopped reaching the edge.
        if scene.is_empty() {
            assert_eq!(
                restored(&embedded(SCENE_FORMAT, &payload)),
                None,
                "a scene of no elements is refused, not restored"
            );
            continue;
        }
        let through_png = restored(&embedded(SCENE_FORMAT, &payload));
        let through_svg = restored(svg_of(&scene, true).as_bytes());
        assert_eq!(through_png, Some(scene.clone()), "png, seed {seed}");
        assert_eq!(through_svg, Some(scene.clone()), "svg, seed {seed}");
    }
}

#[test]
fn every_element_id_comes_back_as_the_id_it_went_in_as() {
    // Property 2 again, on its own, because `assert_eq!` over forty-odd fields reports a
    // mismatch as two printed structs and an id is the one field a person can check by
    // reading. Ids are what a board is: two elements with the same drawing and different
    // ids are two elements.
    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0xA24B_AED4_963E_E407) | 1);
        let scene = a_scene(&mut rng);
        if scene.is_empty() {
            continue;
        }
        let payload = payload_of(&scene);
        let back = restored(&embedded(SCENE_FORMAT, &payload)).expect("the corpus restores");
        let before: Vec<&str> = scene.iter().map(|element| element.id.as_str()).collect();
        let after: Vec<&str> = back.iter().map(|element| element.id.as_str()).collect();
        assert_eq!(before, after, "seed {seed}");
        // And the order, which is the z-order the drawing is painted in and is not
        // something a `HashMap` on the way through would have preserved.
        assert_eq!(scene, back, "seed {seed}");
    }
}

#[test]
fn the_text_of_every_text_element_comes_back_the_same_string() {
    // The corpus's texts are the interesting bytes: accents, an emoji, an em dash, a `--`
    // and a NUL. A `tEXt` chunk is Latin-1 with no NUL and an XML comment ends at `--`, so
    // every one of those is a way for the payload to come back subtly wrong — and a wrong
    // string is not a refusal, it is a drawing with a different word in it.
    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed | 1);
        let scene = a_scene(&mut rng);
        if scene.is_empty() {
            continue;
        }
        let payload = payload_of(&scene);
        let back = restored(&embedded(SCENE_FORMAT, &payload)).expect("the corpus restores");
        let before: Vec<Option<&str>> = scene
            .iter()
            .map(|element| element.text.as_deref())
            .collect();
        let after: Vec<Option<&str>> = back.iter().map(|element| element.text.as_deref()).collect();
        assert_eq!(before, after, "seed {seed}");
    }
}

#[test]
fn the_payload_of_every_scene_is_a_text_a_latin1_chunk_can_hold() {
    // `png-chunk-text@1.0.0`'s rule, over a corpus rather than in one case: the chunk is
    // Latin-1 with no NUL, and a payload that broke it would restore as a refusal (loud) or
    // as mojibake (quiet). The ASCII claim is stronger than the format needs — base64 is
    // ASCII — and it is checked because it is the property that makes the quiet one
    // impossible.
    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0xC2B2_AE3D_27D4_EB4F) | 1);
        let scene = a_scene(&mut rng);
        let payload = payload_of(&scene);
        assert!(
            payload.is_ascii() && !payload.contains('\0'),
            "seed {seed}: a tEXt payload must be ASCII and NUL-free: {payload:?}"
        );
        // And the engine's writer accepts it, which is the same rule stated by the code
        // that would have to obey it.
        assert!(
            insert_text_chunk(&plain_png(), SCENE_FORMAT, &payload).is_ok(),
            "seed {seed}: the engine refused its own payload"
        );
    }
}

#[test]
fn the_two_containers_never_disagree_about_a_scene() {
    // One payload, two containers, over the corpus. The failure this catches is a second
    // serialisation decision that happens to agree today: valid Rust, green in both files
    // above, and wrong the moment one of them learns a case the other has not been shown.
    for seed in 1..=CORPORA {
        let mut rng = Rng(seed.wrapping_mul(0x1656_67B1_9E37_79F9) | 1);
        let scene = a_scene(&mut rng);
        if scene.is_empty() {
            continue;
        }
        let payload = payload_of(&scene);
        let svg = svg_of(&scene, true);
        assert!(
            svg.contains(&payload),
            "seed {seed}: the SVG should carry the PNG chunk's very bytes"
        );
        assert_eq!(
            restored(&embedded(SCENE_FORMAT, &payload)),
            restored(svg.as_bytes()),
            "seed {seed}"
        );
    }
}
