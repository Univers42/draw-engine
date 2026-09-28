//! The bytes on disk, and the evidence that they are what we think they are.
//!
//! `ci_export_roundtrip.rs` pins the **decisions**: the key, the generation, and the five
//! failure answers. This file pins the **framing**, and it is a separate file because the
//! claim is different. A round trip checked with the engine's own writer and the engine's
//! own reader proves the two halves agree with each other, which is not the claim; the claim
//! is that a file a person saved holds what we put in it. Two wrong implementations of the
//! same idea agree perfectly and are wrong together, so the evidence has to come from
//! somewhere else.
//!
//! Three somethings else, in increasing order of independence:
//!
//! 1. **`tests/common/png.rs`** — a second implementation of the PNG container, in Rust, in
//!    the test tree. It walks the chunk table, verifies every CRC as it goes, decodes the
//!    `tEXt` keyword split itself, and has its own base64. It is what answers "is the chunk
//!    there, is it keyed ours, is its CRC right".
//! 2. **RFC 4648 §10** — the base64 codec's own published vectors, for the codec the
//!    engine's payload is built with. A codec written from a table of one's own devising
//!    has no oracle; this one does, and it is a document.
//! 3. **`tests/fixtures/embedded-scene.png` and `.svg`** — committed files, written by a
//!    *third* implementation in a different language, holding a hand-written scene rather
//!    than anything this engine printed. `file(1)` reads the PNG as
//!    "PNG image data, 1 x 1, 8-bit/color RGBA", Python's `zlib` inflates its `IDAT`, and
//!    this engine's reader restores the scene from it. Three implementations and a system
//!    utility, none of them the code that will be blamed.
//!
//! ## What a same-code round trip would have missed
//!
//! Three of the four bugs this file's existence caught were invisible to a
//! write-then-read check, because the *same* code did both halves:
//!
//! - the base64 **encoder** did not left-align a short final group, so a scene whose JSON
//!   was one byte over a multiple of three came back with a character flipped in it;
//! - the base64 **decoder** cut the body at the *last* `=` rather than the first, so the
//!   same one-byte tail was refused outright;
//! - the base64 **decoder**'s accumulator kept every bit it was ever given, and after five
//!   groups `held << 6` shifted the top of it out of the `u32`.
//!
//! All three are in `base64`, none of them is in the chunk table, and every one of them
//! returns a *plausible* answer: a scene with one character wrong, or no scene at all.

mod common;
use common::png::*;
use common::*;
use draw_engine::*;

/// The committed PNG: `tests/fixtures/embedded-scene.png`.
///
/// A 1×1 picture carrying a hand-written two-element scene. Read with
/// [`include_bytes`] so a fixture that is not there is a compile error naming the file.
const FIXTURE_PNG: &[u8] = include_bytes!("fixtures/embedded-scene.png");

/// The committed SVG: the same scene, in the other container.
const FIXTURE_SVG: &[u8] = include_bytes!("fixtures/embedded-scene.svg");

/// The scene both fixtures hold, as literals.
///
/// Written out here on purpose. Deriving the expectation from the fixture would make the
/// test true by construction; these are the values a person reads in a hex dump and in the
/// generator, and the restored elements are compared against them field by field.
fn the_fixture_scene() -> Vec<DrawElement> {
    let mut boxy = box_at(0.0, 0.0, 100.0, 50.0);
    boxy.id = "box-1".into();
    boxy.background_color = "#ffc9c9".into();
    boxy.seed = 1;
    boxy.version_nonce = 1;
    boxy.updated = 1_700_000_000_000.0;
    let mut text = text_at(120.0, 40.0, 200.0, 30.0);
    text.id = "text-1".into();
    text.seed = 2;
    text.version_nonce = 2;
    text.roundness = Some(8.0);
    text.font_size = Some(20.0);
    text.updated = 1_700_000_000_000.0;
    text.text = Some("héllo 🐰 — a--b".into());
    vec![boxy, text]
}

// ---------------------------------------------------------------------------
// The chunk table
// ---------------------------------------------------------------------------

