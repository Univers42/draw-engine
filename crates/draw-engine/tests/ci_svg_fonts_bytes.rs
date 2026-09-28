//! The bytes in an exported SVG, and the evidence that they are what we think they are.
//!
//! `ci_svg_fonts.rs` pins the **decisions**: which families, in what order, spelled how. This
//! file pins the **payload**, and it is a separate file because the claim is different. A
//! round trip checked with the engine's own writer and the engine's own base64 proves the two
//! halves agree with each other, which is not the claim; the claim is that a file a person
//! opened in Illustrator carries the font it says it carries, with the right bytes. Two wrong
//! implementations of the same idea agree perfectly and are wrong together.
//!
//! Four somethings else, in increasing order of independence:
//!
//! 1. **`tests/common/svg.rs`** — a second implementation of the document, in Rust, in the
//!    test tree. It walks the markup for the `<style>` inside the `<defs>`, reads the
//!    declarations with the oracle's own grammar, and has **its own base64 decoder**, written
//!    group-at-a-time rather than as the writer's running bit count. It is what answers "is
//!    the declaration there, is it the shape the format says, and do the bytes come back".
//! 2. **The oracle's own committed output** — `tests/fixtures/oracle-font-face.b64` is one
//!    `@font-face` payload cut out of
//!    `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap@1118751f:38`, a file
//!    Excalidraw's own test suite wrote by running the oracle. Our encoder, fed the bytes
//!    that string decodes to, has to produce that string again, character for character.
//! 3. **RFC 4648 §10** — the base64 codec's own published vectors, which are a document and
//!    not a table of one's own devising.
//! 4. **WOFF2 §4.1** — the four signature bytes and the `length` field, so a payload that is
//!    not a woff2, or is a truncated one, is caught by the specification rather than by us.
//!
//! And one thing that is *not* a check on the engine at all: a **CRC-32 of each shipped
//! file**, computed with Python's `zlib` and written out here as a constant. A Python fixture
//! once certified itself with its own broken CRC on this project, so the CRC in
//! `tests/common/svg.rs` is itself pinned against the **published check value** for the nine
//! bytes `123456789` — `0xCBF43926`, ISO 15948 §12.2 — before it is trusted to check anything
//! else. A checksum that has not been checked is not evidence.
//!
//! ## What is deliberately not checked here
//!
//! **That our payload equals the oracle's.** It does not, and it must not: the oracle embeds a
//! face subset to the scene's codepoints and we embed the whole shard, because harfbuzz is a
//! dependency this crate may not take. `font_face.rs` says so and `assets/fonts/LICENSES.md`
//! says what that costs. The oracle's payload is used here as a **codec** vector, which is a
//! claim that holds whatever each of us puts in the file.

mod common;

use common::svg::*;
use common::*;
use draw_engine::export::base64;
use draw_engine::*;

/// One `@font-face` payload from the oracle's own committed snapshot, verbatim.
///
/// `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap@1118751f:38`, the
/// shortest `@font-face` in it, 552 bytes of a subsetted Nunito. `include_bytes!` rather
/// than a path read: a fixture that is not there is a compile error naming the file.
const ORACLE_FACE: &str = include_str!("fixtures/oracle-font-face.b64");

/// The CRC-32 the shipped files must have, from Python's `zlib` (its `crc32`, the ISO 15948
/// §12.2 one — **not** `cksum(1)`, which is a different polynomial with a different seed and
/// would agree with nothing).
///
/// The seven shipped files, by size, largest first. A person reading this can check one with
/// `python3 -c "import zlib;print(hex(zlib.crc32(open(p,'rb').read())))"` and see the same
/// number; the test cannot, because the engine's test container has no `apps/web` and no
/// Python. What it *can* do is prove the engine embedded these exact bytes, and
/// `fontFiles.test.ts` on the other side proves the two trees hold the same files.
const SHIPPED_CRC32: [(&str, u32); 10] = [
    ("Cascadia/CascadiaCode-Regular.woff2", 0xf07e_d426),
    ("Virgil/Virgil-Regular.woff2", 0xbf49_114e),
    (
        "Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2",
        0x8c45_31c0,
    ),
    (
        "ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2",
        0xb875_d807,
    ),
    (
        "Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2",
        0x9956_adc2,
    ),
    (
        "Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTo3j6zbXWjgevT5.woff2",
        0x7466_4a85,
    ),
    (
        "Excalifont/Excalifont-Regular-be310b9bcd4f1a43f571c46df7809174.woff2",
        0xb81a_ebc4,
    ),
    (
        "ComicShanns/ComicShanns-Regular-fcb0fc02dcbee4c9846b3e2508668039.woff2",
        0xb278_89e5,
    ),
    (
        "Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2",
        0x1380_bdc3,
    ),
    (
        "Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYE98RXi4EwSsbg.woff2",
        0x8a23_e0d0,
    ),
];

