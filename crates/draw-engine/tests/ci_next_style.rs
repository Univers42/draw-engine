//! The panel's second half: the font family, the font size steps, the wrap rows, the
//! arrow type and the next arrow's heads — each read through `selection_style` and set
//! the way the oracle's actions set them (`actions/actionProperties.tsx@1118751f`).

mod common;
use common::*;
use draw_engine::engine::{ArrowType, Edges};
use draw_engine::*;

fn with_id(mut element: DrawElement, id: &str) -> DrawElement {
    element.id = id.into();
    element
}

fn text(id: &str, size: f64) -> DrawElement {
    let mut text = with_id(text_at(0.0, 0.0, 54.0, 25.0), id);
    text.text = Some("hello".into());
    text.original_text = Some("hello".into());
    text.font_size = Some(size);
    text
}

fn labelled(mut shape: DrawElement, id: &str, label_id: &str) -> Vec<DrawElement> {
    shape.id = id.into();
    shape.bound_text_id = Some(label_id.into());
    let mut label = text(label_id, 20.0);
    label.container_id = Some(id.into());
    vec![shape, label]
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("element exists")
}

fn selected(elements: Vec<DrawElement>, ids: &[&str]) -> DrawEngine {
    let mut engine = engine_with_measure(elements);
    engine.select(ids.iter().map(|id| id.to_string()).collect());
    engine
}

/// Draws with `tool` from `from` to `to`, in screen pixels (the camera is identity), and
/// returns what was drawn.
fn draw(engine: &mut DrawEngine, tool: DrawTool, from: (f64, f64), to: (f64, f64)) -> DrawElement {
    engine.set_tool(tool);
    engine.begin_pointer(from.0, from.1, false, false);
    for step in 1..=5 {
        let t = step as f64 / 5.0;
        engine.move_pointer(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            false,
            false,
        );
    }
    engine.end_pointer();
    engine.get_scene().pop().expect("drawn")
}

mod arrowheads {
    use super::*;

    /// `actionChangeArrowhead` always sets `currentItemStartArrowhead` /
    /// `currentItemEndArrowhead` (`:1944-1982`), and a new arrow is drawn with them
    /// (`components/App.tsx@1118751f:10251-10254`). Nothing selected, the choice is only
    /// that.
    #[test]
    fn a_head_chosen_with_nothing_selected_is_the_next_arrows() {
        let mut engine = engine_with_measure(Vec::new());
        engine.set_arrowheads(Some(Arrowhead::Triangle), None);
        let style = engine.selection_style();
        assert_eq!(style.start_arrowhead, Some(Arrowhead::Triangle));
        assert_eq!(style.end_arrowhead, Some(Arrowhead::Arrow), "untouched");

        let arrow = draw(&mut engine, DrawTool::Arrow, (100.0, 100.0), (300.0, 100.0));
        assert_eq!(arrow.kind, DrawElementType::Arrow);
        assert_eq!(arrow.start_arrowhead, Some(Arrowhead::Triangle));
        assert_eq!(arrow.end_arrowhead, None, "the default it resolves to");
    }

    /// A line is drawn headless whatever was chosen: `[null, null]` for anything but an
    /// arrow (`App.tsx@1118751f:10252-10254`).
    #[test]
    fn a_line_takes_no_heads() {
        let mut engine = engine_with_measure(Vec::new());
        engine.set_arrowheads(Some(Arrowhead::Dot), Some(Arrowhead::Bar));
        let line = draw(&mut engine, DrawTool::Line, (100.0, 100.0), (300.0, 100.0));
        assert_eq!(line.kind, DrawElementType::Line);
        assert_eq!(line.start_arrowhead, None);
        assert_eq!(line.end_arrowhead, None);
    }

    #[test]
    fn a_head_chosen_for_a_selection_is_the_next_arrows_too() {
        let arrow = with_id(connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow), "a");
        let mut engine = selected(vec![arrow], &["a"]);
        engine.set_arrowheads(None, Some(Arrowhead::Dot));
        assert_eq!(element(&engine, "a").end_arrowhead, Some(Arrowhead::Dot));
        engine.clear_selection();
        assert_eq!(engine.selection_style().end_arrowhead, Some(Arrowhead::Dot));
    }

    /// The wave-2 rule for every style: a loose locked element is passed by.
    #[test]
    fn a_locked_arrow_keeps_its_heads() {
        let mut arrow = with_id(connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow), "a");
        arrow.locked = Some(true);
        let mut engine = selected(vec![arrow], &["a"]);
        engine.set_arrowheads(Some(Arrowhead::Bar), None);
        assert_eq!(element(&engine, "a").start_arrowhead, None);
    }

    /// `newElementWith` leaves an unchanged element untouched, stamp and all.
    #[test]
    fn a_head_it_already_has_is_no_edit() {
        let mut arrow = with_id(connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow), "a");
        arrow.start_arrowhead = Some(Arrowhead::Triangle);
        let mut engine = selected(vec![arrow], &["a"]);
        let version = element(&engine, "a").version;
        engine.set_arrowheads(Some(Arrowhead::Triangle), None);
        assert_eq!(element(&engine, "a").version, version);
    }
}

