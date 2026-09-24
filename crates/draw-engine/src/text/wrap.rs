//! A line-for-line port of Excalidraw's `packages/element/src/textWrapping.ts` at the pinned
//! SHA (`scripts/oracle-sha.txt` in the host repo; the fixture records it). Line numbers
//! below cite that file unless another is named.
//!
//! Where the port departs from the source, it says so. Two departures matter outside this
//! file: `char_width` is cached per char rather than per first UTF-16 unit
//! (`MeasureCache`), and offsets point into the source even when NFC rewrote the line
//! (the oracle's are code units of the NFC text, :378-381).

use std::borrow::Cow;

use super::measure::TextMetrics;
use super::unicode;

/// One rendered line and the bytes of the source text it came from (`WrappedTextLine`,
/// :414-418), end-exclusive.
///
/// `text` is exactly `source[start..end]`, except when NFC rewrote the hard line (the
/// tokenizer composes decomposed characters, :385-388): then `text` is the composed form
/// and the range still covers the source bytes it was composed from.
#[derive(Clone, Debug, PartialEq)]
pub struct WrappedLine {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// `wrapText` (:397-405): the rendered lines joined by `\n`.
pub fn wrap_text(text: &str, max_width: f64, metrics: &dyn TextMetrics) -> String {
    join(&wrap_lines(text, max_width, metrics))
}

/// `getWrappedTextLines` (:446-478), with byte offsets into `text`.
pub fn wrap_lines(text: &str, max_width: f64, metrics: &dyn TextMetrics) -> Vec<WrappedLine> {
    wrap_lines_with(text, max_width, |line| {
        wrap_hard_line(line, max_width, metrics)
    })
}

/// `parseTokens` (:382-389): the hard line in NFC, split at every break opportunity.
pub fn parse_tokens(line: &str) -> Vec<String> {
    let line = Prepared::new(line);
    line.tokens()
        .into_iter()
        .map(|(a, b)| line.slice(a, b).to_string())
        .collect()
}

pub(crate) fn join(lines: &[WrappedLine]) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&line.text);
    }
    out
}

/// The hard-line loop of `getWrappedTextLines` (:446-478), with the wrapping of one hard
/// line left to `wrap_hard` — `wrap_hard_line`, or `MeasureCache`'s memo in front of it.
/// `wrap_hard` returns offsets relative to its line.
pub(crate) fn wrap_lines_with(
    text: &str,
    max_width: f64,
    mut wrap_hard: impl FnMut(&str) -> Vec<WrappedLine>,
) -> Vec<WrappedLine> {
    // "if maxWidth is not finite or NaN [...] we'll end up in an infinite loop" (:451-456):
    // hard lines only (`getHardLineBreaks`, :423-438).
    let valid = max_width.is_finite() && max_width >= 0.0;
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        if valid {
            out.extend(wrap_hard(line).into_iter().map(|mut wrapped| {
                wrapped.start += offset;
                wrapped.end += offset;
                wrapped
            }));
        } else {
            out.push(WrappedLine {
                text: line.to_string(),
                start: offset,
                end: offset + line.len(),
            });
        }
        offset += line.len() + 1;
    }
    out
}

/// One hard line: verbatim when it fits (:462-469, not normalised), otherwise `wrapLine`.
pub(crate) fn wrap_hard_line(
    line: &str,
    max_width: f64,
    metrics: &dyn TextMetrics,
) -> Vec<WrappedLine> {
    if metrics.line_width(line) <= max_width {
        return vec![WrappedLine {
            text: line.to_string(),
            start: 0,
            end: line.len(),
        }];
    }
    wrap_line(&Prepared::new(line), max_width, metrics)
}

// ------------------------------------------------------------------ character classes