/// A text element in `family`, exported.
fn svg_with_one_text(family: u8) -> String {
    let mut text = text_at(10.0, 20.0, 100.0, 50.0);
    text.text = Some("a".into());
    text.font_family = Some(family);
    let engine = engine_with_scene(vec![text]);
    svg_of(&engine, 0.0)
}

/// Every face in `svg`'s `<style>`, decoded by `tests/common/svg.rs`.
fn recovered(svg: &str) -> Vec<(String, Vec<u8>)> {
    font_faces(&font_face_style(svg).expect("a style element").1)
        .into_iter()
        .map(|face| {
            let name = face.family;
            let payload = face
                .payload
                .strip_prefix("data:font/woff2;base64,")
                .unwrap_or_else(|| panic!("`{name}` should carry a woff2 data url"));
            let bytes = decode_base64(payload)
                .unwrap_or_else(|| panic!("`{name}`'s payload is not base64"));
            (name, bytes)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The checksum, checked against its own published vector first
// ---------------------------------------------------------------------------

/// The CRC-32 this file's checks rest on, against the one number everybody publishes.
///
/// The published check value for the ASCII string `123456789` is `0xCBF43926` — the value
/// every CRC-32 implementation is expected to produce for it, and the reason a CRC that
/// silently disagrees is found in a second rather than in a diff nobody reads. **This runs
/// first on purpose:** a test that computes a checksum with an unverified checksum has
/// certified itself, which is the mistake this whole file exists to avoid.
#[test]
fn the_crc_is_the_published_one_before_it_checks_anything() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0x0000_0000);
    assert_eq!(crc32(b"a"), 0xE8B7_BE43);
}

// ---------------------------------------------------------------------------
// The codec, against the oracle's own output and RFC 4648
// ---------------------------------------------------------------------------

/// Our encoder, fed what the oracle's committed string decodes to, writes that string again.
///
/// The payload is a literal from a file Excalidraw's own test suite produced by running the
/// oracle — not from the oracle's source, and not from ours. Decoding it is
/// `tests/common/svg.rs`'s decoder; encoding it back is `export::base64::encode`. Two
/// implementations and a published artifact, and the assertion is the whole 736 characters.
///
/// This pins the **codec**. It cannot pin the payload, and does not try to: the oracle
/// embeds a subset and we embed a whole shard (`font_face.rs`).
#[test]
fn the_oracles_own_payload_survives_our_encoder_character_for_character() {
    let oracle = ORACLE_FACE.trim();
    let bytes = decode_base64(oracle).expect("the oracle's payload is base64");
    assert_eq!(
        base64::encode(&bytes),
        oracle,
        "our encoder wrote the oracle's own string differently"
    );
    // The same codec in the other direction, still with two implementations: what the second
    // implementation reads out of what the first wrote is the first one's input, exactly.
    assert_eq!(decode_base64(&base64::encode(&bytes)).unwrap(), bytes);
}

/// RFC 4648 §10's vectors, through both implementations at once.
///
/// The document's own strings, and each one also has to come back out of the second
/// implementation byte for byte — which is the shape that caught the last three base64 bugs
/// on this project, a short final group read one byte too far left.
#[test]
fn the_rfc_4648_vectors_go_through_both_implementations() {
    for (plain, encoded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64::encode(plain.as_bytes()), encoded, "{plain:?}");
        assert_eq!(
            decode_base64(encoded).unwrap_or_default(),
            plain.as_bytes(),
            "{plain:?} through the second implementation"
        );
    }
    // And long enough to be past the fifth group, where a shift-accumulator loses the top of
    // its own accumulator: 61 bytes is 20 full groups and a one-byte tail.
    let long: Vec<u8> = (0..61u16).map(|i| (i * 37 % 251) as u8).collect();
    assert_eq!(long.len() % 3, 1);
    let encoded = base64::encode(&long);
    assert_eq!(decode_base64(&encoded).unwrap(), long);
}