mod arrow_type {
    use super::*;

    fn arrow(id: &str, round: bool) -> DrawElement {
        let mut arrow = with_id(connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow), id);
        arrow.roundness = round.then_some(8.0);
        arrow
    }

    /// `actionChangeArrowType`'s form value reads arrows alone (`:2275-2296`); the
    /// Edges row reads everything else.
    #[test]
    fn the_row_reads_arrows_only() {
        let mut shape = with_id(box_at(0.0, 100.0, 50.0, 50.0), "box");
        shape.roundness = None;
        let engine = selected(vec![arrow("a", true), shape], &["a", "box"]);
        let style = engine.selection_style();
        assert_eq!(style.arrow_type, Some(ArrowType::Round));
        assert_eq!(style.edges, Some(Edges::Sharp));

        let engine = selected(vec![arrow("a", true), arrow("b", false)], &["a", "b"]);
        assert_eq!(engine.selection_style().arrow_type, None, "mixed");
    }

    #[test]
    fn sharp_straightens_the_selected_arrows_in_one_step() {
        let mut engine = selected(vec![arrow("a", true), arrow("b", true)], &["a", "b"]);
        engine.set_arrow_type(ArrowType::Sharp);
        assert_eq!(element(&engine, "a").roundness, None);
        assert_eq!(element(&engine, "b").roundness, None);
        engine.undo();
        assert!(element(&engine, "a").roundness.is_some());
        assert!(element(&engine, "b").roundness.is_some());
    }

    #[test]
    fn a_locked_arrow_keeps_its_type() {
        let mut locked = arrow("a", true);
        locked.locked = Some(true);
        let mut engine = selected(vec![locked], &["a"]);
        engine.set_arrow_type(ArrowType::Sharp);
        assert!(element(&engine, "a").roundness.is_some());
    }

    /// `currentItemArrowType`, curved until chosen (`appState.ts@1118751f:45`), is what a
    /// new arrow is drawn with (`App.tsx@1118751f:10270-10276`) — and only an arrow.
    #[test]
    fn the_next_arrow_is_drawn_in_the_type_chosen() {
        let mut engine = engine_with_measure(Vec::new());
        assert_eq!(engine.selection_style().arrow_type, Some(ArrowType::Round));
        let curved = draw(&mut engine, DrawTool::Arrow, (100.0, 100.0), (300.0, 100.0));
        assert!(curved.roundness.is_some());

        engine.clear_selection();
        engine.set_arrow_type(ArrowType::Sharp);
        assert_eq!(engine.selection_style().arrow_type, Some(ArrowType::Sharp));
        let sharp = draw(&mut engine, DrawTool::Arrow, (100.0, 200.0), (300.0, 200.0));
        assert_eq!(sharp.roundness, None);
    }

    /// The Edges row is `currentItemRoundness`, which lines and shapes take and arrows do
    /// not.
    #[test]
    fn sharp_edges_do_not_reach_the_next_arrow() {
        let mut engine = engine_with_measure(Vec::new());
        engine.set_next_style(DrawElementStylePatch {
            roundness: Some(None),
            ..Default::default()
        });
        let arrow = draw(&mut engine, DrawTool::Arrow, (100.0, 100.0), (300.0, 100.0));
        assert!(arrow.roundness.is_some(), "the arrow type decides");
        let line = draw(&mut engine, DrawTool::Line, (100.0, 200.0), (300.0, 200.0));
        assert_eq!(line.roundness, None, "the edges decide");
    }
}

mod font_size {
    use super::*;

