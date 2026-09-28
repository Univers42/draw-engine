//! The PNG container, and nothing about the scene: a chunk table, a CRC, and a `tEXt`
//! chunk in and out.
//!
//! The oracle's is three npm packages — `png-chunks-extract`, `png-chunks-encode` and
//! `png-chunk-text` — and 30-odd lines of `data/image.ts@1118751f` on top. The parts that
//! matter here, and where each is specified rather than guessed:
//!
//! - **A chunk is `length ++ type ++ data ++ crc`**, big-endian throughout, where the CRC
//!   covers `type ++ data` and is the reflected 0xEDB88320 polynomial seeded with
//!   0xFFFFFFFF and finally inverted (ISO 15948 §5.3 and §12.2). `png-chunks-encode` is
//!   that and nothing else; there is no room in the format to be wrong here quietly.
//! - **A `tEXt` chunk's data is `keyword ++ 0x00 ++ text`**, the keyword 1–79 Latin-1 bytes
//!   with no NUL and the text Latin-1 with no NUL either — `png-chunk-text@1.0.0`'s
//!   `encode.js:15-41`, whose `decode.js:25` throws on a NUL inside the text.
//!
//! ## What this deliberately does not do
//!
//! **It does not check CRCs on the way in**, and that is the oracle's behaviour and not an
//! oversight: `png-chunks-extract` walks the table and hands each chunk's bytes over, and
//! the reader above it (`data/image.ts@1118751f:14-23`) does no more than that. A file a
//! browser wrote with a quirk in a chunk we never look at still opens. A CRC we invented on
//! the way in would refuse files the oracle reads, which is the more expensive mistake.
//!
//! **It takes the first `tEXt` chunk and stops.** `chunks.find(chunk => chunk.name ===
//! "tEXt")` (`image.ts@1118751f:18`) is a `find`, not a `filter`, so a second chunk is
//! invisible to the oracle and would be to us. Two chunks is not a file this engine wrote —
//! it writes exactly one — and picking the one we liked would be a silent choice, which is
//! the failure mode this whole module exists to make loud.

use crate::export::PngError;

/// The 8-byte file signature, ISO 15948 §5.2. A file without it is not a PNG and saying so
/// is a different answer from "a PNG with nothing in it".
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// `IEND`, the chunk every PNG ends with (`image.ts@1118751f:44` splices the metadata in
/// "before last chunk (iEND)").
const IEND: &[u8; 4] = b"IEND";

/// `tEXt`, the chunk that carries uncompressed text (ISO 15948 §11.3.4).
const TEXT: &[u8; 4] = b"tEXt";

/// Where a chunk sits in the file, so an insert can be one splice rather than a rebuild.
struct Span {
    /// The offset of the chunk's 4-byte length — the whole chunk starts here.
    start: usize,
    kind: [u8; 4],
    /// The offsets of the chunk's data, between the type and the CRC.
    data: std::ops::Range<usize>,
}

/// Every chunk's place in `png`, or why the table cannot be walked.
///
/// The reading is [`read_span`]'s; what is decided here is what the table has to be before
/// it is a PNG's at all.
fn spans(png: &[u8]) -> Result<Vec<Span>, PngError> {
    if !png.starts_with(&PNG_SIGNATURE) {
        return Err(PngError::NotAPng);
    }
    let mut found = Vec::new();
    let mut at = PNG_SIGNATURE.len();
    while at < png.len() {
        let (span, next) = read_span(png, at)?;
        found.push(span);
        at = next;
    }
    // A PNG ends with `IEND` and nothing else (ISO 15948 §5.6), so a file without one is
    // not a PNG however well the table walked. This is what tells the eight-byte signature
    // on its own — `Malformed`, "that is not a picture" — from a real PNG with no `tEXt`
    // chunk in it, which is `NotOurs`, "a picture, with no scene in it". The oracle cannot
    // tell them apart: `png-chunks-extract` hands back an empty list and `image.ts:111`…
    // `:51` reports `INVALID`.
    if !found.last().is_some_and(|span| &span.kind == IEND) {
        return Err(PngError::NoEnd);
    }
    Ok(found)
}

/// The chunk at `at`, and the offset the next one starts at.
///
/// Every offset is computed with checked arithmetic, and that is not defensive noise: the
/// length field is four bytes a stranger controls, so `at + 8 + len` overflows a corrupt
/// file rather than only a hostile one. `ci_export_roundtrip_props.rs` feeds this function
/// random bytes for exactly that reason.
fn read_span(png: &[u8], at: usize) -> Result<(Span, usize), PngError> {
    let Some(length) = read_length(png, at) else {
        return Err(PngError::Truncated);
    };
    let Some(data_start) = at.checked_add(8) else {
        return Err(PngError::Truncated);
    };
    let Some(data_end) = data_start.checked_add(length) else {
        return Err(PngError::Truncated);
    };
    let Some(end) = data_end.checked_add(4) else {
        return Err(PngError::Truncated);
    };
    if end > png.len() {
        return Err(PngError::Truncated);
    }
    let mut kind = [0u8; 4];
    kind.copy_from_slice(&png[at + 4..data_start]);
    Ok((
        Span {
            start: at,
            kind,
            data: data_start..data_end,
        },
        end,
    ))
}

