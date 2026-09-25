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