fn in_table(table: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// JavaScript's `\s`, which is also exactly what `trimEnd()` strips.
fn is_whitespace(c: char) -> bool {
    in_table(unicode::WHITESPACE, c)
}

// The literal halves of the classes, :61-118.
const COMMON_OPENING: &str = "<([{";
const COMMON_CLOSING: &str = ">)]}.,:;!?…/";
const CJK_CHAR_SYMBOLS: &str = "｀＇＾〃〰〆＃＆＊＋－ー／＼＝｜￤〒￢￣";
const CJK_OPENING: &str = "（［｛〈《｟｢「『【〖〔〘〚＜〝";
const CJK_CLOSING: &str = "）］｝〉》｠｣」』】〗〕〙〛＞。．，、〟‥？！：；・〜〞";
const CJK_CURRENCY: &str = "￥￦￡￠＄";

fn is_cjk_char(c: char) -> bool {
    in_table(unicode::CJK_SCRIPT, c) || CJK_CHAR_SYMBOLS.contains(c)
}

fn is_opening(c: char) -> bool {
    COMMON_OPENING.contains(c) || CJK_OPENING.contains(c)
}

/// The zero-width alternatives of `getLineBreakRegexAdvanced` (:151-169) at `q`: is there
/// a break between `cs[q - 1]` and `cs[q]`?
fn break_at(cs: &[char], q: usize) -> bool {
    let next = cs[q];
    let prev = q.checked_sub(1).map(|i| cs[i]);
    let after = |class: &dyn Fn(char) -> bool| prev.is_some_and(class);
    let common_closing = |c: char| COMMON_CLOSING.contains(c);
    let cjk_closing = |c: char| CJK_CLOSING.contains(c);
    // Break.Before(WHITESPACE)
    is_whitespace(next)
        // Break.After(WHITESPACE, HYPHEN)
        || after(&|c| is_whitespace(c) || c == '-')
        // Break.Before(CJK.CHAR, CJK.CURRENCY).NotPrecededBy(COMMON.OPENING, CJK.OPENING)
        || (!after(&is_opening) && (is_cjk_char(next) || CJK_CURRENCY.contains(next)))
        // Break.After(CJK.CHAR).NotFollowedBy(HYPHEN, COMMON.CLOSING, CJK.CLOSING)
        || (after(&is_cjk_char) && !(next == '-' || common_closing(next) || cjk_closing(next)))
        // Break.BeforeMany(CJK.OPENING).NotPrecededBy(COMMON.OPENING)
        || (!after(&is_opening) && CJK_OPENING.contains(next))
        // Break.AfterMany(CJK.CLOSING).NotFollowedBy(COMMON.CLOSING)
        || (after(&cjk_closing) && !cjk_closing(next) && !common_closing(next))
        // Break.AfterMany(COMMON.CLOSING).FollowedBy(COMMON.OPENING)
        || (after(&common_closing) && !common_closing(next) && COMMON_OPENING.contains(next))
}

/// `EMOJI.JOINER` at `i` (:122-123): an optional modifier, `FE0F` with an optional keycap,
/// or a tag run closed by `E007F`. Returns where it ends (`i` when absent).
fn joiner_end(cs: &[char], i: usize) -> usize {
    match cs.get(i) {
        Some(&c) if in_table(unicode::EMOJI_MODIFIER, c) => i + 1,
        Some('\u{FE0F}') => {
            if cs.get(i + 1) == Some(&'\u{20E3}') {
                i + 2
            } else {
                i + 1
            }
        }
        _ => {
            let tags = cs[i.min(cs.len())..]
                .iter()
                .take_while(|c| ('\u{E0020}'..='\u{E007E}').contains(*c))
                .count();
            if tags > 0 && cs.get(i + tags) == Some(&'\u{E007F}') {
                i + tags + 1
            } else {
                i
            }
        }
    }
}

fn is_flag_at(cs: &[char], i: usize) -> bool {
    let ri = |k: usize| {
        cs.get(k)
            .is_some_and(|&c| in_table(unicode::REGIONAL_INDICATOR, c))
    };
    ri(i) && ri(i + 1)
}

/// `getEmojiRegexUnicode()` (:194-206) matched at `q`: where the emoji sequence ends.
fn emoji_at(cs: &[char], q: usize) -> Option<usize> {
    if is_flag_at(cs, q) {
        return Some(q + 2);
    }
    if !in_table(unicode::EMOJI_START, cs[q]) {
        return None;
    }
    let mut end = joiner_end(cs, q + 1);
    // (?:ZWJ(?:FLAG|ANY JOINER))*
    while cs.get(end) == Some(&'\u{200D}') {
        let next = end + 1;
        if is_flag_at(cs, next) {
            end = next + 2;
        } else if cs.get(next).is_some_and(|&c| in_table(unicode::EMOJI, c)) {
            end = joiner_end(cs, next + 1);
        } else {
            break;
        }
    }
    Some(end)
}

/// `getEmojiRegex().test(word)` (:585). The regex matches wherever a flag or an
/// `EMOJI.MOST` char starts, and regional indicators are `Emoji_Presentation`, so this is
/// "holds an `EMOJI.MOST` char".
fn contains_emoji(cs: &[char]) -> bool {
    cs.iter().any(|&c| in_table(unicode::EMOJI_START, c))
}

// ------------------------------------------------------------------ NFC

/// NFC leaves the char alone whatever surrounds it (ccc 0, NFC_QC=Yes).
fn nfc_stable(c: char) -> bool {
    !in_table(unicode::NFC_UNSTABLE, c)
}

/// NFC never reaches across the start of this char: what precedes it normalises on its own.
fn nfc_boundary_before(c: char) -> bool {
    !in_table(unicode::NFC_CONTINUES, c)
}

/// `String.prototype.normalize("NFC")` — the platform's, as the oracle calls it. In the
/// browser that is the browser's own; natively (tests, benches) the `unicode-normalization`
/// crate stands in, kept out of the wasm, where it would weigh 126 KB (71 KB gzipped).
#[cfg(target_arch = "wasm32")]
fn platform_nfc(text: &str) -> String {
    js_sys::JsString::from(text).normalize("NFC").into()
}

#[cfg(not(target_arch = "wasm32"))]
fn platform_nfc(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfc().collect()
}

/// A hard line as the tokenizer sees it: its NFC form (:388) as chars, where each char
/// sits in that form, and which source byte it maps back to.
struct Prepared<'a> {
    text: Cow<'a, str>,
    chars: Vec<char>,
    /// Byte offset of char `i` in `text`; one extra entry for the end.
    at: Vec<usize>,
    /// Byte offset of char `i` in the source; `None` when NFC left the line alone.
    source: Option<Vec<usize>>,
}

