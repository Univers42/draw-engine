//! The typing session: a text open in the host's editor, laid out on every keystroke and
//! committed once — a port of Excalidraw's `textWysiwyg` session
//! (`packages/excalidraw/wysiwyg/textWysiwyg.tsx@1118751f`) and the `handleTextWysiwyg`
//! the app runs it with (`App.tsx@1118751f:6344-6515`). See `engine/text_session.rs`.

mod common;
use common::*;
use draw_engine::*;

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the element is in the scene")
}

fn find(engine: &DrawEngine, id: &str) -> Option<DrawElement> {
    engine.get_scene().into_iter().find(|el| el.id == id)
}

fn middle(element: &DrawElement) -> (f64, f64) {
    (
        element.x + element.width / 2.0,
        element.y + element.height / 2.0,
    )
}

/// A double click at `at`, and the id of the text it opened.
fn open_at(engine: &mut DrawEngine, at: (f64, f64)) -> String {
    engine.handle_double_click(at.0, at.1);
    engine
        .drain_events()
        .text_edit
        .expect("the double click opens a text")
        .id
}

/// A shape with a label typed and committed into it. Returns the engine and the ids of
/// the shape and the label.
fn labelled(shape: DrawElement, text: &str) -> (DrawEngine, String, String) {
    let shape_id = shape.id.clone();
    let at = middle(&shape);
    let mut engine = engine_with_measure(vec![shape]);
    let label = open_at(&mut engine, at);
    engine.update_text_edit(text);
    engine.commit_text_edit(text, true);
    engine.drain_events();
    (engine, shape_id, label)
}

fn painted(engine: &DrawEngine) -> Vec<String> {
    engine
        .paint_view()
        .elements
        .iter()
        .map(|el| el.id.clone())
        .collect()
}

fn framed(engine: &DrawEngine) -> Vec<String> {
    engine
        .paint_view()
        .selected
        .iter()
        .map(|el| el.id.clone())
        .collect()
}

/// A peer's patch carrying `element` — tombstone included, which `scene_to_json` drops.
fn remote(engine: &mut DrawEngine, element: &DrawElement) {
    let patch = serde_json::json!({ "type": "osidraw", "version": 1, "elements": [element] });
    engine.apply_remote_patch(&patch.to_string());
}

/// Every keystroke lays the text out and leaves the history, the stamps and the host's
/// autosave alone; the commit records it all as one step, one version up.
mod one_step {
    use super::*;

    /// Typed into, a text keeps its stamp and nothing is sent.
    fn type_unsent(engine: &mut DrawEngine, id: &str, typed: &[&str]) {
        let opened = element(engine, id);
        for typed in typed {
            assert!(engine.update_text_edit(typed));
            let events = engine.drain_events();
            assert!(events.scene_delta.is_none() && events.scene_json.is_none());
            let now = element(engine, id);
            assert_eq!(now.original_text.as_deref(), Some(*typed));
            assert_eq!(
                (now.version, now.version_nonce),
                (opened.version, opened.version_nonce)
            );
        }
    }

    #[test]
    fn typing_is_one_step_and_one_stamp_at_the_commit() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        let created = element(&engine, &id).version;
        type_unsent(&mut engine, &id, &["h", "he", "hel", "hello"]);
        engine.commit_text_edit("hello", true);
        let delta = engine
            .drain_events()
            .scene_delta
            .expect("the commit is sent");
        let sent = delta.updated.iter().find(|el| el.id == id).unwrap();
        assert_eq!(
            (sent.text.as_deref(), sent.version),
            (Some("hello"), created)
        );

        engine.select(vec![id.clone()]);
        assert!(engine.edit_selected_text());
        type_unsent(&mut engine, &id, &["hello ", "hello w", "hello world"]);
        engine.commit_text_edit("hello world", true);
        let delta = engine
            .drain_events()
            .scene_delta
            .expect("the commit is sent");
        let sent = delta.updated.iter().find(|el| el.id == id).unwrap();
        assert_eq!(sent.version, created + 1, "one stamp for the whole edit");