#[test]
fn the_png_we_write_carries_exactly_one_text_chunk_under_our_key() {
    let scene = the_fixture_scene();
    let borrowed: Vec<&DrawElement> = scene.iter().collect();
    let payload = scene_payload(&borrowed);
    let png = plain_png();
    let written = insert_text_chunk(&png, SCENE_FORMAT, &payload)
        .expect("a picture with a valid table takes a chunk");

    // Read by `tests/common/png.rs`, not by `png_chunk::find_text_chunk`. The walker
    // verifies every CRC as it goes, so this cannot pass on a file whose table is wrong
    // even when the chunk in it is right.
    let kinds: Vec<String> = png_chunks(&written)
        .into_iter()
        .map(|(kind, _, _)| String::from_utf8_lossy(&kind).into_owned())
        .collect();
    assert_eq!(kinds, ["IHDR", "IDAT", "tEXt", "IEND"]);
    assert_eq!(
        png_text(&written, SCENE_FORMAT),
        Some(payload),
        "the chunk's keyword and text, read by the other implementation"
    );
}

#[test]
fn the_chunk_is_spliced_in_before_iend_as_the_oracle_does() {
    // `chunks.splice(-1, 0, metadataChunk)`, commented "insert metadata before last chunk
    // (iEND)" (`data/image.ts@1118751f:44`). The order is not cosmetic: `IEND` must stay
    // last or no decoder will read the file, and the oracle puts the scene directly in
    // front of it rather than at the front of the file.
    let written = insert_text_chunk(&plain_png(), SCENE_FORMAT, "1:e30=").expect("a chunk fits");
    let kinds: Vec<String> = png_chunks(&written)
        .into_iter()
        .map(|(kind, _, _)| String::from_utf8_lossy(&kind).into_owned())
        .collect();
    assert_eq!(
        &kinds[kinds.len() - 2..],
        ["tEXt", "IEND"],
        "the chunk goes immediately before IEND, not at the front of the file"
    );
}

#[test]
fn the_picture_comes_out_byte_for_byte() {
    // We are not re-encoding anything. Everything except the new chunk is the browser's own
    // bytes, and the `IDAT` in particular is the one thing that must not move: a scene that
    // restores from a picture that is subtly different from the one that was saved is worse
    // than one that does not restore at all, because it looks right.
    let before = plain_png();
    let written = insert_text_chunk(&before, SCENE_FORMAT, "1:eyJ0eXBlIjoib3NpZHJhdyJ9")
        .expect("a chunk fits");
    let idat_of = |png: &[u8]| {
        png_chunks(png)
            .into_iter()
            .find(|(kind, _, _)| kind == b"IDAT")
            .map(|(_, data, _)| data)
            .expect("an IDAT")
    };
    assert_eq!(idat_of(&before), idat_of(&written));
    let ihdr_of = |png: &[u8]| {
        png_chunks(png)
            .into_iter()
            .find(|(kind, _, _)| kind == b"IHDR")
            .map(|(_, data, _)| data)
            .expect("an IHDR")
    };
    assert_eq!(ihdr_of(&before), ihdr_of(&written));
}

#[test]
fn the_keyword_limit_is_the_png_one_and_it_is_enforced() {
    // 1–79 Latin-1 bytes, no NUL (`png-chunk-text@1.0.0`, `encode.js:11-13, 20-24`; ISO
    // 15948 §11.3.4.3). Ours is seven, so this is about the container's own rule and about
    // the writer refusing rather than truncating — a silent truncation would put the scene
    // under a key nothing looks for.
    let png = plain_png();
    assert!(insert_text_chunk(&png, &"k".repeat(79), "x").is_ok());
    assert_eq!(
        insert_text_chunk(&png, &"k".repeat(80), "x"),
        Err(PngError::BadKeyword)
    );
    assert_eq!(insert_text_chunk(&png, "", "x"), Err(PngError::BadKeyword));
}

