//! Plain text pasted as text elements — the oracle's `addTextFromPaste`
//! (`packages/excalidraw/components/App.tsx@1118751f:4979-5096`).
//!
//! `paste_json` (`clipboard.rs`) takes this app's own element JSON and refuses everything
//! else, so a pasted line of text used to become nothing at all. This file is the other
//! branch of the oracle's `insertClipboardContent`: JSON that is not elements is text, and
//! text becomes elements (`clipboard.ts@1118751f:538-553` → `App.tsx@1118751f:4757`).
//!
//! Four decisions, each asserted directly rather than through what they add up to:
//!
//! 1. **One element per line**, split on `\n` alone — `const lines = isPlainPaste ? [text]
//!    : text.split("\n")` (`App.tsx@1118751f:5017`). Ctrl+Shift+V is the one-element form
//!    (`isPlainPaste`); it is Phase 4.8's `shortkey.md:421,423`, not this.
//! 2. **The wrap width** is half the visible width in scene units, capped at 800 and
//!    floored at 200 — `Math.max(Math.min((x2 - x1) * 0.5, 800), 200)`
//!    (`App.tsx@1118751f:5011-5013`), where `x2 - x1` is the viewport in scene units
//!    (`getVisibleSceneBounds`, `bounds.ts@1118751f:1149-1159`).
//! 3. **Placement**: each line's box is centred on the pointer's `x`, and the running
//!    `currentY` starts at the pointer's `y` and grows by the line's own height plus a
//!    10-unit gap — `startX = x - metrics.width / 2`, `startY = currentY - metrics.height
//!    / 2`, `currentY += element.height + LINE_GAP` (`App.tsx@1118751f:5014`, `:5038-5039`,
//!    `:5052`).
//! 4. **The element is minted the way any other new text is** (`new_text_element`, the text
//!    tool's own path): a fresh id, a fresh seed, `version` 1, no container, and the frame
//!    under the pointer settled by the commit (`judge_created_frame_membership`), which is
//!    what the oracle's `getTopLayerFrameAtSceneCoords` names as `frameId`
//!    (`App.tsx@1118751f:5022-5025`, `:5049`).
//!
//! The measurer is `common::measure_text`: a line of `n` characters is `n * size * 0.5 + 4`
//! wide, so at the default 20pt a character is 14 units and an 800-unit viewport wraps at
//! 400 — the numbers below are read off that, not guessed.

mod common;
use common::*;
use draw_engine::*;

/// A fresh text is written in 20pt Excalifont, whose line height is 1.25
/// (`constants.ts@1118751f:265` and the family's own metrics), so one line of it is 25
/// units tall.
const LINE_PX: f64 = 25.0;
/// `LINE_GAP` (`App.tsx@1118751f:5014`).
const GAP: f64 = 10.0;
/// A character in `common::measure_text` at 20pt: `size * 0.5` per character, plus 4 for
/// the line whatever it holds.
const CHAR_PX: f64 = 10.0;
const LINE_PAD: f64 = 4.0;

/// How wide `count` characters measure, through `common::measure_text`.
fn measured(count: usize) -> f64 {
    count as f64 * CHAR_PX + LINE_PAD
}
/// `maxTextWidth` for an 800-unit viewport at 1× — `800 / 2` — and the cap and the floor
/// it is held between.
const WIDTH_800: f64 = 400.0;
const MAX_WRAP: f64 = 800.0;
const MIN_WRAP: f64 = 200.0;

/// A line of `count` characters (`x` is one character wide to the measurer).
fn line_of(count: usize) -> String {
    "x".repeat(count)
}

fn board() -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_measure_text(measure_text);
    engine
}

/// The texts a paste just made, in the order it made them: the selection it leaves behind
/// is exactly what it pasted (`App.tsx@1118751f:5075-5080`).
fn pasted_texts(engine: &DrawEngine) -> Vec<DrawElement> {
    let selected = engine.get_selection();
    let scene = engine.get_scene();
    let mut out: Vec<DrawElement> = selected
        .iter()
        .filter_map(|id| scene.iter().find(|e| e.id == *id).cloned())
        .collect();
    out.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    out
}

/// The elements a board actually shows: an undone creation is tombstoned, not dropped, so
/// that the deletion reaches the server and the peers (`engine/stamp.rs`).
fn live(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .collect()
}

