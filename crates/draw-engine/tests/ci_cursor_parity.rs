//! The two cursor tables, checked against each other.
//!
//! `HoverCursor` is a fieldless enum whose discriminant *is* the wire value, and
//! `HOVER_CURSORS` in `engine/src/engine.ts` is an array indexed by it. Nothing in the type
//! system connects them: a variant added to the Rust enum and forgotten in TypeScript hands the
//! host `undefined`, and one added to the array and forgotten in Rust shifts every cursor after
//! it. The array is append-only by convention, and a convention with no check is a comment.
//!
//! This file is that check. The expected pairs are written out literally, so this test is the
//! specification rather than a restatement of the thing it is verifying: reorder either table and
//! it fails, and the failure names the index and both names.
//!
//! It replaced a claim. `engine/src/engine.ts` carried the sentence "the order is the contract
//! with `engine/hover.rs` and is append-only -- inserting in the middle silently reassigns every
//! cursor after it. `ci_ts_parity` asserts the two agree." There was no `ci_ts_parity` anywhere in
//! the repository; the only occurrence of the string was inside the sentence asserting it. The
//! risk it names is real and the guarantee was not, which is the same shape as a ratchet aimed at
//! the wrong line: worse than none, because a reader trusts it. The claim is now true or the gate
//! is red.

use draw_engine::engine::HoverCursor;

/// `HoverCursor` in discriminant order, against the CSS cursor name the host must use for it.
///
/// The two names differ on purpose and not consistently: `ResizeNs` is `ns-resize` because that is
/// the CSS keyword, while `PointHandle` is `pointer` because a point handle is picked up rather
/// than resized. A test that derived the CSS name from the variant name would pass on the five
/// resize cursors and fail on the sixth, so the pairs are written out.
const EXPECTED: &[(HoverCursor, &str)] = &[
    (HoverCursor::Default, "default"),
    (HoverCursor::Move, "move"),
    (HoverCursor::ResizeNs, "ns-resize"),
    (HoverCursor::ResizeEw, "ew-resize"),
    (HoverCursor::ResizeNesw, "nesw-resize"),
    (HoverCursor::ResizeNwse, "nwse-resize"),
    (HoverCursor::Grab, "grab"),
    (HoverCursor::Grabbing, "grabbing"),
    (HoverCursor::PointHandle, "pointer"),
    (HoverCursor::Crosshair, "crosshair"),
    (HoverCursor::Text, "text"),
];

/// The `HOVER_CURSORS` array out of the host source, as its string literals in order.
///
/// Read from the file rather than imported: the array is module-private and not exported, and
/// importing it would need this test to live in the TypeScript project, where the Rust half would
/// be invisible. The trade is that a rename of the constant fails this test with a file-not-found
/// style message rather than a compile error, which is why the constant name is asserted too.
fn host_cursors() -> Vec<String> {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/engine.ts"))
        .expect("the host wrapper is present at engine/src/engine.ts");

    let start = raw
        .find("const HOVER_CURSORS = [")
        .expect("engine/src/engine.ts still declares HOVER_CURSORS");
    let decl = &raw[start..];
    // Bracket on the brackets, not on a literal `];`. The declaration closes with `] as const;`,
    // so searching for `];` runs straight past the array and collects the quoted names of
    // whatever tables follow it -- 18 "cursors" for 11. The length assertion below caught that,
    // and so did the one before it: the first version of this parser walked every quote instead
    // of pairing them and found 36. Both were mine, both were caught here rather than in review.
    let open = decl.find('[').expect("the declaration opens its array");
    let close = open
        + decl[open..]
            .find(']')
            .expect("the HOVER_CURSORS array is closed");
    let body = &decl[open + 1..close];

    // Pair the quotes, so each name and not each gap between names comes out. `as_chunks`
    // rather than `chunks_exact` because clippy is right that the former says what is meant,
    // and because the remainder it hands back is worth asserting on: an odd number of quotes
    // means the parse has desynchronised, and every name after it would be nonsense.
    let quotes: Vec<usize> = body.match_indices('"').map(|(i, _)| i).collect();
    let (pairs, leftover) = quotes.as_chunks::<2>();
    assert!(
        leftover.is_empty(),
        "the HOVER_CURSORS body has {} quotes, an odd number, so the names cannot be read out \
         of it. Either a name is unquoted or something else in the array is quoted; read the \
         array before trusting anything this file says.",
        quotes.len()
    );
    pairs
        .iter()
        .map(|pair| body[pair[0] + 1..pair[1]].to_string())
        .collect()
}

#[test]
fn the_host_declares_the_table_this_test_checks() {
    // Guards the parse above: if the constant is renamed, every other test here fails on an
    // empty list and the message points at the wrong thing. This one says what happened.
    let host = host_cursors();
    assert!(
        !host.is_empty(),
        "HOVER_CURSORS was not found in engine/src/engine.ts, or it is empty. If the constant \
         was renamed, update this test and the comment on it in the same commit -- the constant \
         is the contract this file exists to check."
    );
}

#[test]
fn the_two_tables_have_the_same_length() {
    let host = host_cursors();
    assert_eq!(
        host.len(),
        EXPECTED.len(),
        "the cursor tables disagree on how many cursors there are: HoverCursor has {} and \
         HOVER_CURSORS has {}. A variant added to one and not the other hands the host a \
         missing or a shifted cursor.\n  HoverCursor: {:?}\n  HOVER_CURSORS: {:?}",
        EXPECTED.len(),
        host.len(),
        EXPECTED.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        host,
    );
}

#[test]
fn every_discriminant_lands_on_its_own_css_cursor() {
    let host = host_cursors();
    for (index, (cursor, css)) in EXPECTED.iter().enumerate() {
        let got = host.get(index).unwrap_or_else(|| {
            panic!(
                "HOVER_CURSORS has no entry at index {index} ({}); the table is shorter than \
                 HoverCursor, so every cursor from here on is shifted",
                cursor.code()
            )
        });
        assert_eq!(
            got,
            css,
            "index {index} disagrees: HoverCursor::{cursor:?} is discriminant {} and the host \
             says {got:?}, expected {css:?}. Either the array was reordered -- it is \
             append-only, because inserting in the middle reassigns every cursor after it -- or \
             one side gained a variant the other did not.",
            cursor.code()
        );
    }
}

#[test]
fn the_discriminants_are_the_indices_assumed_here() {
    // The array is indexed by the discriminant, so a variant whose explicit discriminant is
    // wrong is invisible everywhere else: the tables would still "agree" by position while the
    // engine and the host disagreed about the number. Cheap, and it is the assumption the whole
    // file rests on.
    for (expected_index, (cursor, _)) in EXPECTED.iter().enumerate() {
        assert_eq!(
            cursor.code() as usize,
            expected_index,
            "HoverCursor::{cursor:?} has discriminant {} but sits at index {expected_index} of \
             the expected table. The host indexes by discriminant, so these must be the same \
             number for any of this to mean anything.",
            cursor.code()
        );
    }
}
