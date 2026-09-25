//! The text layout model: where a text's lines break, how big its box is, where a label
//! sits in its shape and how far the shape grows to hold it — Excalidraw's
//! `redrawTextBoundingBox` and the geometry under it (`packages/element/src/
//! textElement.ts@1118751f`), ported as `text::layout`.
//!
//! The numbers asserted against the oracle's formulae are worked out in the comments
//! beside them from the lines cited; the engine-level tests drive the writers a person
//! reaches (typing, the font size and family, alignment, the style panel, a font
//! arriving) and check that each goes through the one layout.

mod common;
use common::*;
use draw_engine::text::layout::{
    bound_text_max_height, bound_text_max_width, bound_text_position, container_coords,
    container_dimension_for_bound_text, BOUND_TEXT_PADDING,
};
use draw_engine::text::FontKey;
use draw_engine::*;

const SQRT_2: f64 = std::f64::consts::SQRT_2;

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the element is in the scene")
}

/// A shape with a label typed into it, the way a person makes one: a double click on
/// the shape, then the text committed. Returns the engine and the label's id.
fn labelled(shape: DrawElement, text: &str) -> (DrawEngine, String) {
    let centre = (shape.x + shape.width / 2.0, shape.y + shape.height / 2.0);
    let mut engine = engine_with_measure(vec![shape]);
    engine.handle_double_click(centre.0, centre.1);
    let request = engine
        .drain_events()
        .text_edit
        .expect("a double click on a shape opens its label");
    engine.set_element_text(&request.id, text);
    (engine, request.id)
}

/// Every drawn line of `label` measures no wider than `max_width` with the test hook.
fn assert_lines_fit(label: &DrawElement, max_width: f64) {
    let size = label.font_size.unwrap();
    for line in label.text.as_deref().unwrap().split('\n') {
        let width = measure_text(line, size).0;
        assert!(
            width <= max_width + EPS,
            "{line:?} is {width} wide in {max_width}: {:?}",
            label.text
        );
    }
}

/// The oracle's container geometry, for a 200 × 100 shape at (10, 20).
mod geometry {
    use super::*;

    fn shape(kind: DrawElementType) -> DrawElement {
        let mut shape = box_at(10.0, 20.0, 200.0, 100.0);
        shape.kind = kind;
        shape
    }

    /// `getContainerCoords` (`textElement.ts@1118751f:396-417`): the padding, plus for an
    /// ellipse `w/2 · (1 − √2/2)` and for a diamond `w/4` (and the same in height).
    #[test]
    fn the_label_box_starts_where_the_oracle_insets_it() {
        let rect = container_coords(&shape(DrawElementType::Rectangle));
        assert_close(rect.x, 15.0);
        assert_close(rect.y, 25.0);
        let ellipse = container_coords(&shape(DrawElementType::Ellipse));
        assert_close(ellipse.x, 10.0 + 5.0 + 100.0 * (1.0 - SQRT_2 / 2.0));
        assert_close(ellipse.y, 20.0 + 5.0 + 50.0 * (1.0 - SQRT_2 / 2.0));
        assert!((ellipse.x - 44.2893).abs() < 1e-4 && (ellipse.y - 39.6447).abs() < 1e-4);
        let diamond = container_coords(&shape(DrawElementType::Diamond));
        assert_close(diamond.x, 65.0);
        assert_close(diamond.y, 50.0);
    }

    /// `getBoundTextMaxWidth` / `getBoundTextMaxHeight` (`:511-570`) for 178 × 194:
    /// a rectangle less the padding twice; the largest box inscribed in an ellipse,
    /// `round(w/2 · √2) − 10` (round(125.87) = 126 → 116; round(137.18) = 137 → 127); in a
    /// diamond `round(w/2) − 10` (79, 87).
    #[test]
    fn the_room_a_label_has_is_the_oracles() {
        let mut shape = box_at(0.0, 0.0, 178.0, 194.0);
        assert_close(bound_text_max_width(&shape, 20.0), 168.0);
        assert_close(bound_text_max_height(&shape, 25.0), 184.0);
        shape.kind = DrawElementType::Ellipse;
        assert_close(bound_text_max_width(&shape, 20.0), 116.0);
        assert_close(bound_text_max_height(&shape, 25.0), 127.0);
        shape.kind = DrawElementType::Diamond;
        assert_close(bound_text_max_width(&shape, 20.0), 79.0);
        assert_close(bound_text_max_height(&shape, 25.0), 87.0);
    }

    /// An arrow's label may take `max(0.7 × width, 11 × fontSize)` (`:515-520`, constants
    /// `constants.ts@1118751f:418-419`), and its height is the arrow's unless the arrow is
    /// under 80 tall, when it is the label's own (`:554-560`).
    #[test]
    fn an_arrows_label_room_is_the_oracles() {
        let long = connector(0.0, 0.0, 400.0, 194.0, DrawElementType::Arrow);
        assert_close(bound_text_max_width(&long, 20.0), 280.0);
        assert_close(bound_text_max_height(&long, 25.0), 194.0);
        let short = connector(0.0, 0.0, 100.0, 70.0, DrawElementType::Arrow);
        assert_close(bound_text_max_width(&short, 20.0), 220.0);
        assert_close(bound_text_max_height(&short, 25.0), 25.0);
        // Drawn right to left, it is as wide: the room is its points' extent.
        let leftward = connector(400.0, 0.0, 0.0, 0.0, DrawElementType::Arrow);
        assert_close(bound_text_max_width(&leftward, 20.0), 280.0);
    }

