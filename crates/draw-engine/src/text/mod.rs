//! Text layout primitives, held to Excalidraw's own code by committed oracle fixtures.
//!
//! The public interface:
//!
//! - [`wrap_text`] / [`wrap_lines`] — soft-wrap a text to a width, the way Excalidraw's
//!   `wrapText` / `getWrappedTextLines` do (`packages/element/src/textWrapping.ts`). Hard
//!   lines that fit are kept verbatim; the rest break on the oracle's rules (whitespace,
//!   after a hyphen, around CJK with its opening/closing/currency exceptions), keep emoji
//!   sequences whole, and trim whitespace exactly where the oracle does. [`wrap_lines`] also
//!   says which bytes of the source each rendered line came from, for caret mapping.
//! - [`parse_tokens`] — the tokenizer underneath (`parseTokens`): break opportunities.
//! - [`normalize_text`] — the oracle's `normalizeText`: CRLF and CR become LF, a tab
//!   becomes eight spaces. For input on its way into a text element; wrapping does not
//!   apply it.
//! - [`TextMetrics`] — what wrapping needs from a font: the advance width of a line
//!   (kerning included) and of a single char measured alone.
//! - [`MeasureCache`] — one per measurer: per-font char widths, as the oracle's
//!   `charWidth` caches them, plus a memo of wrapped hard lines keyed by (line, width,
//!   [`FontKey`]) and bounded in entries and in bytes, so a keystroke re-wraps only the
//!   line it touched.
//!
//! Parity is asserted, not assumed: `tests/ci_text_wrap_oracle.rs` replays
//! `tests/fixtures/text-wrap.oracle.json`, which `tools/text-oracle/generate.mjs` produces by
//! running the oracle's unmodified TypeScript under three width models.

pub mod measure;
mod unicode;
pub mod wrap;

pub use measure::{CachedMetrics, FontKey, MeasureCache, TextMetrics, MEMO_BYTES, MEMO_LIMIT};
pub use wrap::{normalize_text, parse_tokens, wrap_lines, wrap_text, WrappedLine};
