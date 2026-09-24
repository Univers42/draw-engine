//! Text wrapping held to Excalidraw's own code.
//!
//! `fixtures/text-wrap.oracle.json` is written by `tools/text-oracle/generate.mjs`, which
//! runs the oracle's unmodified `textWrapping.ts` over a seeded corpus under three width
//! models (see the fixture's `models`), after replaying the oracle's own unit
//! expectations. Every case has to come out identical — the port is line for line, so a
//! single mismatch is a bug, never noise.
//!
//! - `jsdom`: 10 per UTF-16 unit, what the oracle's unit tests run under;
//! - `table`: additive per-code-point widths;
//! - `kern`: `table` plus pair kerning on whole-line measures only, which fails a port
//!   that sums char widths where the oracle measures the whole line, or the reverse.

mod common;
use common::*;
use draw_engine::text::{parse_tokens, wrap_lines, wrap_text, TextMetrics};
use draw_engine::*;

use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

#[derive(Deserialize)]
struct Fixture {
    oracle: Oracle,
    node: String,
    unicode: String,
    models: Models,
    cases: Vec<Case>,
    tokens: Vec<TokenCase>,
}

#[derive(Deserialize)]
struct Oracle {
    excalidraw: String,
}

#[derive(Deserialize)]
struct Models {
    table: Table,
    kern: Kern,
}

#[derive(Deserialize)]
struct Table {
    widths: HashMap<String, f64>,
}

#[derive(Deserialize)]
struct Kern {
    pairs: HashMap<String, f64>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    model: String,
    text: String,
    #[serde(rename = "maxWidth")]
    max_width: f64,
    expected: String,
    /// `getWrappedTextLines` start/end, in UTF-16 units; only for texts NFC leaves alone.
    offsets: Option<Vec<[usize; 2]>>,
}

#[derive(Deserialize)]
struct TokenCase {
    line: String,
    tokens: Vec<String>,
}

fn load() -> Fixture {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/text-wrap.oracle.json"
    ))
    .expect("fixture present — regenerate with the host's `make oracle-fixtures`");
    serde_json::from_str(&raw).expect("fixture parses")
}

/// The fixture's width models, as a `TextMetrics`.
#[derive(Clone)]
struct Model {
    kind: String,
    widths: HashMap<u32, f64>,
    pairs: HashMap<(char, char), f64>,
}

impl Model {
    fn new(fixture: &Fixture, kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            widths: fixture
                .models
                .table
                .widths
                .iter()
                .map(|(cp, width)| (cp.parse().expect("code point"), *width))
                .collect(),
            pairs: fixture
                .models
                .kern
                .pairs
                .iter()
                .map(|(pair, width)| {
                    let mut chars = pair.chars();
                    let a = chars.next().expect("pair");
                    let b = chars.next().expect("pair");
                    ((a, b), *width)
                })
                .collect(),
        }
    }
}

