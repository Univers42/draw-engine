//! Resizing text, and resizing what text lives in: Excalidraw's `resizeSingleTextElement`,
//! the label half of `resizeSingleElement` (`handleBindTextResize`) and the text half of
//! `resizeMultipleElements` (`packages/element/src/resizeElements.ts@1118751f`,
//! `textElement.ts@1118751f:155-247`).
//!
//! - A free text's corners and its top and bottom scale its font, and the box with it.
//!   Only the height the drag asks for counts (`:328-358`), so a sideways pull on a
//!   corner changes nothing.
//! - Its left and right sides fix its width and wrap it there from what was typed
//!   (`:360-408`), and it stops at the width of a space plus the padding; a glyph wider
//!   than that hangs out of the box.
//! - A shape with a label cannot be made smaller than one line of it (`:778-789`), and
//!   the label is wrapped again on every move of the drag, the shape growing back from
//!   the side the drag holds (`handleBindTextResize`).
//! - Several elements with a text among them scale as one, fonts included; a label in
//!   them is wrapped again (`:1370-1377`, `:1491-1514`).
//!
//! Every drag goes in several steps: a resize has to be measured from where it started,
//! and one step cannot tell.
//!
//! Measured with the test hook: every char is `size / 2` wide plus 4 per line, so at 20px
//! one char is 14 and "hello world foo bar" 194; a line is 25 tall.

mod common;
use common::*;
use draw_engine::*;

const WORDS: &str = "hello world foo bar";

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the element is in the scene")
}

/// A free, auto-sizing text at `(x, y)`, 20px, sized as the test hook measures it.
fn free_text(x: f64, y: f64, text: &str) -> DrawElement {
    let (width, height) = measure_text(text, 20.0);
    let mut element = text_at(x, y, width, height);
    element.text = Some(text.to_owned());
    element.original_text = Some(text.to_owned());
    element
}

/// An engine holding `elements`, with `selected` selected.
fn engine_selecting(elements: Vec<DrawElement>, selected: &[&str]) -> DrawEngine {
    let mut engine = engine_with_measure(elements);
    engine.select(selected.iter().map(|id| (*id).to_owned()).collect());
    engine
}

/// A press at `from`, then `steps` moves along the line to `to`, then the release.
fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64), steps: u32, shift: bool) {
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=steps {
        let t = f64::from(step) / f64::from(steps);
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            shift,
            false,
        );
    }
    engine.end_pointer();
}

/// A shape with a label typed into it, the way a person makes one, and the shape
/// selected. Returns the engine, the shape's id and the label's.
fn labelled(shape: DrawElement, text: &str) -> (DrawEngine, String, String) {
    let centre = (shape.x + shape.width / 2.0, shape.y + shape.height / 2.0);
    let shape_id = shape.id.clone();
    let mut engine = engine_with_measure(vec![shape]);
    engine.handle_double_click(centre.0, centre.1);
    let request = engine
        .drain_events()
        .text_edit
        .expect("a double click on a shape opens its label");
    engine.set_element_text(&request.id, text);
    engine.select(vec![shape_id.clone()]);
    (engine, shape_id, request.id)
}

/// The handle ring sits 8px out at 1:1 (4px frame margin + half an 8px handle), and a
/// text's side is taken on the frame line, 4px out.
const HANDLE: f64 = 8.0;
const SIDE: f64 = 4.0;

mod free_text {
    use super::*;