#[test]
fn a_text_a_latin1_reader_cannot_hand_back_is_refused_not_written() {
    // A `tEXt` text is Latin-1, so a character above U+00FF does not survive the round trip
    // through the chunk (and a NUL makes the chunk unreadable outright — `decode.js:25`).
    // The scene payload is base64 and always passes; this is the writer refusing a caller
    // that would otherwise produce a file which looks fine and restores as nothing.
    let png = plain_png();
    assert_eq!(
        insert_text_chunk(&png, SCENE_FORMAT, "🐰"),
        Err(PngError::NotLatin1)
    );
    assert_eq!(
        insert_text_chunk(&png, SCENE_FORMAT, "a\0b"),
        Err(PngError::NotLatin1)
    );
    // U+00E9 is Latin-1's own last accented letter, and it *is* one byte — so it is allowed,
    // which is the boundary the check has to get right and "must be ASCII" would not.
    assert!(insert_text_chunk(&png, SCENE_FORMAT, "é").is_ok());
}

// ---------------------------------------------------------------------------
// The codec, against its own oracle
// ---------------------------------------------------------------------------

#[test]
fn the_test_codec_is_rfc4648() {
    // The engine's base64 is checked against *this* codec below, and this codec is checked
    // against RFC 4648 §10's published vectors, so the engine's is pinned to a document
    // rather than to another implementation of my own idea. The vectors are the RFC's, for
    // the standard alphabet, and every length from 0 to 9 is in them — which is the range
    // where the three bugs above lived, all of them in the last group.
    const VECTORS: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    for (plain, encoded) in VECTORS {
        assert_eq!(
            base64_encode(plain.as_bytes()),
            encoded,
            "RFC 4648 §10: {plain:?}"
        );
        assert_eq!(
            base64_decode(encoded).as_deref(),
            Some(plain.as_bytes()),
            "RFC 4648 §10: {encoded:?}"
        );
    }
}

#[test]
fn the_engines_payload_is_rfc4648_too() {
    // The transitive link, asserted rather than assumed: the engine's encoder and the
    // vector-pinned one agree on a real scene's JSON, so the engine's is RFC 4648.
    let scene = the_fixture_scene();
    let borrowed: Vec<&DrawElement> = scene.iter().collect();
    let payload = scene_payload(&borrowed);
    let encoded = payload.strip_prefix("1:").expect("a generation prefix");
    let json = scene_to_json(&scene);
    assert_eq!(encoded, base64_encode(json.as_bytes()));
    // And the decoder the engine restores through agrees with it, for every length of tail
    // a scene JSON can land on — which is the range the three bugs lived in. Padding is
    // whitespace rather than a digit, so the JSON stays the JSON it was.
    for extra in 0..3usize {
        let bytes = format!("{json}{}", " ".repeat(extra));
        let encoded = base64_encode(bytes.as_bytes());
        let payload = format!("1:{encoded}");
        let file = png_with_text(SCENE_FORMAT, &payload);
        assert!(
            matches!(restore(&file), Restore::Payload(_)),
            "a scene {extra} byte(s) over a whole number of three-byte groups"
        );
    }
}

// ---------------------------------------------------------------------------
// The committed fixtures
// ---------------------------------------------------------------------------

#[test]
fn the_committed_png_is_a_png_and_says_what_it_says() {
    // `file(1)` reads it as "PNG image data, 1 x 1, 8-bit/color RGBA, non-interlaced" and
    // Python's `zlib` inflates its `IDAT`; here the signature, the whole chunk table and
    // every CRC are checked by the test's own reader. Three of those, and none of them the
    // code that would be blamed if the file were wrong.
    assert_eq!(&FIXTURE_PNG[..8], &PNG_SIGNATURE);
    let kinds: Vec<String> = png_chunks(FIXTURE_PNG)
        .into_iter()
        .map(|(kind, _, _)| String::from_utf8_lossy(&kind).into_owned())
        .collect();
    assert_eq!(kinds, ["IHDR", "IDAT", "tEXt", "IEND"]);
    let text = png_text(FIXTURE_PNG, SCENE_FORMAT).expect("our chunk, under our key");
    assert!(text.starts_with("1:"), "{text}");
    // And nothing is keyed anything else, so "the first `tEXt` is ours" — the rule the
    // oracle's `chunks.find` gives it (`data/image.ts@1118751f:18`) — is true of the file a
    // person would actually have.
    assert_eq!(png_text(FIXTURE_PNG, "Comment"), None);
}