fn source_of(element: &DrawElement) -> &str {
    element
        .original_text
        .as_deref()
        .expect("a pasted text keeps what was pasted")
}

fn drawn_of(element: &DrawElement) -> &str {
    element.text.as_deref().expect("a pasted text is drawn")
}

// ---------------------------------------------------------------- 1. per line

/// The load-bearing one: a paste of two lines is **two** text elements. Ctrl+V is not
/// "one element holding a paragraph" — that is what Ctrl+Shift+V is for, and this is not
/// it (`App.tsx@1118751f:5017`).
#[test]
fn two_lines_make_two_text_elements() {
    let mut engine = board();
    assert!(engine.paste_text("alpha\nbeta", Some((400.0, 300.0))));
    let made = pasted_texts(&engine);
    assert_eq!(made.len(), 2, "one element per line, not one for the blob");
    assert_eq!(source_of(&made[0]), "alpha");
    assert_eq!(source_of(&made[1]), "beta");
    assert!(made.iter().all(|e| e.kind == DrawElementType::Text));
}

/// One line with no newline in it is one element — the split that produces nothing must not
/// be read as "no text was pasted".
#[test]
fn a_single_line_with_no_newline_makes_one_element() {
    let mut engine = board();
    assert!(engine.paste_text("alpha", Some((400.0, 300.0))));
    assert_eq!(pasted_texts(&engine).len(), 1);
}

/// Nothing pasted is nothing made: an empty paste, and one of nothing but a newline, leave
/// the scene alone and report no paste (`App.tsx@1118751f:5069-5071` returns when there is
/// no element to insert).
#[test]
fn an_empty_paste_makes_nothing() {
    for empty in ["", "\n", "   ", "\r\n", "\n\n\n"] {
        let mut engine = board();
        assert!(
            !engine.paste_text(empty, Some((400.0, 300.0))),
            "{empty:?} is not a paste"
        );
        assert!(live(&engine).is_empty(), "{empty:?} left {empty:?} behind");
        assert!(engine.get_selection().is_empty());
    }
}

/// A trailing newline is the end of the last line, not an empty line to skip past: the
/// element it ends is still made (`"alpha\n".split("\n")` is `["alpha", ""]`, and the empty
/// one is dropped because its text is empty, `App.tsx@1118751f:5020-5021`).
#[test]
fn a_trailing_newline_does_not_lose_the_line_before_it() {
    let mut engine = board();
    assert!(engine.paste_text("alpha\n", Some((400.0, 300.0))));
    assert_eq!(pasted_texts(&engine).len(), 1);
}

/// `\r\n` from a Windows clipboard is one break, not two: each line keeps no carriage
/// return. The oracle splits on `\n` alone and lets `trim()` take the `\r` off
/// (`App.tsx@1118751f:5017`, `:5020`).
#[test]
fn crlf_is_one_break_and_leaves_no_carriage_return() {
    let mut engine = board();
    assert!(engine.paste_text("alpha\r\nbeta\r\n", Some((400.0, 300.0))));
    let made = pasted_texts(&engine);
    assert_eq!(made.len(), 2);
    for element in &made {
        assert!(
            !source_of(element).contains('\r'),
            "{:?} kept a carriage return",
            source_of(element)
        );
    }
    assert_eq!(source_of(&made[0]), "alpha");
    assert_eq!(source_of(&made[1]), "beta");
}

/// What was on the clipboard is what lands, minus the whitespace around each line:
/// `normalizeText(line).trim()` (`App.tsx@1118751f:5020`). A line that is nothing but
/// whitespace is dropped, as the trimmed text is empty.
#[test]
fn surrounding_whitespace_is_trimmed_and_a_blank_line_is_dropped() {
    let mut engine = board();
    assert!(engine.paste_text("  alpha  \n   \n\tbeta\t", Some((400.0, 300.0))));
    let made = pasted_texts(&engine);
    assert_eq!(made.len(), 2, "the whitespace-only line is not a text");
    assert_eq!(source_of(&made[0]), "alpha");
    assert_eq!(source_of(&made[1]), "beta");
}