impl TextMetrics for Model {
    fn line_width(&self, line: &str) -> f64 {
        if self.kind == "jsdom" {
            return line.encode_utf16().count() as f64 * 10.0;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut width: f64 = chars.iter().map(|c| self.widths[&(*c as u32)]).sum();
        if self.kind == "kern" {
            for pair in chars.windows(2) {
                width += self.pairs.get(&(pair[0], pair[1])).copied().unwrap_or(0.0);
            }
        }
        width
    }
}

fn models(fixture: &Fixture) -> HashMap<String, Model> {
    ["jsdom", "table", "kern"]
        .into_iter()
        .map(|kind| (kind.to_string(), Model::new(fixture, kind)))
        .collect()
}

/// Reports every mismatch (the first few in full) and the tally per model, then fails.
fn verdict(label: &str, total: &BTreeMap<String, (usize, usize)>, shown: &[String]) {
    for line in shown {
        println!("{line}");
    }
    let (pass, all) = total
        .values()
        .fold((0, 0), |(p, a), (pass, all)| (p + pass, a + all));
    println!("== {label}: {pass}/{all} identical {total:?}");
    assert_eq!(
        pass,
        all,
        "{label}: {} cases differ from the oracle",
        all - pass
    );
}

#[test]
fn the_fixture_names_its_oracle() {
    let fixture = load();
    assert_eq!(fixture.oracle.excalidraw.len(), 40, "a full SHA");
    assert!(fixture.node.starts_with('v') && !fixture.unicode.is_empty());
    assert!(fixture.cases.len() > 3000 && fixture.tokens.len() > 1000);
}

#[test]
fn every_case_wraps_as_the_oracle_does() {
    let fixture = load();
    let models = models(&fixture);
    let mut total: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut shown = Vec::new();
    for case in &fixture.cases {
        let got = wrap_text(&case.text, case.max_width, &models[&case.model]);
        let entry = total.entry(case.model.clone()).or_default();
        entry.1 += 1;
        if got == case.expected {
            entry.0 += 1;
        } else if shown.len() < 15 {
            shown.push(format!(
                "{} [{}] maxWidth={}\n  text     {:?}\n  expected {:?}\n  got      {:?}",
                case.id, case.model, case.max_width, case.text, case.expected, got
            ));
        }
    }
    verdict("wrap_text", &total, &shown);
}

/// UTF-16 unit index → byte index, for comparing the oracle's offsets with ours.
fn utf16_to_bytes(text: &str) -> Vec<usize> {
    let mut map = Vec::new();
    for (i, c) in text.char_indices() {
        map.extend(std::iter::repeat_n(i, c.len_utf16()));
    }
    map.push(text.len());
    map
}

#[test]
fn every_line_points_where_the_oracle_says() {
    let fixture = load();
    let models = models(&fixture);
    let mut total: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut shown = Vec::new();
    for case in &fixture.cases {
        let Some(expected) = &case.offsets else {
            continue;
        };
        let bytes = utf16_to_bytes(&case.text);
        let expected: Vec<(usize, usize)> = expected
            .iter()
            .map(|&[start, end]| (bytes[start], bytes[end]))
            .collect();
        let got: Vec<(usize, usize)> = wrap_lines(&case.text, case.max_width, &models[&case.model])
            .iter()
            .map(|line| (line.start, line.end))
            .collect();
        let entry = total.entry(case.model.clone()).or_default();
        entry.1 += 1;
        if got == expected {
            entry.0 += 1;
        } else if shown.len() < 15 {
            shown.push(format!(
                "{} [{}] {:?}\n  expected {expected:?}\n  got      {got:?}",
                case.id, case.model, case.text
            ));
        }
    }
    verdict("wrap_lines offsets", &total, &shown);
}

#[test]
fn every_line_tokenizes_as_the_oracle_does() {
    let fixture = load();
    let mut total: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut shown = Vec::new();
    for case in &fixture.tokens {
        let got = parse_tokens(&case.line);
        let entry = total.entry("tokens".into()).or_default();
        entry.1 += 1;
        if got == case.tokens {
            entry.0 += 1;
        } else if shown.len() < 15 {
            shown.push(format!(
                "{:?}\n  expected {:?}\n  got      {:?}",
                case.line, case.tokens, got
            ));
        }
    }
    verdict("parse_tokens", &total, &shown);
}

thread_local! {
    /// The model the engine's measure hook reads: the hook is a plain `fn`.
    static ENGINE_MODEL: RefCell<Option<Model>> = const { RefCell::new(None) };
}

fn engine_measure(text: &str, font_size: f64) -> (f64, f64) {
    ENGINE_MODEL.with(|model| {
        let model = model.borrow();
        let model = model.as_ref().expect("a model is set");
        let lines: Vec<&str> = text.split('\n').collect();
        let width = lines
            .iter()
            .map(|line| model.line_width(line))
            .fold(0.0, f64::max);
        (width, lines.len() as f64 * font_size * TEXT_LINE_HEIGHT)
    })
}

/// The shared function every text wrap in the engine goes through, driven the way a
/// dragged-out (fixed-width) text is: set its text and read back what it stores.
fn engine_wrap(text: &str, max_width: f64) -> Option<String> {
    let mut element = text_at(0.0, 0.0, max_width, 25.0);
    element.auto_resize = Some(false);
    let id = element.id.clone();
    let mut engine = engine_with_scene(vec![element]);
    engine.set_measure_text(engine_measure);
    engine.set_element_text(&id, text);
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id && !element.is_deleted)
        .and_then(|element| element.text)
}

#[test]
fn the_engine_wraps_text_as_the_oracle_does() {
    let fixture = load();
    let models = models(&fixture);
    let mut total: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut shown = Vec::new();
    for case in &fixture.cases {
        // Not wrapping: the engine gives a column at least 8px (`with_text`) and deletes
        // a text emptied of all but whitespace (`set_element_text`).
        if case.max_width < 8.0 || case.text.trim().is_empty() {
            continue;
        }
        ENGINE_MODEL.with(|model| *model.borrow_mut() = Some(models[&case.model].clone()));
        let got = engine_wrap(&case.text, case.max_width);
        let entry = total.entry(case.model.clone()).or_default();
        entry.1 += 1;
        if got.as_deref() == Some(case.expected.as_str()) {
            entry.0 += 1;
        } else if shown.len() < 15 {
            shown.push(format!(
                "{} [{}] maxWidth={}\n  text     {:?}\n  expected {:?}\n  engine   {:?}",
                case.id, case.model, case.max_width, case.text, case.expected, got
            ));
        }
    }
    verdict("engine", &total, &shown);
}