    /// `resizeSingleTextElement` (`:328-358`): the new height over the old scales the font
    /// and the width with it; the top-left stays where a south-east handle leaves it.
    /// The drag also pulls 100 to the right, which a text's corner ignores.
    #[test]
    fn a_corner_scales_the_font_and_the_box_by_the_height_it_is_given() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);

        let grab = (294.0 + HANDLE, 125.0 + HANDLE);
        drag(&mut engine, grab, (grab.0 + 100.0, grab.1 + 25.0), 4, false);

        let after = element(&engine, &id);
        assert_close(after.font_size.unwrap(), 40.0);
        assert_close(after.height, 50.0);
        assert_close(after.width, 388.0);
        assert_close(after.x, 100.0);
        assert_close(after.y, 100.0);
        assert_eq!(
            after.text.as_deref(),
            Some(WORDS),
            "the lines are only scaled"
        );
    }

    /// Alt scales a free text about its own centre too: which corner or side moved is
    /// irrelevant, since `getResizedOrigin`'s "center" case recentres the box however it
    /// is reached (`resizeElements.ts@1118751f:688-691`, taken by `resizeSingleTextElement`
    /// via `shouldResizeFromCenter`, `:334-343`).
    #[test]
    fn alt_resizes_a_free_text_from_its_centre() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);
        let centre = (197.0, 112.5);

        engine.set_alt_held(true);
        let grab = (294.0 + HANDLE, 125.0 + HANDLE);
        drag(&mut engine, grab, (grab.0, 140.0 + HANDLE), 4, false);

        let after = element(&engine, &id);
        assert_close(after.height, 55.0);
        assert_close(after.font_size.unwrap(), 44.0);
        assert_close(after.x + after.width / 2.0, centre.0);
        assert_close(after.y + after.height / 2.0, centre.1);
    }

    /// A north-west corner holds the bottom-right (`getResizeAnchor`, `:611-618`).
    #[test]
    fn a_north_west_corner_holds_the_bottom_right() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);

        let grab = (100.0 - HANDLE, 100.0 - HANDLE);
        drag(&mut engine, grab, (grab.0, grab.1 - 25.0), 4, false);

        let after = element(&engine, &id);
        assert_close(after.font_size.unwrap(), 40.0);
        assert_close(after.x + after.width, 294.0);
        assert_close(after.y + after.height, 125.0);
    }

    /// Past its own anchor the font would go below `MIN_FONT_SIZE`, and the oracle leaves
    /// the text as the last move had it (`measureFontSizeFromWidth`, `:307-310`): a text
    /// never turns inside out, so its glyphs are never mirrored.
    #[test]
    fn a_corner_dragged_through_the_anchor_never_mirrors_the_text() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);

        let grab = (294.0 + HANDLE, 125.0 + HANDLE);
        engine.begin_pointer(grab.0, grab.1, false, false);
        // 2 tall: a font of 1.6, still a font.
        engine.move_pointer(200.0 + HANDLE, 102.0 + HANDLE, false, false);
        let smallest = element(&engine, &id);
        // Well past the anchor: nothing changes.
        engine.move_pointer(0.0, 0.0, false, false);
        engine.end_pointer();

        let after = element(&engine, &id);
        assert_close(smallest.font_size.unwrap(), 1.6);
        assert_eq!(after.font_size, smallest.font_size);
        assert!(after.width > 0.0 && after.height > 0.0, "{after:?}");
        assert_close(after.x, 100.0);
        assert_close(after.y, 100.0);
    }

    /// `resizeSingleTextElement` (`:360-408`): an east side sets the width, wraps what was
    /// typed at it and makes the text fixed-width; the top-left stays.
    #[test]
    fn the_east_side_wraps_the_text_and_fixes_its_width() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);

        let grab = (294.0 + SIDE, 112.5);
        drag(&mut engine, grab, (grab.0 - 94.0, grab.1), 4, false);

        let after = element(&engine, &id);
        assert_eq!(after.auto_resize, Some(false));
        assert_close(after.width, 100.0);
        // "hello world" is 114, "world foo" 94.
        assert_eq!(after.text.as_deref(), Some("hello\nworld foo\nbar"));
        assert_eq!(after.original_text.as_deref(), Some(WORDS));
        assert_close(after.height, 75.0);
        assert_close(after.x, 100.0);
        assert_close(after.y, 100.0);
        assert_close(after.font_size.unwrap(), 20.0);
    }

    /// A west side holds the bottom-right, as its anchor says (`:611-618`): the text grows
    /// upward as it wraps onto more lines.
    #[test]
    fn the_west_side_holds_the_bottom_right() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);

        let grab = (100.0 - SIDE, 112.5);
        drag(&mut engine, grab, (grab.0 + 94.0, grab.1), 4, false);

        let after = element(&engine, &id);
        assert_close(after.width, 100.0);
        assert_close(after.x + after.width, 294.0);
        assert_close(after.y + after.height, 125.0);
        assert_eq!(after.text.as_deref(), Some("hello\nworld foo\nbar"));
    }

    /// Widening a fixed-width text again re-wraps it from what was typed, never from the
    /// lines it was drawn with at the narrow width.
    #[test]
    fn widening_a_wrapped_text_unwraps_it() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);
        let grab = (294.0 + SIDE, 112.5);
        drag(&mut engine, grab, (grab.0 - 94.0, grab.1), 4, false);

        let grab = (200.0 + SIDE, 137.5);
        drag(&mut engine, grab, (grab.0 + 200.0, grab.1), 4, false);

        let after = element(&engine, &id);
        assert_eq!(after.text.as_deref(), Some(WORDS));
        assert_close(after.width, 300.0);
        assert_close(after.height, 25.0);
        assert_eq!(after.auto_resize, Some(false));
    }

    /// A side fixes the width; "Enable text auto-resizing" in the context menu, or Grow in
    /// the panel's Wrap row, gives the text its own width back — both are
    /// `set_text_auto_resize(true)` (`actionTextAutoResize.ts@1118751f`): its typed lines,
    /// measured, the left edge a left-aligned text pins held, as one step of undo.
    #[test]
    fn a_side_resized_text_takes_its_own_width_back() {
        let text = free_text(100.0, 100.0, WORDS);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);
        let grab = (294.0 + SIDE, 112.5);
        drag(&mut engine, grab, (grab.0 - 94.0, grab.1), 4, false);
        let fixed = element(&engine, &id);
        assert_eq!(fixed.auto_resize, Some(false));

        engine.set_text_auto_resize(true);
        let auto = element(&engine, &id);
        assert_eq!(auto.auto_resize, Some(true));
        assert_eq!(auto.text.as_deref(), Some(WORDS));
        assert_close(auto.width, measure_text(WORDS, 20.0).0);
        assert_close(auto.height, 25.0);
        assert_close(auto.x, 100.0);
        assert_close(auto.y, 100.0);

        engine.undo();
        assert_eq!(element(&engine, &id).auto_resize, Some(false), "one step");
    }

    /// A side dragged through the other one stops at the oracle's minimum, a space and the
    /// padding (`getMinTextElementWidth`: 14 + 10) — the width is never negative.
    #[test]
    fn a_side_stops_at_the_minimum_width() {
        let text = free_text(100.0, 100.0, "hi");
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);

        let grab = (124.0 + SIDE, 112.5);
        drag(&mut engine, grab, (0.0, grab.1), 4, false);

        let after = element(&engine, &id);
        assert_close(after.width, 24.0);
        assert_close(after.x, 100.0);
        assert!(after.height > 0.0);
    }

    /// Where a glyph is wider than that minimum it hangs out of its box, as the oracle's
    /// does: the width is the one the lines were wrapped at (`:398-405`), so laying them
    /// out again — a font arriving, an edit — gives the same lines.
    #[test]
    fn a_glyph_wider_than_the_box_hangs_out_of_it() {
        let mut text = free_text(100.0, 100.0, "Wiiiii");
        text.font_family = Some(5);
        let id = text.id.clone();
        let mut engine = engine_selecting(vec![text], &[&id]);
        // A W three font sizes wide, everything else half one.
        engine.set_measure_line(|line, font| {
            line.chars()
                .map(|c| if c == 'W' { 3.0 } else { 0.5 } * font.size())
                .sum()
        });
        let before = element(&engine, &id);
        let right = before.x + before.width;

        drag(&mut engine, (right + SIDE, 112.5), (0.0, 112.5), 4, false);

        let after = element(&engine, &id);
        // A space and the padding: 10 + 10.
        assert_close(after.width, 20.0);
        assert_eq!(after.text.as_deref(), Some("W\nii\nii\ni"));
        assert_close(after.height, 100.0);

        engine.fonts_loaded();
        let again = element(&engine, &id);
        assert_eq!(again.text, after.text, "the same lines, laid out again");
        assert_close(again.width, 20.0);
        assert_close(again.height, 100.0);
    }
}