/// A newline inside one line of text — a wrapped paragraph pasted as a single blob — is
/// still a break, and so still one element per line. This is the oracle's behaviour, not
/// the user's intention, which is exactly what Ctrl+Shift+V (`isPlainPaste`) is for: the
/// whole paragraph is *not* forced into one element here
/// (`App.tsx@1118751f:5017`, `5082-5095`; `shortkey.md:421,423` is Phase 4.8).
#[test]
fn a_newline_the_user_did_not_mean_as_a_break_still_splits() {
    let mut engine = board();
    let paragraph = "one two three\nfour five six\nseven eight nine";
    assert!(engine.paste_text(paragraph, Some((400.0, 300.0))));
    assert_eq!(pasted_texts(&engine).len(), 3);
}

// ---------------------------------------------------------------- 2. wrap width

/// A line wider than `maxTextWidth` is wrapped, and the element is told to keep that width
/// (`autoResize: !isTextUnwrapped`, `App.tsx@1118751f:5048`): a free text that does not
/// auto-size wraps at its own width on every later edit
/// (`packages/element/src/textElement.ts@1118751f:89-99`), which is what `wrap_width` gives
/// ours (`text/layout.rs`).
#[test]
fn a_line_wider_than_the_wrap_width_is_wrapped() {
    let mut engine = board();
    let long = line_of(60); // 604 > 400
    assert!(engine.paste_text(&long, Some((400.0, 300.0))));
    let made = pasted_texts(&engine);
    assert_eq!(made.len(), 1, "wrapping is not another element");
    let element = &made[0];
    assert!(
        drawn_of(element).contains('\n'),
        "a 604-wide line was not wrapped"
    );
    assert_eq!(
        source_of(element),
        long,
        "what was pasted is kept whole; only the drawn lines are broken"
    );
    assert_eq!(element.auto_resize, Some(false));
    for rendered in drawn_of(element).split('\n') {
        assert!(
            measured(rendered.len()) <= WIDTH_800,
            "{rendered:?} is wider than {WIDTH_800}"
        );
    }
    assert!(element.width <= WIDTH_800);
}

/// The width is `max(min(visible_width / 2, 800), 200)` — half the visible width in scene
/// units (`App.tsx@1118751f:5011-5013`, `bounds.ts@1118751f:1149-1159`). All three edges,
/// read off the measurer: a line of 28 characters is 396 wide and stays one line, 29 is
/// 410 and wraps.
#[test]
fn the_wrap_width_is_half_the_visible_width() {
    // 39 characters: 394 <= 400.
    let mut engine = board();
    assert!(engine.paste_text(&line_of(39), Some((400.0, 300.0))));
    let element = &pasted_texts(&engine)[0];
    assert!(
        !drawn_of(element).contains('\n'),
        "394 fits inside a 400-wide half viewport"
    );
    assert_eq!(element.auto_resize, Some(true));

    // 40 characters: 404 > 400.
    let mut engine = board();
    assert!(engine.paste_text(&line_of(40), Some((400.0, 300.0))));
    let element = &pasted_texts(&engine)[0];
    assert!(
        drawn_of(element).contains('\n'),
        "404 does not fit inside a 400-wide half viewport"
    );
}

/// Zoomed out, the visible width in scene units grows and the cap is 800 — the same line
/// that wrapped at 1× does not at 0.5× (`x2 - x1` is `width / zoom`,
/// `bounds.ts@1118751f:1157`).
#[test]
fn the_wrap_width_is_capped_at_800() {
    let mut engine = board();
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale: 0.5,
    });
    // 800 / 0.5 = 1600 visible units, half is 800 — the cap, and what a 90-character line
    // (904) is measured against.
    let ninety = line_of(90);
    assert!(engine.paste_text(&ninety, Some((0.0, 0.0))));
    let element = &pasted_texts(&engine)[0];
    assert!(drawn_of(element).contains('\n'), "904 > 800, the cap");
    for rendered in drawn_of(element).split('\n') {
        assert!(measured(rendered.len()) <= MAX_WRAP, "{rendered:?}");
    }

    // 1000 / 0.5 = 2000 visible units, half is 1000 — capped down to 800, so a line of
    // 804 wraps at 800 here even though half the viewport could have held it whole.
    let mut engine = board();
    engine.set_viewport(1000.0, 600.0, 1.0);
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale: 0.5,
    });
    let eight_oh_four = line_of(80); // 804 > the 800 cap, < the 1000 half-viewport.
    assert!(engine.paste_text(&eight_oh_four, Some((0.0, 0.0))));
    let element = &pasted_texts(&engine)[0];
    assert!(drawn_of(element).contains('\n'), "804 > the 800 cap");
    for rendered in drawn_of(element).split('\n') {
        assert!(measured(rendered.len()) <= MAX_WRAP, "{rendered:?}");
    }
}