impl<'a> Prepared<'a> {
    fn new(line: &'a str) -> Self {
        // A line of stable chars is its own NFC: the common case costs a table lookup per
        // char and never reaches the normaliser.
        if line.chars().all(nfc_stable) {
            let (chars, mut at): (Vec<char>, Vec<usize>) =
                line.char_indices().map(|(i, c)| (c, i)).unzip();
            at.push(line.len());
            return Self {
                text: Cow::Borrowed(line),
                chars,
                at,
                source: None,
            };
        }
        // Normalise segment by segment, a segment running from one normalisation boundary
        // to the next, so each piece of the NFC text knows the piece of source it came
        // from. The segments NFC could change go to the normaliser in one call, joined by
        // '\n', which is stable and which no hard line contains.
        let mut bounds: Vec<usize> = line
            .char_indices()
            .filter(|&(i, c)| i == 0 || nfc_boundary_before(c))
            .map(|(i, _)| i)
            .collect();
        bounds.push(line.len());
        let segments: Vec<&str> = bounds.windows(2).map(|w| &line[w[0]..w[1]]).collect();
        let is_plain = |segment: &str| segment.chars().all(nfc_stable);
        let pending: Vec<&str> = segments.iter().copied().filter(|s| !is_plain(s)).collect();
        let normalized = platform_nfc(&pending.join("\n"));
        let mut normalized = normalized.split('\n');

        let mut text = String::with_capacity(line.len());
        let (mut chars, mut at, mut source) = (Vec::new(), Vec::new(), Vec::new());
        for (k, segment) in segments.iter().enumerate() {
            let (start, end) = (bounds[k], bounds[k + 1]);
            let composed = if is_plain(segment) {
                segment
            } else {
                normalized.next().unwrap_or(segment)
            };
            let same = composed == *segment;
            for (j, (i, c)) in composed.char_indices().enumerate() {
                chars.push(c);
                at.push(text.len() + i);
                // Inside a segment NFC rewrote, only its edges map back exactly: its first
                // char to where it started, the rest to where it ended. A break lands inside
                // one only after a whitespace, hyphen or CJK char carrying marks NFC
                // reorders or composes — the whole segment then counts as the earlier line's.
                source.push(if same {
                    start + i
                } else if j == 0 {
                    start
                } else {
                    end
                });
            }
            text.push_str(composed);
        }
        at.push(text.len());
        source.push(line.len());
        Self {
            text: Cow::Owned(text),
            chars,
            at,
            source: Some(source),
        }
    }

