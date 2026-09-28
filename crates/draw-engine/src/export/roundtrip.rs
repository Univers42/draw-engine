//! Round trip: a scene written into a file the person saved, and read back out of the one
//! they open again.
//!
//! Two containers and **one** payload. The oracle has the same shape and the same two
//! doors — `encodePngMetadata` / `decodePngMetadata`
//! (`packages/excalidraw/data/image.ts@1118751f:25-71`) for the raster, and
//! `encodeSvgBase64Payload` / `decodeSvgBase64Payload`
//! (`packages/excalidraw/scene/export.ts@1118751f:510-563`) for the vector — and both of
//! the oracle's write the *same* `encode({text: payload})` string, base64'd, into a
//! container that names its own payload version. One payload is the property worth keeping:
//! two encoders would be two things to keep in step, and a file that had drifted between
//! them would still be a file.
//!
//! ## The three generations, and the one we cannot write
//!
//! | generation | what the payload is | the oracle | ours |
//! | ---------- | ------------------ | --------- | ----- |
//! | 1 | scene JSON, plain | `image.ts:111`… `:54-63`, the "legacy, un-encoded scene JSON" arm, accepted when `type === "excalidraw"` | **what we write** — base64 of the JSON, uncompressed |
//! | 2 | `encode({compress: true})`: a zlib byte string in a JSON envelope | `image.ts:111`… `:37-41`, `data/encode.ts@1118751f:99-121` | **not written, and not readable** |
//! | — | no payload | `INVALID` (`:70`) | [`RestoreRefusal::NotOurs`] |
//!
//! Generation 2 is pako's `deflate` — a **zlib** stream — and reading it needs an inflater.
//! This crate has no compressor and may not add one (BUNNY.md §3.2, library-first), so
//! generation 2 is a named limitation rather than a missing feature, and
//! `docs/reference/export.md` carries it. The cost is exact and it is on the *container*,
//! not the scene: a scene saved by Excalidraw as a PNG reaches us with its scene JSON
//! intact inside a zlib envelope we cannot open, and the refusal below says so instead of
//! returning a blank board.
//!
//! ## Why the payload is base64, which is not the oracle's shape
//!
//! Two constraints, both reachable, neither decoration:
//!
//! - a `tEXt` chunk is **Latin-1 with no NUL** (`png-chunk-text@1.0.0`, `encode.js:7-13,
//!   31-34`), and scene text is UTF-8 — so an emoji written raw comes back as four Latin-1
//!   characters, and a text element holding a NUL makes the chunk unreadable to any
//!   conforming reader;
//! - the SVG's payload lives inside an XML comment, and `--` closes a comment, and a text
//!   element may well hold `--`.
//!
//! The base64 alphabet is ASCII with no `-` and no NUL, so one encoding satisfies both and
//! one function serves both containers.
//!
//! ## What is deliberately refused
//!
//! **A payload with no elements.** A restore that yields nothing is indistinguishable from
//! a fresh board, and the owner finds that by opening a file and finding nothing there. The
//! oracle refuses to *export* an empty scene for the same reason
//! (`data/index.ts@1118751f:120-122`, `alerts.cannotExportEmptyCanvas`), so an empty
//! payload is a file this engine did not write.
//!
//! **A scene's own `version`.** It is not a gate. The oracle's legacy arm exists because
//! *it* changed its payload once, and the promise that keeps holdable here is the same:
//! every generation this module has ever written stays readable, and one it does not know
//! is refused rather than misread.

use crate::export::png_chunk::{self, PNG_SIGNATURE};
use crate::scene::element::DrawElement;

/// The key this engine writes its scene under, in both containers.
///
/// **Not the oracle's `MIME_TYPES.excalidraw` (`application/vnd.excalidraw+json`,
/// `constants.ts@1118751f:310`)**, and the reason is that a key is a claim about what is
/// inside. What is inside is an `osidraw` scene — the word our own scene JSON already says
/// in its `type` (`export/json.rs`) — and the oracle's own reader would reject it three
/// lines later for exactly that reason (`image.ts@111`… `:57-58` asks for `type ===
/// "excalidraw"`). Borrowing the key would buy nothing and cost a clean "this file is not
/// mine" in exchange for a claim we cannot keep. It is also 7 bytes against 40, and it is
/// not a MIME type, so nothing that sniffs types will reach for it.
pub const SCENE_FORMAT: &str = "osidraw";

/// The payload generation this build writes and reads: base64 of the scene JSON.
///
/// The oracle's SVG carries the same number in a comment
/// (`<!-- payload-version:1 -->`, `export.ts@1118751f:525, 540`) and its reader uses it to
/// decide how to decode (`isByteString = version !== "1"`, `:541`). Ours is the uncompressed
/// branch of that same idea: the prefix names the generation, so a future compressed one is
/// one more arm of one match rather than a guess about the bytes.
pub const SCENE_PAYLOAD_VERSION: &str = "1";

