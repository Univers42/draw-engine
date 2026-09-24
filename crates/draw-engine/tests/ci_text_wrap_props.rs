//! What the text wrapper promises beyond matching the oracle case by case
//! (`ci_text_wrap_oracle.rs`): offsets that lead back to the source, lines that fit, a cost
//! linear in the text, and caches that answer instead of measuring again.

mod common;
use common::*;
use draw_engine::text::{
    parse_tokens, wrap_lines, wrap_text, FontKey, MeasureCache, TextMetrics, WrappedLine,
    MEMO_BYTES, MEMO_LIMIT,
};
use std::cell::Cell;
use unicode_normalization::UnicodeNormalization;

/// 10 per UTF-16 unit: the model the oracle's own unit tests run under.
struct Units;

impl TextMetrics for Units {
    fn line_width(&self, line: &str) -> f64 {
        line.encode_utf16().count() as f64 * 10.0
    }
}

/// Proportional and additive: narrow and wide latin, fullwidth CJK, wide emoji, marks
/// that take no room.
struct Proportional;

impl TextMetrics for Proportional {
    fn line_width(&self, line: &str) -> f64 {
        line.chars()
            .map(|c| match c {
                'i' | 'l' | 'j' | '.' | ',' | '\'' | '!' => 4.25,
                'm' | 'w' | 'M' | 'W' => 15.5,
                '\u{300}'..='\u{36f}' | '\u{3099}' | '\u{200d}' | '\u{fe0f}' => 0.0,
                '\u{1100}'..='\u{11ff}' | '\u{3000}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' => 20.0,
                '\u{f900}'..='\u{faff}' | '\u{ff00}'..='\u{ffef}' => 20.0,
                c if c >= '\u{1f000}' => 24.0,
                _ => 9.75,
            })
            .sum()
    }
}

/// Counts what it is asked to measure.
#[derive(Default)]
struct Counting {
    calls: Cell<usize>,
    chars: Cell<usize>,
}

impl Counting {
    fn measure(&self, line: &str) -> f64 {
        self.calls.set(self.calls.get() + 1);
        self.chars.set(self.chars.get() + line.chars().count());
        Units.line_width(line)
    }
}

impl TextMetrics for Counting {
    fn line_width(&self, line: &str) -> f64 {
        self.measure(line)
    }
}

/// JavaScript's `\s`: what a soft break or the end of a hard line may drop.
fn is_js_space(c: char) -> bool {
    c.is_whitespace() && c != '\u{85}' || c == '\u{feff}'
}

/// A seeded corpus: latin, hyphens, punctuation, CJK, emoji, decomposed text, and the
/// whitespace JavaScript and NFC treat unusually (NBSP, ideographic space, U+2001, which NFC
/// turns into U+2003, a lone CR, the line separator).
fn corpus() -> Vec<String> {
    const WORDS: &[&str] = &[
        "the",
        "quick",
        "Excalidraw",
        "whiteboard",
        "a",
        "I",
        "state-of-the-art",
        "non-profit",
        "(hello)",
        "end.",
        "a/b/c",
        "99,100.99",
        "Hippopotomonstrosesquippedaliophobia",
        "こんにちは世界",
        "你好，世界。",
        "「引用」",
        "안녕하세요",
        "￥1000",
        "😀",
        "👩🏽‍🦰",
        "🇨🇿",
        "✅",
        "cafe\u{301}",
        "\u{3066}\u{3099}",
        "\u{1100}\u{1161}",
        "o\u{302}\u{301}",
        "\u{f900}\u{f901}",
        "x\u{2001}y",
        "wait...",
    ];
    const SEPARATORS: &[&str] = &[
        " ", " ", "  ", "", "\n", "\t", "\u{a0}", "\u{3000}", "\u{2028}", "\r", "\u{2001}",
    ];
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = |n: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % n as u64) as usize
    };
    (0..400)
        .map(|_| {
            let mut text = String::new();
            for i in 0..1 + next(12) {
                if i > 0 {
                    text.push_str(SEPARATORS[next(SEPARATORS.len())]);
                }
                text.push_str(WORDS[next(WORDS.len())]);
            }
            if next(6) == 0 {
                text.push_str("   ");
            }
            text
        })
        .collect()
}

const WIDTHS: &[f64] = &[0.0, 5.0, 10.0, 25.0, 40.0, 60.0, 100.0, 150.0, 1e9];

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

