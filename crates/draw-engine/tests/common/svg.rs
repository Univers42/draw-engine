// An SVG document read by this file rather than by the module under test.
//
// Every helper here walks the markup a second time, by hand, in a different shape from
// `export/svg.rs`'s single `format!` per element. That is the whole reason it lives in the
// test tree: the exporter builds a document by appending strings together and a test that
// greps for the same substrings the exporter appends proves the two agree with each other,
// which is not the claim. The claim is that a file a person opened in another program holds
// the declarations, inside the element the format puts them in, with the bytes recoverable.
//
// Deliberately *not* an XML parser. What has to be right here is a fixed, known shape —
// attributes are double-quoted, `<style>` holds character data, there is no CDATA, no
// comment, no processing instruction and no entity beyond the five named ones — and a
// general parser would be a dependency this crate may not add (BUNNY.md §3.2) plus a lot of
// code that answers questions no test in this repository asks.

/// One `name="value"` pair, read from `tag` — the text between `<` and the first `>`.
///
/// Panics rather than returning `None` for a missing attribute: every caller here is asking
/// about an attribute the format is required to write, so "absent" is a failure to report
/// with the document in hand, not a case to handle.
pub fn attribute(tag: &str, name: &str) -> String {
    let marker = format!(" {name}=\"");
    let start = tag
        .find(&marker)
        .unwrap_or_else(|| panic!("<{tag}> should carry a {name}"))
        + marker.len();
    let rest = &tag[start..];
    let end = rest
        .find('"')
        .unwrap_or_else(|| panic!("the {name} attribute should be closed"));
    unescape(&rest[..end])
}

/// The five named entities of XML §4.1 plus a numeric one, and nothing else.
///
/// A `&` that is not one of those is left alone rather than resolved, because a document
/// that contains one is malformed and the test that reads it should fail on the bytes, not
/// on a guess about what they were meant to be.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest
            .find(';')
            .unwrap_or_else(|| panic!("an & should be closed, in {rest:?}"));
        let entity = &rest[1..end];
        let character = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            other => other
                .strip_prefix('#')
                .and_then(
                    |digits| match digits.strip_prefix('x').or(digits.strip_prefix('X')) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => digits.parse().ok(),
                    },
                )
                .and_then(char::from_u32)
                .unwrap_or_else(|| panic!("{entity} is not an entity this reads")),
        };
        out.push(character);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The character data of the first `<tag …>…</tag>`, tags included at both ends.
///
/// Depth-free on purpose: `<style>` and `<defs>` never nest inside themselves in what this
/// module's caller writes, and a counter that handled arbitrary nesting would be a parser.
pub fn element(document: &str, tag: &str) -> Option<String> {
    // `<defs>` carries no attributes and `<style>` carries one, so the opening tag ends at
    // either a space or the `>` — matching only `<tag ` would skip a bare element and the
    // failure would read "there is no defs" about a document that has one.
    let start = document.find(&format!("<{tag}")).filter(|at| {
        matches!(
            document[at + tag.len() + 1..].chars().next(),
            Some(' ') | Some('>')
        )
    })?;
    let content_at = document[start..].find('>')? + start + 1;
    let close = format!("</{tag}>");
    let end = document[content_at..].find(&close)? + content_at;
    Some(document[start..end + close.len()].to_string())
}

/// The `<style class="style-fonts">` element's character data, and the `<defs>` it is in.
///
/// Returned as a pair because "the style is there" and "the style is where the format says
/// it is" are different claims, and a writer that appended the style to the root instead
/// of to the `<defs>` would pass the first.
pub fn font_face_style(document: &str) -> Option<(String, String)> {
    let defs = element(document, "defs")?;
    let style = element(&defs, "style")?;
    let tag = &style[..style.find('>')?];
    assert_eq!(
        attribute(tag, "class"),
        "style-fonts",
        "the oracle's class name: `style.classList.add(\"style-fonts\")` (export.ts:446)"
    );
    let content_at = style.find('>')? + 1;
    Some((
        defs,
        style[content_at..style.len() - "</style>".len()].to_string(),
    ))
}

/// One `@font-face` declaration: the family it names and the base64 payload it carries.
///
/// `Debug` because an assertion that prints a `Vec<FontFace>` on failure is worth having
/// and a face's payload is 20KB of base64, so the derived one clips it.
pub struct FontFace {
    pub family: String,
    pub payload: String,
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FontFace({:?}, <{} chars>)",
            self.family,
            self.payload.len()
        )
    }
}