/// The length field at `at`, if there is one and it is not the length of a lie.
fn read_length(png: &[u8], at: usize) -> Option<usize> {
    let raw = png.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes(raw.try_into().ok()?) as usize)
}

/// The text of the first `tEXt` chunk under `keyword` — `None` when there is no such
/// chunk, and the first chunk wins whether or not it is ours.
///
/// The oracle's `getTEXtChunk` (`data/image.ts@1118751f:14-23`) returns `null` for a PNG
/// with no `tEXt` chunk at all and decodes the *first* one it finds, keyword or not; the
/// keyword test is its caller's (`image.ts:111`… `:51`). Splitting it the same way is what
/// lets the caller tell "no chunk" from "someone else's chunk" without re-walking the file.
pub fn find_text_chunk(png: &[u8], keyword: &str) -> Result<Option<String>, PngError> {
    let table = spans(png)?;
    for span in table.iter().filter(|span| &span.kind == TEXT) {
        if let Some(text) = text_of(&png[span.data.clone()], keyword) {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

/// The text of a `tEXt` chunk, when the chunk is keyed `keyword`.
///
/// `None` for a different keyword, and `None` for a chunk with no NUL in it at all — a
/// `tEXt` with no separator is not a chunk any reader can make sense of, and
/// `png-chunk-text`'s own `decode` would hand back an empty keyword and an empty text
/// (`decode.js:12-28`) rather than complain. Treating it as "not ours" is the answer that
/// does not invent a scene.
fn text_of(data: &[u8], keyword: &str) -> Option<String> {
    let split = data.iter().position(|byte| *byte == 0)?;
    let (found, text) = data.split_at(split);
    if found != keyword.as_bytes() {
        return None;
    }
    // A NUL inside the text is the one thing `png-chunk-text` refuses outright
    // (`decode.js:25`); ours never writes one (the payload is base64) and a chunk that
    // holds one is not readable, so it is not a scene.
    if text[1..].contains(&0) {
        return None;
    }
    std::str::from_utf8(&text[1..]).ok().map(str::to_string)
}

/// `png` with a `tEXt` chunk of `text` under `keyword`, spliced in before its `IEND`.
///
/// The oracle's one line is `chunks.splice(-1, 0, metadataChunk)` with the comment
/// "insert metadata before last chunk (iEND)" (`image.ts@1118751f:44`), and this is the
/// same splice expressed on the bytes: everything before `IEND`, then the new chunk, then
/// `IEND` and whatever followed it. Nothing else in the file is touched, so the picture
/// comes out bit for bit as the browser encoded it.
pub fn insert_text_chunk(png: &[u8], keyword: &str, text: &str) -> Result<Vec<u8>, PngError> {
    let table = spans(png)?;
    let last = table.last().ok_or(PngError::Truncated)?;
    if &last.kind != IEND {
        return Err(PngError::NoEnd);
    }
    let chunk = text_chunk(keyword, text)?;
    let mut out = Vec::with_capacity(png.len() + chunk.len());
    out.extend_from_slice(&png[..last.start]);
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[last.start..]);
    Ok(out)
}

/// One whole `tEXt` chunk, CRC and all.
fn text_chunk(keyword: &str, text: &str) -> Result<Vec<u8>, PngError> {
    if keyword.is_empty() || keyword.len() >= 80 || keyword.contains('\0') {
        return Err(PngError::BadKeyword);
    }
    if !is_latin1(text) {
        return Err(PngError::NotLatin1);
    }
    let mut data = Vec::new();
    data.extend_from_slice(keyword.as_bytes());
    data.push(0);
    data.extend_from_slice(text.as_bytes());
    Ok(chunk(TEXT, &data))
}

/// `type ++ data` framed with its length and CRC — ISO 15948 §5.3.
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 12);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(kind, data).to_be_bytes());
    out
}

/// The CRC a chunk carries: the reflected 0xEDB88320 polynomial over `type ++ data`,
/// seeded with 0xFFFFFFFF and finally inverted (ISO 15948 §12.2).
///
/// Two calls rather than one slice, because the covered bytes are not contiguous in the
/// file — the length sits between them — and copying them into one buffer to hash it would
/// be a second copy of a payload that may be a megabyte.
pub fn crc32(kind: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in kind.iter().chain(data) {
        crc = step(crc, *byte);
    }
    !crc
}

/// One byte through the reflected polynomial.
fn step(crc: u32, byte: u8) -> u32 {
    let mut value = crc ^ u32::from(byte);
    for _ in 0..8 {
        value = if value & 1 == 1 {
            (value >> 1) ^ 0xEDB8_8320
        } else {
            value >> 1
        };
    }
    value
}

/// Whether a `tEXt` text is one a Latin-1 reader gets back unchanged.
///
/// Every byte of a `u8` is in Latin-1's range, so the range test is not the point: the
/// point is that each **character** is one byte, so writing the string and reading the
/// bytes as Latin-1 gives the same string back. A character above U+00FF would come back
/// as mojibake, and a NUL would make the chunk unreadable to any conforming reader
/// (`png-chunk-text@1.0.0`, `decode.js:25`).
///
/// The scene payload is base64, so this always holds in practice. It is checked anyway
/// because the failure it prevents is a file that looks fine and restores as nothing.
fn is_latin1(text: &str) -> bool {
    !text.contains('\0') && text.chars().all(|character| (character as u32) < 0x100)
}
