//! What wrapping needs from a font, and the caches in front of it.
//!
//! In the browser every measure is a `measureText` round trip across the wasm boundary,
//! so what is measured twice should be measured once: char widths per font, as the oracle
//! caches them (`textMeasurements.ts:179-208`), and whole wrapped hard lines, so that a
//! keystroke re-wraps the line it touched and looks the others up.
//!
//! Only single chars are summed from the cache. Anything longer is measured whole, kerning
//! included, exactly where the oracle does — a token-sum cache would be faster and would
//! break lines where the oracle does not (the `kern` fixtures fail it).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::wrap::{self, WrappedLine};

/// A font's measurements, as the canvas reports them.
pub trait TextMetrics {
    /// The advance width of `line` (never holding `\n`), kerning included:
    /// `measureText(line).width`.
    fn line_width(&self, line: &str) -> f64;

    /// The advance width of `c` measured alone — never kerned against a neighbour
    /// (`charWidth.calculate`).
    fn char_width(&self, c: char) -> f64 {
        self.line_width(c.encode_utf8(&mut [0; 4]))
    }
}

/// Which font a width belongs to: a family id and a size.
///
/// Family ids are Excalidraw's (`FONT_FAMILY`, 1 Virgil … 9 Liberation Sans), with
/// [`FontKey::LEGACY`] for the system stack text has been drawn with until now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontKey {
    family: u8,
    size_bits: u64,
}

impl FontKey {
    /// The system font stack (`render::FONT_FAMILY`): no Excalidraw family.
    pub const LEGACY: u8 = 0;

    pub fn new(family: u8, size: f64) -> Self {
        Self {
            family,
            size_bits: size.to_bits(),
        }
    }

    pub fn legacy(size: f64) -> Self {
        Self::new(Self::LEGACY, size)
    }

    /// The family id, [`FontKey::LEGACY`] for the system stack.
    pub fn family(self) -> u8 {
        self.family
    }

    pub fn size(self) -> f64 {
        f64::from_bits(self.size_bits)
    }
}

/// How many wrapped hard lines the memo holds before it starts over.
///
/// ponytail: cleared wholesale when full rather than evicting the least recently used —
/// a board being typed into re-fills it in one pass; an LRU is the upgrade if profiles
/// ever show it thrashing.
pub const MEMO_LIMIT: usize = 4096;

/// How many bytes of text the memo holds before it starts over: each hard line plus the
/// lines it wrapped to. The entry count alone does not bound it — with a peer watching,
/// every keystroke previews the whole hard line typed so far, and 4,096 of a long
/// paragraph is tens of MB that a wasm memory, which never shrinks, keeps for good.
/// Table and allocator overhead is not counted; [`MEMO_LIMIT`] bounds that.
pub const MEMO_BYTES: usize = 1 << 20;

type Memo = HashMap<(FontKey, u64), HashMap<String, Vec<WrappedLine>>>;

/// Char widths, wrapped hard lines and the widths of laid-out lines, for ONE measurer: a
/// cache outlives no change of the function it caches — [`clear`](Self::clear) it when
/// that changes, or when a font finishes loading and the same string starts measuring
/// differently.
#[derive(Default)]
pub struct MeasureCache {
    chars: RefCell<HashMap<(FontKey, char), f64>>,
    lines: RefCell<Memo>,
    memo_len: Cell<usize>,
    memo_bytes: Cell<usize>,
    /// The widths of lines as laid out — each rendered line of a text, measured whole
    /// for its box (`measureText`, `textMeasurements.ts:12-27`). Only final lines go in,
    /// never the candidates the wrapper tries, so it holds what is on the board. Bounded
    /// by [`MEMO_BYTES`] of text like the wrap memo, and cleared wholesale when full.
    widths: RefCell<HashMap<FontKey, HashMap<Box<str>, f64>>>,
    widths_len: Cell<usize>,
    widths_bytes: Cell<usize>,
}