/// Zoomed in, the floor is 200: a half viewport narrower than 400 still wraps at 200, and
/// never below it (`Math.max(..., 200)`).
#[test]
fn the_wrap_width_is_floored_at_200() {
    let mut engine = board();
    engine.set_viewport(300.0, 600.0, 1.0); // 150 wide in scene units; half is 75.
    assert!(engine.paste_text(&line_of(30), Some((0.0, 0.0)))); // 304 > 200.
    let element = &pasted_texts(&engine)[0];
    assert!(drawn_of(element).contains('\n'), "304 > the 200 floor");
    for rendered in drawn_of(element).split('\n') {
        assert!(measured(rendered.len()) <= MIN_WRAP, "{rendered:?}");
    }
}

// ---------------------------------------------------------------- 3. placement

/// Every line's box is centred on the pointer's `x`, and the first is centred on its `y`
/// (`startX = x - metrics.width / 2`, `startY = currentY - metrics.height / 2`,
/// `App.tsx@1118751f:5038-5039`).
#[test]
fn the_first_line_is_centred_on_the_pointer() {
    let mut engine = board();
    assert!(engine.paste_text("alpha", Some((400.0, 300.0))));
    let element = &pasted_texts(&engine)[0];
    assert_close(element.x + element.width / 2.0, 400.0);
    assert_close(element.y + element.height / 2.0, 300.0);
    assert_close(element.height, LINE_PX);
}

/// The second line is one line height plus the gap below the first — `currentY +=
/// element.height + LINE_GAP` (`App.tsx@1118751f:5052`).
#[test]
fn the_next_line_is_one_line_and_a_gap_below() {
    let mut engine = board();
    assert!(engine.paste_text("alpha\nbeta", Some((400.0, 300.0))));
    let made = pasted_texts(&engine);
    assert_close(made[1].y - made[0].y, LINE_PX + GAP);
    assert_close(made[0].y + made[0].height / 2.0, 300.0);
}

/// A blank line between two is one paragraph's worth of space, however many there are:
/// `getLineHeightInPx(fontSize, lineHeight) + LINE_GAP`, and only when the line before it
/// was not itself blank (`App.tsx@1118751f:5053-5061`).
#[test]
fn blank_lines_make_one_paragraph_gap_not_one_each() {
    let mut engine = board();
    assert!(engine.paste_text("alpha\n\n\n\nbeta", Some((400.0, 300.0))));
    let made = pasted_texts(&engine);
    assert_eq!(made.len(), 2);
    assert!(
        (made[1].y - made[0].y - (LINE_PX + GAP + LINE_PX + GAP)).abs() < EPS,
        "three blank lines are one paragraph ({}), not three ({})",
        made[1].y - made[0].y,
        LINE_PX + GAP + LINE_PX + GAP
    );

    // A paste that *starts* blank has nothing before it to be a gap from, so the first
    // line lands on the pointer.
    let mut engine = board();
    assert!(engine.paste_text("\n\nalpha", Some((400.0, 300.0))));
    let made = pasted_texts(&engine);
    assert_eq!(made.len(), 1);
    assert_close(made[0].y + made[0].height / 2.0, 300.0);
}

/// With no pointer to paste at, the text lands where the pointer would have been at the
/// top left of the canvas — `viewport.lastPosition` starts at `{x: 0, y: 0}`
/// (`App.viewport.ts@1118751f:438`).
#[test]
fn no_pointer_puts_the_text_at_the_top_left_of_the_viewport() {
    let mut engine = board();
    assert!(engine.paste_text("alpha", None));
    let element = &pasted_texts(&engine)[0];
    assert_close(element.x + element.width / 2.0, 0.0);
    assert_close(element.y + element.height / 2.0, 0.0);
}

// ------------------------------------------------- 4. the element that is minted