    /// `computeContainerDimensionForBoundText` (`:492-509`) for a label 150 across:
    /// rectangle 150 + 10, ellipse round(160 / √2 · 2) = 226, diamond 2 · 160, arrow + 80.
    #[test]
    fn a_shape_grows_to_the_oracles_size() {
        assert_close(
            container_dimension_for_bound_text(150.0, DrawElementType::Rectangle),
            160.0,
        );
        assert_close(
            container_dimension_for_bound_text(150.0, DrawElementType::Ellipse),
            226.0,
        );
        assert_close(
            container_dimension_for_bound_text(150.0, DrawElementType::Diamond),
            320.0,
        );
        assert_close(
            container_dimension_for_bound_text(150.0, DrawElementType::Arrow),
            230.0,
        );
        // The dimension is rounded up first.
        assert_close(
            container_dimension_for_bound_text(149.2, DrawElementType::Rectangle),
            160.0,
        );
    }

    /// `computeBoundTextPosition` (`:249-324`) in a 200 × 100 rectangle at (100, 100)
    /// turned 90°, an 80 × 40 label: laid out in the box from (105, 105), 190 × 90, then
    /// turned about the box's middle (200, 150). Worked: the label's centre relative to
    /// the middle is (dx, dy) = (x + 40 − 200, y + 20 − 150), turned to (−dy, dx).
    #[test]
    fn a_turned_shape_turns_its_label_about_the_middle_of_its_room() {
        let mut shape = box_at(100.0, 100.0, 200.0, 100.0);
        shape.angle = std::f64::consts::FRAC_PI_2;
        let mut label = text_at(0.0, 0.0, 80.0, 40.0);
        label.container_id = Some(shape.id.clone());
        let cases = [
            (TextAlign::Left, VerticalAlign::Top, 185.0, 75.0),
            (TextAlign::Left, VerticalAlign::Middle, 160.0, 75.0),
            (TextAlign::Left, VerticalAlign::Bottom, 135.0, 75.0),
            (TextAlign::Center, VerticalAlign::Top, 185.0, 130.0),
            (TextAlign::Center, VerticalAlign::Middle, 160.0, 130.0),
            (TextAlign::Center, VerticalAlign::Bottom, 135.0, 130.0),
            (TextAlign::Right, VerticalAlign::Top, 185.0, 185.0),
            (TextAlign::Right, VerticalAlign::Middle, 160.0, 185.0),
            (TextAlign::Right, VerticalAlign::Bottom, 135.0, 185.0),
        ];
        for (align, valign, x, y) in cases {
            label.text_align = Some(align);
            label.vertical_align = Some(valign);
            let at = bound_text_position(&shape, &label);
            assert!(
                (at.x - x).abs() < 1e-9 && (at.y - y).abs() < 1e-9,
                "{align:?} {valign:?}: ({}, {}) not ({x}, {y})",
                at.x,
                at.y
            );
        }
    }

    /// An arrow's label is centred on its middle point when it has an odd number, and
    /// half way along its middle segment when even (`getBoundTextElementCenter`,
    /// `linearElementEditor.ts@1118751f:1942-1960`).
    #[test]
    fn an_arrows_label_sits_on_its_middle() {
        let mut label = text_at(0.0, 0.0, 60.0, 20.0);
        let mut arrow = connector(0.0, 0.0, 400.0, 0.0, DrawElementType::Arrow);
        arrow.roundness = None;
        label.container_id = Some(arrow.id.clone());
        let at = bound_text_position(&arrow, &label);
        assert_close(at.x, 200.0 - 30.0);
        assert_close(at.y, -10.0);
        // Three points: the middle one, wherever the chord between the ends passes.
        arrow.points = Some(vec![[0.0, 0.0], [100.0, 300.0], [400.0, 0.0]]);
        let at = bound_text_position(&arrow, &label);
        assert_close(at.x, 100.0 - 30.0);
        assert_close(at.y, 300.0 - 10.0);
    }
}

/// A label typed into a shape wraps inside the room the oracle gives it, and the shape
/// grows in height to hold what does not fit.
mod labels {
    use super::*;