    /// `actionIncreaseFontSize` / `actionDecreaseFontSize` (`:1095-1141`): a tenth up, or
    /// back by the same factor, rounded as `Math.round` rounds.
    #[test]
    fn a_step_is_a_tenth_rounded() {
        let mut engine = selected(vec![text("t", 20.0)], &["t"]);
        engine.step_font_size(true);
        assert_eq!(element(&engine, "t").font_size, Some(22.0));
        engine.step_font_size(true);
        assert_eq!(element(&engine, "t").font_size, Some(24.0), "round(24.2)");
        engine.step_font_size(false);
        assert_eq!(element(&engine, "t").font_size, Some(22.0), "round(21.8)");

        let mut engine = selected(vec![text("t", 20.0)], &["t"]);
        engine.step_font_size(false);
        assert_eq!(element(&engine, "t").font_size, Some(18.0), "round(18.18)");
    }

    /// Each text steps from its own size, and the next text's size moves only when they
    /// all end the same (`changeFontSize`, `:341-351`).
    #[test]
    fn each_text_steps_from_its_own_size() {
        let mut engine = selected(vec![text("t", 20.0), text("u", 30.0)], &["t", "u"]);
        engine.step_font_size(true);
        assert_eq!(element(&engine, "t").font_size, Some(22.0));
        assert_eq!(element(&engine, "u").font_size, Some(33.0));
        engine.clear_selection();
        assert_eq!(
            engine.selection_style().font_size,
            Some(20.0),
            "sizes differ"
        );

        let mut engine = selected(vec![text("t", 20.0), text("u", 20.0)], &["t", "u"]);
        engine.step_font_size(true);
        engine.clear_selection();
        assert_eq!(
            engine.selection_style().font_size,
            Some(22.0),
            "all the same"
        );
    }

    #[test]
    fn a_label_steps_through_its_shape_in_one_step() {
        let mut engine = selected(
            labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"),
            &["box"],
        );
        engine.step_font_size(true);
        assert_eq!(element(&engine, "l").font_size, Some(22.0));
        engine.undo();
        assert_eq!(element(&engine, "l").font_size, Some(20.0));
    }

    /// The chord reaches the text editor (`wysiwyg/textWysiwyg.tsx@1118751f:675-678`). A
    /// text still being typed is not in the scene until its editor closes, so its size
    /// is written into it, and the commit that makes it is still the one that closes the
    /// editor: one undo takes it away rather than leaving an empty text behind.
    #[test]
    fn a_text_being_typed_takes_the_step_in_the_commit_that_makes_it() {
        let mut engine = engine_with_measure(vec![]);
        engine.handle_double_click(100.0, 100.0);
        let draft = engine
            .get_selected_elements()
            .pop()
            .expect("a text to type into");
        engine.step_font_size(true);
        assert_eq!(element(&engine, &draft.id).font_size, Some(22.0));
        engine.set_element_text(&draft.id, "hi");
        assert_eq!(element(&engine, &draft.id).font_size, Some(22.0));
        engine.undo();
        let live: Vec<DrawElement> = engine
            .get_scene()
            .into_iter()
            .filter(|element| !element.is_deleted)
            .collect();
        assert!(live.is_empty(), "one step made it: {live:?}");
    }

    #[test]
    fn nothing_selected_changes_nothing() {
        let mut engine = engine_with_measure(vec![text("t", 20.0)]);
        engine.step_font_size(true);
        assert_eq!(element(&engine, "t").font_size, Some(20.0));
        assert_eq!(engine.selection_style().font_size, Some(20.0));
    }

    /// The contract stores sizes from 1 to 1000 (`packages/contract/src/element.ts`);
    /// the oracle has no ceiling, and a size above it would never save.
    #[test]
    fn sizes_stay_within_what_the_contract_stores() {
        let mut engine = selected(vec![text("t", 1000.0)], &["t"]);
        engine.step_font_size(true);
        assert_eq!(element(&engine, "t").font_size, Some(1000.0));

        engine.set_font_size(5000.0);
        assert_eq!(element(&engine, "t").font_size, Some(1000.0));
        engine.set_font_size(0.2);
        assert_eq!(element(&engine, "t").font_size, Some(1.0));
        engine.set_font_size(f64::NAN);
        assert_eq!(element(&engine, "t").font_size, Some(1.0), "ignored");
    }
}

mod font_family {
    use super::*;

    fn in_family(id: &str, family: Option<u8>) -> DrawElement {
        let mut element = text(id, 20.0);
        element.font_family = family;
        element
    }