mod labels {
    use super::*;

    /// A peer taking the shape mid-drag abandons the resize, as it abandons a move
    /// (`ci_peers.rs`): the shape and the label laid out on the way both go back as they
    /// were, and nothing is committed over them.
    #[test]
    fn a_resize_a_peer_takes_the_shape_from_puts_the_label_back() {
        let (mut engine, shape_id, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);
        let before = (element(&engine, &shape_id), element(&engine, &label_id));
        let grab = (400.0 + HANDLE, 125.0);
        engine.begin_pointer(grab.0, grab.1, false, false);
        engine.move_pointer(grab.0 - 200.0, grab.1, false, false);
        assert_ne!(element(&engine, &label_id).text, before.1.text, "laid out");
        engine.set_peers(vec![Peer {
            id: "ana".into(),
            name: "Ana".into(),
            color: "#e03131".into(),
            holds: vec![shape_id.clone()],
            preview: Vec::new(),
        }]);
        engine.move_pointer(grab.0 - 250.0, grab.1, false, false);
        engine.end_pointer();
        assert_eq!(
            (element(&engine, &shape_id), element(&engine, &label_id)),
            before
        );
    }

    /// `handleBindTextResize` on every move: narrowed by its east handle, the shape's
    /// label is wrapped at its new room while the pointer is still down, and the shape
    /// grows down to hold it — from its top, which the drag holds.
    #[test]
    fn narrowing_a_labelled_shape_wraps_its_label_as_the_drag_goes() {
        let (mut engine, shape_id, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);
        assert_eq!(element(&engine, &label_id).text.as_deref(), Some(WORDS));

        let grab = (400.0 + HANDLE, 125.0);
        engine.begin_pointer(grab.0, grab.1, false, false);
        for step in 1..=4 {
            engine.move_pointer(grab.0 - 50.0 * f64::from(step), grab.1, false, false);
        }
        // Mid-drag: 100 wide, room for 90. "world foo" is 94.
        let label = element(&engine, &label_id);
        let shape = element(&engine, &shape_id);
        assert_eq!(label.text.as_deref(), Some("hello\nworld\nfoo bar"));
        assert_close(shape.width, 100.0);
        assert_close(shape.height, 75.0 + 10.0);
        assert_close(shape.y, 100.0);
        engine.end_pointer();

        let label = element(&engine, &label_id);
        assert_eq!(label.text.as_deref(), Some("hello\nworld\nfoo bar"));
        assert_eq!(label.original_text.as_deref(), Some(WORDS));
        assert!(label.x >= shape.x && label.x + label.width <= shape.x + shape.width);
        assert!(label.y >= shape.y && label.y + label.height <= shape.y + shape.height);
    }