/// Why a file could not be opened, in the three answers there are.
///
/// The oracle has two — `INVALID` and `FAILED` (`image.ts@111`… `:62, 67, 70`) — and
/// collapses "no chunk" into `INVALID`, because `:51`'s keyword test fails just as well
/// when `getTEXtChunk` answered `null` as when it answered someone else's keyword. That
/// collapse is kept: both are one fact, *this file carries no scene of ours*.
///
/// The third is a file that is not an image at all. The oracle reports that as a raw throw
/// from `png-chunks-extract`, which escapes `decodePngMetadata` uncaught because
/// `getTEXtChunk` is awaited outside its `try` (`image.ts:111`… `:50`), and a person is
/// told a library name instead of being told that the file is not a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreRefusal {
    /// Not a PNG we can walk, or not an SVG document. The oracle's uncaught throw.
    Malformed,
    /// A picture, or a drawing, with no scene of ours in it. The oracle's `INVALID`.
    NotOurs,
    /// Ours, and we cannot read it. The oracle's `FAILED`.
    Unreadable,
}

/// What came back: the scene JSON to load, or the one reason it did not.
///
/// `Payload` carries **text**, not elements, on purpose. It is handed straight to
/// [`crate::DrawEngine::load_scene`], which is the one door a scene comes in through and
/// the one place its schema is decided (`export/json.rs`). A `Vec<DrawElement>` here would
/// mean a second door and a second `elements_from_json`, and the brief for this task is
/// explicit that there is to be one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Restore {
    /// The scene JSON, ready for `load_scene`. Never an empty scene — see [`restore`].
    Payload(String),
    /// Why not. Never both, so "opened with nothing in it" is not a state a caller can
    /// reach by ignoring a flag.
    Refused(RestoreRefusal),
}

/// The scene as a payload: the generation, a colon, and the base64 of its JSON.
///
/// One function, called by both containers, which is what makes "the PNG and the SVG carry
/// the same bytes" true by construction rather than by two implementations agreeing.
///
/// The clone is `scene_to_json`'s own requirement, not this one's: it takes
/// `&[DrawElement]` because it writes an `OsidrawFile`
/// (`export/json.rs`), and this is a borrowed scene — the scope an export is built from
/// holds references, and re-deriving owned elements at the call sites would put the
/// decision of *how* to get them in three places.
pub fn scene_payload(elements: &[&DrawElement]) -> String {
    let owned: Vec<DrawElement> = elements.iter().map(|element| (*element).clone()).collect();
    let json = crate::scene_to_json(&owned);
    format!(
        "{SCENE_PAYLOAD_VERSION}:{}",
        super::base64::encode(json.as_bytes())
    )
}

/// The scene in `bytes`, or why there is none.
///
/// Two containers, chosen by the PNG signature (`image.ts:111`… `:17-18` is a PNG walk and
/// `export.ts@1118751f:532` is a text scan, and the oracle keeps the two apart by which
/// function the caller wanted). Choosing here rather than in the host is BUNNY.md §2: a
/// front that picked the container would be a second answer to a question with one.
///
/// **Never answers `Payload` with a scene of no elements.** That is the whole silent
/// failure: a file that "opens" to nothing looks exactly like a fresh board, and the only
/// way to be sure it cannot happen is for the function that decides to say no.
pub fn restore(bytes: &[u8]) -> Restore {
    if bytes.starts_with(&PNG_SIGNATURE) {
        restore_png(bytes)
    } else {
        restore_svg(bytes)
    }
}

/// The `tEXt` chunk of a PNG, once it has been checked for being a scene of ours.
fn restore_png(bytes: &[u8]) -> Restore {
    let text = match png_chunk::find_text_chunk(bytes, SCENE_FORMAT) {
        Ok(Some(text)) => text,
        // A PNG with no `tEXt` chunk and a PNG whose first `tEXt` chunk is someone else's
        // are the same answer, because `image.ts@111`… `:51` cannot tell them apart either.
        Ok(None) => return Restore::Refused(RestoreRefusal::NotOurs),
        Err(_) => return Restore::Refused(RestoreRefusal::Malformed),
    };
    scene_in(&text)
}

/// The `<metadata>` element an SVG carries, with or without a scene in it.
///
/// `exportToSvg` appends `<metadata>` unconditionally (`export.ts@1118751f:363-369`) and
/// only fills it when `exportEmbedScene` says so (`:378-391`), so the element is here
/// either way: one document shape, and "has a metadata element" is not a second thing to
/// be right about.
pub fn svg_metadata_element(payload: Option<&str>) -> String {
    let Some(payload) = payload else {
        return "<metadata></metadata>".to_string();
    };
    format!(
        "<metadata><!-- payload-type:{SCENE_FORMAT} --><!-- payload-version:{SCENE_PAYLOAD_VERSION} \
         --><!-- payload-start -->{payload}<!-- payload-end --></metadata>"
    )
}