        engine.undo();
        assert_eq!(element(&engine, &id).text.as_deref(), Some("hello"));
        engine.undo();
        assert!(element(&engine, &id).is_deleted, "one step made it");
    }

    #[test]
    fn undo_and_redo_wait_for_the_commit() {
        let (mut engine, _, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "hello");
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("hello there");
        engine.undo();
        engine.redo();
        let now = element(&engine, &label);
        assert!(!now.is_deleted);
        assert_eq!(now.original_text.as_deref(), Some("hello there"));
        assert!(engine.text_edit_session().is_some());

        engine.commit_text_edit("hello there", true);
        engine.undo();
        assert_eq!(
            element(&engine, &label).original_text.as_deref(),
            Some("hello")
        );
    }

    /// `set_element_text`, the one-shot write, still commits — and ends a session open
    /// on the same text.
    #[test]
    fn the_one_shot_write_still_commits() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.set_element_text(&id, "whole");
        assert!(engine.text_edit_session().is_none());
        assert!(engine.drain_events().scene_delta.is_some());
        assert_eq!(element(&engine, &id).text.as_deref(), Some("whole"));
    }

    /// Emptied through the one-shot write, a new label goes as it goes from the session:
    /// its shape back as it was, one-line growth included.
    #[test]
    fn the_one_shot_write_empties_a_new_label_as_the_session_does() {
        let shape = filled(box_at(100.0, 100.0, 10.0, 10.0));
        let before = shape.clone();
        let mut engine = engine_with_measure(vec![shape]);
        let label = open_at(&mut engine, (105.0, 105.0));
        engine.set_element_text(&label, "");
        assert!(find(&engine, &label).is_none(), "no tombstone");
        assert_eq!(element(&engine, &before.id), before);
    }
}

/// The editor is the only copy of the text being typed (`Renderer.ts@1118751f:259-267`),
/// and what moves with it is live.
mod painting {
    use super::*;

    #[test]
    fn the_text_being_typed_is_not_painted_and_its_shape_is_live() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "hi");
        let mut arrow = connector(400.0, 150.0, 305.0, 150.0, DrawElementType::Arrow);
        arrow.end_binding = Some(shape.clone());
        let arrow_id = arrow.id.clone();
        let mut scene = engine.get_scene();
        scene.push(arrow);
        engine.set_scene(Scene::new(scene));
        assert!(painted(&engine).contains(&label));

        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("hi\nthere");
        assert!(
            !painted(&engine).contains(&label),
            "painted under the editor"
        );
        assert!(painted(&engine).contains(&shape));
        assert!(framed(&engine).is_empty(), "no frame or handles on it");
        assert_eq!(
            engine.get_selection(),
            vec![label.clone()],
            "still selected"
        );
        let live = engine.debug_live();
        for id in [&label, &shape, &arrow_id] {
            assert!(live.contains(id), "{id} is live");
        }

        engine.commit_text_edit("hi\nthere", false);
        assert!(painted(&engine).contains(&label), "painted again");
        assert!(engine.debug_live().is_empty());
    }
}

/// A label's shape grows as its label is typed and shrinks back as it is deleted, never
/// below its height when the edit began (`textWysiwyg.tsx@1118751f:331-373`).
mod growth {
    use super::*;

    fn grows_and_shrinks_back(shape: DrawElement) {
        let start = shape.height;
        let (mut engine, id, label) = labelled(shape, "a");
        assert_eq!(
            element(&engine, &id).height,
            start,
            "{:?}",
            element(&engine, &id).kind
        );
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());

        let tall = "a\nb\nc\nd\ne\nf\ng\nh";
        engine.update_text_edit(tall);
        let grown = element(&engine, &id).height;
        assert!(grown > start, "grows while typed");
        let text = element(&engine, &label);
        let room =
            draw_engine::text::layout::bound_text_max_height(&element(&engine, &id), text.height);
        assert!(text.height <= room + EPS, "holds what was typed");