    /// Widened again, the label goes back onto one line: it is wrapped from what was
    /// typed.
    #[test]
    fn widening_it_again_unwraps_the_label() {
        let (mut engine, shape_id, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);
        let grab = (400.0 + HANDLE, 125.0);
        drag(&mut engine, grab, (grab.0 - 200.0, grab.1), 4, false);
        let narrow = element(&engine, &shape_id);
        assert_eq!(
            element(&engine, &label_id).text.as_deref(),
            Some("hello\nworld\nfoo bar"),
            "narrowed, it wrapped"
        );

        let grab = (200.0 + HANDLE, narrow.y + narrow.height / 2.0);
        drag(&mut engine, grab, (grab.0 + 300.0, grab.1), 4, false);

        assert_eq!(element(&engine, &label_id).text.as_deref(), Some(WORDS));
    }

    /// `getApproxMinLineWidth` (`textMeasurements.ts@1118751f:32-44`): the widest char
    /// and the padding either side — 14 + 10 — however far the side is pulled.
    #[test]
    fn a_labelled_shape_stops_at_one_char_of_its_label() {
        let (mut engine, shape_id, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);

        let grab = (400.0 + HANDLE, 125.0);
        drag(&mut engine, grab, (110.0 + HANDLE, grab.1), 6, false);

        let shape = element(&engine, &shape_id);
        let label = element(&engine, &label_id);
        assert_close(shape.width, 24.0);
        for line in label.text.as_deref().unwrap().split('\n') {
            assert!(measure_text(line, 20.0).0 <= 14.0, "{line:?} overflows");
        }
        assert!(label.y + label.height <= shape.y + shape.height);
    }