/// The comment an SVG's root carries before its `<metadata>` — `export.ts@1118751f:368`
/// writes `svg-source:excalidraw` there and ours says what wrote ours.
pub fn svg_source_comment() -> String {
    format!("<!-- svg-source:{SCENE_FORMAT} -->")
}

/// The scene in an SVG's metadata, once it has been checked for being one of ours.
fn restore_svg(bytes: &[u8]) -> Restore {
    let text = String::from_utf8_lossy(bytes);
    if !text.contains("<svg") {
        return Restore::Refused(RestoreRefusal::Malformed);
    }
    if !text.contains(&format!("<!-- payload-type:{SCENE_FORMAT} -->")) {
        return Restore::Refused(RestoreRefusal::NotOurs);
    }
    match between_markers(&text) {
        Some(payload) => scene_in(&payload),
        None => Restore::Refused(RestoreRefusal::Unreadable),
    }
}

/// The text between `<!-- payload-start -->` and the first `<!-- payload-end -->` after it.
///
/// The oracle's `svg.match(/<!-- payload-start -->\s*(.+?)\s*<!-- payload-end -->/)`
/// (`export.ts@1118751f:533-535`) in all but the whitespace, and the reason for taking the
/// *first* end marker is the same as for the first `tEXt` chunk: two payloads in one
/// document is not one this engine wrote.
fn between_markers(svg: &str) -> Option<String> {
    const START: &str = "<!-- payload-start -->";
    const END: &str = "<!-- payload-end -->";
    let after = svg.find(START)? + START.len();
    let end = svg[after..].find(END)? + after;
    Some(svg[after..end].trim().to_string())
}

/// The scene JSON a payload holds, or the one reason it is not one.
///
/// The generation is checked first and the scene second, and both refusals are
/// [`RestoreRefusal::Unreadable`] because that is what the oracle's `try` turns anything
/// into (`image.ts:111`… `:65-68`): a file that claims to be ours and is not readable is
/// one fact, and splitting it further would give a host something to interpret.
fn scene_in(payload: &str) -> Restore {
    let Some(json) = scene_json_of(payload) else {
        return Restore::Refused(RestoreRefusal::Unreadable);
    };
    match crate::elements_from_json(&json) {
        // The silent failure, refused: an empty board and a board that restored to nothing
        // are the same picture. `data/index.ts@1118751f:120-122` refuses to write one.
        Some(elements) if elements.is_empty() => Restore::Refused(RestoreRefusal::Unreadable),
        Some(_) => Restore::Payload(json),
        None => Restore::Refused(RestoreRefusal::Unreadable),
    }
}

/// The scene JSON inside a payload, or `None` for a generation or an encoding we do not
/// have.
///
/// The one match a future generation adds. Every generation written so far has to keep
/// being read, and one that is not known has to be refused rather than guessed at, so
/// both halves of that promise are here rather than in a comment.
fn scene_json_of(payload: &str) -> Option<String> {
    let (generation, encoded) = payload.split_once(':')?;
    if generation != SCENE_PAYLOAD_VERSION {
        return None;
    }
    String::from_utf8(base64_decode(encoded)?).ok()
}

/// Base64, strictly: a length that is not a multiple of four, a character outside the
/// alphabet, or padding anywhere but the end is `None`.
///
/// Strict because the reader must be able to say no. A decoder that quietly accepted a
/// truncated payload would hand back a shorter scene JSON, which the parse below would
/// reject — but a decoder that accepted a payload cut in the middle of the *last* group
/// would hand back a JSON cut in the middle of a string, and how that is reported is
/// exactly the kind of thing this module is not guessing about.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = super::base64::ALPHABET;
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    // Cut at the **first** `=`, not the last. Padding is one or two characters and it is
    // always at the end, so the first one is where the body stops; cutting at the last
    // leaves one `=` in the body for a one-byte tail, and `=` is not in the alphabet, so
    // such a payload is refused. Which is what happened: a scene whose JSON happened to be
    // a whole number of three-byte groups round-tripped and a scene one byte over did not,
    // and the two were indistinguishable from the outside.
    let body = match bytes.iter().position(|byte| *byte == b'=') {
        Some(at) => &bytes[..at],
        None => bytes,
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut held = 0u32;
    let mut bits = 0u32;
    for byte in body {
        let six = ALPHABET.iter().position(|letter| letter == byte)? as u32;
        held = (held << 6) | six;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((held >> bits) as u8);
            // Keep only the bits the next character can still need. Without this the
            // accumulator keeps everything it has ever been handed, and after five groups
            // `held << 6` shifts the top of it out of the `u32` — a decoder that is right
            // about the alphabet, the padding and the length, and still hands back a scene
            // with a character flipped in it. The first version of this function had
            // exactly that, and so did the decoder written beside it in the tests, which
            // is the whole argument for writing a second one.
            held &= (1 << bits) - 1;
        }
    }
    Some(out)
}