    /// `actionChangeFontFamily`'s form value (`:1370-1392`): what the texts and labels
    /// share, and the next text's with nothing selected. A text with no family is drawn
    /// in the system stack, reported as 0.
    #[test]
    fn the_row_reads_what_the_texts_share() {
        let engine = selected(
            vec![in_family("t", Some(5)), in_family("u", Some(6))],
            &["t", "u"],
        );
        assert_eq!(engine.selection_style().font_family, None);

        let engine = selected(
            vec![in_family("t", Some(6)), in_family("u", Some(6))],
            &["t", "u"],
        );
        assert_eq!(engine.selection_style().font_family, Some(6));

        let engine = selected(vec![in_family("t", None)], &["t"]);
        assert_eq!(engine.selection_style().font_family, Some(0));

        let mut elements = labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l");
        elements[1].font_family = Some(7);
        let engine = selected(elements, &["box"]);
        assert_eq!(engine.selection_style().font_family, Some(7));
    }

    #[test]
    fn with_nothing_selected_it_is_the_next_texts() {
        let mut engine = engine_with_measure(Vec::new());
        assert_eq!(engine.selection_style().font_family, Some(5));
        let revision = engine.style_revision();
        engine.set_font_family(8);
        assert_ne!(engine.style_revision(), revision, "the panel is told");
        assert_eq!(engine.selection_style().font_family, Some(8));
    }

    /// The picker previews a hovered family on the canvas and gives it back on leave
    /// (`onHover` / `onLeave`, `:1465-1478`): nothing committed, nothing to undo.
    #[test]
    fn a_hovered_family_is_shown_then_given_back() {
        let mut engine = selected(vec![in_family("t", Some(5))], &["t"]);
        let before = element(&engine, "t");
        engine.preview_font_family(Some(7));
        let previewed = element(&engine, "t");
        assert_eq!(previewed.font_family, Some(7));
        assert_eq!(previewed.line_height, Some(1.15));
        assert_eq!(previewed.version, before.version, "not stamped");

        engine.preview_font_family(None);
        assert_eq!(element(&engine, "t"), before);
        engine.undo();
        assert_eq!(
            element(&engine, "t").font_family,
            Some(5),
            "nothing to undo"
        );
    }

    #[test]
    fn a_preview_then_a_pick_is_one_step() {
        let mut engine = selected(vec![in_family("t", Some(5))], &["t"]);
        engine.preview_font_family(Some(6));
        engine.preview_font_family(Some(7));
        engine.set_font_family(7);
        assert_eq!(element(&engine, "t").font_family, Some(7));
        engine.undo();
        assert_eq!(element(&engine, "t").font_family, Some(5));
    }

    /// "In this scene" (`components/FontPicker/FontPickerList.tsx@1118751f`): the
    /// families the board's texts use, each once.
    #[test]
    fn the_scene_families_are_listed_once() {
        let mut gone = in_family("gone", Some(3));
        gone.is_deleted = true;
        let engine = engine_with_measure(vec![
            in_family("t", Some(6)),
            in_family("u", Some(6)),
            in_family("v", Some(1)),
            gone,
            box_at(0.0, 0.0, 10.0, 10.0),
        ]);
        let mut families = engine.scene_font_families();
        families.sort_unstable();
        assert_eq!(families, [1, 6]);
    }
}

mod wrap_rows {
    use super::*;

    #[test]
    fn free_text_reports_whether_it_sizes_itself() {
        let engine = selected(vec![text("t", 20.0)], &["t"]);
        let style = engine.selection_style();
        assert!(style.has_free_text);
        assert!(!style.has_label);
        assert_eq!(style.auto_resize, Some(true));

        let mut fixed = text("t", 20.0);
        fixed.auto_resize = Some(false);
        let engine = selected(vec![fixed, text("u", 20.0)], &["t", "u"]);
        assert_eq!(engine.selection_style().auto_resize, None, "mixed");
    }

    #[test]
    fn a_label_reports_whether_it_wraps() {
        let mut engine = selected(
            labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"),
            &["box"],
        );
        let style = engine.selection_style();
        assert!(style.has_label);
        assert!(!style.has_free_text);
        assert_eq!(style.label_wrap, Some(true));
        engine.set_label_wrap(false);
        assert_eq!(engine.selection_style().label_wrap, Some(false));
    }

    #[test]
    fn a_shape_alone_has_neither() {
        let engine = selected(vec![with_id(box_at(0.0, 0.0, 10.0, 10.0), "b")], &["b"]);
        let style = engine.selection_style();
        assert!(!style.has_free_text && !style.has_label);
    }
}