/// Minted by the engine's own `new_text_element` — a fresh id, a fresh seed, `version` 1,
/// nothing to be a label of. Getting this wrong is what makes every pasted element a
/// spurious autosave patch: an id that was not new is an *edit* of something else.
#[test]
fn a_pasted_text_is_a_new_element_not_an_edit() {
    let mut engine = board();
    assert!(engine.paste_text("alpha", Some((400.0, 300.0))));
    let first = pasted_texts(&engine)[0].id.clone();

    assert!(engine.paste_text("alpha", Some((400.0, 300.0))));
    let second = pasted_texts(&engine)[0].id.clone();
    assert_ne!(first, second, "the same text pasted twice is two elements");

    let element = live(&engine)
        .into_iter()
        .find(|e| e.id == second)
        .expect("the second paste is in the scene");
    assert_eq!(element.version, 1, "a new element, not a bumped one");
    assert!(element.container_id.is_none(), "a free text is in no shape");
    assert!(!element.is_deleted);
    assert_eq!(element.kind, DrawElementType::Text);
}

/// A paste into a frame joins it, as the oracle's `getTopLayerFrameAtSceneCoords` names as
/// its `frameId` (`App.tsx@1118751f:5022-5025`, `:5049`). Ours is judged by the commit,
/// from where the element is (`judge_created_frame_membership`,
/// `engine/pointer_end.rs:238-248`) — the same path a shape drawn into a frame takes, so
/// there is no second way to be in a frame.
#[test]
fn a_pasted_text_joins_the_frame_it_lands_in() {
    let mut frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 400.0,
        },
    );
    frame.name = Some("Frame 1".to_string());
    let mut engine = board();
    engine.set_scene(Scene::new(vec![frame.clone()]));

    assert!(engine.paste_text("alpha", Some((300.0, 300.0))));
    let inside = pasted_texts(&engine)[0].id.clone();
    assert!(
        live(&engine).iter().any(|e| e.id == frame.id),
        "the frame is still there"
    );
    let element = live(&engine)
        .into_iter()
        .find(|e| e.id == inside)
        .expect("the pasted text is in the scene");
    assert_eq!(element.frame_id.as_deref(), Some(frame.id.as_str()));

    // And outside every frame, it is in none.
    let mut engine = board();
    engine.set_scene(Scene::new(vec![frame]));
    assert!(engine.paste_text("alpha", Some((900.0, 300.0))));
    assert_eq!(pasted_texts(&engine)[0].frame_id, None);
}

/// One paste is one step of undo, and the tool ends up back on the selection —
/// `insertNewElements` then the select, then `pasteFromClipboard` restoring the preferred
/// tool (`App.tsx@1118751f:5073-5080`, `:4809-4812`).
#[test]
fn one_paste_is_one_step_of_undo() {
    let mut engine = board();
    assert!(engine.paste_text("alpha\nbeta", Some((400.0, 300.0))));
    assert_eq!(live(&engine).len(), 2);
    engine.undo();
    assert!(
        live(&engine).is_empty(),
        "one undo takes the whole paste, not one of its lines"
    );
    // And redo brings both back, as one step again.
    engine.redo();
    assert_eq!(live(&engine).len(), 2);
}

/// The other branch is untouched: this app's own element JSON still pastes as elements, and
/// does **not** become one text element holding its JSON. `paste_json` is tried first
/// (`keyboardInput.ts`), so a regression here is a paste of the user's own scene arriving as
/// a wall of JSON text.
#[test]
fn our_own_element_json_still_pastes_as_elements() {
    let mut engine = board();
    engine.set_scene(Scene::new(vec![box_at(10.0, 10.0, 20.0, 20.0)]));
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id]);
    let copied = engine.copy_selection();
    engine.clear_selection();

    assert!(engine.paste_json(copied.as_deref(), Some((300.0, 300.0))));
    let scene = engine.get_scene();
    let pasted: Vec<&DrawElement> = scene.iter().filter(|e| e.x != 10.0).collect();
    assert_eq!(pasted.len(), 1);
    assert_eq!(pasted[0].kind, DrawElementType::Rectangle);
    assert!(
        pasted[0].text.is_none(),
        "a rectangle did not become a text holding its JSON"
    );
}

/// And the engine says no to a payload that is neither: `paste_text` refuses text that
/// would make no element, so the caller can fall through to whatever it had.
#[test]
fn paste_text_reports_no_paste_for_text_it_cannot_use() {
    let mut engine = board();
    assert!(!engine.paste_text("", Some((400.0, 300.0))));
    assert!(!engine.paste_text("   \n  \n", Some((400.0, 300.0))));
}