/// textWrapping.test.ts:22-27, and the offsets of `getHardLineBreaks`.
#[test]
fn an_invalid_width_keeps_the_hard_lines() {
    let text = "Hello Excalidraw\nx";
    for width in [f64::NAN, -1.0, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(wrap_text(text, width, &Units), text, "{width}");
        let spans: Vec<(usize, usize)> = wrap_lines(text, width, &Units)
            .iter()
            .map(|line| (line.start, line.end))
            .collect();
        assert_eq!(spans, [(0, 16), (17, 18)], "{width}");
    }
}

/// Every rendered line is a stretch of the source, in order; only whitespace falls
/// between them; and where NFC left the text alone, the stretch IS the line.
#[test]
fn offsets_lead_back_to_the_source() {
    let mut lines_checked = 0;
    for text in corpus() {
        for &width in WIDTHS {
            let lines = wrap_lines(&text, width, &Units);
            let joined: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
            assert_eq!(joined.join("\n"), wrap_text(&text, width, &Units));
            let mut cursor = 0;
            for line in &lines {
                let context = format!("{text:?} @{width}: {line:?}");
                assert!(cursor <= line.start && line.start <= line.end, "{context}");
                assert!(
                    text[cursor..line.start].chars().all(is_js_space),
                    "only whitespace is dropped: {context}"
                );
                let source = &text[line.start..line.end];
                if text == nfc(&text) {
                    assert_eq!(source, line.text, "{context}");
                } else {
                    assert_eq!(nfc(source), nfc(&line.text), "{context}");
                }
                cursor = line.end;
                lines_checked += 1;
            }
            assert!(text[cursor..].chars().all(is_js_space), "{text:?} @{width}");
        }
    }
    assert!(lines_checked > 10_000, "{lines_checked}");
}

/// The oracle keeps a line that fits verbatim (textWrapping.ts:462-469) and composes only
/// the lines it has to break; the offsets still count bytes of the decomposed source.
#[test]
fn only_a_line_that_wraps_is_composed() {
    let text = "cafe\u{301} cafe\u{301}";
    assert_eq!(wrap_text(text, 1000.0, &Units), text, "fits: verbatim");
    let line = |text: &str, start, end| WrappedLine {
        text: text.into(),
        start,
        end,
    };
    assert_eq!(
        wrap_lines(text, 45.0, &Units),
        [line("café", 0, 6), line("café", 7, 13)]
    );
}

/// A char NFC rewrites on its own still starts a segment of its own, so a break before it
/// maps back exactly: U+2001 becomes U+2003, and the CJK compatibility ideographs become
/// their unified forms.
#[test]
fn a_rewritten_char_keeps_exact_offsets() {
    let spans = |text: &str, width: f64| -> Vec<(String, usize, usize)> {
        wrap_lines(text, width, &Units)
            .into_iter()
            .map(|line| (line.text, line.start, line.end))
            .collect()
    };
    assert_eq!(
        spans("a\u{2001}b", 10.0),
        [("a".into(), 0, 1), ("b".into(), 4, 5)]
    );
    assert_eq!(
        spans("\u{f900}\u{f901}", 10.0),
        [("\u{8c48}".into(), 0, 3), ("\u{66f4}".into(), 3, 6)]
    );
}

/// The one place offsets are approximate: marks NFC reorders, carried by a char a break
/// follows. The segment goes to the earlier line whole; the lines stay in order.
#[test]
fn reordered_marks_after_a_break_stay_in_order() {
    let text = "ab \u{301}\u{327}cd";
    assert_ne!(nfc(text), text, "NFC reorders the marks");
    let lines = wrap_lines(text, 20.0, &Units);
    let mut cursor = 0;
    for line in &lines {
        assert!(cursor <= line.start && line.start <= line.end && line.end <= text.len());
        cursor = line.end;
    }
    assert_eq!(parse_tokens(text).concat(), nfc(text), "no char is lost");
}

/// Additive widths leave the oracle nothing to disagree with itself about, so every line
/// of two chars or more fits — unless it holds an emoji, which is never broken.
#[test]
fn no_line_is_wider_than_the_width_unless_it_holds_an_emoji() {
    let holds_emoji = |line: &str| line.chars().any(|c| c >= '\u{1f000}' || c == '✅');
    let models: [&dyn TextMetrics; 2] = [&Units, &Proportional];
    for text in corpus() {
        for &width in WIDTHS {
            for metrics in models {
                for line in wrap_lines(&text, width, metrics) {
                    if line.text.chars().count() < 2 || holds_emoji(&line.text) {
                        continue;
                    }
                    assert!(
                        metrics.line_width(&line.text) <= width,
                        "{:?} is wider than {width} (from {text:?})",
                        line.text
                    );
                }
            }
        }
    }
}

