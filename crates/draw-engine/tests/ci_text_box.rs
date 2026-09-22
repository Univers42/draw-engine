//! Text that keeps the width you gave it, instead of growing to fit.
//!
//! Until now every text element auto-sized: `set_element_text` set `width` to whatever
//! the glyphs measured, so a paragraph typed on the canvas became one very long line and
//! there was no way to ask for a column. Bound labels wrapped, but only because their
//! container supplied a width — free text had none to wrap to.
//!
//! The distinction is the gesture, as it is in Excalidraw: **click** with the text tool
//! and you get auto-sizing text, **drag out a box** and you get one fixed to that width.
//! `auto_resize` is what the element remembers about which it was.
//!
//! `Option<bool>` rather than `bool`, for the same reason the alignments are optional
//! (see `ci_text_align.rs`): every text ever saved auto-sizes and carries no such field,
//! so `None` has to keep meaning that, and only `false` is ever written out.

mod common;
use common::*;
use draw_engine::*;

/// A long single line, so wrapping has something to do.
const PARAGRAPH: &str = "the quick brown fox jumps over the lazy dog and keeps on running";

fn find(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == id)
        .expect("element is still in the scene")
}

fn only_text(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|e| e.kind == DrawElementType::Text)
        .expect("a text element was created")
}

/// Click: press and release in the same place.
fn click_text_tool(engine: &mut DrawEngine, x: f64, y: f64) {
    engine.set_tool(DrawTool::Text);
    engine.begin_pointer(x, y, false, false);
    engine.end_pointer();
}

/// Drag: press, move, release — the gesture that asks for a column.
fn drag_text_tool(engine: &mut DrawEngine, x: f64, y: f64, to_x: f64, to_y: f64) {
    engine.set_tool(DrawTool::Text);
    engine.begin_pointer(x, y, false, false);
    engine.move_pointer(to_x, to_y, false, false);
    engine.end_pointer();
}

mod which_gesture_made_it {
    use super::*;

    #[test]
    fn a_click_makes_text_that_grows_with_what_you_type() {
        let mut engine = engine_with_measure(vec![]);
        click_text_tool(&mut engine, 100.0, 100.0);
        let created = only_text(&engine);
        assert!(is_auto_resize(&created), "a click means auto-sizing");
        assert!(
            created.auto_resize.is_none(),
            "auto is the default, so nothing is written down"
        );
    }

    #[test]
    fn a_drag_makes_a_box_of_the_width_you_dragged() {
        let mut engine = engine_with_measure(vec![]);
        drag_text_tool(&mut engine, 100.0, 100.0, 400.0, 180.0);
        let created = only_text(&engine);
        assert_eq!(created.auto_resize, Some(false));
        assert!(!is_auto_resize(&created));
        assert_close(created.width, 300.0);
    }

    /// A click is never perfectly still. Without a threshold, a two-pixel wobble would
    /// silently produce a two-pixel-wide column that wraps every character onto its own
    /// line — the worst possible outcome, and indistinguishable from a click to the
    /// person who made it.
    #[test]
    fn a_wobble_is_still_a_click() {
        let mut engine = engine_with_measure(vec![]);
        drag_text_tool(&mut engine, 100.0, 100.0, 103.0, 101.0);
        let created = only_text(&engine);
        assert!(
            is_auto_resize(&created),
            "a 3px drag is a click, not a column"
        );
    }

    /// The editor has to open either way, or the gesture produces an empty element and
    /// no way to type into it.
    #[test]
    fn both_gestures_open_the_editor() {
        let mut engine = engine_with_measure(vec![]);
        click_text_tool(&mut engine, 100.0, 100.0);
        assert!(engine.drain_events().text_edit.is_some(), "click");

        let mut engine = engine_with_measure(vec![]);
        drag_text_tool(&mut engine, 100.0, 100.0, 400.0, 180.0);
        assert!(engine.drain_events().text_edit.is_some(), "drag");
    }

    /// The overlay has to be as wide as the box, or what you type is laid out to one
    /// width and committed at another.
    #[test]
    fn a_box_tells_the_editor_how_wide_it_is() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_viewport(800.0, 600.0, 1.0);
        drag_text_tool(&mut engine, 100.0, 100.0, 400.0, 180.0);
        let request = engine.drain_events().text_edit.expect("editor opened");
        assert_close(request.width.expect("a box has a width"), 300.0);
    }

    #[test]
    fn a_click_leaves_the_editor_free_to_size_itself() {
        let mut engine = engine_with_measure(vec![]);
        engine.set_viewport(800.0, 600.0, 1.0);
        click_text_tool(&mut engine, 100.0, 100.0);
        let request = engine.drain_events().text_edit.expect("editor opened");
        assert!(request.width.is_none(), "auto text has no width to impose");
    }
}

mod wrapping {
    use super::*;