        engine.update_text_edit("a\nb\nc\nd");
        let fewer = element(&engine, &id).height;
        assert!(fewer < grown && fewer >= start, "shrinks as lines go");
        engine.update_text_edit("a");
        assert_eq!(
            element(&engine, &id).height,
            start,
            "no further than it was"
        );
        engine.update_text_edit("");
        assert_eq!(element(&engine, &id).height, start);
    }

    #[test]
    fn a_rectangle() {
        grows_and_shrinks_back(box_at(100.0, 100.0, 200.0, 80.0));
    }

    #[test]
    fn an_ellipse() {
        grows_and_shrinks_back(ellipse_at(100.0, 100.0, 200.0, 120.0));
    }

    #[test]
    fn a_diamond() {
        grows_and_shrinks_back(diamond_at(100.0, 100.0, 240.0, 160.0));
    }

    /// A new label's shape too small for one line grows to hold one, from its top-left
    /// (`App.tsx@1118751f:6974-7006`): the widest capital or digit and a line, each with
    /// the padding either side. With the test measure a one-letter line is
    /// `20 · 0.5 + 4 = 14` wide and `20 · 1.25 = 25` tall, so `24 × 35`.
    #[test]
    fn a_new_label_grows_its_shape_to_one_line() {
        let shape = filled(box_at(100.0, 100.0, 10.0, 10.0));
        let id = shape.id.clone();
        let mut engine = engine_with_measure(vec![shape]);
        let label = open_at(&mut engine, (105.0, 105.0));
        assert_eq!(element(&engine, &label).container_id.as_deref(), Some(&*id));
        let grown = element(&engine, &id);
        assert_eq!(
            (grown.x, grown.y, grown.width, grown.height),
            (100.0, 100.0, 24.0, 35.0)
        );
    }
}

/// What the host's editor is placed and styled by.
mod layout {
    use super::*;

    #[test]
    fn the_editor_sits_on_the_elements_box_through_the_camera() {
        let mut shape = box_at(100.0, 100.0, 200.0, 100.0);
        shape.angle = 0.5;
        shape.stroke_color = "#1971c2".into();
        shape.opacity = 60.0;
        let (mut engine, id, label) = labelled(shape, "hello");
        let camera = Camera {
            x: 30.0,
            y: -20.0,
            scale: 2.0,
        };
        engine.set_camera(camera);
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("hello there");
        let text = element(&engine, &label);
        let layout = engine.text_edit_layout().expect("a text is open");
        let at = world_to_screen(camera, text.x, text.y);
        assert_eq!((layout.x, layout.y), (at.x, at.y));
        assert_eq!((layout.width, layout.height), (text.width, text.height));
        assert_eq!(layout.zoom, 2.0);
        assert_eq!(layout.angle, 0.5);
        assert_eq!(layout.font_size, 20.0);
        assert_eq!(layout.line_height, 1.25);
        assert_eq!(layout.color, text.stroke_color);
        assert!((layout.opacity - text.opacity / 100.0).abs() < EPS);
        assert!(layout.wrap, "a label wraps at its shape");
        assert_eq!(layout.container_id.as_deref(), Some(&*id));

        let json = serde_json::to_value(&layout).unwrap();
        for key in [
            "fontSize",
            "lineHeight",
            "fontFamily",
            "textAlign",
            "verticalAlign",
            "containerId",
        ] {
            assert!(json.get(key).is_some(), "{key} in {json}");
        }

        engine.commit_text_edit("hello there", true);
        assert!(engine.text_edit_layout().is_none());
    }

    #[test]
    fn free_text_keeps_its_hard_breaks() {
        let mut engine = engine_with_measure(Vec::new());
        open_at(&mut engine, (300.0, 200.0));
        engine.update_text_edit("one\ntwo");
        let layout = engine.text_edit_layout().unwrap();
        assert!(!layout.wrap && layout.container_id.is_none());
    }
}

/// How an edit ends.
mod ending {
    use super::*;