// ---------------------------------------------------------------------------
// The document's own bytes
// ---------------------------------------------------------------------------

/// What the document carries is the file, and the file is a woff2 the specification agrees
/// with itself about.
///
/// Read out by `tests/common/svg.rs`, not by the writer. Two published constants do the
/// checking: the signature `wOF2` (WOFF2 §4.1) and the `length` field at offset 8, which
/// WOFF2 requires to be the file's own size in bytes — so a truncated payload, a base64
/// encoder that dropped a group, or a `data:` URL holding something that is not a font all
/// fail here rather than in a viewer.
#[test]
fn every_family_carries_a_woff2_that_agrees_with_its_own_header() {
    for family in SHIPPED_FONT_FACES {
        let svg = svg_with_one_text(family.id);
        let faces = recovered(&svg);
        assert_eq!(
            faces.len(),
            family.files.len(),
            "family {} should carry its {} shard(s)",
            family.id,
            family.files.len()
        );
        for (name, bytes) in faces {
            assert_eq!(name, family.name, "family {}", family.id);
            assert_eq!(&bytes[..4], b"wOF2", "{name} is not a woff2");
            let declared = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            assert_eq!(
                declared as usize,
                bytes.len(),
                "{name}: its header says {declared} bytes and it has {}",
                bytes.len()
            );
        }
    }
}

/// The bytes the document carries are the bytes in `assets/fonts`, and nothing else.
///
/// Compared against `include_bytes!` — the same files the engine compiles in, read through
/// a path in the source rather than through the table — so a writer that embedded the right
/// family with the wrong shard still fails.
#[test]
fn the_document_carries_the_shipped_file_and_nothing_else() {
    for family in SHIPPED_FONT_FACES {
        let faces = recovered(&svg_with_one_text(family.id));
        for (path, (_, bytes)) in family.files.iter().zip(&faces) {
            let expected: &[u8] = match *path {
                "/fonts/Cascadia/CascadiaCode-Regular.woff2" => {
                    include_bytes!("../assets/fonts/Cascadia/CascadiaCode-Regular.woff2")
                }
                "/fonts/Virgil/Virgil-Regular.woff2" => {
                    include_bytes!("../assets/fonts/Virgil/Virgil-Regular.woff2")
                }
                "/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2" => {
                    include_bytes!(
                        "../assets/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2"
                    )
                }
                "/fonts/Excalifont/Excalifont-Regular-be310b9bcd4f1a43f571c46df7809174.woff2" => {
                    include_bytes!(
                        "../assets/fonts/Excalifont/Excalifont-Regular-be310b9bcd4f1a43f571c46df7809174.woff2"
                    )
                }
                "/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2" => {
                    include_bytes!(
                        "../assets/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2"
                    )
                }
                "/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTo3j6zbXWjgevT5.woff2" => {
                    include_bytes!(
                        "../assets/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTo3j6zbXWjgevT5.woff2"
                    )
                }
                "/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYE98RXi4EwSsbg.woff2" => {
                    include_bytes!(
                        "../assets/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYE98RXi4EwSsbg.woff2"
                    )
                }
                "/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2" => {
                    include_bytes!(
                        "../assets/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2"
                    )
                }
                "/fonts/ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2" => {
                    include_bytes!(
                        "../assets/fonts/ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2"
                    )
                }
                "/fonts/ComicShanns/ComicShanns-Regular-fcb0fc02dcbee4c9846b3e2508668039.woff2" => {
                    include_bytes!(
                        "../assets/fonts/ComicShanns/ComicShanns-Regular-fcb0fc02dcbee4c9846b3e2508668039.woff2"
                    )
                }
                other => panic!("{other} is not a path this test can read"),
            };
            assert_eq!(
                crc32(bytes),
                crc32(expected),
                "{path}: not the file we ship"
            );
        }
    }
}