    fn slice(&self, a: usize, b: usize) -> &str {
        &self.text[self.at[a]..self.at[b]]
    }

    fn source(&self, i: usize) -> usize {
        self.source.as_ref().map_or(self.at[i], |source| source[i])
    }

    fn line(&self, a: usize, b: usize) -> WrappedLine {
        WrappedLine {
            text: self.slice(a, b).to_string(),
            start: self.source(a),
            end: self.source(b),
        }
    }

    /// `line.split(breakLineRegex).filter(Boolean)` (:388) as char ranges, reproducing
    /// `RegExp.prototype[Symbol.split]`: the regex is tried sticky at `q`; a zero-width
    /// match where the previous piece ended advances instead; the emoji alternative comes
    /// first and its capture is kept as a piece of its own.
    fn tokens(&self) -> Vec<(usize, usize)> {
        let cs = &self.chars;
        let mut out = Vec::new();
        let (mut p, mut q) = (0, 0);
        while q < cs.len() {
            if let Some(e) = emoji_at(cs, q) {
                out.push((p, q));
                out.push((q, e));
                p = e;
                q = e;
            } else if break_at(cs, q) && q != p {
                out.push((p, q));
                p = q;
            } else {
                q += 1;
            }
        }
        out.push((p, cs.len()));
        out.retain(|&(a, b)| a < b);
        out
    }
}

// ------------------------------------------------------------------ wrapping

/// `isSingleCharacter` (:724-729) on a one-char token: `codePointAt(1)` is undefined only
/// when the char is a single UTF-16 code unit.
fn is_single_code_unit(c: char) -> bool {
    (c as u32) <= 0xFFFF
}