    /// 100 × 60: room 90 × 50. Four lines of 25 need 100, so the rectangle grows to
    /// 100 + 10 (`textElement.ts@1118751f:115-124`), and the label is centred in it.
    #[test]
    fn a_rectangles_label_wraps_inside_and_the_rectangle_grows() {
        let rect = box_at(0.0, 0.0, 100.0, 60.0);
        let rect_id = rect.id.clone();
        let (engine, label_id) = labelled(rect, "the quick brown fox");
        let label = element(&engine, &label_id);
        let rect = element(&engine, &rect_id);
        assert_lines_fit(&label, 90.0);
        assert!(label.text.as_deref().unwrap().contains('\n'), "it wrapped");
        let lines = label.text.as_deref().unwrap().split('\n').count() as f64;
        assert_close(label.height, lines * 20.0 * 1.25);
        assert_close(rect.width, 100.0);
        assert_close(rect.height, label.height.ceil() + 2.0 * BOUND_TEXT_PADDING);
        // Inside, horizontally and vertically.
        assert!(label.x >= BOUND_TEXT_PADDING - EPS);
        assert!(label.x + label.width <= 100.0 - BOUND_TEXT_PADDING + EPS);
        assert_close(label.y, BOUND_TEXT_PADDING);
        assert_close(label.x, BOUND_TEXT_PADDING + (90.0 - label.width) / 2.0);
        assert_eq!(label.original_text.as_deref(), Some("the quick brown fox"));
    }

    /// A short label in a roomy shape leaves the shape as it was.
    #[test]
    fn a_label_that_fits_grows_nothing() {
        let rect = box_at(0.0, 0.0, 300.0, 100.0);
        let rect_id = rect.id.clone();
        let (engine, label_id) = labelled(rect, "hi");
        let rect = element(&engine, &rect_id);
        let label = element(&engine, &label_id);
        assert_close(rect.width, 300.0);
        assert_close(rect.height, 100.0);
        assert_eq!(label.text.as_deref(), Some("hi"));
        assert_close(label.width, measure_text("hi", 20.0).0);
        assert_close(label.x, 5.0 + (290.0 - label.width) / 2.0);
        assert_close(label.y, 5.0 + (90.0 - 25.0) / 2.0);
    }

    /// 200 × 100 ellipse: its lines wrap at round(100 · √2) − 10 = 131, inside the
    /// inscribed box from x = 5 + 100 · (1 − √2/2); it grows to round((h + 10) / √2 · 2).
    #[test]
    fn an_ellipses_label_wraps_in_the_inscribed_box() {
        let ellipse = ellipse_at(0.0, 0.0, 200.0, 100.0);
        let ellipse_id = ellipse.id.clone();
        let (engine, label_id) = labelled(ellipse, "the quick brown fox jumps over the lazy dog");
        let label = element(&engine, &label_id);
        let ellipse = element(&engine, &ellipse_id);
        assert_lines_fit(&label, 131.0);
        let inset = 5.0 + 100.0 * (1.0 - SQRT_2 / 2.0);
        assert!(label.x >= inset - EPS && label.x + label.width <= inset + 131.0 + EPS);
        assert_close(ellipse.width, 200.0);
        assert_close(
            ellipse.height,
            ((label.height.ceil() + 10.0) / SQRT_2 * 2.0 + 0.5).floor(),
        );
    }

    /// 200 × 100 diamond: lines wrap at round(200 / 2) − 10 = 90, from x = 5 + 50; it
    /// grows to 2 · (h + 10).
    #[test]
    fn a_diamonds_label_wraps_in_the_inscribed_box() {
        let diamond = diamond_at(0.0, 0.0, 200.0, 100.0);
        let diamond_id = diamond.id.clone();
        let (engine, label_id) = labelled(diamond, "the quick brown fox");
        let label = element(&engine, &label_id);
        let diamond = element(&engine, &diamond_id);
        assert_lines_fit(&label, 90.0);
        assert!(label.x >= 55.0 - EPS && label.x + label.width <= 145.0 + EPS);
        assert_close(diamond.height, 2.0 * (label.height.ceil() + 10.0));
    }

    /// An arrow's label wraps at the oracle's width and never turns; the arrow does not
    /// grow (its extent is its points').
    #[test]
    fn an_arrows_label_wraps_at_the_oracles_width_and_centres_on_it() {
        let mut arrow = connector(0.0, 0.0, 400.0, 0.0, DrawElementType::Arrow);
        arrow.roundness = None;
        let arrow_id = arrow.id.clone();
        let mut engine = engine_with_measure(vec![arrow]);
        engine.select(vec![arrow_id.clone()]);
        assert!(engine.edit_selected_text());
        let id = engine.drain_events().text_edit.unwrap().id;
        // 30 chars measure 304 with the hook: wider than 280, so it wraps.
        engine.set_element_text(&id, "the quick brown fox jumps over");
        let label = element(&engine, &id);
        assert_lines_fit(&label, 280.0);
        assert_eq!(label.text.as_deref().unwrap().split('\n').count(), 2);
        assert_close(label.x + label.width / 2.0, 200.0);
        assert_close(label.y + label.height / 2.0, 0.0);
        assert_close(label.angle, 0.0);
        let arrow = element(&engine, &arrow_id);
        assert_eq!(arrow.points, Some(vec![[0.0, 0.0], [400.0, 0.0]]));
    }

