//! Resizing text, and resizing what text lives in: Excalidraw's `resizeSingleTextElement`,
//! the label half of `resizeSingleElement` (`handleBindTextResize`) and the text half of
//! `resizeMultipleElements` (`packages/element/src/resizeElements.ts@1118751f`,
//! `textElement.ts@1118751f:155-247`).
//!
//! - A free text's corners and its top and bottom scale its font, and the box with it.
//!   Only the height the drag asks for counts (`:328-358`), so a sideways pull on a
//!   corner changes nothing.
//! - Its left and right sides fix its width and wrap it there from what was typed
//!   (`:360-408`), and it stops at the width of a space plus the padding — here also at
//!   its widest glyph.
//! - A shape with a label cannot be made smaller than one line of it (`:778-789`), and
//!   the label is wrapped again on every move of the drag, the shape growing back from
//!   the side the drag holds (`handleBindTextResize`).
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

    /// Where a glyph is wider than that minimum the oracle lets it hang out of its box;
    /// here the box stops at the glyph (divergence, `docs/reference/resize.md`).
    #[test]
    fn a_side_stops_at_the_widest_glyph() {
        let text = free_text(100.0, 100.0, "WWW");
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
        assert_eq!(after.text.as_deref(), Some("W\nW\nW"));
        assert_close(after.width, 60.0);
        assert_close(after.height, 75.0);
    }
}

mod labels {
    use super::*;

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