impl MeasureCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&self) {
        self.chars.borrow_mut().clear();
        self.lines.borrow_mut().clear();
        self.memo_len.set(0);
        self.memo_bytes.set(0);
        self.widths.borrow_mut().clear();
        self.widths_len.set(0);
        self.widths_bytes.set(0);
    }

    /// The width of one laid-out line (never holding `\n`) in `font`, measured whole —
    /// kerning included — once and then remembered, so laying the same text out again
    /// (a drag, a relayout of every label) crosses to the measurer for nothing.
    pub fn line_width(&self, font: FontKey, line: &str, line_width: &dyn Fn(&str) -> f64) -> f64 {
        let hit = self
            .widths
            .borrow()
            .get(&font)
            .and_then(|widths| widths.get(line).copied());
        if let Some(width) = hit {
            return width;
        }
        let width = line_width(line);
        let bytes = line.len() + std::mem::size_of::<(Box<str>, f64)>();
        let mut widths = self.widths.borrow_mut();
        if self.widths_len.get() >= MEMO_LIMIT * 4 || self.widths_bytes.get() + bytes > MEMO_BYTES {
            widths.clear();
            self.widths_len.set(0);
            self.widths_bytes.set(0);
        }
        widths
            .entry(font)
            .or_default()
            .insert(Box::from(line), width);
        self.widths_len.set(self.widths_len.get() + 1);
        self.widths_bytes.set(self.widths_bytes.get() + bytes);
        width
    }

    /// `line_width` for `font`, with char widths cached per full char. The oracle keys its
    /// cache by the first UTF-16 unit (`textMeasurements.ts:183`), so astral chars sharing a
    /// high surrogate share one width there; here each char has its own.
    pub fn metrics<'a>(
        &'a self,
        font: FontKey,
        line_width: &'a dyn Fn(&str) -> f64,
    ) -> CachedMetrics<'a> {
        CachedMetrics {
            cache: self,
            font,
            line_width,
        }
    }

    /// [`wrap::wrap_lines`], each hard line looked up before it is wrapped.
    pub fn wrap_lines(
        &self,
        text: &str,
        max_width: f64,
        font: FontKey,
        line_width: &dyn Fn(&str) -> f64,
    ) -> Vec<WrappedLine> {
        let metrics = self.metrics(font, line_width);
        let key = (font, max_width.to_bits());
        wrap::wrap_lines_with(text, max_width, |line| {
            let hit = self
                .lines
                .borrow()
                .get(&key)
                .and_then(|lines| lines.get(line))
                .cloned();
            hit.unwrap_or_else(|| {
                let wrapped = wrap::wrap_hard_line(line, max_width, &metrics);
                self.remember(key, line, &wrapped);
                wrapped
            })
        })
    }

    /// [`wrap::wrap_text`] through the memo.
    pub fn wrap_text(
        &self,
        text: &str,
        max_width: f64,
        font: FontKey,
        line_width: &dyn Fn(&str) -> f64,
    ) -> String {
        wrap::join(&self.wrap_lines(text, max_width, font, line_width))
    }

    fn remember(&self, key: (FontKey, u64), line: &str, wrapped: &[WrappedLine]) {
        let bytes = line.len()
            + wrapped
                .iter()
                .map(|w| w.text.len() + std::mem::size_of::<WrappedLine>())
                .sum::<usize>();
        let mut lines = self.lines.borrow_mut();
        if self.memo_len.get() >= MEMO_LIMIT || self.memo_bytes.get() + bytes > MEMO_BYTES {
            lines.clear();
            self.memo_len.set(0);
            self.memo_bytes.set(0);
        }
        lines
            .entry(key)
            .or_default()
            .insert(line.to_string(), wrapped.to_vec());
        self.memo_len.set(self.memo_len.get() + 1);
        self.memo_bytes.set(self.memo_bytes.get() + bytes);
    }
}

/// [`MeasureCache::metrics`]: a measurer with its char widths cached.
pub struct CachedMetrics<'a> {
    cache: &'a MeasureCache,
    font: FontKey,
    line_width: &'a dyn Fn(&str) -> f64,
}

impl TextMetrics for CachedMetrics<'_> {
    fn line_width(&self, line: &str) -> f64 {
        (self.line_width)(line)
    }

    fn char_width(&self, c: char) -> f64 {
        let key = (self.font, c);
        if let Some(&width) = self.cache.chars.borrow().get(&key) {
            return width;
        }
        let width = (self.line_width)(c.encode_utf8(&mut [0; 4]));
        self.cache.chars.borrow_mut().insert(key, width);
        width
    }
}