    /// A label told not to wrap keeps its typed lines, and its shape grows in width to
    /// hold the longest (`textElement.ts@1118751f:126-133`).
    #[test]
    fn an_unwrapped_label_widens_its_shape() {
        let rect = box_at(0.0, 0.0, 100.0, 60.0);
        let rect_id = rect.id.clone();
        let (mut engine, label_id) = labelled(rect, "the quick brown fox");
        engine.select(vec![rect_id.clone()]);
        engine.set_label_wrap(false);
        let label = element(&engine, &label_id);
        let rect = element(&engine, &rect_id);
        assert_eq!(label.wrap, Some(false));
        assert_eq!(label.text.as_deref(), Some("the quick brown fox"));
        assert_close(label.width, measure_text("the quick brown fox", 20.0).0);
        assert_close(rect.width, label.width.ceil() + 10.0);
        // And back: it wraps inside the width it grew to.
        engine.set_label_wrap(true);
        let label = element(&engine, &label_id);
        assert_lines_fit(&label, rect.width - 10.0);
    }

    /// A turned shape's label is turned with it.
    #[test]
    fn a_label_turns_with_its_shape() {
        let mut rect = box_at(0.0, 0.0, 200.0, 100.0);
        rect.angle = 0.5;
        let rect_id = rect.id.clone();
        let mut engine = engine_with_measure(vec![rect]);
        engine.select(vec![rect_id.clone()]);
        assert!(engine.edit_selected_text());
        let id = engine.drain_events().text_edit.unwrap().id;
        engine.set_element_text(&id, "hi");
        let label = element(&engine, &id);
        assert_close(label.angle, 0.5);
        // Its centre is the shape's: a centred label in a shape turned about its middle.
        assert!((label.x + label.width / 2.0 - 100.0).abs() < 1e-9);
        assert!((label.y + label.height / 2.0 - 50.0).abs() < 1e-9);
    }

    /// Re-wrapping is from what was typed, however often it happens: a label laid out
    /// again in a narrowed shape wraps, and laid out again once the shape is wide again
    /// comes back to one line. Laid out through a writer, the alignment.
    #[test]
    fn relayout_wraps_the_source_not_the_drawn_lines() {
        let rect = box_at(0.0, 0.0, 400.0, 60.0);
        let rect_id = rect.id.clone();
        let (mut engine, label_id) = labelled(rect, "the quick brown fox");
        assert_eq!(
            element(&engine, &label_id).text.as_deref(),
            Some("the quick brown fox")
        );
        let mut scene = engine.get_scene();
        scene.iter_mut().find(|el| el.id == rect_id).unwrap().width = 100.0;
        engine.set_scene(Scene::new(scene));
        engine.select(vec![rect_id.clone()]);
        engine.set_text_align(TextAlign::Left);
        assert!(element(&engine, &label_id).text.unwrap().contains('\n'));

        let mut scene = engine.get_scene();
        scene.iter_mut().find(|el| el.id == rect_id).unwrap().width = 400.0;
        engine.set_scene(Scene::new(scene));
        engine.select(vec![rect_id.clone()]);
        engine.set_text_align(TextAlign::Center);
        assert_eq!(
            element(&engine, &label_id).text.as_deref(),
            Some("the quick brown fox")
        );
    }

    /// A label saved taller than its shape — before shapes grew for their labels, or in
    /// a shape since resized smaller — starts at the shape's padded top when the shape
    /// moves, rather than being centred up off the shape: text that overflows downward is
    /// still read from its first line.
    #[test]
    fn a_label_taller_than_its_shape_starts_at_its_top_when_the_shape_moves() {
        let mut rect = box_at(0.0, 0.0, 120.0, 30.0);
        let mut label = text_at(8.0, 0.0, 104.0, 90.0);
        label.text = Some("one\ntwo\nthree".into());
        label.container_id = Some(rect.id.clone());
        rect.bound_text_id = Some(label.id.clone());
        let (rect_id, label_id) = (rect.id.clone(), label.id.clone());
        let mut engine = engine_with_measure(vec![rect, label]);
        engine.select(vec![rect_id.clone()]);
        engine.nudge_selection(1.0, 0.0);
        let rect = element(&engine, &rect_id);
        let label = element(&engine, &label_id);
        assert_close(label.y, rect.y + BOUND_TEXT_PADDING);
        assert_close(label.x, rect.x + 8.0);
    }
}

/// Free text: auto-sizing or a fixed width, and the edge it keeps as it changes.
mod free_text {
    use super::*;

    fn free(align: TextAlign) -> (DrawEngine, String) {
        let mut text = text_at(100.0, 100.0, 0.0, 0.0);
        text.text_align = Some(align);
        let id = text.id.clone();
        let mut engine = engine_with_measure(vec![text]);
        engine.set_element_text(&id, "hello");
        (engine, id)
    }

    /// An auto-sizing text's box is its text's.
    #[test]
    fn an_auto_sizing_text_is_as_big_as_its_text() {
        let (engine, id) = free(TextAlign::Left);
        let text = element(&engine, &id);
        assert_close(text.width, measure_text("hello", 20.0).0);
        assert_close(text.height, 25.0);
        assert_close(text.x, 100.0);
    }