    /// `getApproxMinLineHeight` (`:99-104`): a line of the label and the padding — 25 + 10.
    #[test]
    fn a_labelled_shape_stops_at_one_line_of_its_label() {
        let (mut engine, shape_id, _) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);

        let grab = (250.0, 150.0 + HANDLE);
        drag(&mut engine, grab, (grab.0, 110.0 + HANDLE), 4, false);

        let shape = element(&engine, &shape_id);
        assert_close(shape.height, 35.0);
        assert_close(shape.y, 100.0);
    }

    /// A north-west handle holds the bottom-right, so a label that needs more lines
    /// grows the shape upward (`handleBindTextResize`, `:209-229`).
    #[test]
    fn a_north_handle_grows_the_shape_from_its_bottom() {
        let (mut engine, shape_id, _) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);

        let grab = (100.0 - HANDLE, 100.0 - HANDLE);
        drag(&mut engine, grab, (grab.0 + 200.0, grab.1), 4, false);

        let shape = element(&engine, &shape_id);
        assert_close(shape.width, 100.0);
        assert_close(shape.x, 300.0);
        assert_close(shape.height, 85.0);
        assert_close(shape.y + shape.height, 150.0);
    }

    /// A label told not to wrap keeps its lines, and its shape cannot be made narrower
    /// than they are (DECISIONS: `wrap == false`, the shape grows in width).
    #[test]
    fn an_unwrapped_label_keeps_its_lines() {
        let (mut engine, shape_id, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);
        engine.set_label_wrap(false);
        assert_eq!(element(&engine, &label_id).wrap, Some(false));

        let grab = (400.0 + HANDLE, 125.0);
        drag(&mut engine, grab, (grab.0 - 200.0, grab.1), 4, false);

        let label = element(&engine, &label_id);
        let shape = element(&engine, &shape_id);
        assert_eq!(label.text.as_deref(), Some(WORDS));
        assert_close(shape.width, 194.0 + 10.0);
        assert_close(shape.x, 100.0);
    }

    /// With Shift the shape keeps its proportions, and its label's font scales with the
    /// room it has (`resizeSingleElement`, `:815-833`): 20 × 590 / 290.
    #[test]
    fn shift_scales_the_labels_font_with_the_shape() {
        let (mut engine, shape_id, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);

        let grab = (400.0 + HANDLE, 150.0 + HANDLE);
        drag(&mut engine, grab, (grab.0 + 300.0, grab.1 + 50.0), 4, true);

        let shape = element(&engine, &shape_id);
        let label = element(&engine, &label_id);
        assert_close(shape.width, 600.0);
        assert_close(shape.height, 100.0);
        assert_close(label.font_size.unwrap(), 20.0 * 590.0 / 290.0);
    }

    /// Shift's font lasts only as long as Shift: a move without it lays the label out at
    /// the font it had when the drag began (`resizeSingleElement`, `:805-814`, reads the
    /// label as it was at pointer-down; only a move with the aspect kept overrides it).
    #[test]
    fn letting_go_of_shift_gives_the_label_its_font_back() {
        let (mut engine, _, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);
        let font = |engine: &DrawEngine| element(engine, &label_id).font_size.unwrap();

        let grab = (400.0 + HANDLE, 150.0 + HANDLE);
        engine.begin_pointer(grab.0, grab.1, false, false);
        engine.move_pointer(grab.0 + 150.0, grab.1 + 25.0, true, false);
        engine.move_pointer(grab.0 + 300.0, grab.1 + 50.0, true, false);
        assert!(font(&engine) > 40.0, "Shift scaled it: {}", font(&engine));
        engine.move_pointer(grab.0 + 250.0, grab.1 + 50.0, false, false);
        assert_close(font(&engine), 20.0);
        engine.move_pointer(grab.0 + 200.0, grab.1 + 50.0, false, false);
        engine.end_pointer();

        assert_close(font(&engine), 20.0);
    }

    thread_local! {
        static MEASURED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// The test hook's widths, counted.
    fn counted(line: &str, font: draw_engine::text::FontKey) -> f64 {
        MEASURED.with(|calls| calls.set(calls.get() + 1));
        measure_text(line, font.size()).0
    }

    /// Once the caches are warm a resize move asks the host to measure nothing: the
    /// label's smallest room reads its chars through the same cache the wrap fills
    /// (`getApproxMinLineWidth` reads the oracle's `charWidth` cache,
    /// `textMeasurements.ts@1118751f:32-44`). In a browser every measure crosses to JS.
    #[test]
    fn a_warm_resize_measures_nothing() {
        let (mut engine, _, _) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);
        engine.set_measure_line(counted);

        let grab = (250.0, 150.0 + HANDLE);
        engine.begin_pointer(grab.0, grab.1, false, false);
        engine.move_pointer(grab.0, grab.1 + 10.0, false, false);
        let mut per_move = Vec::new();
        for step in 2..=5 {
            MEASURED.with(|calls| calls.set(0));
            engine.move_pointer(grab.0, grab.1 + 10.0 * f64::from(step), false, false);
            per_move.push(MEASURED.with(std::cell::Cell::get));
        }
        engine.end_pointer();

        assert_eq!(per_move, [0, 0, 0, 0]);
    }

    /// A drag is one edit: nothing is stamped while it goes, and the shape and the label
    /// it re-wrapped are stamped once, on release.
    #[test]
    fn a_drag_stamps_the_shape_and_its_label_once() {
        let (mut engine, shape_id, label_id) = labelled(box_at(100.0, 100.0, 300.0, 50.0), WORDS);
        let version = |engine: &DrawEngine, id: &str| element(engine, id).version;
        let (shape_before, label_before) =
            (version(&engine, &shape_id), version(&engine, &label_id));

        let grab = (400.0 + HANDLE, 125.0);
        engine.begin_pointer(grab.0, grab.1, false, false);
        for step in 1..=5 {
            engine.move_pointer(grab.0 - 40.0 * f64::from(step), grab.1, false, false);
            assert_eq!(version(&engine, &shape_id), shape_before);
            assert_eq!(version(&engine, &label_id), label_before);
        }
        engine.end_pointer();

        assert_eq!(version(&engine, &shape_id), shape_before + 1);
        assert_eq!(version(&engine, &label_id), label_before + 1);
    }
}