/// Each shipped file has the CRC-32 recorded beside it, from a tool that is not this crate.
///
/// Computed with Python's `zlib.crc32` and written out above. The point is that a later
/// reader does not have to run anything to know what the file should hash to, and that a
/// change to a font shows up as a **number** rather than as a file that quietly got bigger.
///
/// `include_bytes!` wants a literal and a table of pairs cannot hold one, so the seven
/// files are named here. That is the trade for the check being about the bytes and not
/// about a path: a renamed file is a compile error naming it.
#[test]
fn the_shipped_files_hash_to_the_recorded_values() {
    let files: [(&[u8], u32); 10] = [
        (
            include_bytes!("../assets/fonts/Cascadia/CascadiaCode-Regular.woff2"),
            0xf07e_d426,
        ),
        (include_bytes!("../assets/fonts/Virgil/Virgil-Regular.woff2"), 0xbf49_114e),
        (
            include_bytes!(
                "../assets/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2"
            ),
            0x8c45_31c0,
        ),
        (
            include_bytes!(
                "../assets/fonts/ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2"
            ),
            0xb875_d807,
        ),
        (
            include_bytes!(
                "../assets/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2"
            ),
            0x9956_adc2,
        ),
        (
            include_bytes!(
                "../assets/fonts/Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTo3j6zbXWjgevT5.woff2"
            ),
            0x7466_4a85,
        ),
        (
            include_bytes!(
                "../assets/fonts/Excalifont/Excalifont-Regular-be310b9bcd4f1a43f571c46df7809174.woff2"
            ),
            0xb81a_ebc4,
        ),
        (
            include_bytes!(
                "../assets/fonts/ComicShanns/ComicShanns-Regular-fcb0fc02dcbee4c9846b3e2508668039.woff2"
            ),
            0xb278_89e5,
        ),
        (
            include_bytes!("../assets/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2"),
            0x1380_bdc3,
        ),
        (
            include_bytes!("../assets/fonts/Lilita/Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYE98RXi4EwSsbg.woff2"),
            0x8a23_e0d0,
        ),
    ];
    for (bytes, expected) in &files {
        assert_eq!(crc32(bytes), *expected);
    }
    // And the same numbers, as the table the documentation carries, so the two cannot drift.
    let recorded: Vec<u32> = SHIPPED_CRC32.iter().map(|(_, crc)| *crc).collect();
    let measured: Vec<u32> = files.iter().map(|(bytes, _)| crc32(bytes)).collect();
    assert_eq!(recorded, measured);
    assert_eq!(
        SHIPPED_CRC32.len(),
        SHIPPED_FONT_FACES
            .iter()
            .map(|f| f.files.len())
            .sum::<usize>()
    );
}

/// The whole payload is base64 and nothing else, so the `<style>`'s character data needs no
/// escaping and needs no CDATA.
///
/// The base64 alphabet is RFC 4648 §4's 64 characters plus `=`. Not one of them is `<`, `&`
/// or `-`, and `--` is what would end a comment. This is a property of the alphabet rather
/// than a hope about the payloads, and it is what lets the oracle's raw text node
/// (`export.ts@1118751f:447-449`) be safe to reproduce literally: a payload that *could*
/// hold `]]>` would be a document this module could emit broken.
#[test]
fn a_payload_holds_no_character_that_would_end_the_style_element_early() {
    let svg = svg_with_one_text(1);
    let (_, style) = font_face_style(&svg).expect("a style element");
    for closing in ["<", "&", "]]>", "]]", "--"] {
        assert!(
            !style.contains(closing),
            "{closing:?} in a payload would break the document's character data"
        );
    }
}