    /// Typed into, a right-aligned text grows to the left and a centred one both ways
    /// (`getAdjustedDimensions`, `newElement.ts@1118751f:393-480`).
    #[test]
    fn typing_keeps_the_edge_the_alignment_names() {
        let (mut engine, id) = free(TextAlign::Right);
        let before = element(&engine, &id);
        engine.set_element_text(&id, "hello there");
        let after = element(&engine, &id);
        assert_close(after.x + after.width, before.x + before.width);

        let (mut engine, id) = free(TextAlign::Center);
        let before = element(&engine, &id);
        engine.set_element_text(&id, "hello there");
        let after = element(&engine, &id);
        assert_close(after.x + after.width / 2.0, before.x + before.width / 2.0);
    }

    /// A new font size keeps the aligned edge and the vertical middle
    /// (`offsetElementAfterFontResize`, `actionProperties.tsx@1118751f:273-292`).
    #[test]
    fn a_font_size_change_keeps_the_oracles_anchor() {
        for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
            let (mut engine, id) = free(align);
            let before = element(&engine, &id);
            engine.select(vec![id.clone()]);
            engine.set_font_size(40.0);
            let after = element(&engine, &id);
            assert_close(after.width, measure_text("hello", 40.0).0);
            assert_close(after.height, 50.0);
            let x = match align {
                TextAlign::Left => before.x,
                TextAlign::Center => before.x + (before.width - after.width) / 2.0,
                TextAlign::Right => before.x + (before.width - after.width),
            };
            assert_close(after.x, x);
            assert_close(after.y, before.y + (before.height - after.height) / 2.0);
        }
    }

    /// A fixed-width text wraps at its width and keeps it; made auto-sizing again it
    /// takes its typed lines and its measured size, pinned where its alignment says
    /// (`actionTextAutoResize`).
    #[test]
    fn a_fixed_width_text_wraps_and_can_size_itself_again() {
        let mut text = text_at(100.0, 100.0, 80.0, 25.0);
        text.auto_resize = Some(false);
        text.text_align = Some(TextAlign::Right);
        let id = text.id.clone();
        let mut engine = engine_with_measure(vec![text]);
        engine.set_element_text(&id, "the quick brown fox");
        let fixed = element(&engine, &id);
        assert_close(fixed.width, 80.0);
        assert_lines_fit(&fixed, 80.0);
        assert!(fixed.text.as_deref().unwrap().contains('\n'));

        engine.select(vec![id.clone()]);
        engine.set_text_auto_resize(true);
        let auto = element(&engine, &id);
        assert_eq!(auto.auto_resize, Some(true));
        assert_eq!(auto.text.as_deref(), Some("the quick brown fox"));
        assert_close(auto.width, measure_text("the quick brown fox", 20.0).0);
        assert_close(auto.height, 25.0);
        // Right-aligned: the right edge stays; top-aligned: the top stays.
        assert_close(auto.x + auto.width, fixed.x + fixed.width);
        assert_close(auto.y, fixed.y);
    }

    /// Sizing itself again moves and resizes a text, and an arrow bound to it follows —
    /// "any other arrow bound to this text still has to be re-routed"
    /// (`actionTextAutoResize.ts@1118751f`, `updateBoundElements`).
    #[test]
    fn a_text_sizing_itself_again_takes_its_arrows_with_it() {
        let mut text = text_at(100.0, 100.0, 300.0, 25.0);
        text.auto_resize = Some(false);
        let mut arrow = connector(700.0, 112.0, 406.0, 112.0, DrawElementType::Arrow);
        arrow.end_binding = Some(text.id.clone());
        let (id, arrow_id) = (text.id.clone(), arrow.id.clone());
        let mut engine = engine_with_measure(vec![text, arrow]);
        engine.set_element_text(&id, "hi");
        let fixed = element(&engine, &id);
        assert_close(fixed.x + fixed.width, 400.0);

        engine.select(vec![id.clone()]);
        engine.set_text_auto_resize(true);
        let auto = element(&engine, &id);
        let right = auto.x + auto.width;
        assert_close(right, 100.0 + measure_text("hi", 20.0).0);
        let (_, end) = linear_endpoints(&element(&engine, &arrow_id));
        assert!(
            end.x > right && end.x < right + 15.0,
            "the arrow ends at {} with the text's right edge at {right}",
            end.x
        );
    }
}

/// Families: new text is written in Excalifont; a family change brings its line height
/// and lays the text out again; a text with none keeps the system stack.
mod families {
    use super::*;

    #[test]
    fn new_text_is_excalifont_with_its_line_height() {
        let mut engine = engine_with_measure(vec![]);
        engine.handle_double_click(300.0, 300.0);
        let request = engine.drain_events().text_edit.unwrap();
        let text = element(&engine, &request.id);
        assert_eq!(text.font_family, Some(5));
        assert_eq!(text.line_height, Some(1.25));
        assert_eq!(text.original_text.as_deref(), Some(""));
        assert_eq!(request.font_family, 5);
        assert_close(request.line_height, 1.25);
    }