mod several {
    use super::*;

    /// Two 300 × 50 shapes side by side at (0, 0) and (400, 0), each with a label typed
    /// into it. Returns each shape's id with its label's.
    fn labelled_pair() -> (DrawEngine, [(String, String); 2]) {
        let shapes = [
            box_at(0.0, 0.0, 300.0, 50.0),
            box_at(400.0, 0.0, 300.0, 50.0),
        ];
        let ids = shapes.clone().map(|shape| shape.id);
        let mut engine = engine_with_measure(shapes.to_vec());
        let labels = [150.0, 550.0].map(|x| {
            engine.handle_double_click(x, 25.0);
            let id = engine
                .drain_events()
                .text_edit
                .expect("a double click on a shape opens its label")
                .id;
            engine.set_element_text(&id, WORDS);
            id
        });
        let [left, right] = ids;
        let [left_label, right_label] = labels;
        (engine, [(left, left_label), (right, right_label)])
    }

    /// With a text among them, elements scale as one (`keepAspectRatio`, `:1370-1377`):
    /// twice as wide and no taller is twice as big, fonts included (`:1491-1497`).
    #[test]
    fn a_text_makes_the_selection_scale_as_one() {
        let shape = box_at(0.0, 0.0, 100.0, 100.0);
        let text = free_text(200.0, 0.0, WORDS);
        let (shape_id, text_id) = (shape.id.clone(), text.id.clone());
        let mut engine = engine_selecting(vec![shape, text], &[&shape_id, &text_id]);

        // The frame is (0, 0)-(394, 100).
        let grab = (394.0 + HANDLE, 100.0 + HANDLE);
        drag(
            &mut engine,
            grab,
            (788.0 + HANDLE, 100.0 + HANDLE),
            4,
            false,
        );

        let shape = element(&engine, &shape_id);
        let text = element(&engine, &text_id);
        assert_close(shape.width, 200.0);
        assert_close(shape.height, 200.0);
        assert_close(text.font_size.unwrap(), 40.0);
        assert_close(text.width, 388.0);
        assert_close(text.height, 50.0);
        assert_close(text.x, 400.0);
    }