    /// Escape and Ctrl+Enter leave the shape — or the text — selected; a click away lets
    /// go (`App.tsx@1118751f:6439-6498`); with the tool locked nothing stays selected.
    #[test]
    fn the_keyboard_keeps_the_selection_and_a_click_lets_go() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "a");
        assert_eq!(engine.get_selection(), vec![shape.clone()]);
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.commit_text_edit("b", false);
        assert!(engine.get_selection().is_empty());

        let id = open_at(&mut engine, (500.0, 400.0));
        engine.commit_text_edit("free", true);
        assert_eq!(engine.get_selection(), vec![id]);

        engine.set_tool_locked(true);
        open_at(&mut engine, (500.0, 500.0));
        engine.commit_text_edit("locked", true);
        assert!(engine.get_selection().is_empty());
    }

    /// Made for the edit and left empty, a text was never there: no tombstone, no step,
    /// and a label's shape as it was before it was grown for it.
    #[test]
    fn a_new_text_left_empty_leaves_nothing() {
        let shape = filled(box_at(100.0, 100.0, 10.0, 10.0));
        let before = shape.clone();
        let mut engine = engine_with_measure(vec![shape]);
        let label = open_at(&mut engine, (105.0, 105.0));
        engine.update_text_edit("x");
        engine.commit_text_edit("", true);
        assert!(find(&engine, &label).is_none(), "no tombstone");
        assert_eq!(element(&engine, &before.id), before);
        assert_eq!(engine.get_selection(), vec![before.id.clone()]);

        let free = open_at(&mut engine, (400.0, 300.0));
        engine.commit_text_edit("   ", false);
        assert!(find(&engine, &free).is_none());
        engine.undo();
        assert_eq!(engine.get_scene(), vec![before], "no step to undo");
    }

    /// Emptied, a new label puts back everything its shape moved when it grew to hold
    /// one line: the arrows bound to the shape and their own labels too — no step.
    #[test]
    fn a_new_label_left_empty_puts_back_the_arrows_labels() {
        let shape = filled(box_at(100.0, 100.0, 10.0, 10.0));
        let shape_id = shape.id.clone();
        let mut arrow = connector(300.0, 105.0, 112.5, 105.0, DrawElementType::Arrow);
        arrow.end_binding = Some(shape_id.clone());
        let arrow_id = arrow.id.clone();
        let mut engine = engine_with_measure(vec![shape, arrow]);
        engine.select(vec![arrow_id.clone()]);
        assert!(engine.edit_selected_text());
        let arrow_label = engine.text_edit_session().unwrap().id.clone();
        engine.update_text_edit("arrow label");
        engine.commit_text_edit("arrow label", false);
        engine.drain_events();
        let before = engine.get_scene();

        engine.select(vec![shape_id.clone()]);
        assert!(engine.edit_selected_text());
        let placed = before.iter().find(|el| el.id == arrow_label).unwrap();
        assert_ne!(
            &element(&engine, &arrow_label),
            placed,
            "the grown shape moved it"
        );
        engine.update_text_edit("");
        engine.commit_text_edit("", true);
        assert_eq!(engine.get_scene(), before, "everything as it was");

        engine.undo();
        assert!(
            find(&engine, &arrow_label).is_none_or(|label| label.is_deleted),
            "the undo reached the arrow's label: the empty edit recorded no step"
        );
    }

    /// With the autoshape tool nothing is selected when an edit ends, by the keyboard
    /// or not: the tool stays on through it (`App.tsx@1118751f:6446-6453`).
    #[test]
    fn the_autoshape_tool_selects_nothing() {
        let mut engine = engine_with_measure(vec![box_at(100.0, 100.0, 200.0, 100.0)]);
        engine.set_tool(DrawTool::AutoShape);
        open_at(&mut engine, (500.0, 400.0));
        engine.commit_text_edit("free", true);
        assert!(engine.get_selection().is_empty(), "a free text");
        let label = open_at(&mut engine, (200.0, 150.0));
        assert!(element(&engine, &label).container_id.is_some());
        engine.commit_text_edit("label", true);
        assert!(engine.get_selection().is_empty(), "a label");
    }

    /// A text deleted while open — by a host that ends an edit the old way, with
    /// `delete_selection` — ends its session: nothing is left to type into, and undo and
    /// redo work again.
    #[test]
    fn a_text_deleted_while_open_ends_its_session() {
        let mut engine = engine_with_measure(Vec::new());
        open_at(&mut engine, (400.0, 300.0));
        engine.delete_selection();
        assert!(engine.text_edit_session().is_none(), "a new one");
        assert!(!engine.update_text_edit("x"));

        let id = open_at(&mut engine, (100.0, 100.0));
        engine.commit_text_edit("keep", true);
        assert!(engine.edit_selected_text());
        engine.delete_selection();
        assert!(engine.text_edit_session().is_none(), "an existing one");
        assert!(engine.debug_live().is_empty());
        engine.undo();
        let back = element(&engine, &id);
        assert!(!back.is_deleted, "undo works again");
        assert_eq!(back.text.as_deref(), Some("keep"));
    }

    /// An existing text emptied is deleted: a tombstone, one step, its shape unbound.
    #[test]
    fn an_existing_text_emptied_is_deleted() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "hello");
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("");
        engine.commit_text_edit("", true);
        assert!(element(&engine, &label).is_deleted);
        assert_eq!(element(&engine, &shape).bound_text_id, None);
        assert_eq!(engine.get_selection(), vec![shape.clone()]);
        engine.undo();
        assert!(!element(&engine, &label).is_deleted);
        assert_eq!(
            element(&engine, &shape).bound_text_id.as_deref(),
            Some(&*label)
        );
    }

    /// Opening another text commits the one open: only one is typed at a time.
    #[test]
    fn opening_another_commits_the_first() {
        let mut engine = engine_with_measure(Vec::new());
        let first = open_at(&mut engine, (100.0, 100.0));
        engine.update_text_edit("first");
        let second = open_at(&mut engine, (400.0, 400.0));
        assert_eq!(element(&engine, &first).text.as_deref(), Some("first"));
        assert_eq!(engine.text_edit_session().unwrap().id, second);
        assert_eq!(engine.get_selection(), vec![second]);
    }

    /// A scene replaced under the editor takes the text with it: nothing is left to type
    /// into, and the host's editor closes.
    #[test]
    fn a_scene_replaced_ends_the_session() {
        let mut engine = engine_with_measure(Vec::new());
        open_at(&mut engine, (100.0, 100.0));
        engine.set_scene(Scene::new(Vec::new()));
        assert!(!engine.update_text_edit("x"));
        assert!(engine.text_edit_layout().is_none());
    }
}