    /// `changeFontFamily` sets the family's line height too
    /// (`actionProperties.tsx@1118751f:1285-1290`) and redraws the bounding box.
    #[test]
    fn a_family_change_brings_its_line_height_and_relays_the_label() {
        let rect = box_at(0.0, 0.0, 300.0, 100.0);
        let rect_id = rect.id.clone();
        let (mut engine, label_id) = labelled(rect, "one\ntwo");
        engine.select(vec![rect_id]);
        engine.set_font_family(7);
        let label = element(&engine, &label_id);
        assert_eq!(label.font_family, Some(7));
        assert_eq!(label.line_height, Some(1.15));
        assert_close(label.height, 2.0 * 20.0 * 1.15);
        assert_close(label.y, 5.0 + (90.0 - label.height) / 2.0);
        assert_eq!(engine.get_font_family(), 7);
        // Nothing selected, it is the next text's family.
        engine.clear_selection();
        engine.set_font_family(8);
        assert_eq!(engine.get_font_family(), 8);
        // An id the engine does not draw with is ignored.
        engine.set_font_family(42);
        assert_eq!(engine.get_font_family(), 8);
    }

    /// Measured per family: the layout asks the measurer with the text's own font.
    #[test]
    fn the_measurer_is_asked_in_the_texts_font() {
        fn by_family(line: &str, font: FontKey) -> f64 {
            // Comic Shanns twice as wide as anything else.
            let per_char = if font.family() == 8 { 20.0 } else { 10.0 };
            line.chars().count() as f64 * per_char * font.size() / 20.0
        }
        let mut text = text_at(0.0, 0.0, 0.0, 0.0);
        text.font_family = Some(8);
        let id = text.id.clone();
        let mut engine = engine_with_scene(vec![text]);
        engine.set_measure_line(by_family);
        engine.set_element_text(&id, "abc");
        assert_close(element(&engine, &id).width, 60.0);
        engine.select(vec![id.clone()]);
        engine.set_font_family(5);
        assert_close(element(&engine, &id).width, 30.0);
    }

    /// A text with no family draws as it always has: the system stack, the top of each
    /// line on the top of its line box, 1.25 apart.
    #[test]
    fn a_text_with_no_family_keeps_its_old_placement() {
        let legacy = text_at(0.0, 0.0, 80.0, 25.0);
        assert_eq!(render::text_baseline(&legacy), "top");
        let (first, line_height) = render::text_line_placement(&legacy);
        assert_close(first, 0.0);
        assert_close(line_height, 25.0);
        assert_eq!(
            text::font::font_string(text::layout::font_of(&legacy)),
            format!("20px {FONT_FAMILY}")
        );
    }

    /// A text in a family sits on the oracle's baseline (`getVerticalOffset`,
    /// `font-metadata.ts@1118751f:155-170`): Excalifont at 20px, line height 1.25 —
    /// ascent 17.72, descent 7.48, gap (25 − 17.72 − 7.48) / 2 = −0.1, offset 17.62.
    #[test]
    fn a_text_in_a_family_sits_on_the_oracles_baseline() {
        let mut text = text_at(0.0, 0.0, 80.0, 25.0);
        text.font_family = Some(5);
        assert_eq!(render::text_baseline(&text), "alphabetic");
        let (first, line_height) = render::text_line_placement(&text);
        assert!((first - 17.62).abs() < 1e-9, "{first}");
        assert_close(line_height, 25.0);
        // Lilita One 1.15: em 0.02, ascent 18.46, descent 4.4, gap (23 − 18.46 − 4.4) / 2.
        text.font_family = Some(7);
        let (first, line_height) = render::text_line_placement(&text);
        assert!((first - (18.46 + (23.0 - 18.46 - 4.4) / 2.0)).abs() < 1e-9);
        assert_close(line_height, 23.0);
    }
}

/// The style panel on a shape reaches its label for what a label draws with.
mod style {
    use super::*;

    /// `actionChangeStrokeColor` and `actionChangeOpacity` apply to the bound text of
    /// what is selected (`changeProperty(…, true)`, `actionProperties.tsx@1118751f`); a
    /// fill does not.
    #[test]
    fn a_shapes_stroke_colour_and_opacity_reach_its_label() {
        let rect = box_at(0.0, 0.0, 200.0, 100.0);
        let rect_id = rect.id.clone();
        let (mut engine, label_id) = labelled(rect, "hi");
        let before = element(&engine, &label_id);
        engine.select(vec![rect_id.clone()]);
        engine.apply_style(DrawElementStylePatch {
            stroke_color: Some("#e03131".into()),
            opacity: Some(40.0),
            background_color: Some("#ffec99".into()),
            ..DrawElementStylePatch::default()
        });
        let label = element(&engine, &label_id);
        assert_eq!(label.stroke_color, "#e03131");
        assert_close(label.opacity, 40.0);
        assert_eq!(label.background_color, before.background_color);
        assert!(
            label.version > before.version,
            "the label's change is stamped"
        );
        let rect = element(&engine, &rect_id);
        assert_eq!(rect.background_color, "#ffec99");

        // One edit: one undo puts both back.
        engine.undo();
        assert_eq!(
            element(&engine, &label_id).stroke_color,
            before.stroke_color
        );
    }
}