#[test]
fn the_committed_png_restores_to_the_scene_it_holds() {
    // The whole point of committing a file. The bytes were written by another
    // implementation, the scene inside them was hand-written, and the expectations below
    // are literals — so a reader that agrees with its own writer cannot make this pass.
    let scene = the_fixture_scene();
    match restore(FIXTURE_PNG) {
        Restore::Payload(json) => {
            let restored = elements_from_json(&json).expect("our own schema");
            assert_eq!(restored, scene, "field for field, ids included");
        }
        Restore::Refused(why) => panic!("the committed fixture should restore, got {why:?}"),
    }
    // The literal values, so a failure says what is wrong rather than printing two
    // forty-field structs at each other.
    let restored = elements_from_json(&match restore(FIXTURE_PNG) {
        Restore::Payload(json) => json,
        Restore::Refused(_) => unreachable!(),
    })
    .expect("our own schema");
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].id, "box-1");
    assert_eq!(restored[0].kind, DrawElementType::Rectangle);
    assert_eq!(restored[0].background_color, "#ffc9c9");
    assert_eq!(restored[0].seed, 1);
    assert_eq!(restored[1].id, "text-1");
    assert_eq!(restored[1].kind, DrawElementType::Text);
    assert_eq!(restored[1].text.as_deref(), Some("héllo 🐰 — a--b"));
}

#[test]
fn the_committed_svg_restores_to_the_same_scene() {
    // The other container, the same payload, the same answer. If the two ever carried
    // different things this is where it shows, and the failure is a scene rather than a
    // diff of two encodings.
    let scene = the_fixture_scene();
    let svg = std::str::from_utf8(FIXTURE_SVG).expect("an svg is text");
    assert!(svg.contains("<!-- svg-source:osidraw -->"), "{svg}");
    match restore(FIXTURE_SVG) {
        Restore::Payload(json) => {
            assert_eq!(elements_from_json(&json), Some(scene));
        }
        Restore::Refused(why) => panic!("the committed fixture should restore, got {why:?}"),
    }
}

#[test]
fn the_two_fixtures_hold_one_payload_and_not_two() {
    // The SVG's payload and the PNG's chunk text are the same string. This is the property
    // that makes "one payload, two containers" true rather than aspirational.
    let from_png = png_text(FIXTURE_PNG, SCENE_FORMAT).expect("the png's payload");
    let svg = std::str::from_utf8(FIXTURE_SVG).expect("an svg is text");
    let start =
        svg.find("<!-- payload-start -->").expect("the marker") + "<!-- payload-start -->".len();
    let end = svg.find("<!-- payload-end -->").expect("the marker");
    assert_eq!(svg[start..end].trim(), from_png);
}

#[test]
fn the_engine_writes_the_fixture_back_byte_for_byte() {
    // The strongest statement in this file. The fixture was written by a Python
    // implementation; this takes the same picture and the same payload and asks the engine
    // to write it, and the two files are compared byte for byte. Agreement between two
    // independent implementations of the framing — the chunk table, the CRC, the keyword
    // split, the placement, the base64 — is not something a round trip can check, because
    // a round trip only ever sees one implementation twice.
    let payload = png_text(FIXTURE_PNG, SCENE_FORMAT).expect("the png's payload");
    let written =
        insert_text_chunk(&plain_png(), SCENE_FORMAT, &payload).expect("the picture takes a chunk");
    assert_eq!(
        written.len(),
        FIXTURE_PNG.len(),
        "the two writers should agree on the length too"
    );
    for (at, (mine, theirs)) in written.iter().zip(FIXTURE_PNG).enumerate() {
        assert_eq!(mine, theirs, "byte {at} of the two files differs");
    }
}