/// What peers see and send while a text is typed.
mod peers {
    use super::*;

    #[test]
    fn peers_are_sent_what_is_typed_as_it_is_typed() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "a");
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("typed so far");
        let sent = engine.gesture_elements();
        let text = sent.iter().find(|el| el.id == label).expect("the text");
        assert_eq!(text.original_text.as_deref(), Some("typed so far"));
        assert!(sent.iter().any(|el| el.id == shape), "and its shape");
        engine.commit_text_edit("typed so far", false);
        assert!(engine.gesture_elements().is_empty());
    }

    /// Their copy is refused until the commit, which stamps above it — past a style set
    /// from the panel meanwhile, which commits what was typed so far.
    #[test]
    fn a_peers_copy_is_refused_until_the_commit() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.commit_text_edit("mine", true);
        engine.select(vec![id.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("mine, more");
        engine.set_font_size(28.0);
        let mut theirs = element(&engine, &id);
        theirs.version += 5;
        theirs.text = Some("theirs".into());
        theirs.original_text = Some("theirs".into());
        remote(&mut engine, &theirs);
        assert_eq!(
            element(&engine, &id).original_text.as_deref(),
            Some("mine, more")
        );
        engine.update_text_edit("mine, more still");
        engine.commit_text_edit("mine, more still", true);
        let now = element(&engine, &id);
        assert_eq!(now.original_text.as_deref(), Some("mine, more still"));
        assert!(now.version > theirs.version);
    }

    /// A peer deleting the text meanwhile loses to what was typed, as any edit refused
    /// mid-gesture does — and wins over an edit that came to nothing.
    #[test]
    fn a_remote_delete_loses_to_what_was_typed() {
        for (typed, survives) in [("mine, more", true), ("mine", false)] {
            let mut engine = engine_with_measure(Vec::new());
            let id = open_at(&mut engine, (300.0, 200.0));
            engine.commit_text_edit("mine", true);
            engine.select(vec![id.clone()]);
            assert!(engine.edit_selected_text());
            let mut gone = element(&engine, &id);
            gone.version += 3;
            gone.is_deleted = true;
            remote(&mut engine, &gone);
            assert!(!element(&engine, &id).is_deleted, "refused meanwhile");
            engine.update_text_edit(typed);
            engine.commit_text_edit(typed, true);
            assert_eq!(!element(&engine, &id).is_deleted, survives, "{typed}");
        }
    }

    /// A peer taking the label's shape ends the edit as it ends a drag of it: what was
    /// typed goes back and the editor has nothing left to type into.
    #[test]
    fn a_peer_taking_the_shape_ends_the_edit() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "a");
        let before = (element(&engine, &shape), element(&engine, &label));
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("a\nb\nc\nd\ne\nf");
        engine.set_peers(vec![Peer {
            id: "ana".into(),
            name: "Ana".into(),
            color: "#e03131".into(),
            holds: vec![shape.clone()],
            preview: Vec::new(),
        }]);
        assert!(!engine.update_text_edit("a\nb"));
        assert_eq!((element(&engine, &shape), element(&engine, &label)), before);
    }
}