    /// Without one, the shapes stretch and their labels keep their size and wrap again at
    /// the room they are left, the shapes growing to hold them (`:1512`, `:1581-1588`).
    #[test]
    fn labels_wrap_again_when_their_shapes_are_stretched_together() {
        let (mut engine, pair) = labelled_pair();
        engine.select(pair.iter().map(|(shape, _)| shape.clone()).collect());

        // The frame is (0, 0)-(700, 50): half as wide, as tall.
        let grab = (700.0 + HANDLE, 50.0 + HANDLE);
        drag(&mut engine, grab, (350.0 + HANDLE, 50.0 + HANDLE), 4, false);

        for (shape_id, label_id) in &pair {
            let shape = element(&engine, shape_id);
            let label = element(&engine, label_id);
            assert_close(shape.width, 150.0);
            assert_close(label.font_size.unwrap(), 20.0);
            // Room for 140: "hello world foo" is 154.
            assert_eq!(label.text.as_deref(), Some("hello world\nfoo bar"));
            assert_close(shape.height, 60.0);
            assert_close(shape.y, 0.0);
        }
        assert_close(element(&engine, &pair[1].0).x, 200.0);
    }

    /// A shape can still name a label someone else deleted: each element's newest copy
    /// wins on its own, so one person's deletion of the label and another's edit of the
    /// shape both stand. Resized with another shape, it is resized as a shape with no label
    /// — the tombstone is not laid out, not stamped, and grows nothing.
    #[test]
    fn a_deleted_label_is_left_out_of_a_resize_of_several() {
        let mut shape = box_at(0.0, 0.0, 300.0, 50.0);
        let other = box_at(400.0, 0.0, 300.0, 50.0);
        let mut gone = free_text(100.0, 12.5, WORDS);
        gone.container_id = Some(shape.id.clone());
        gone.is_deleted = true;
        shape.bound_text_id = Some(gone.id.clone());
        let (shape_id, other_id, gone_id) = (shape.id.clone(), other.id.clone(), gone.id.clone());
        let before = gone.clone();
        let mut engine = engine_selecting(vec![shape, other, gone], &[&shape_id, &other_id]);

        let grab = (700.0 + HANDLE, 50.0 + HANDLE);
        drag(&mut engine, grab, (350.0 + HANDLE, 50.0 + HANDLE), 4, false);

        let shape = element(&engine, &shape_id);
        assert_close(shape.width, 150.0);
        assert_close(shape.height, 50.0);
        let gone = element(&engine, &gone_id);
        assert_eq!(gone, before, "the tombstone is untouched");
    }

