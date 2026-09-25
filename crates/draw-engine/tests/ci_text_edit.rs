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

/// A style written while a text is typed — a font size chord, a family, any panel row —
/// is written into the text being typed and read back by its editor at once, and stays
/// with the edit: nothing stamped or sent before the commit, which records it all as one
/// step (DECISIONS, wave 3).
mod style_while_typing {
    use super::*;

    /// Nothing leaves the engine and nothing is stamped: the text is as it was committed.
    fn assert_unsent(engine: &mut DrawEngine, id: &str, committed: Option<&DrawElement>) {
        let events = engine.drain_events();
        assert!(
            events.scene_delta.is_none() && events.scene_json.is_none(),
            "nothing is sent before the commit"
        );
        if let Some(committed) = committed {
            let now = element(engine, id);
            assert_eq!(
                (now.version, now.version_nonce),
                (committed.version, committed.version_nonce),
                "nothing is stamped before the commit"
            );
        }
    }

    /// The size stepped twice, a family picked and a colour set: each lands on the text
    /// being typed and in the editor's layout, width included.
    fn restyle(engine: &mut DrawEngine, id: &str, committed: Option<&DrawElement>) {
        let before = element(engine, id);
        for size in [22.0, 24.0] {
            engine.step_font_size(true);
            assert_unsent(engine, id, committed);
            let now = element(engine, id);
            let layout = engine.text_edit_layout().expect("still open");
            assert_eq!(now.font_size, Some(size));
            assert_eq!((layout.font_size, layout.width), (size, now.width));
        }
        assert!(element(engine, id).width > before.width, "wider as it grew");
        engine.set_font_family(8);
        assert_unsent(engine, id, committed);
        let layout = engine.text_edit_layout().expect("still open");
        assert_eq!(
            (layout.font_family, layout.width),
            (8, element(engine, id).width)
        );
        engine.apply_style(DrawElementStylePatch {
            stroke_color: Some("#e03131".into()),
            ..Default::default()
        });
        assert_unsent(engine, id, committed);
        assert_eq!(
            engine.text_edit_layout().expect("still open").color,
            "#e03131"
        );
        assert!(engine.text_edit_session().is_some(), "the edit carries on");
    }

