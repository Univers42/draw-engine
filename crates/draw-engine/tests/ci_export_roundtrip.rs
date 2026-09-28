//! Round trip: the scene in the file, and the file read back.
//!
//! The oracle's is `packages/excalidraw/data/image.ts@1118751f`, 71 lines and three exports,
//! and the shape of it is quoted throughout this crate:
//!
//! - `getTEXtChunk` (`:14-23`) is `chunks.find(chunk => chunk.name === "tEXt")` and then
//!   `tEXt.decode`, and it answers `null` for a PNG with no such chunk — a different thing
//!   from a corrupt one, and `image.ts:51` is what tells them apart.
//! - `encodePngMetadata` (`:25-47`) keys the chunk with `MIME_TYPES.excalidraw` and writes
//!   `JSON.stringify(encode({ text: metadata, compress: true }))`, then splices it in
//!   **before the last chunk** (`:44`, "insert metadata before last chunk (iEND)").
//! - `decodePngMetadata` (`:49-71`) is the three-way decision this file pins: the keyword
//!   must match or it throws `INVALID` (`:70`); inside the match it `JSON.parse`s and asks
//!   `!("encoded" in encodedData)` (`:54`) — **no `encoded` means the payload is legacy,
//!   un-encoded scene JSON**, accepted only when `type === EXPORT_DATA_TYPES.excalidraw`
//!   (`:57-58`, `constants.ts@1118751f:342`) — and anything that throws becomes `FAILED`
//!   (`:62, 67`).
//!
//! ## What we write, and the one thing we cannot
//!
//! [`SCENE_FORMAT`] is `osidraw` and not the oracle's `application/vnd.excalidraw+json`,
//! and [`SCENE_PAYLOAD_VERSION`] is `1`. The oracle's current generation is
//! `encode({compress: true})` — **pako zlib** (`data/encode.ts@1118751f:99-121`,
//! `pako@2.0.3`) — a byte string in a JSON envelope. Reading that needs an inflater, and
//! this crate has no compressor and may not add one (BUNNY.md §3.2), so our payload is
//! generation 1, **uncompressed**: base64 of the scene JSON, named by a prefix. That is a
//! divergence from the oracle; `docs/reference/export.md` records it as a limitation with
//! its cause, and it is not hidden here.
//!
//! Base64 is not decoration and the reason is in the package the oracle itself uses:
//! a `tEXt` chunk is **Latin-1 with no NUL** (`png-chunk-text@1.0.0`, `encode.js:15-41`
//! writes `keyword ++ 0x00 ++ content`; `decode.js:25` throws on a NUL inside the text). A
//! scene with "héllo 🐰" in it is UTF-8 and not Latin-1: written raw it comes back as
//! mojibake, and a text element holding a NUL makes the chunk unreadable to any
//! conforming reader. The base64 alphabet has neither problem, and the same payload then
//! goes in the SVG, where an XML comment cannot contain `--` and scene text can.
//!
//! ## Why the failures are three and not two
//!
//! The oracle has two and collapses "no chunk" into `INVALID`: `image.ts:51` fails the
//! keyword test just as well when `getTEXtChunk` answered `null` as when it answered
//! someone else's keyword. So do we — those really are one fact, "this file carries no
//! scene of ours". The third is a file that is not a PNG at all, which the oracle reports
//! as a raw throw from `png-chunks-extract`, because `getTEXtChunk` is awaited outside its
//! `try` block (`:50`). A person has to be told "that is not a picture" rather than "that
//! is a picture, but it is not a drawing", so it is its own answer here.
//!
//! **Nothing in this file returns an empty scene.** That is the silent failure worth the
//! most: a restore that "works" by yielding nothing is indistinguishable from a fresh
//! board, and the owner finds it by opening a file and finding nothing there. An
//! `elements: []` is refused, for the reason the oracle refuses to export one
//! (`data/index.ts@1118751f:120-122`, `alerts.cannotExportEmptyCanvas`).
//!
//! The bytes themselves — that the chunk is there, keyed ours, with a CRC a third
//! implementation agrees with — are `ci_export_roundtrip_bytes.rs`. This file is the
//! decisions; that one is the evidence that the decisions reached the disk.

mod common;
use common::png::*;
use common::*;
use draw_engine::*;