/// Where a text starts and what a press on one reaches — only the old entry points, so
/// these ran against the engine before the session.
mod entry {
    use super::*;

    /// A new free text's first line is centred on the pointer
    /// (`App.tsx@1118751f:7019-7027`): 20px lines are 25px tall.
    #[test]
    fn a_new_free_text_has_its_first_line_on_the_pointer() {
        let mut engine = engine_with_measure(Vec::new());
        engine.handle_double_click(300.0, 200.0);
        let id = engine.drain_events().text_edit.unwrap().id;
        assert_eq!(element(&engine, &id).y, 200.0 - 12.5);

        engine.set_scene(Scene::new(Vec::new()));
        engine.set_tool(DrawTool::Text);
        engine.begin_pointer(300.0, 200.0, false, false);
        engine.end_pointer();
        let id = engine.drain_events().text_edit.unwrap().id;
        let text = element(&engine, &id);
        assert_eq!((text.x, text.y), (300.0, 200.0 - 12.5));
    }

    /// A click on a label selects its shape: bound text is not hit on its own, a shape is
    /// hit through it (`App.tsx@1118751f:6713-6737`, `:6784-6829`). The shape here has no
    /// fill, so only its outline would be hit without the label.
    #[test]
    fn a_click_on_a_label_selects_its_shape() {
        let shape = box_at(100.0, 100.0, 200.0, 100.0);
        let shape_id = shape.id.clone();
        let mut engine = engine_with_measure(vec![shape]);
        engine.handle_double_click(200.0, 150.0);
        let label = engine.drain_events().text_edit.unwrap().id;
        engine.set_element_text(&label, "hello");
        engine.clear_selection();
        let (x, y) = middle(&element(&engine, &label));
        engine.begin_pointer(x, y, false, false);
        engine.end_pointer();
        assert_eq!(engine.get_selection(), vec![shape_id.clone()]);

        // The context menu's hit, locked shapes included (`openContextMenu`,
        // `App.tsx@1118751f:13276-13279`), finds the shape through its label too.
        engine.clear_selection();
        assert_eq!(
            engine.hit_test(x, y, 4.0).map(|el| el.id),
            Some(shape_id.clone())
        );
        engine.select(vec![shape_id.clone()]);
        engine.toggle_lock_selection();
        assert_eq!(engine.hit_test(x, y, 4.0).map(|el| el.id), Some(shape_id));
    }
}