/// `wrapLine` (:486-573). The line being built is always a contiguous run of chars,
/// `current..end`, so it is sliced rather than copied.
fn wrap_line(line: &Prepared, max_width: f64, metrics: &dyn TextMetrics) -> Vec<WrappedLine> {
    let mut lines = Vec::new();
    let tokens = line.tokens();
    let (mut current, mut end) = (0, 0);
    let mut width = 0.0;
    let mut index = 0;
    while index < tokens.len() {
        let (a, b) = tokens[index];
        let empty = current == end;
        debug_assert!(empty || end == a, "tokens are consecutive");
        // "cache single codepoint whitespace, CJK or emoji width calc. as kerning should not
        // apply here" (:509-512); anything longer is measured whole, kerning included.
        let test_width = if b - a == 1 && is_single_code_unit(line.chars[a]) {
            width + metrics.char_width(line.chars[a])
        } else {
            metrics.line_width(line.slice(if empty { a } else { current }, b))
        };
        // A whitespace token always joins, past the width if need be (:514-525).
        if line.chars[a..b].iter().any(|&c| is_whitespace(c)) || test_width <= max_width {
            if empty {
                current = a;
            }
            end = b;
            width = test_width;
            index += 1;
            continue;
        }
        if empty {
            // The token alone is wider than the line (:527-545): break it, and keep its
            // last piece open for the tokens after it.
            let pieces = wrap_word(line, a, b, max_width, metrics);
            let (&(x, y), preceding) = pieces.split_last().unwrap_or((&(a, a), &[]));
            lines.extend(preceding.iter().map(|&(x, y)| line.line(x, y)));
            current = x;
            end = y;
            width = metrics.line_width(line.slice(x, y));
            index += 1;
        } else {
            // Push and reset, and try the same token again on an empty line (:546-557).
            lines.push(trim_end_at_soft_break(line, current, end));
            current = a;
            end = a;
            width = 0.0;
        }
    }
    if current != end {
        lines.push(trim_line(line, current, end, max_width, metrics));
    }
    lines
}

/// `wrapWord` (:578-647): one char at a time, summing unkerned `char_width`s. A word
/// holding an emoji is never split.
fn wrap_word(
    line: &Prepared,
    a: usize,
    b: usize,
    max_width: f64,
    metrics: &dyn TextMetrics,
) -> Vec<(usize, usize)> {
    let chars = &line.chars[a..b];
    if contains_emoji(chars) {
        return vec![(a, b)];
    }
    // satisfiesWordInvariant (:734-740)
    debug_assert!(
        !chars.iter().any(|&c| is_whitespace(c)),
        "a word holds no whitespace"
    );
    let mut pieces = Vec::new();
    let (mut start, mut width) = (a, 0.0);
    for (k, &c) in (a..b).zip(chars) {
        let char_width = metrics.char_width(c);
        if width + char_width <= max_width {
            width += char_width;
            continue;
        }
        if k > start {
            pieces.push((start, k));
        }
        start = k;
        width = char_width;
    }
    if b > start {
        pieces.push((start, b));
    }
    pieces
}

/// `trimLineEndAtSoftBreak` (:705-717): `trimEnd()`.
fn trim_end_at_soft_break(line: &Prepared, a: usize, b: usize) -> WrappedLine {
    let mut end = b;
    while end > a && is_whitespace(line.chars[end - 1]) {
        end -= 1;
    }
    line.line(a, end)
}

/// JavaScript's line terminators, which `.` refuses without the `s` flag.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `trimLine` (:655-698): the last line of a hard line keeps the trailing whitespace that
/// still fits, re-added one unkerned char at a time.
fn trim_line(
    line: &Prepared,
    a: usize,
    b: usize,
    max_width: f64,
    metrics: &dyn TextMetrics,
) -> WrappedLine {
    if metrics.line_width(line.slice(a, b)) <= max_width {
        return line.line(a, b);
    }
    // `line.match(/^(.+?)(\s+)$/)`, no `u` flag. Group 1 is everything before the trailing
    // whitespace run, but at least one code unit — so a line of only whitespace keeps its
    // first char — and it cannot hold a line terminator. Without a match, `trimEnd()`.
    let mut run = b;
    while run > a && is_whitespace(line.chars[run - 1]) {
        run -= 1;
    }
    let head = if run == a { a + 1 } else { run };
    let matched = head < b && !line.chars[a..head].iter().any(|&c| is_line_terminator(c));
    let (mut end, whitespace_end) = if matched { (head, b) } else { (run, run) };
    let mut width = metrics.line_width(line.slice(a, end));
    while end < whitespace_end {
        let char_width = metrics.char_width(line.chars[end]);
        if width + char_width > max_width {
            break;
        }
        width += char_width;
        end += 1;
    }
    line.line(a, end)
}