/// The canonical scene: two elements, a box and a text.
///
/// Built here rather than written out, so `a_scene_survives_a_round_trip_field_for_field`
/// is a comparison and not a description. The text is not ASCII on purpose and holds `--`
/// as well, because the payload has to survive both a Latin-1 chunk and an XML comment.
fn a_scene() -> Vec<DrawElement> {
    let mut text = text_saying("héllo 🐰 — a--b");
    text.id = "text-1".into();
    text.x = 120.0;
    text.y = 40.0;
    text.width = 200.0;
    text.height = 30.0;
    let mut boxy = box_at(0.0, 0.0, 100.0, 50.0);
    boxy.id = "box-1".into();
    vec![boxy, text]
}

/// One text element holding `text`, so a case can name the characters it is about.
fn text_saying(text: &str) -> DrawElement {
    let mut element = text_at(0.0, 0.0, 100.0, 30.0);
    element.text = Some(text.to_string());
    element
}

/// The payload for `scene`, exactly as the engine writes one.
fn payload_of(scene: &[DrawElement]) -> String {
    let borrowed: Vec<&DrawElement> = scene.iter().collect();
    scene_payload(&borrowed)
}

/// A PNG carrying `payload` under our key — the file a person saves and opens again.
fn embed(payload: &str) -> Vec<u8> {
    png_with_text(SCENE_FORMAT, payload)
}

/// The scene `payload` restores to, or `None` if it is refused or comes back empty.
fn restored(payload: &str) -> Option<Vec<DrawElement>> {
    match restore(&embed(payload)) {
        Restore::Payload(json) => elements_from_json(&json),
        Restore::Refused(_) => None,
    }
}

/// An SVG of `scene` with the payload embedded or left out, the oracle's `exportEmbedScene`.
fn svg_of_scene(scene: &[DrawElement], embed_scene: bool) -> String {
    let engine = engine_with_scene(scene.to_vec());
    let options = ExportOptions {
        embed_scene,
        ..Default::default()
    };
    engine
        .export_svg_of(&engine.export_scope(false, &options), &options)
        .expect("a scene with elements exports")
}

// ---------------------------------------------------------------------------
// The key, and the version
// ---------------------------------------------------------------------------

#[test]
fn the_chunk_is_keyed_with_our_format_and_not_the_oracles() {
    // `MIME_TYPES.excalidraw` is `application/vnd.excalidraw+json`
    // (`constants.ts@1118751f:310`). Ours is `osidraw`, the word our own scene JSON already
    // says in its `type` (`export/json.rs`), because the key is a claim about what is inside
    // and what is inside is not an Excalidraw scene — the oracle's own reader would reject
    // it at `image.ts:111`… `:57-58`, which asks for `type === "excalidraw"`.
    assert_eq!(SCENE_FORMAT, "osidraw");
    assert_ne!(SCENE_FORMAT, "application/vnd.excalidraw+json");
}

#[test]
fn the_payload_names_the_generation_it_is_in() {
    // The oracle keeps the container's version and the scene's version apart on purpose: the
    // `version: "1"` at `encode.ts@1118751f:116` wraps the bytes, while `version` inside the
    // scene belongs to the scene. Ours has the same two, and this prefix is the PNG's half
    // of the oracle's `payload-version` comment (`export.ts@1118751f:525`) — so a future
    // compressed generation is one more arm of one match rather than a guess.
    let payload = payload_of(&a_scene());
    assert!(
        payload.starts_with("1:"),
        "the payload should name its generation, got {payload}"
    );
    assert_eq!(SCENE_PAYLOAD_VERSION, "1");
}