    fn box_with(engine: &mut DrawEngine, text: &str) -> DrawElement {
        drag_text_tool(engine, 0.0, 0.0, 200.0, 60.0);
        let id = only_text(engine).id;
        engine.set_element_text(&id, text);
        find(engine, &id)
    }

    #[test]
    fn a_box_keeps_its_width_and_grows_downwards() {
        let mut engine = engine_with_measure(vec![]);
        let short = box_with(&mut engine, "hi");
        assert_close(short.width, 200.0);

        let mut engine = engine_with_measure(vec![]);
        let long = box_with(&mut engine, PARAGRAPH);
        assert_close(long.width, 200.0);
        assert!(
            long.height > short.height,
            "a wrapped paragraph is taller than one word: {} vs {}",
            long.height,
            short.height
        );
    }

    #[test]
    fn the_text_is_actually_broken_into_lines() {
        let mut engine = engine_with_measure(vec![]);
        let wrapped = box_with(&mut engine, PARAGRAPH);
        assert!(
            wrapped.text.as_deref().unwrap_or("").contains('\n'),
            "the stored text carries the line breaks: {:?}",
            wrapped.text
        );
    }

    /// The control. Auto-sizing text must *not* wrap — it grows sideways, which is the
    /// whole difference between the two.
    #[test]
    fn auto_text_grows_sideways_instead() {
        let mut engine = engine_with_measure(vec![]);
        click_text_tool(&mut engine, 0.0, 0.0);
        let id = only_text(&engine).id;

        engine.set_element_text(&id, "hi");
        let short = find(&engine, &id).width;
        engine.set_element_text(&id, PARAGRAPH);
        let long = find(&engine, &id);

        assert!(long.width > short, "{} should exceed {short}", long.width);
        assert!(
            !long.text.as_deref().unwrap_or("").contains('\n'),
            "auto text has no reason to break a line"
        );
    }

    /// A hard newline is the author's, and survives either way.
    #[test]
    fn typed_line_breaks_are_kept() {
        let mut engine = engine_with_measure(vec![]);
        let two = box_with(&mut engine, "one\ntwo");
        assert_eq!(two.text.as_deref(), Some("one\ntwo"));
    }
}

mod resizing {
    use super::*;

    /// Narrowing a column has to re-wrap it. Without this the text keeps the line breaks
    /// it had when it was wide and simply overflows the box it is supposedly inside —
    /// and the box is the only thing the person moved.
    #[test]
    fn making_the_box_narrower_rewraps_the_text() {
        let mut engine = engine_with_measure(vec![]);
        drag_text_tool(&mut engine, 0.0, 0.0, 400.0, 60.0);
        let id = only_text(&engine).id;
        engine.set_element_text(&id, PARAGRAPH);
        let wide = find(&engine, &id);

        engine.set_text_box_width(&id, 150.0);
        let narrow = find(&engine, &id);

        assert_close(narrow.width, 150.0);
        assert!(
            narrow.height > wide.height,
            "narrower means more lines: {} vs {}",
            narrow.height,
            wide.height
        );
        let lines = |el: &DrawElement| el.text.as_deref().unwrap_or("").lines().count();
        assert!(lines(&narrow) > lines(&wide));
    }

    /// Auto-sizing text has no width of its own to impose, so this must not silently
    /// turn it into a column — that would make a resize handle change what the element
    /// *is*.
    #[test]
    fn it_does_nothing_to_auto_text() {
        let mut engine = engine_with_measure(vec![]);
        click_text_tool(&mut engine, 0.0, 0.0);
        let id = only_text(&engine).id;
        engine.set_element_text(&id, "hello");
        let before = find(&engine, &id);

        engine.set_text_box_width(&id, 150.0);
        let after = find(&engine, &id);

        assert!(is_auto_resize(&after), "still auto");
        assert_close(after.width, before.width);
    }
}

mod persistence {
    use super::*;

    #[test]
    fn a_fixed_box_survives_the_wire() {
        let mut text = text_at(0.0, 0.0, 200.0, 40.0);
        text.text = Some("wrapped".into());
        text.auto_resize = Some(false);

        let json = scene_to_json(std::slice::from_ref(&text));
        let parsed = elements_from_json(&json).expect("round trip");
        assert_eq!(parsed[0].auto_resize, Some(false));
        assert!(!is_auto_resize(&parsed[0]));

        let compact: String = json.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains(r#""autoResize":false"#), "{json}");
    }

    /// Every text saved before this existed auto-sizes, so absent has to keep meaning
    /// that — and writing `true` onto them all would change nothing except the diff.
    #[test]
    fn text_from_before_the_field_existed_still_auto_sizes() {
        let mut text = text_at(0.0, 0.0, 50.0, 20.0);
        text.text = Some("old".into());
        assert!(text.auto_resize.is_none());
        assert!(is_auto_resize(&text));

        let json = scene_to_json(std::slice::from_ref(&text));
        assert!(!json.contains("autoResize"), "{json}");
    }
}