/// What a peer holds is untouchable (`engine/peers.rs`), and a change that changes
/// nothing is not an edit.
mod holds_and_no_ops {
    use super::*;

    /// A peer typing into a label holds the label alone, so its shape stays selectable
    /// here. Restyling the shape, or changing its label's font, size, alignment or wrap,
    /// must not reach into the label under their hands.
    #[test]
    fn a_label_a_peer_holds_is_left_alone() {
        let rect = box_at(0.0, 0.0, 200.0, 100.0);
        let rect_id = rect.id.clone();
        let (mut engine, label_id) = labelled(rect, "hello");
        engine.set_peers(vec![Peer {
            id: "ana".into(),
            name: "Ana".into(),
            color: "#e03131".into(),
            holds: vec![label_id.clone()],
            preview: Vec::new(),
        }]);
        let before = element(&engine, &label_id);
        engine.select(vec![rect_id.clone()]);
        assert_eq!(engine.get_selection(), vec![rect_id.clone()]);
        engine.apply_style(stroke_patch("#e03131"));
        engine.set_font_family(7);
        engine.set_font_size(36.0);
        engine.set_text_align(TextAlign::Right);
        engine.set_vertical_align(VerticalAlign::Top);
        engine.set_label_wrap(false);
        assert_eq!(element(&engine, &label_id), before);
        // The shape itself is still anyone's.
        assert_eq!(element(&engine, &rect_id).stroke_color, "#e03131");
    }

    /// Picking the family a text already has, or the wrap it already has, is not an edit:
    /// nothing is stamped, and the redo stack survives it (`newElementWith` returns the
    /// element unchanged when no value differs, `mutateElement.ts@1118751f:149-181`).
    #[test]
    fn a_change_that_changes_nothing_is_not_an_edit() {
        let rect = box_at(0.0, 0.0, 200.0, 100.0);
        let rect_id = rect.id.clone();
        let (mut engine, label_id) = labelled(rect, "hello");
        engine.select(vec![rect_id.clone()]);
        engine.set_label_wrap(false);
        let unwrapped = element(&engine, &label_id);
        engine.set_text_align(TextAlign::Right);
        engine.undo();
        let before = element(&engine, &label_id);
        assert_eq!(before.text_align, unwrapped.text_align);

        engine.select(vec![rect_id.clone()]);
        engine.set_font_family(before.font_family.unwrap());
        engine.set_label_wrap(false);
        engine.apply_style(stroke_patch(&before.stroke_color));
        let after = element(&engine, &label_id);
        assert_eq!(
            (after.version, after.version_nonce),
            (before.version, before.version_nonce),
            "nothing changed, nothing is stamped"
        );
        // And nothing was recorded over the undone step.
        engine.redo();
        assert_eq!(
            element(&engine, &label_id).text_align,
            Some(TextAlign::Right)
        );
    }

    fn ana_holds(id: &str) -> Vec<Peer> {
        vec![Peer {
            id: "ana".into(),
            name: "Ana".into(),
            color: "#e03131".into(),
            holds: vec![id.to_owned()],
            preview: Vec::new(),
        }]
    }

    /// Where a label's middle is on screen, the camera being the identity here.
    fn middle_of(engine: &DrawEngine, id: &str) -> (f64, f64) {
        let label = element(engine, id);
        (label.x + label.width / 2.0, label.y + label.height / 2.0)
    }

    /// A shape a peer holds holds its words. A label is laid out in its shape and grows
    /// it, so one reached without the shape — clicked on its own, or taken by Select All,
    /// both of which leave out what a peer holds — is neither restyled nor laid out
    /// again: grown and stamped, the shape went to the server and to them.
    #[test]
    fn a_shape_a_peer_holds_is_not_grown_through_its_label() {
        let rect = box_at(0.0, 0.0, 200.0, 100.0);
        let rect_id = rect.id.clone();
        let (mut engine, label_id) = labelled(rect, "hello");
        // A style to paste: a shape whose label is big enough to outgrow the first one.
        let mut scene = engine.get_scene();
        scene.push(box_at(400.0, 0.0, 200.0, 100.0));
        engine.set_scene(Scene::new(scene));
        engine.handle_double_click(500.0, 50.0);
        let big = engine.drain_events().text_edit.expect("a second label");
        engine.set_element_text(&big.id, "big");
        engine.select(vec![big.id.clone()]);
        engine.set_font_size(80.0);
        let source = element(&engine, &big.id).container_id.unwrap();
        engine.select(vec![source]);
        assert!(engine.copy_styles());

        engine.set_peers(ana_holds(&rect_id));
        let (shape, label) = (element(&engine, &rect_id), element(&engine, &label_id));
        let (x, y) = middle_of(&engine, &label_id);
        engine.begin_pointer(x, y, false, false);
        engine.end_pointer();
        assert_eq!(engine.get_selection(), vec![label_id.clone()], "setup");
        assert_eq!(engine.selection_style().font_size, None, "no text rows");
        engine.set_font_size(80.0);
        engine.set_font_family(7);
        engine.paste_styles();
        engine.select_all();
        engine.set_font_size(60.0);

        assert_eq!(element(&engine, &rect_id), shape);
        assert_eq!(element(&engine, &label_id), label);
    }
}