#[test]
fn the_payload_is_base64_and_holds_the_scene_json() {
    let payload = payload_of(&a_scene());
    let json = base64_decode(payload.split_once(':').expect("a generation").1)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .expect("the payload is base64 of the scene JSON");
    let file: serde_json::Value = serde_json::from_str(&json).expect("the payload is JSON");
    assert_eq!(file["type"], "osidraw");
    let elements = file["elements"].as_array().expect("an elements array");
    assert_eq!(elements.len(), 2);
    let ids: Vec<&str> = elements
        .iter()
        .map(|element| element["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, ["box-1", "text-1"]);
}

#[test]
fn the_payload_holds_no_byte_a_latin1_chunk_cannot_carry() {
    // `png-chunk-text@1.0.0`'s own rule: Latin-1, and no NUL (`encode.js:7-13, 31-34`).
    // Without this an emoji comes back as four Latin-1 characters, and a text element
    // holding a NUL makes the chunk unreadable to the oracle as well as to us.
    for scene in [
        a_scene(),
        vec![text_saying("nul\0inside")],
        vec![text_saying("plain")],
    ] {
        let payload = payload_of(&scene);
        assert!(
            payload.is_ascii(),
            "a tEXt payload must be ASCII for a Latin-1 chunk: {payload:?}"
        );
        assert!(!payload.contains('\0'), "no NUL in a tEXt text");
    }
}

// ---------------------------------------------------------------------------
// The round trip
// ---------------------------------------------------------------------------

#[test]
fn a_scene_survives_a_round_trip_field_for_field() {
    let scene = a_scene();
    let restored = restored(&payload_of(&scene)).expect("the scene restores");
    // Every field, every element, ids included. `DrawElement` is `PartialEq` over the whole
    // struct, so one `assert_eq!` is the strongest comparison available and the only honest
    // spelling of "the same drawing".
    assert_eq!(restored, scene);
}

#[test]
fn a_scene_of_one_element_survives() {
    let scene = vec![box_at(5.0, 6.0, 7.0, 8.0)];
    assert_eq!(restored(&payload_of(&scene)), Some(scene));
}

#[test]
fn a_scene_with_an_emoji_survives_its_text_intact() {
    let scene = vec![text_saying("héllo 🐰 — a--b")];
    let restored = restored(&payload_of(&scene)).expect("the scene restores");
    assert_eq!(restored[0].text.as_deref(), Some("héllo 🐰 — a--b"));
}

#[test]
fn the_scene_s_own_version_is_not_a_gate() {
    // The oracle's legacy arm exists because *it* changed its payload once
    // (`image.ts@1118751f:54-58`). The same promise has to be keepable here, and it is kept
    // by not gating on the scene's `version` at all: `OsidrawFile.version` is metadata
    // (`export/json.rs`) and a file written by a build that knew a later number still opens.
    let json = r##"{"type":"osidraw","version":99,"elements":[{"id":"later","type":"rectangle",
        "x":1,"y":2,"width":3,"height":4,"angle":0,"strokeColor":"#1e1e1e",
        "backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,
        "strokeStyle":"solid","roughness":1,"opacity":100,"isDeleted":false,"seed":1,
        "roundness":null,"version":1,"versionNonce":1,"updated":1}]}"##;
    let payload = format!("1:{}", base64_encode(json.as_bytes()));
    let elements = restored(&payload).expect("a scene from a later build still opens");
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].id, "later");
}

// ---------------------------------------------------------------------------
// The failure matrix. Five cases, one answer each, pinned.
// ---------------------------------------------------------------------------

#[test]
fn a_png_with_no_text_chunk_is_not_ours() {
    // `getTEXtChunk` answering `null` (`image.ts:111`… `:19-22`) and `decodePngMetadata`
    // then failing the keyword test (`:51`): a picture with no scene in it is `INVALID`, and
    // never a blank board.
    assert_eq!(
        restore(&plain_png()),
        Restore::Refused(RestoreRefusal::NotOurs)
    );
}

#[test]
fn a_text_chunk_under_someone_elses_key_is_not_ours() {
    // The oracle's own `tEXt` keyword in a file we are handed: right container, foreign key.
    // `image.ts:111`… `:51` throws `INVALID` for it, and so do we — one answer for both,
    // because both are "this file carries no scene of ours".
    let foreign = png_with_text("Comment", "a plain exported picture, with no scene in it");
    assert_eq!(restore(&foreign), Restore::Refused(RestoreRefusal::NotOurs));
}

#[test]
fn a_text_chunk_with_our_key_and_garbage_is_unreadable() {
    // Ours, and we cannot read it — the oracle's `FAILED` (`image.ts:111`… `:62, 67`) for
    // the same shape, whose `try` wraps the `JSON.parse` and everything after it.
    for garbage in [
        "1:",
        "1:!!!!not base64!!!!",
        "1:e30",  // not a whole group
        "0:e30=", // a generation we have not written
        "1:e30=", // base64 of `{}`, and not our payload
    ] {
        assert_eq!(
            restore(&embed(garbage)),
            Restore::Refused(RestoreRefusal::Unreadable),
            "{garbage:?} carries our key and is not a drawing we can open"
        );
    }
}