    #[test]
    fn a_new_text_restyled_as_it_is_typed_is_one_step() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.update_text_edit("hello");
        restyle(&mut engine, &id, None);
        engine.update_text_edit("hello there");
        engine.commit_text_edit("hello there", true);
        let made = element(&engine, &id);
        assert_eq!(
            (made.font_size, made.font_family, made.stroke_color.as_str()),
            (Some(24.0), Some(8), "#e03131")
        );
        engine.undo();
        assert!(
            find(&engine, &id).is_none_or(|el| el.is_deleted),
            "one undo takes it all away"
        );
    }

    #[test]
    fn an_existing_text_restyled_as_it_is_typed_is_one_step() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.commit_text_edit("mine", true);
        let committed = element(&engine, &id);
        engine.select(vec![id.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("mine, more");
        engine.drain_events();
        restyle(&mut engine, &id, Some(&committed));
        engine.commit_text_edit("mine, more", true);
        assert!(element(&engine, &id).version > committed.version);
        engine.undo();
        let back = element(&engine, &id);
        assert_eq!(back.original_text.as_deref(), Some("mine"));
        assert_eq!(
            (back.font_size, back.font_family, back.stroke_color),
            (
                committed.font_size,
                committed.font_family,
                committed.stroke_color
            ),
            "one undo puts back the words and the look"
        );
    }

    /// The font picker's hover shows a family on the text being typed, and leaving it
    /// gives back the text as the hover found it — the words typed so far and the size
    /// stepped meanwhile — as the oracle's picker caches the editing text when it opens
    /// (`actionProperties.tsx@1118751f:1484-1499`). Given back to what was committed, the
    /// words went and the text was no longer the edit's: a peer's copy got in.
    #[test]
    fn a_family_hovered_while_typing_gives_back_what_was_typed() {
        for existing in [false, true] {
            let mut engine = engine_with_measure(Vec::new());
            let id = open_at(&mut engine, (300.0, 200.0));
            if existing {
                engine.commit_text_edit("mine", true);
                engine.select(vec![id.clone()]);
                assert!(engine.edit_selected_text());
            }
            engine.update_text_edit("mine, more");
            engine.step_font_size(true);
            let typed = element(&engine, &id);
            engine.preview_font_family(Some(6));
            assert_eq!(element(&engine, &id).font_family, Some(6));
            assert_eq!(engine.text_edit_layout().expect("open").font_family, 6);
            engine.preview_font_family(Some(8));
            engine.preview_font_family(None);
            assert_eq!(element(&engine, &id), typed, "existing: {existing}");

            let mut theirs = typed.clone();
            theirs.version += 5;
            theirs.text = Some("theirs".into());
            theirs.original_text = Some("theirs".into());
            remote(&mut engine, &theirs);
            assert_eq!(
                element(&engine, &id).original_text.as_deref(),
                Some("mine, more"),
                "still the edit's, existing: {existing}"
            );
        }
    }

    /// A family only hovered, never picked, is not what the edit commits. A press on the
    /// board ends the edit before the list closes, so the commit gives the hover back
    /// first, as the oracle's picker puts back what it cached when it closes
    /// (`actionProperties.tsx@1118751f:1483-1500`). Committed instead, the hovered family
    /// was stamped and sent, and the list closing afterwards had nothing to give back.
    #[test]
    fn a_family_hovered_and_never_picked_is_not_committed() {
        for existing in [false, true] {
            let mut engine = engine_with_measure(Vec::new());
            let id = open_at(&mut engine, (300.0, 200.0));
            if existing {
                engine.commit_text_edit("mine", true);
                engine.select(vec![id.clone()]);
                assert!(engine.edit_selected_text());
            }
            engine.update_text_edit("abc");
            let before = element(&engine, &id);
            let look = |el: &DrawElement| (el.font_family, el.line_height);
            engine.preview_font_family(Some(7));
            assert_eq!(element(&engine, &id).font_family, Some(7));
            engine.drain_events();

            engine.commit_text_edit("abc", false);
            let delta = engine
                .drain_events()
                .scene_delta
                .expect("the commit is sent");
            let sent = delta.updated.iter().find(|el| el.id == id).expect("sent");
            assert_eq!(look(sent), look(&before), "sent, existing: {existing}");
            // The list closing afterwards finds nothing left to give back.
            engine.preview_font_family(None);
            let now = element(&engine, &id);
            assert_eq!(look(&now), look(&before), "existing: {existing}");
            assert_eq!(now.original_text.as_deref(), Some("abc"));
        }
    }
}

/// A label typed into a shape and then let go gives the shape back the height it had
/// when the editor opened on it: the oracle's editor caches that height the first time it
/// lays the label out (`textWysiwyg.tsx@1118751f:326-345`), after a new label's shape grew
/// to hold one line (`App.tsx@1118751f:6974-7006`), and "Unbind text" writes it back
/// (`actionBoundText.tsx@1118751f:80-83`, `:105-110`). What typing grew is not kept.
mod unbind {
    use super::*;

    fn height(engine: &DrawEngine, id: &str) -> f64 {
        element(engine, id).height
    }

    #[test]
    fn a_new_label_gives_back_the_height_its_editor_opened_on() {
        let shape = box_at(100.0, 100.0, 200.0, 10.0);
        let shape_id = shape.id.clone();
        let mut engine = engine_with_measure(vec![shape]);
        open_at(&mut engine, (200.0, 105.0));
        let opened = height(&engine, &shape_id);
        assert!(opened > 10.0, "grown to hold one line");
        let typed = "one\ntwo\nthree\nfour\nfive";
        engine.update_text_edit(typed);
        engine.commit_text_edit(typed, true);
        assert!(height(&engine, &shape_id) > opened, "typing grew it");
        engine.select(vec![shape_id.clone()]);
        engine.unbind_text();
        assert_eq!(height(&engine, &shape_id), opened);
    }

