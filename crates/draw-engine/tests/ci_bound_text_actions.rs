//! The three bound-text actions (`packages/excalidraw/actions/actionBoundText.tsx@1118751f`):
//! bind a free text into the shape selected with it, give a label back as free text, and
//! wrap a free text in a new rectangle. Each is one step of undo, and each is offered
//! only where the oracle's predicate offers it — which the panel reads from
//! `selection_style`, so the host never re-derives the rule.

mod common;
use common::*;
use draw_engine::*;

fn with_id(mut element: DrawElement, id: &str) -> DrawElement {
    element.id = id.into();
    element
}

/// A free text holding one line of `words`, measured as `engine_with_measure` measures
/// them at size 20: ten units a character and four more, 1.25 lines high.
fn words(id: &str, x: f64, y: f64, words: &str) -> DrawElement {
    let width = words.len() as f64 * 10.0 + 4.0;
    let mut text = with_id(text_at(x, y, width, 25.0), id);
    text.text = Some(words.into());
    text.original_text = Some(words.into());
    text
}

/// A shape holding a label, as the engine makes one.
fn labelled(mut shape: DrawElement, id: &str, label_id: &str) -> Vec<DrawElement> {
    shape.id = id.into();
    shape.bound_text_id = Some(label_id.into());
    let mut label = words(label_id, shape.x + 10.0, shape.y + 10.0, "label");
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

/// The live ids, bottom to top.
fn order(engine: &DrawEngine) -> Vec<String> {
    engine
        .get_scene()
        .into_iter()
        .filter(|element| !element.is_deleted)
        .map(|element| element.id)
        .collect()
}

fn selected(elements: Vec<DrawElement>, ids: &[&str]) -> DrawEngine {
    let mut engine = engine_with_measure(elements);
    engine.select(ids.iter().map(|id| id.to_string()).collect());
    engine
}

fn centre(element: &DrawElement) -> (f64, f64) {
    (
        element.x + element.width / 2.0,
        element.y + element.height / 2.0,
    )
}

mod bind {
    use super::*;

    /// `actionBindText.perform` (`:155-215`): the text is the shape's label, centred in
    /// the middle, directly above the shape in the stack, and the shape is what is left
    /// selected.
    #[test]
    fn a_text_selected_with_a_shape_becomes_its_label() {
        let mut engine = selected(
            vec![
                words("t", 400.0, 300.0, "hello"),
                with_id(box_at(0.0, 0.0, 200.0, 100.0), "box"),
                with_id(box_at(600.0, 0.0, 50.0, 50.0), "other"),
            ],
            &["t", "box"],
        );
        assert!(engine.selection_style().can_bind_text);

        engine.bind_text();

        let text = element(&engine, "t");
        assert_eq!(text.container_id.as_deref(), Some("box"));
        assert_eq!(element(&engine, "box").bound_text_id.as_deref(), Some("t"));
        assert_eq!(text.text_align, Some(TextAlign::Center));
        assert_eq!(text.vertical_align, Some(VerticalAlign::Middle));
        let (cx, cy) = centre(&text);
        assert_close(cx, 100.0);
        assert_close(cy, 50.0);
        assert_eq!(order(&engine), ["box", "t", "other"], "directly above");
        assert_eq!(engine.get_selection(), ["box"]);
    }

    #[test]
    fn it_is_one_step_of_undo() {
        let mut engine = selected(
            vec![
                with_id(box_at(0.0, 0.0, 200.0, 100.0), "box"),
                words("t", 400.0, 300.0, "hello"),
            ],
            &["box", "t"],
        );
        let before = element(&engine, "t");
        engine.bind_text();
        engine.undo();
        let text = element(&engine, "t");
        assert_eq!(text.container_id, None);
        assert_eq!((text.x, text.y), (before.x, before.y));
        assert_eq!(element(&engine, "box").bound_text_id, None);
    }

    /// `isTextBindableContainer` (`packages/element/src/typeChecks.ts@1118751f:240-253`):
    /// an arrow holds a label too, which never turns.
    #[test]
    fn an_arrow_takes_the_text_as_its_label() {
        let arrow = with_id(connector(0.0, 0.0, 200.0, 0.0, DrawElementType::Arrow), "a");
        let mut text = words("t", 400.0, 300.0, "hi");
        text.angle = 0.4;
        let mut engine = selected(vec![arrow, text], &["a", "t"]);
        engine.bind_text();
        let label = element(&engine, "t");
        assert_eq!(label.container_id.as_deref(), Some("a"));
        assert_eq!(label.angle, 0.0);
        let (cx, cy) = centre(&label);
        assert_close(cx, 100.0);
        assert_close(cy, 0.0);
    }

    #[test]
    fn the_label_turns_with_its_shape() {
        let mut shape = with_id(ellipse_at(0.0, 0.0, 200.0, 100.0), "e");
        shape.angle = 0.5;
        let mut engine = selected(vec![shape, words("t", 400.0, 300.0, "hello")], &["e", "t"]);
        engine.bind_text();
        assert_eq!(element(&engine, "t").angle, 0.5);
    }

    /// The predicate (`:128-154`): two selected, one a text, the other a shape that can
    /// hold a label and holds none.
    #[test]
    fn it_is_offered_only_for_a_text_and_an_empty_container() {
        let mut elements = labelled(box_at(0.0, 0.0, 200.0, 100.0), "full", "full-l");
        elements.extend([
            words("t", 400.0, 300.0, "hello"),
            words("u", 400.0, 400.0, "world"),
            with_id(box_at(0.0, 300.0, 100.0, 100.0), "box"),
            with_id(
                connector(0.0, 500.0, 100.0, 500.0, DrawElementType::Line),
                "line",
            ),
        ]);
        let mut engine = engine_with_measure(elements);
        for ids in [
            vec!["t", "full"],
            vec!["t", "u"],
            vec!["t", "line"],
            vec!["t", "box", "u"],
            vec!["t"],
        ] {
            engine.select(ids.iter().map(|id| id.to_string()).collect());
            assert!(!engine.selection_style().can_bind_text, "{ids:?}");
            engine.bind_text();
            assert_eq!(element(&engine, "t").container_id, None, "{ids:?}");
        }
    }

    /// Wave 2's rule for every style change holds for this one too: a loose locked
    /// element the selection holds is not changed.
    #[test]
    fn a_locked_shape_takes_no_text() {
        let mut shape = with_id(box_at(0.0, 0.0, 200.0, 100.0), "box");
        shape.locked = Some(true);
        let mut engine = selected(
            vec![shape, words("t", 400.0, 300.0, "hello")],
            &["box", "t"],
        );
        assert!(!engine.selection_style().can_bind_text);
        engine.bind_text();
        assert_eq!(element(&engine, "t").container_id, None);
    }

    /// `redrawTextBoundingBox` grows a shape too small for its new label, and the height
    /// it had is remembered for an unbind to give back (`:204-208`).
    #[test]
    fn a_shape_too_small_grows_and_unbinding_gives_its_height_back() {
        let mut engine = selected(
            vec![
                with_id(box_at(0.0, 0.0, 60.0, 30.0), "box"),
                words("t", 400.0, 300.0, "one two three four five"),
            ],
            &["box", "t"],
        );
        engine.bind_text();
        assert!(element(&engine, "box").height > 30.0, "grown to hold it");

        engine.unbind_text();
        assert_close(element(&engine, "box").height, 30.0);
    }
}

mod unbind {
    use super::*;

    /// `actionUnbindText.perform` (`:69-121`): the label is free text again, its typed
    /// lines measured as they are, where it stood.
    #[test]
    fn a_label_is_given_back_as_free_text() {
        let mut engine = selected(
            labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"),
            &["box"],
        );
        // Laid out in its shape first, as a label the engine made would be.
        engine.set_text_align(TextAlign::Center);
        let label = element(&engine, "l");
        assert!(engine.selection_style().can_unbind_text);

        engine.unbind_text();

        let text = element(&engine, "l");
        assert_eq!(text.container_id, None);
        assert_eq!(element(&engine, "box").bound_text_id, None);
        assert_eq!(text.text.as_deref(), Some("label"));
        assert_close(text.x, label.x);
        assert_close(text.y, label.y);
        assert_eq!(element(&engine, "box").height, 100.0, "nothing remembered");
    }

    #[test]
    fn it_is_one_step_of_undo() {
        let mut engine = selected(
            labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l"),
            &["box"],
        );
        engine.unbind_text();
        engine.undo();
        assert_eq!(element(&engine, "l").container_id.as_deref(), Some("box"));
        assert_eq!(element(&engine, "box").bound_text_id.as_deref(), Some("l"));
    }

    #[test]
    fn a_shape_without_a_label_offers_nothing() {
        let engine = selected(vec![with_id(box_at(0.0, 0.0, 50.0, 50.0), "box")], &["box"]);
        assert!(!engine.selection_style().can_unbind_text);
    }

    /// An arrow's extent is its points: the height the oracle writes back onto any
    /// container would contradict them, so an arrow keeps its geometry.
    #[test]
    fn an_arrow_keeps_its_points() {
        let arrow = connector(0.0, 0.0, 200.0, 0.0, DrawElementType::Arrow);
        let mut engine = selected(labelled(arrow, "a", "l"), &["a"]);
        let before = element(&engine, "a");
        engine.unbind_text();
        let after = element(&engine, "a");
        assert_eq!(after.points, before.points);
        assert_eq!(after.height, before.height);
        assert_eq!(element(&engine, "l").container_id, None);
    }
}

mod wrap {
    use super::*;

    /// `actionWrapTextInContainer.perform` (`:269-376`): a rectangle around the text, a
    /// padding clear of it, the text its centred label, the rectangle directly below it,
    /// and the rectangle selected.
    #[test]
    fn a_free_text_is_wrapped_in_a_rectangle() {
        let mut engine = selected(
            vec![
                words("t", 100.0, 100.0, "hello"),
                with_id(box_at(600.0, 0.0, 50.0, 50.0), "other"),
            ],
            &["t"],
        );
        let before = element(&engine, "t");
        assert!(engine.selection_style().has_free_text);

        engine.wrap_text_in_container();

        let text = element(&engine, "t");
        let container_id = text.container_id.clone().expect("wrapped");
        let container = element(&engine, &container_id);
        assert_eq!(container.kind, DrawElementType::Rectangle);
        assert_eq!(container.bound_text_id.as_deref(), Some("t"));
        assert_close(container.x, 95.0);
        assert_close(container.y, 95.0);
        assert_close(container.width, before.width + 10.0);
        assert_close(container.height, before.height + 10.0);
        assert_eq!(text.text_align, Some(TextAlign::Center));
        assert_eq!(text.vertical_align, Some(VerticalAlign::Middle));
        assert_eq!(order(&engine), [container_id.as_str(), "t", "other"]);
        assert_eq!(engine.get_selection(), [container_id]);
    }

    /// In the next element's style (`appState.currentItem*`), fully opaque and unlocked.
    #[test]
    fn the_rectangle_is_drawn_in_the_next_style() {
        let mut engine = engine_with_measure(vec![words("t", 100.0, 100.0, "hello")]);
        engine.set_next_style(DrawElementStylePatch {
            stroke_color: Some("#e03131".into()),
            background_color: Some("#ffec99".into()),
            opacity: Some(40.0),
            roundness: Some(None),
            ..Default::default()
        });
        engine.select(vec!["t".into()]);
        engine.wrap_text_in_container();
        let id = element(&engine, "t").container_id.expect("wrapped");
        let container = element(&engine, &id);
        assert_eq!(container.stroke_color, "#e03131");
        assert_eq!(container.background_color, "#ffec99");
        assert_eq!(container.opacity, 100.0);
        assert_eq!(container.roundness, None);
        assert_eq!(container.locked, None);
    }

    /// `:316-346`: an arrow bound to the text is bound to its rectangle instead.
    #[test]
    fn arrows_bound_to_the_text_move_to_the_rectangle() {
        let mut arrow = with_id(
            connector(0.0, 0.0, 90.0, 100.0, DrawElementType::Arrow),
            "a",
        );
        arrow.end_binding = Some("t".into());
        let mut engine = selected(vec![arrow, words("t", 100.0, 100.0, "hello")], &["t"]);
        engine.wrap_text_in_container();
        let id = element(&engine, "t").container_id.expect("wrapped");
        assert_eq!(element(&engine, "a").end_binding, Some(id));
    }

    #[test]
    fn the_rectangle_is_in_the_texts_groups_frame_and_angle() {
        let mut text = words("t", 100.0, 100.0, "hello");
        text.group_ids = vec!["g".into()];
        text.frame_id = Some("f".into());
        text.angle = 0.3;
        let mut engine = selected(vec![text], &["t"]);
        engine.wrap_text_in_container();
        let id = element(&engine, "t").container_id.expect("wrapped");
        let container = element(&engine, &id);
        assert_eq!(container.group_ids, ["g"]);
        assert_eq!(container.frame_id.as_deref(), Some("f"));
        assert_eq!(container.angle, 0.3);
    }

    /// Every free text selected gets its own, in one step; a shape selected with them is
    /// left alone.
    #[test]
    fn every_selected_text_is_wrapped_in_one_step() {
        let mut engine = selected(
            vec![
                words("t", 100.0, 100.0, "hello"),
                words("u", 100.0, 300.0, "world"),
                with_id(box_at(600.0, 0.0, 50.0, 50.0), "box"),
            ],
            &["t", "u", "box"],
        );
        engine.wrap_text_in_container();
        assert_eq!(order(&engine).len(), 5);
        assert_eq!(engine.get_selection().len(), 2);
        engine.undo();
        assert_eq!(order(&engine), ["t", "u", "box"]);
        assert_eq!(element(&engine, "t").container_id, None);
    }

    #[test]
    fn it_is_offered_only_for_a_free_text() {
        let mut elements = labelled(box_at(0.0, 0.0, 200.0, 100.0), "box", "l");
        elements.push(with_id(ellipse_at(300.0, 0.0, 50.0, 50.0), "e"));
        let engine = selected(elements, &["box", "l", "e"]);
        assert!(!engine.selection_style().has_free_text);
    }
}