#[test]
fn a_png_carrying_a_scene_with_no_elements_is_unreadable() {
    // The silent failure, refused on purpose. A restore that yields nothing looks exactly
    // like a fresh board, and the oracle refuses to export one for the same reason
    // (`data/index.ts@1118751f:120-122`).
    let empty = format!(
        "1:{}",
        base64_encode(br#"{"type":"osidraw","version":1,"elements":[]}"#)
    );
    assert_eq!(
        restore(&embed(&empty)),
        Restore::Refused(RestoreRefusal::Unreadable)
    );
}

#[test]
fn bytes_that_are_not_a_png_are_malformed() {
    // Neither of the oracle's answers. `png-chunks-extract` throws on these and the rejection
    // escapes `decodePngMetadata` uncaught, because `getTEXtChunk` is awaited outside its
    // `try` (`image.ts:111`… `:50`): a raw library error rather than `INVALID`, and a person
    // needs to be told it is not a picture.
    for bytes in [
        Vec::new(),
        b"not a png at all".to_vec(),
        PNG_SIGNATURE.to_vec(),
        b"{\"type\":\"osidraw\"}".to_vec(),
    ] {
        assert_eq!(
            restore(&bytes),
            Restore::Refused(RestoreRefusal::Malformed),
            "{bytes:?} is not a file we can walk"
        );
    }
}

#[test]
fn a_png_cut_short_anywhere_is_never_silently_a_scene() {
    // Every truncation of a good file, at every length. The property is one line — nothing
    // here may answer `Payload` — and the sweep is what stops a length from going untested.
    let full = embed(&payload_of(&a_scene()));
    for cut in 0..full.len() {
        match restore(&full[..cut]) {
            Restore::Payload(json) => {
                panic!(
                    "a PNG cut to {cut} of {} bytes restored: {json}",
                    full.len()
                )
            }
            Restore::Refused(_) => {}
        }
    }
}

#[test]
fn a_payload_generation_this_build_does_not_know_is_unreadable() {
    // The rule a future generation has to keep: every version we have ever written stays
    // readable, and a version we do not know is refused rather than misread. Both halves,
    // because "unreadable" is only safe if it is what actually happens.
    for unknown in ["2:e30=", "99:e30=", "x:e30=", ":e30=", "e30=", "1 :e30="] {
        assert_eq!(
            restore(&embed(unknown)),
            Restore::Refused(RestoreRefusal::Unreadable),
            "{unknown:?} names no generation this build has"
        );
    }
}

#[test]
fn the_chunk_s_text_is_read_whole_however_long_the_scene_is() {
    // A scene big enough that its base64 runs past a single PNG chunk's comfort is still
    // one chunk, not two: `find` takes the first `tEXt` (`image.ts:111`… `:18`), so a
    // second one would be invisible. One chunk per file, always.
    let scene: Vec<DrawElement> = (0..200)
        .map(|i| box_at(i as f64, 0.0, 10.0, 10.0))
        .collect();
    let payload = payload_of(&scene);
    let file = png_with_text(SCENE_FORMAT, &payload);
    let text_chunks = png_chunks(&file)
        .into_iter()
        .filter(|(kind, _, _)| kind == b"tEXt")
        .count();
    assert_eq!(
        text_chunks, 1,
        "one scene, one chunk, however long the scene"
    );
    assert_eq!(restored(&payload), Some(scene));
}

// ---------------------------------------------------------------------------
// The SVG's half
// ---------------------------------------------------------------------------

#[test]
fn an_exported_svg_carries_the_scene_in_its_metadata_element() {
    // `encodeSvgBase64Payload` (`export.ts@1118751f:510-529`) appends four comments and a
    // text node to the `<metadata>` the root already carries (`:363-369`). Ours is the same
    // shape with our own name in it.
    let svg = svg_of_scene(&a_scene(), true);
    assert!(
        svg.contains("<metadata>"),
        "the root carries a metadata element"
    );
    assert!(svg.contains("<!-- payload-type:osidraw -->"), "{svg}");
    assert!(
        svg.contains("<!-- payload-version:1 -->"),
        "the payload names its generation, as the oracle's does (`export.ts:525`)"
    );
    assert!(svg.contains("<!-- payload-start -->"), "{svg}");
    assert!(svg.contains("<!-- payload-end -->"), "{svg}");
    assert!(
        svg.contains("<!-- svg-source:osidraw -->"),
        "the root says where it came from, as the oracle's does (`export.ts:368`)"
    );
}

#[test]
fn the_svg_and_the_png_carry_one_payload_and_not_two() {
    // The point of building the payload in one place: one string, one generation prefix, one
    // alphabet, so the two containers cannot drift into holding different things.
    let scene = a_scene();
    let payload = payload_of(&scene);
    assert!(
        svg_of_scene(&scene, true).contains(&payload),
        "the SVG should carry the very bytes the PNG chunk does"
    );
    assert_eq!(restored(&payload), Some(scene));
}

#[test]
fn the_svg_metadata_element_is_there_whether_or_not_it_holds_a_scene() {
    // `exportToSvg` appends `<metadata>` unconditionally (`export.ts:111`… `:369`) and only
    // fills it when `exportEmbedScene` is set (`:378-391`). One document shape either way,
    // so "has a metadata element" is not a second thing to be right about.
    let bare = svg_of_scene(&a_scene(), false);
    assert!(bare.contains("<metadata></metadata>"), "{bare}");
    assert!(!bare.contains("payload-start"), "{bare}");
}

#[test]
fn the_base64_payload_cannot_terminate_the_comment_it_lives_in() {
    // The reason the payload is base64 and not the scene text. `--` ends an XML comment, and
    // a text element may hold one; the base64 alphabet has no `-` at all, so the closing
    // marker is unfindable inside the payload and the document stays well formed.
    let scene = vec![text_saying("a--b <!-- payload-end --> <script>")];
    let svg = svg_of_scene(&scene, true);
    assert_eq!(svg.matches("<!-- payload-end -->").count(), 1);
    assert!(!svg.contains("<script>"), "{svg}");
    match restore(svg.as_bytes()) {
        Restore::Payload(json) => assert_eq!(elements_from_json(&json), Some(scene)),
        Restore::Refused(why) => panic!("an SVG we just wrote should restore, got {why:?}"),
    }
}

#[test]
fn an_svg_with_no_metadata_is_not_ours() {
    // The oracle's `decodeSvgBase64Payload` finds no `payload-type:` and throws `INVALID`
    // (`export.ts:1118751f:532, 562`) — the same third refusal a PNG with no chunk is.
    let bare = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
    assert_eq!(
        restore(bare.as_bytes()),
        Restore::Refused(RestoreRefusal::NotOurs)
    );
}

#[test]
fn an_svg_whose_payload_is_cut_short_is_unreadable() {
    let svg = svg_of_scene(&a_scene(), true);
    let end = svg.find("<!-- payload-end -->").expect("the marker");
    for cut in 1..=4 {
        let truncated = format!("{}{}", &svg[..end - cut], &svg[end..]);
        assert_eq!(
            restore(truncated.as_bytes()),
            Restore::Refused(RestoreRefusal::Unreadable),
            "cutting {cut} off the payload must not yield a scene"
        );
    }
}

#[test]
fn an_svg_under_another_programs_payload_type_is_not_ours() {
    // The oracle's own marker in a file we are handed: `payload-type:` names a format, and
    // a name that is not ours is `INVALID` (`export.ts:1118751f:532, 562`).
    let svg =
        svg_of_scene(&a_scene(), true).replace("payload-type:osidraw", "payload-type:inkscape");
    assert_eq!(
        restore(svg.as_bytes()),
        Restore::Refused(RestoreRefusal::NotOurs)
    );
}

#[test]
fn an_svg_that_is_not_an_svg_is_malformed() {
    for bytes in [
        "just some text",
        "<html><body>not a drawing</body></html>",
        "{\"type\":\"osidraw\"}",
    ] {
        assert_eq!(
            restore(bytes.as_bytes()),
            Restore::Refused(RestoreRefusal::Malformed),
            "{bytes:?} is not a file we can walk"
        );
    }
}

#[test]
fn the_clipboard_svg_carries_no_scene() {
    // `exportEmbedScene: appState.exportEmbedScene && type === "svg"`
    // (`data/index.ts@1118751f:132`), so a *clipboard* SVG is `type === "clipboard-svg"` and
    // embeds nothing. Ours matches: a copy is a copy, and the scene travels in the
    // `.osidraw` file.
    let scene = a_scene();
    let engine = engine_with_scene(scene);
    let copy = engine.clipboard_copy(
        ClipboardFormat::Svg,
        &ClipboardHost {
            can_write_blob: true,
            can_write_text: true,
        },
        &ExportOptions::default(),
    );
    let text = copy.text.expect("a supported copy has a payload");
    assert!(!text.contains("payload-start"), "{text}");
    assert!(
        !text.contains("payload-end"),
        "a clipboard copy has no payload to close: {text}"
    );
    assert_eq!(
        restore(text.as_bytes()),
        Restore::Refused(RestoreRefusal::NotOurs)
    );
}