    /// Reopened, a label keeps the height remembered the first time.
    #[test]
    fn a_label_typed_into_again_keeps_the_first_height() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "a");
        let first = height(&engine, &shape);
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        let typed = "a\nb\nc\nd\ne\nf\ng";
        engine.update_text_edit(typed);
        engine.commit_text_edit(typed, true);
        assert!(height(&engine, &shape) > first);
        engine.select(vec![shape.clone()]);
        engine.unbind_text();
        assert_eq!(height(&engine, &shape), first);
    }

    /// A new label left empty takes back what it grew its shape by — and what a size
    /// stepped meanwhile made the shape remember: the next label typed there is the one
    /// whose height counts.
    #[test]
    fn a_new_label_left_empty_is_not_remembered() {
        let shape = box_at(100.0, 100.0, 200.0, 10.0);
        let shape_id = shape.id.clone();
        let mut engine = engine_with_measure(vec![shape]);
        open_at(&mut engine, (200.0, 105.0));
        for _ in 0..6 {
            engine.step_font_size(true);
        }
        engine.commit_text_edit("", true);
        assert_eq!(height(&engine, &shape_id), 10.0, "given back");
        open_at(&mut engine, (200.0, 105.0));
        let opened = height(&engine, &shape_id);
        let typed = "one\ntwo\nthree";
        engine.update_text_edit(typed);
        engine.commit_text_edit(typed, true);
        engine.select(vec![shape_id.clone()]);
        engine.unbind_text();
        assert_eq!(height(&engine, &shape_id), opened);
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

    /// Its frame and handles are not drawn, so they offer nothing: where a corner or the
    /// side line of a free text would be, the cursor promises what a press does once the
    /// host has ended the edit — pick the text up, as those lie within a click's reach of
    /// it — and past that reach, inside the box that is not drawn, nothing. A press on a
    /// hidden handle resizes nothing.
    #[test]
    fn the_hidden_frame_offers_nothing() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.update_text_edit("hello world");
        let typed = element(&engine, &id);
        let (right, bottom) = (typed.x + typed.width, typed.y + typed.height);
        let middle = typed.y + typed.height / 2.0;
        let corner = (right + 8.0, bottom + 8.0);
        for (at, name, cursor) in [
            (corner, "corner", HoverCursor::Move),
            ((right + 4.0, middle), "side", HoverCursor::Move),
            ((right + 14.0, middle), "box", HoverCursor::Default),
        ] {
            assert_eq!(engine.hover_cursor(at.0, at.1), cursor, "{name}");
        }

        engine.begin_pointer(corner.0, corner.1, false, false);
        engine.move_pointer(corner.0 + 80.0, corner.1 + 60.0, false, false);
        engine.end_pointer();
        let now = element(&engine, &id);
        assert_eq!(
            (now.font_size, now.width, now.height),
            (typed.font_size, typed.width, typed.height),
            "resized"
        );
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

    /// A size or a family written while the label is typed makes the height its shape has
    /// then the floor it shrinks back to, as the oracle's editor caches the height again
    /// whenever either changes (`textPropertiesUpdated`, `textWysiwyg.tsx@1118751f:246-265`,
    /// `:326-335`) and shrinks only a shape taller than that (`:359-373`) — a size stepped
    /// up keeps what it grew, a family keeps what the typing grew.
    #[test]
    fn a_size_or_family_written_while_typing_is_the_new_floor() {
        for pick_family in [false, true] {
            let (mut engine, id, label) = labelled(box_at(100.0, 100.0, 200.0, 60.0), "a");
            engine.select(vec![label.clone()]);
            assert!(engine.edit_selected_text());
            engine.update_text_edit("a\nb\nc");
            let typed = element(&engine, &id).height;
            if pick_family {
                engine.set_font_family(7);
            } else {
                for _ in 0..4 {
                    engine.step_font_size(true);
                }
                assert!(element(&engine, &id).height > typed, "grown by the size");
            }
            let styled = element(&engine, &id).height;
            assert!(styled > 60.0);
            engine.update_text_edit("a");
            assert_eq!(
                element(&engine, &id).height,
                styled,
                "family: {pick_family}"
            );
            engine.commit_text_edit("a", false);
            assert_eq!(
                element(&engine, &id).height,
                styled,
                "family: {pick_family}"
            );
        }
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

    /// Undo and redo of an edit select what its step began and ended with (`stamp.rs`,
    /// "Undo puts the selection back"): the shape the double click or Enter left
    /// selected — never its label on its own, which the session selects only while it is
    /// open — and, after the keyboard ended it, the shape again, its label emptied or not.
    #[test]
    fn undo_and_redo_of_an_edit_select_the_shape() {
        for (typed, by_enter) in [
            ("hello there", false),
            ("", false),
            ("hi", true),
            ("", true),
        ] {
            let (mut engine, shape, _) = labelled(box_at(100.0, 100.0, 300.0, 100.0), "hello");
            let case = format!("typed {typed:?}, by Enter: {by_enter}");
            if by_enter {
                engine.select(vec![shape.clone()]);
                assert!(engine.edit_selected_text());
            } else {
                engine.clear_selection();
                // A double click as a pointer makes one: two presses, then the event.
                for _ in 0..2 {
                    engine.begin_pointer(250.0, 150.0, false, false);
                    engine.end_pointer();
                }
                engine.handle_double_click(250.0, 150.0);
            }
            assert!(engine.text_edit_session().is_some(), "{case}");
            engine.update_text_edit(typed);
            engine.commit_text_edit(typed, true);
            assert_eq!(engine.get_selection(), vec![shape.clone()], "{case}");
            engine.undo();
            assert_eq!(engine.get_selection(), vec![shape.clone()], "undo, {case}");
            engine.redo();
            assert_eq!(engine.get_selection(), vec![shape.clone()], "redo, {case}");
        }
    }

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
        assert!(engine.get_scene().is_empty(), "no tombstone for a draft");
        assert!(
            !engine.debug_state().scene.can_undo,
            "no step for a text never committed"
        );

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

    /// A label made for a click that never typed into it, deleted through the context
    /// menu while its session is still open — the same `delete_selection` a host used to
    /// end an edit with before the session existed — leaves the shape it was on exactly
    /// as it was: no tombstone, no step, nothing to undo.
    #[test]
    fn a_never_typed_labels_deletion_leaves_no_trace() {
        let shape = filled(box_at(100.0, 100.0, 200.0, 100.0));
        let shape_id = shape.id.clone();
        let mut engine = engine_with_measure(vec![shape]);
        let before = element(&engine, &shape_id);
        open_at(&mut engine, middle(&before));
        engine.delete_selection();
        assert!(engine.text_edit_session().is_none());
        assert_eq!(
            element(&engine, &shape_id),
            before,
            "the shape is untouched"
        );
        assert!(
            !engine.debug_state().scene.can_undo,
            "no step for a label never typed"
        );
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
    /// from the panel meanwhile, which stays with the edit.
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

    /// The label of a shape a peer takes is theirs as well: the edit ends with nothing
    /// selected, so the Delete that follows reaches neither the label nor its shape.
    #[test]
    fn a_peer_taking_the_shape_takes_its_label_out_of_the_selection() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "hello");
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("hello world");
        engine.set_peers(vec![Peer {
            id: "ana".into(),
            name: "Ana".into(),
            color: "#e03131".into(),
            holds: vec![shape.clone()],
            preview: Vec::new(),
        }]);
        assert!(engine.get_selection().is_empty(), "still selected");
        engine.delete_selection();
        assert!(!element(&engine, &label).is_deleted);
        assert_eq!(
            element(&engine, &shape).bound_text_id.as_deref(),
            Some(label.as_str())
        );
    }

    /// A shape a peer takes while its label is typed ends the edit, and a style written
    /// after it reaches neither: the label of a held shape is out of every style's reach
    /// (`style.rs` › `restylable`), the text being typed or not.
    #[test]
    fn a_style_after_a_peer_took_the_shape_reaches_nothing() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "a");
        let before = (element(&engine, &shape), element(&engine, &label));
        engine.select(vec![label.clone()]);
        assert!(engine.edit_selected_text());
        engine.update_text_edit("typed");
        engine.set_peers(vec![Peer {
            id: "ana".into(),
            name: "Ana".into(),
            color: "#e03131".into(),
            holds: vec![shape.clone()],
            preview: Vec::new(),
        }]);
        assert!(engine.text_edit_session().is_none());
        engine.step_font_size(true);
        engine.apply_style(DrawElementStylePatch {
            stroke_color: Some("#e03131".into()),
            ..Default::default()
        });
        assert_eq!((element(&engine, &shape), element(&engine, &label)), before);
    }

    /// A peer's patch landing while a new label is typed leaves it directly above its
    /// shape, where the oracle's fractional index keeps it, and the commit tells the host
    /// so: its copy of the board — the one saved and sent — is stacked as this one is. A
    /// peer's order never lists the label, and the patch's own delta, thrown away, was
    /// where its placement had been noted.
    #[test]
    fn a_new_label_keeps_its_place_through_a_peers_patch() {
        for with_order in [true, false] {
            let shape = box_at(100.0, 100.0, 200.0, 100.0);
            let other = box_at(500.0, 100.0, 100.0, 100.0);
            let (shape_id, other_id) = (shape.id.clone(), other.id.clone());
            let at = middle(&shape);
            let mut engine = engine_with_measure(vec![shape, other.clone()]);
            let label = open_at(&mut engine, at);
            engine.update_text_edit("typed");
            engine.drain_events();

            let mut theirs = other;
            theirs.version += 1;
            theirs.stroke_color = "#e03131".into();
            let mut elements = vec![serde_json::to_value(&theirs).unwrap()];
            let mut placed = vec![shape_id.clone(), label.clone(), other_id.clone()];
            if with_order {
                let new = box_at(700.0, 100.0, 50.0, 50.0);
                placed.push(new.id.clone());
                elements.push(serde_json::to_value(&new).unwrap());
            }
            let mut patch =
                serde_json::json!({ "type": "osidraw", "version": 1, "elements": elements });
            if with_order {
                let listed: Vec<&String> = placed.iter().filter(|id| **id != label).collect();
                patch["order"] = serde_json::json!(listed);
            }
            engine.apply_remote_patch(&patch.to_string());
            let live = |engine: &DrawEngine| -> Vec<String> {
                engine
                    .get_scene()
                    .into_iter()
                    .filter(|el| !el.is_deleted)
                    .map(|el| el.id)
                    .collect()
            };
            assert_eq!(live(&engine), placed, "order: {with_order}");

            engine.commit_text_edit("typed", false);
            let delta = engine
                .drain_events()
                .scene_delta
                .expect("the commit is sent");
            assert_eq!(delta.order, Some(placed), "order: {with_order}");
        }
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

    /// With the grid on, a new free text lands on the grid instead of centring its first
    /// line on the pointer, from the *raw* press either way: a double click reaches
    /// `text_creation_point` with it directly, and the text tool's click carries it
    /// alongside the per-gesture-snapped `start` its draft box drags from
    /// (`TextDraft::press`, `begin_text`) so `end_text` floors the same unsnapped point
    /// `text_creation_point` floors for a double click — the oracle's
    /// `getTextCreationGridPoint` also floors the raw scene point
    /// (`App.tsx@1118751f:1524-1542`). Both entry points land in the same cell.
    #[test]
    fn a_new_free_text_snaps_to_the_grid_when_it_is_on() {
        let mut engine = engine_with_measure(Vec::new());
        engine.set_grid(GridSettings {
            enabled: true,
            size: 20.0,
            step: 5,
            snap: true,
        });

        engine.handle_double_click(103.0, 97.0);
        let id = engine.drain_events().text_edit.unwrap().id;
        assert_eq!(
            (element(&engine, &id).x, element(&engine, &id).y),
            (100.0, 80.0)
        );

        engine.set_scene(Scene::new(Vec::new()));
        engine.set_tool(DrawTool::Text);
        engine.begin_pointer(103.0, 97.0, false, false);
        engine.end_pointer();
        let id = engine.drain_events().text_edit.unwrap().id;
        let text = element(&engine, &id);
        assert_eq!(
            (text.x, text.y),
            (100.0, 80.0),
            "the click floors the same raw press a double click does"
        );
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

    /// A click on a text that is already the sole selection reopens it with the caret at
    /// the click, rather than the whole text selected — `wasAddedToSelection`
    /// (`App.tsx@1118751f:12402-12428`, `textWysiwyg.tsx@1118751f:491-538`). Left-aligned,
    /// so the click's world x maps straight onto the line; the test measurer gives each
    /// of "abc"'s chars a 14-wide advance (`0.5·fontSize + 4`), so the boundaries are at
    /// 0, 14, 28 and 42.
    #[test]
    fn a_click_on_the_sole_selected_text_reopens_it_at_the_caret() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.commit_text_edit("abc", true);
        engine.drain_events();
        assert_eq!(engine.get_selection(), vec![id.clone()], "left selected");

        let text = element(&engine, &id);
        assert_eq!(text.text_align, None, "default free text: resolves to Left");
        engine.begin_pointer(text.x + 25.0, text.y + 5.0, false, false);
        engine.end_pointer();
        let request = engine.drain_events().text_edit.expect("reopened");
        assert_eq!(request.id, id);
        assert_eq!(
            request.caret,
            Some(2),
            "nearest to 25 is the boundary at 28"
        );
    }

    /// The same click, when the text was not already the selection, only selects it — a
    /// second click, now that it is, is what reopens it.
    #[test]
    fn a_click_that_first_selects_a_text_does_not_reopen_it() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.commit_text_edit("abc", true);
        engine.drain_events();
        engine.clear_selection();

        let text = element(&engine, &id);
        engine.begin_pointer(text.x + 5.0, text.y + 5.0, false, false);
        engine.end_pointer();
        assert!(
            engine.drain_events().text_edit.is_none(),
            "selects, does not reopen"
        );
        assert_eq!(engine.get_selection(), vec![id]);
    }

    /// Dragging the sole selected text moves it instead of reopening it: only a press
    /// that never moved reads as the click that does (`App.tsx@1118751f:10926-10929`).
    #[test]
    fn dragging_the_sole_selected_text_moves_it_instead() {
        let mut engine = engine_with_measure(Vec::new());
        let id = open_at(&mut engine, (300.0, 200.0));
        engine.commit_text_edit("abc", true);
        engine.drain_events();

        let text = element(&engine, &id);
        engine.begin_pointer(text.x + 5.0, text.y + 5.0, false, false);
        engine.move_pointer(text.x + 55.0, text.y + 5.0, false, false);
        engine.end_pointer();
        assert!(
            engine.drain_events().text_edit.is_none(),
            "moved, not reopened"
        );
        assert!(element(&engine, &id).x > text.x, "moved instead");
    }

    /// The same click on a label reopens *it*, not the shape — the label stands for its
    /// shape for every other press, but once the shape alone is the selection, a click on
    /// its label is `selectedTextEditingContainer` (`App.tsx@1118751f:12401`).
    #[test]
    fn a_click_on_a_selected_shapes_label_reopens_the_label() {
        let (mut engine, shape, label) = labelled(box_at(100.0, 100.0, 200.0, 100.0), "hello");
        engine.select(vec![shape.clone()]);
        let at = middle(&element(&engine, &label));
        engine.begin_pointer(at.0, at.1, false, false);
        engine.end_pointer();
        let request = engine.drain_events().text_edit.expect("reopened");
        assert_eq!(request.id, label);
        assert!(request.caret.is_some());
    }

    /// A click near a corner of a sole-selected, filled, labelled shape hits the shape —
    /// filled, so its whole interior is a hit target — but is nowhere near the short
    /// label centred in it, so it only selects: `getSelectedTextEditingContainerAtPosition`
    /// requires `getTextElementAtPosition` at the click to resolve to the label itself,
    /// not merely anywhere on its container (`App.tsx@1118751f:6551-6582`).
    #[test]
    fn a_click_near_a_corner_of_a_labelled_shape_does_not_reopen_its_label() {
        let (mut engine, shape, _label) =
            labelled(filled(box_at(100.0, 100.0, 200.0, 100.0)), "hi");
        engine.select(vec![shape.clone()]);

        engine.begin_pointer(105.0, 105.0, false, false);
        engine.end_pointer();
        assert!(
            engine.drain_events().text_edit.is_none(),
            "a corner is not the label"
        );
        assert_eq!(engine.get_selection(), vec![shape]);
    }

    /// The same shape, clicked on its outline away from the label: still only a hit on
    /// the shape, not the label, so still no reopen.
    #[test]
    fn a_click_on_the_outline_away_from_the_label_does_not_reopen_it() {
        let (mut engine, shape, _label) =
            labelled(filled(box_at(100.0, 100.0, 200.0, 100.0)), "hi");
        engine.select(vec![shape.clone()]);

        // Left edge, at the shape's vertical middle — on the outline, but the short
        // label centred in a 200-wide box is nowhere near it horizontally.
        engine.begin_pointer(100.0, 150.0, false, false);
        engine.end_pointer();
        assert!(
            engine.drain_events().text_edit.is_none(),
            "the outline is not the label"
        );
        assert_eq!(engine.get_selection(), vec![shape]);
    }
}