/// The cut in a line's stroke under its label follows the label as it is painted.
mod painting {
    use super::*;

    /// A peer typing into an arrow's label streams the label alone. Its growing box is
    /// what the arrow's stroke is cut under, and the arrow is painted fresh with it, not
    /// from a cached picture holding the old cut.
    #[test]
    fn the_cut_under_an_arrows_label_follows_a_peers_preview() {
        let mut arrow = connector(0.0, 0.0, 400.0, 0.0, DrawElementType::Arrow);
        arrow.roundness = None;
        let arrow_id = arrow.id.clone();
        let mut engine = engine_with_measure(vec![arrow]);
        engine.select(vec![arrow_id.clone()]);
        assert!(engine.edit_selected_text());
        let label_id = engine.drain_events().text_edit.unwrap().id;
        engine.set_element_text(&label_id, "hi");
        let preview = engine
            .text_preview(&label_id, "hi there, a longer label")
            .unwrap();
        engine.set_peers(vec![Peer {
            id: "ana".into(),
            name: "Ana".into(),
            color: "#e03131".into(),
            holds: vec![label_id.clone()],
            preview: vec![preview.clone()],
        }]);
        assert!(
            engine.debug_live().contains(&arrow_id),
            "the arrow is painted fresh"
        );
        let arrow = element(&engine, &arrow_id);
        let view = engine.paint_view();
        let painted = view.linear_label(&arrow).expect("the arrow has a label");
        assert_close(painted.width, preview.width);
        assert_close(painted.x, preview.x);
    }
}

/// A font arriving is not an edit.
mod fonts_loaded {
    use super::*;

    fn narrow(line: &str, font: FontKey) -> f64 {
        line.chars().count() as f64 * font.size() * 0.5
    }

    fn wide(line: &str, font: FontKey) -> f64 {
        line.chars().count() as f64 * font.size() * 0.9
    }

    /// Laid out with the fallback's metrics, a label is laid out again when its face
    /// arrives — and nothing about it is stamped, sent or undoable.
    #[test]
    fn a_loaded_font_relays_texts_without_stamping_them() {
        let rect = box_at(0.0, 0.0, 200.0, 40.0);
        let rect_id = rect.id.clone();
        let other = box_at(500.0, 500.0, 50.0, 50.0);
        let other_id = other.id.clone();
        let mut engine = engine_with_scene(vec![rect, other]);
        engine.set_measure_line(narrow);
        engine.handle_double_click(100.0, 20.0);
        let label_id = engine.drain_events().text_edit.unwrap().id;
        engine.set_element_text(&label_id, "the quick brown fox");
        let _ = engine.drain_events();
        let before = element(&engine, &label_id);
        let rect_before = element(&engine, &rect_id);
        assert_eq!(before.text.as_deref(), Some("the quick brown fox"));

        // The face arrives: the same text now measures wider.
        engine.set_measure_line(wide);
        engine.fonts_loaded();

        let after = element(&engine, &label_id);
        let rect_after = element(&engine, &rect_id);
        assert!(after.text.as_deref().unwrap().contains('\n'), "re-wrapped");
        assert!(rect_after.height > rect_before.height, "the shape grew");
        assert_eq!(
            (after.version, after.version_nonce),
            (before.version, before.version_nonce)
        );
        assert_eq!(
            (rect_after.version, rect_after.version_nonce),
            (rect_before.version, rect_before.version_nonce)
        );
        let events = engine.drain_events();
        assert!(events.scene_delta.is_none(), "nothing is sent");
        assert!(events.scene_json.is_none());

        // Nor stamped by the next commit, which is about something else.
        engine.select(vec![other_id.clone()]);
        engine.nudge_selection(1.0, 0.0);
        let delta = engine
            .drain_events()
            .scene_delta
            .expect("the nudge is sent");
        let sent: Vec<&str> = delta.updated.iter().map(|el| el.id.as_str()).collect();
        assert_eq!(sent, vec![other_id.as_str()]);
        assert_eq!(element(&engine, &label_id).version, before.version);
        assert_eq!(element(&engine, &rect_id).version, rect_before.version);

        // And not undoable: undo takes back the nudge, not the layout.
        engine.undo();
        assert_eq!(element(&engine, &label_id).text, after.text);
        assert_close(element(&engine, &rect_id).height, rect_after.height);
    }

    /// A text with no family is in the system stack, which was never loading: it is
    /// left exactly as it is.
    #[test]
    fn a_text_with_no_family_is_left_alone() {
        let mut legacy = text_at(10.0, 10.0, 999.0, 25.0);
        legacy.text = Some("hello".into());
        let mut engine = engine_with_scene(vec![legacy.clone()]);
        engine.set_measure_line(wide);
        engine.fonts_loaded();
        assert_eq!(element(&engine, &legacy.id), legacy);
    }
}
