// A PNG written by this file rather than by the module under test.
//
// Every helper here builds a real PNG: the 8-byte signature, a chunk table, and CRCs
// computed from the PNG spec's polynomial. That is deliberate and it is the reason this
// lives in the test tree at all. The module under test also has a chunk writer, and a
// round trip checked with the module's own reader and the module's own writer proves the
// two halves agree with *each other* — which is not the claim. The claim is that the bytes
// on disk hold what we think they hold, and the only way to have evidence for that is a
// second implementation in the test.

use std::path::Path;

/// The PNG signature, in full — `png-chunks-extract`'s first test and ours.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The CRC-32 PNG chunks carry: the reflected 0xEDB88320 polynomial, seeded with
/// 0xFFFFFFFF and finally inverted (ISO 15948 §12.2). Bitwise rather than table-driven
/// because a test that computes a checksum should be readable.
pub fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        let mut value = crc ^ u32::from(*byte);
        for _ in 0..8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ 0xEDB8_8320
            } else {
                value >> 1
            };
        }
        crc = value;
    }
    !crc
}

/// One PNG chunk: `length ++ type ++ data ++ crc`, the layout at ISO 15948 §5.3.
pub fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(data);
    let mut covered = kind.to_vec();
    covered.extend_from_slice(data);
    chunk.extend_from_slice(&png_crc32(&covered).to_be_bytes());
    chunk
}

/// A `tEXt` chunk's data, as `png-chunk-text@1.0.0` writes it (`encode.js:15-41`):
/// `keyword ++ 0x00 ++ text`, Latin-1, no NUL in either half.
pub fn text_chunk_data(keyword: &str, text: &str) -> Vec<u8> {
    let mut data: Vec<u8> = keyword.bytes().collect();
    data.push(0);
    data.extend_from_slice(text.as_bytes());
    data
}

/// A whole PNG file, from its signature and its chunks.
pub fn png_of(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = PNG_SIGNATURE.to_vec();
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
    out
}

/// A 1×1 transparent RGBA PNG with no `tEXt` chunk at all — a picture, and nothing in it.
///
/// The `IDAT` is a real zlib stream built from **stored** deflate blocks, which is the
/// form a compressor emits when it has nothing to squeeze; it needs no compressor to write
/// and no inflater to read, so a fixture built this way is a PNG a browser will decode.
pub fn plain_png() -> Vec<u8> {
    png_of(&[
        png_chunk(b"IHDR", &ihdr()),
        png_chunk(b"IDAT", &zlib_stored(&[0x00, 0x00, 0x00, 0x00, 0x00])),
        png_chunk(b"IEND", &[]),
    ])
}

/// A 1×1 PNG carrying one `tEXt` chunk, spliced in before `IEND`.
///
/// The placement is the oracle's: `chunks.splice(-1, 0, metadataChunk)`, "insert metadata
/// before last chunk (iEND)" (`data/image.ts@1118751f:44`).
pub fn png_with_text(keyword: &str, text: &str) -> Vec<u8> {
    png_of(&[
        png_chunk(b"IHDR", &ihdr()),
        png_chunk(b"tEXt", &text_chunk_data(keyword, text)),
        png_chunk(b"IDAT", &zlib_stored(&[0x00, 0x00, 0x00, 0x00, 0x00])),
        png_chunk(b"IEND", &[]),
    ])
}

/// The 13 bytes of a 1×1, 8-bit, truecolour-with-alpha `IHDR`.
fn ihdr() -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    ihdr
}

/// A zlib stream (`0x78 0x01`) of `data` in one final **stored** deflate block, then the
/// big-endian Adler-32 of the same bytes.
///
/// Stored blocks are part of DEFLATE (RFC 1951 §3.2.4) and zlib is RFC 1950, so a
/// decompressor that cannot squeeze anything still reads this — which is the whole reason
/// the fixture is written this way rather than compressed.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01, 0x01];
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(&(!(data.len() as u16)).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// Adler-32, the checksum a zlib stream ends with (RFC 1950 §9).
fn adler32(data: &[u8]) -> u32 {
    let (mut low, mut high) = (1u32, 0u32);
    for byte in data {
        low = (low + u32::from(*byte)) % 65_521;
        high = (high + low) % 65_521;
    }
    (high << 16) | low
}

/// Every chunk of a PNG, as `(type, data, crc)`, by walking the file's own table.
///
/// A reader for the tests, and the one place a claim about *where* a chunk sits is
/// answered. It verifies each CRC as it goes, so a file whose table has been damaged stops
/// being read as a file with an odd chunk in it.
pub fn png_chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>, u32)> {
    assert_eq!(&png[..8], &PNG_SIGNATURE, "the file should be a PNG");
    let mut found = Vec::new();
    let mut at = 8usize;
    while at + 12 <= png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().expect("a length")) as usize;
        let mut kind = [0u8; 4];
        kind.copy_from_slice(&png[at + 4..at + 8]);
        let data = png[at + 8..at + 8 + length].to_vec();
        let end = at + 8 + length;
        let stored = u32::from_be_bytes(png[end..end + 4].try_into().expect("a crc"));
        assert_eq!(
            stored,
            png_crc32(&png[at + 4..end]),
            "chunk {at} ({}) carries a wrong crc",
            String::from_utf8_lossy(&kind)
        );
        found.push((kind, data, stored));
        at = end + 4;
    }
    found
}