/// Every `@font-face` in `css`, in the order they are written.
///
/// The grammar read is the oracle's own one, character for character:
/// `` `@font-face { font-family: ${family}; src: url(${content}); }` ``
/// (`ExcalidrawFontFace.ts@1118751f:49`). `content` is kept whole, `data:` URL and all,
/// because that is what the declaration carries and what a reader has to reproduce.
///
/// Anything that does not match is a panic with the declaration in hand, because "it was
/// not there" and "it was there but not in that shape" are the two answers a test needs to
/// tell apart and a substring search cannot.
pub fn font_faces(css: &str) -> Vec<FontFace> {
    const MARKER: &str = "@font-face { font-family: ";
    const TAIL: &str = "; src: url(";
    const END: &str = "); }";
    let mut out: Vec<FontFace> = Vec::new();
    let mut rest = css.trim();
    while !rest.is_empty() {
        let start = rest
            .find(MARKER)
            .unwrap_or_else(|| panic!("an unparsed declaration left in {rest:?}"));
        rest = &rest[start + MARKER.len()..];
        let after_family = rest
            .find(TAIL)
            .unwrap_or_else(|| panic!("a family with no `{TAIL}` in {rest:?}"));
        let family = unescape(&rest[..after_family]);
        rest = &rest[after_family + TAIL.len()..];
        // The **first** `); }`, not the last: `strip_suffix` swallows every declaration
        // between this one and the final one into a single payload, and the test is then
        // asserting on a face that is two.
        let end = rest
            .find(END)
            .unwrap_or_else(|| panic!("`{family}` has no `{END}` closing it: {rest:.80}"));
        out.push(FontFace {
            family,
            payload: unescape(&rest[..end]),
        });
        rest = &rest[end + END.len()..];
    }
    out
}

/// The `font-family` attribute of every `<text>` in a document, in order.
pub fn text_font_families(document: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = document;
    while let Some(start) = rest.find("<text ") {
        rest = &rest[start..];
        let end = rest
            .find('>')
            .unwrap_or_else(|| panic!("a <text> should be closed: {rest:?}"));
        let tag = &rest[..end];
        out.push(attribute(tag, "font-family"));
        rest = &rest[end..];
    }
    out
}

/// Base64 decoded by a second implementation, written from RFC 4648 §4 rather than from
/// `export/base64.rs`.
///
/// The shape is deliberately not the writer's: an explicit **bit list**, one entry per
/// sextet, assembled and then read off in bytes, rather than a `u32` accumulator that is
/// shifted and masked. The two disagree in exactly the places a shift-accumulator goes
/// wrong — a group whose last character is padding, and a `held << 6` that eventually shifts
/// the top of its own accumulator out — which is what makes this a check and not a second
/// copy. Every one of the writer's three real bugs on this project was in one of those two
/// places.
///
/// **Padding is cut at the first `=`, and the tail's spare bits are dropped.** Both were
/// wrong in the first version of this function, which is why they are named: a two-byte tail
/// leaves two bits over, and a decoder that insists on a whole number of bytes refuses it; a
/// decoder that cut at the *last* `=` leaves a `=` in the body, which is not in the alphabet,
/// and refuses that too. Between them they called a perfectly good 75KB payload not base64.
///
/// `None` for anything malformed, rather than a shorter answer: a decoder that quietly
/// returned fewer bytes would turn a corrupt document into a plausible one.
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let cut = text.find('=').unwrap_or(text.len());
    let (body, padding) = text.split_at(cut);
    if padding.len() > 2 {
        return None;
    }
    let mut bits: Vec<u8> = Vec::with_capacity(body.len() * 6);
    for letter in body.bytes() {
        let six = ALPHABET.iter().position(|c| *c == letter)? as u8;
        for shift in (0..6).rev() {
            bits.push((six >> shift) & 1);
        }
    }
    // A tail of two or three characters carries fewer than eight bits, and those bits belong
    // to the padding rather than to the data.
    let whole = bits.len() / 8 * 8;
    Some(
        bits[..whole]
            .chunks(8)
            .map(|byte| byte.iter().fold(0u8, |acc, bit| (acc << 1) | bit))
            .collect(),
    )
}

/// RFC 4648 §4's 64 characters, in order.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The CRC-32 of ISO 15948 §12.2: the reflected 0xEDB88320 polynomial, seeded and inverted.
///
/// Bitwise rather than table-driven because a test that computes a checksum should be
/// readable — and because a table would be a second thing to get wrong quietly. This is
/// `tests/common/png.rs`'s, which is the point: the PNG crate's own table walk and the SVG's
/// own payload then agree on what a CRC is, and neither of them wrote it.
pub fn crc32(bytes: &[u8]) -> u32 {
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

/// A document with every base64 payload shortened, so a failure message is readable.
///
/// A face is tens of kilobytes of base64 and a failed assertion that prints the document it
/// was given is a failed assertion nobody reads. The head and the tail are what say *where*
/// it went wrong; the middle is a run of the alphabet.
pub fn clipped(document: &str) -> String {
    let mut out = String::with_capacity(document.len() / 4);
    let mut rest = document;
    while let Some(start) = rest.find("base64,") {
        let (head, tail) = rest.split_at(start + "base64,".len());
        out.push_str(head);
        let payload = &tail[..tail.find(')').unwrap_or(tail.len())];
        let keep = payload.len().min(48);
        out.push_str(&payload[..keep]);
        out.push_str(&format!("…<{} chars>…", payload.len()));
        rest = &tail[payload.len()..];
    }
    out.push_str(rest);
    out
}