    /// A group's labels are in its frame — the one drawn and the one scaled, the same box
    /// (`getNextMultipleWidthAndHeightFromPointer` counts an arrow's label,
    /// `resizeElements.ts@1118751f:1101-1131`) — so the corner opposite the handle holds
    /// when an arrow's label hangs past the shapes.
    #[test]
    fn a_groups_drawn_corner_holds_with_an_arrows_label_past_its_shapes() {
        let rect = box_at(0.0, 0.0, 100.0, 100.0);
        let arrow = connector(0.0, 150.0, 60.0, 150.0, DrawElementType::Arrow);
        let (rect_id, arrow_id) = (rect.id.clone(), arrow.id.clone());
        let mut engine = engine_with_measure(vec![rect, arrow]);
        engine.handle_double_click(30.0, 150.0);
        let label_id = engine
            .drain_events()
            .text_edit
            .expect("a double click on an arrow's middle opens its label")
            .id;
        engine.set_element_text(&label_id, WORDS);
        engine.select(vec![rect_id.clone(), arrow_id]);
        engine.group_selection();
        engine.select(Vec::new());
        // A click on the rectangle's outline takes the whole group, its label with it.
        engine.begin_pointer(100.0, 50.0, false, false);
        engine.end_pointer();
        let drawn = |engine: &DrawEngine| engine.paint_view().group_box.expect("a frame");
        let before = drawn(&engine);
        // The label, 194 wide on the arrow's middle, hangs 67 left of the rectangle.
        assert_close(before.min_x, -67.0);
        assert_close(before.min_y, 0.0);

        let grab = (before.max_x + HANDLE, before.max_y + HANDLE);
        drag(
            &mut engine,
            grab,
            (grab.0 + 100.0, grab.1 + 100.0),
            4,
            false,
        );

        let after = drawn(&engine);
        // The label is laid out again at its scaled font, not scaled: the test hook's 4 per
        // line does not scale, and moves its edge by 2 × (scale − 1) at most.
        assert!((after.min_x - before.min_x).abs() < 2.0, "{after:?}");
        assert_close(after.min_y, 0.0);
        assert!(after.max_x > before.max_x + 90.0, "{after:?}");
    }

    /// Grouped, they scale as one (`isInGroup`, `:1376`), and so do their labels' fonts
    /// (`:1505-1510`).
    #[test]
    fn a_group_scales_as_one_and_its_labels_fonts_with_it() {
        let (mut engine, pair) = labelled_pair();
        let shapes: Vec<String> = pair.iter().map(|(shape, _)| shape.clone()).collect();
        engine.select(shapes.clone());
        engine.group_selection();
        engine.select(shapes);

        // Half as wide and half as tall.
        let grab = (700.0 + HANDLE, 50.0 + HANDLE);
        drag(&mut engine, grab, (350.0 + HANDLE, 25.0 + HANDLE), 4, false);

        for (shape_id, label_id) in &pair {
            let shape = element(&engine, shape_id);
            let label = element(&engine, label_id);
            assert_close(shape.width, 150.0);
            assert_close(shape.height, 25.0);
            assert_close(label.font_size.unwrap(), 10.0);
            assert_eq!(label.text.as_deref(), Some(WORDS));
        }
    }
}