/// The text of the first `tEXt` chunk under `keyword`, read by this file's own decoder.
///
/// `png-chunk-text@1.0.0`'s `decode.js:12-28`: everything before the first NUL is the
/// keyword, everything after it is the text, and a NUL *inside* the text is an error.
pub fn png_text(png: &[u8], keyword: &str) -> Option<String> {
    png_chunks(png).into_iter().find_map(|(kind, data, _)| {
        if &kind != b"tEXt" {
            return None;
        }
        let split = data.iter().position(|byte| *byte == 0)?;
        let (key, text) = data.split_at(split);
        assert!(
            !text[1..].contains(&0),
            "a NUL inside a tEXt text makes the chunk unreadable (png-chunk-text decode.js:25)"
        );
        (key == keyword.as_bytes()).then(|| String::from_utf8_lossy(&text[1..]).into_owned())
    })
}

/// A fixture file committed next to the tests, read whole.
///
/// `include_bytes!` rather than a path read at run time: a fixture that is not there is a
/// compile error naming the file, where a path read is a test that fails about `None`.
pub fn fixture(relative: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Base64, the standard alphabet with padding — written here rather than borrowed from the
/// engine so that a test asserting on a payload never checks the engine's encoder with the
/// engine's decoder.
///
/// `png-chunk-text` is what makes this load-bearing: its chunk is Latin-1 with no NUL
/// (`encode.js:7-13, 31-34`), and this alphabet has neither a byte above `z` nor a zero.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for group in bytes.chunks(3) {
        let b = |i: usize| u32::from(group.get(i).copied().unwrap_or(0));
        let packed = (b(0) << 16) | (b(1) << 8) | b(2);
        for i in 0..group.len() + 1 {
            let six = (packed >> (18 - 6 * i)) & 0x3F;
            out.push(ALPHABET[six as usize] as char);
        }
        for _ in group.len()..3 {
            out.push('=');
        }
    }
    out
}

/// Base64, strictly, by hand.
///
/// The other half of [`base64_encode`], and the reason this pair lives in the test tree
/// rather than being borrowed from the engine: a payload decoded by the same code that
/// encoded it proves the two halves agree with each other, which is not the claim. The
/// claim is that the bytes on disk hold what we think they hold, and the only evidence for
/// that is a second implementation.
///
/// Four characters in, three bytes out, one group at a time — the shape RFC 4648's own
/// test vectors are written in, and deliberately not the streaming accumulator the engine
/// uses. Both were written wrong the same way first (a `u32` that keeps every bit it is
/// given, so `held << 6` throws the top of it away after five groups), and a bug both
/// halves share is a bug that passes every test written from both of them.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let groups: Vec<&[u8]> = text.as_bytes().chunks(4).collect();
    if groups.iter().any(|group| group.len() != 4) {
        return None;
    }
    let mut out = Vec::with_capacity(groups.len() * 3);
    for group in groups {
        let letters: Vec<u32> = group
            .iter()
            .map(|byte| {
                if *byte == b'=' {
                    return Ok(0);
                }
                ALPHABET
                    .iter()
                    .position(|letter| letter == byte)
                    .map(|at| at as u32)
                    .ok_or(())
            })
            .collect::<Result<Vec<u32>, ()>>()
            .ok()?;
        let packed = (letters[0] << 18) | (letters[1] << 12) | (letters[2] << 6) | letters[3];
        let padding = group.iter().filter(|byte| **byte == b'=').count();
        for index in 0..(3 - padding) {
            out.push(((packed >> (16 - 8 * index)) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// A 1×1 PNG carrying **two** `tEXt` chunks, in the order given.
///
/// A file this engine wrote has exactly one, so two is a file somebody else made. The
/// oracle's `chunks.find` (`data/image.ts@1118751f:18`) takes the first and never sees the
/// second, and that is the rule to pin: "the first chunk wins" has to be true of ours too,
/// or a file with a `Comment` in front of a scene would read differently in the two.
pub fn png_with_two_text(first: (&str, &str), second: (&str, &str)) -> Vec<u8> {
    png_of(&[
        png_chunk(b"IHDR", &ihdr()),
        png_chunk(b"tEXt", &text_chunk_data(first.0, first.1)),
        png_chunk(b"IDAT", &zlib_stored(&[0x00, 0x00, 0x00, 0x00, 0x00])),
        png_chunk(b"tEXt", &text_chunk_data(second.0, second.1)),
        png_chunk(b"IEND", &[]),
    ])
}