fn prose(len: usize) -> String {
    const WORDS: &[&str] = &[
        "the",
        "quick",
        "brown",
        "fox",
        "jumps",
        "over",
        "a",
        "lazy",
        "dog",
        "and",
        "keeps",
        "on",
        "running",
        "far",
        "away",
        "from",
        "Excalidraw",
        "whiteboards,",
        "state-of-the-art",
    ];
    let mut text = String::new();
    for word in WORDS.iter().cycle() {
        if text.len() >= len {
            break;
        }
        text.push_str(word);
        text.push(' ');
    }
    text
}

/// Nothing is measured over and over: the old wrapper measured a 3,000-char word 3,003
/// times over 58,569 chars. A counting measurer bounds calls AND chars measured, with and
/// without the cache.
#[test]
fn measuring_stays_linear_in_the_text() {
    for text in [prose(5000), "abcdefghij".repeat(500)] {
        let n = text.chars().count();
        let plain = Counting::default();
        let wrapped = wrap_text(&text, 300.0, &plain);
        let counting = Counting::default();
        let cached = MeasureCache::new().wrap_text(&text, 300.0, FontKey::legacy(20.0), &|line| {
            counting.measure(line)
        });
        assert_eq!(wrapped, cached);
        println!(
            "{n} chars: {} calls over {} chars; cached {} calls over {} chars",
            plain.calls.get(),
            plain.chars.get(),
            counting.calls.get(),
            counting.chars.get()
        );
        for (calls, chars) in [
            (plain.calls.get(), plain.chars.get()),
            (counting.calls.get(), counting.chars.get()),
        ] {
            assert!(calls < 3 * n, "{calls} calls for {n} chars");
            assert!(chars < 10 * n, "{chars} chars measured for {n}");
        }
    }
}

const PARAGRAPHS: &str = "the first hard line, long enough to wrap\na second one\n\
                          and a third hard line that wraps as well";

#[test]
fn a_rewrap_is_answered_by_the_memo() {
    let cache = MeasureCache::new();
    let font = FontKey::legacy(20.0);
    let counting = Counting::default();
    let measure = |line: &str| counting.measure(line);

    let first = cache.wrap_text(PARAGRAPHS, 100.0, font, &measure);
    let cold = counting.calls.get();
    assert!(cold > 0);
    assert_eq!(first, wrap_text(PARAGRAPHS, 100.0, &Units));

    assert_eq!(cache.wrap_text(PARAGRAPHS, 100.0, font, &measure), first);
    assert_eq!(counting.calls.get(), cold, "nothing measured again");

    // A keystroke in one hard line re-wraps that line and looks the others up.
    let edited = PARAGRAPHS.replace("a second one", "a second one, edited");
    let before = counting.calls.get();
    assert_eq!(
        cache.wrap_text(&edited, 100.0, font, &measure),
        wrap_text(&edited, 100.0, &Units)
    );
    let warm = counting.calls.get() - before;
    assert!(
        warm > 0 && warm < cold,
        "{warm} calls for one line, {cold} for three"
    );

    // Another width, another font, or a cleared cache: measured afresh.
    for (width, font) in [(90.0, font), (100.0, FontKey::legacy(21.0))] {
        let before = counting.calls.get();
        cache.wrap_text(PARAGRAPHS, width, font, &measure);
        assert!(counting.calls.get() > before, "{width} {font:?}");
    }
    cache.clear();
    let before = counting.calls.get();
    cache.wrap_text(PARAGRAPHS, 100.0, FontKey::legacy(20.0), &measure);
    assert!(counting.calls.get() > before, "clear() forgets");
}

#[test]
fn the_memo_is_bounded() {
    let cache = MeasureCache::new();
    let font = FontKey::legacy(20.0);
    let counting = Counting::default();
    let measure = |line: &str| counting.measure(line);
    let line = |i: usize| format!("line number {i} wraps");
    for i in 0..=MEMO_LIMIT {
        cache.wrap_text(&line(i), 50.0, font, &measure);
    }
    let before = counting.calls.get();
    cache.wrap_text(&line(MEMO_LIMIT), 50.0, font, &measure);
    assert_eq!(
        counting.calls.get(),
        before,
        "the latest line is remembered"
    );
    cache.wrap_text(&line(0), 50.0, font, &measure);
    assert!(
        counting.calls.get() > before,
        "the memo started over when full"
    );
}

/// Bounded in bytes as well as in entries. With a peer watching, every keystroke into a
/// label is previewed (`text_preview`), and each preview memoises the whole hard line typed
/// so far: 4,096 of those is tens of MB, kept for good by a wasm memory that never shrinks.
#[test]
fn the_memo_is_bounded_in_bytes() {
    let cache = MeasureCache::new();
    let font = FontKey::legacy(20.0);
    let counting = Counting::default();
    let measure = |line: &str| counting.measure(line);
    // One paragraph typed a keystroke at a time: 3,000 lines, all different, 4.5 MB of them.
    let paragraph = prose(3000);
    let typed: Vec<&str> = (1..=paragraph.len()).map(|n| &paragraph[..n]).collect();
    for line in &typed {
        cache.wrap_text(line, 300.0, font, &measure);
    }
    // The memo starts over wholesale, so what it still holds is the newest lines. Walk
    // back until one has to be measured; the memo holds at least the hard lines themselves.
    let mut held = 0;
    let mut held_bytes = 0;
    for line in typed.iter().rev() {
        let before = counting.calls.get();
        cache.wrap_text(line, 300.0, font, &measure);
        if counting.calls.get() > before {
            break;
        }
        held += 1;
        held_bytes += line.len();
    }
    println!("the memo held the last {held} lines, {held_bytes} bytes of them");
    assert!(held > 0, "the latest line is remembered");
    assert!(
        held_bytes <= MEMO_BYTES,
        "{held_bytes} bytes of hard lines held, over {MEMO_BYTES}"
    );
}

/// Char widths are per font, and per char: the oracle keys its cache by the first UTF-16
/// unit, so two emoji sharing a high surrogate share a width there — not here.
#[test]
fn char_widths_are_cached_per_font_and_per_char() {
    fn width(line: &str, scale: f64) -> f64 {
        line.chars()
            .map(|c| (c as u32 % 7 + 1) as f64 * scale)
            .sum()
    }
    let cache = MeasureCache::new();
    let calls = &Cell::new(0);
    let measurer = |scale: f64| {
        move |line: &str| {
            calls.set(calls.get() + 1);
            width(line, scale)
        }
    };
    let (small, large) = (measurer(1.0), measurer(4.0));
    let small_font = cache.metrics(FontKey::new(5, 16.0), &small);
    let large_font = cache.metrics(FontKey::new(5, 64.0), &large);

    assert_eq!(small_font.char_width('x'), width("x", 1.0));
    assert_eq!(small_font.char_width('x'), width("x", 1.0));
    assert_eq!(calls.get(), 1, "the second lookup is cached");

    assert_eq!(
        large_font.char_width('x'),
        width("x", 4.0),
        "fonts are kept apart"
    );
    assert_eq!(small_font.char_width('\u{1f600}'), width("\u{1f600}", 1.0));
    assert_eq!(
        small_font.char_width('\u{1f601}'),
        width("\u{1f601}", 1.0),
        "same high surrogate as U+1F600, its own width"
    );
    assert_ne!(width("\u{1f600}", 1.0), width("\u{1f601}", 1.0));
}

thread_local! {
    /// Calls to, and chars through, the engine's measure hook — a plain `fn`.
    static HOOK: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

fn counted_measure(text: &str, font_size: f64) -> (f64, f64) {
    HOOK.with(|hook| {
        let (calls, chars) = hook.get();
        hook.set((calls + 1, chars + text.chars().count()));
    });
    measure_text(text, font_size)
}

/// The same bounds through the engine's shared wrap, and its memo: typing the same text
/// into a column again measures only the final size.
#[test]
fn the_engine_measures_linearly_and_remembers() {
    let word = "abcdefghij".repeat(500);
    let n = word.chars().count();
    let mut column = text_at(0.0, 0.0, 300.0, 25.0);
    column.auto_resize = Some(false);
    let id = column.id.clone();
    let mut engine = engine_with_scene(vec![column]);
    engine.set_measure_text(counted_measure);

    engine.set_element_text(&id, &word);
    let (calls, chars) = HOOK.with(|hook| hook.replace((0, 0)));
    println!("engine, {n}-char word: {calls} calls over {chars} chars");
    assert!(calls < 3 * n, "{calls} calls for {n} chars");
    assert!(chars < 10 * n, "{chars} chars measured for {n}");

    engine.set_element_text(&id, &word);
    let (calls, _) = HOOK.with(|hook| hook.replace((0, 0)));
    assert_eq!(
        calls, 1,
        "the wrap is remembered; only the final size is measured"
    );
}
