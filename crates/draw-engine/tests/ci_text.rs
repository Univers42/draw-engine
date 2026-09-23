mod common;
use common::*;
use draw_engine::*;

#[test]
fn double_click_empty_canvas_creates_text() {
    let mut engine = engine_with_measure(vec![]);
    engine.handle_double_click(100.0, 150.0);
    let events = engine.drain_events();
    assert!(events.text_edit.is_some());
    assert_eq!(engine.get_scene().len(), 1);
    assert_eq!(engine.get_scene()[0].kind, DrawElementType::Text);
}

#[test]
fn double_click_existing_text_starts_edit() {
    let text = text_at(50.0, 50.0, 100.0, 30.0);
    let mut engine = engine_with_measure(vec![text.clone()]);
    engine.handle_double_click(60.0, 60.0);
    let events = engine.drain_events();
    assert!(events.text_edit.is_some());
    assert_eq!(events.text_edit.unwrap().id, text.id);
}

#[test]
fn double_click_box_creates_bound_label() {
    let rect = box_at(0.0, 0.0, 120.0, 80.0);
    let mut engine = engine_with_measure(vec![rect.clone()]);
    engine.handle_double_click(60.0, 40.0);
    let scene = engine.get_scene();
    assert_eq!(scene.len(), 2);
    let label = scene
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .unwrap();
    let host = scene.iter().find(|e| e.id == rect.id).unwrap();
    assert_eq!(label.container_id.as_deref(), Some(rect.id.as_str()));
    assert_eq!(host.bound_text_id.as_deref(), Some(label.id.as_str()));
}

#[test]
fn double_click_ellipse_creates_bound_label() {
    let el = ellipse_at(0.0, 0.0, 100.0, 100.0);
    let mut engine = engine_with_measure(vec![el.clone()]);
    engine.handle_double_click(50.0, 50.0);
    let scene = engine.get_scene();
    let label = scene
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .unwrap();
    assert_eq!(label.container_id.as_deref(), Some(el.id.as_str()));
}

#[test]
fn double_click_diamond_creates_bound_label() {
    let dia = diamond_at(0.0, 0.0, 100.0, 100.0);
    let mut engine = engine_with_measure(vec![dia.clone()]);
    engine.handle_double_click(50.0, 50.0);
    let scene = engine.get_scene();
    let label = scene
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .unwrap();
    assert_eq!(label.container_id.as_deref(), Some(dia.id.as_str()));
}

#[test]
fn edit_selected_text_with_selection() {
    let text = text_at(0.0, 0.0, 20.0, 20.0);
    let mut engine = engine_with_measure(vec![text.clone()]);
    engine.select(vec![text.id]);
    assert!(engine.edit_selected_text());
}

#[test]
fn edit_selected_text_without_selection_returns_false() {
    let text = text_at(0.0, 0.0, 20.0, 20.0);
    let mut engine = engine_with_measure(vec![text]);
    assert!(!engine.edit_selected_text());
}

#[test]
fn edit_selected_text_on_box_creates_label_if_missing() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut engine = engine_with_measure(vec![rect.clone()]);
    engine.select(vec![rect.id]);
    assert!(engine.edit_selected_text());
    assert_eq!(engine.get_scene().len(), 2);
}

#[test]
fn set_element_text_multiline_increases_height() {
    let mut label = text_at(0.0, 0.0, 50.0, 20.0);
    label.font_size = Some(20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "line 1\nline 2\nline 3\nline 4");

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == label.id)
        .unwrap();
    assert!(updated.height >= 4.0 * 20.0 * TEXT_LINE_HEIGHT);
}

#[test]
fn set_element_text_single_line() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "Quick brown fox");

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == label.id)
        .unwrap();
    assert_eq!(updated.text.as_deref(), Some("Quick brown fox"));
}

#[test]
fn set_element_text_unicode_emojis() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "Hello 👋 World 🌍");

    let updated = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == label.id)
        .unwrap();
    assert_eq!(updated.text.as_deref(), Some("Hello 👋 World 🌍"));
}

#[test]
fn set_element_text_empty_string_deletes_label() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let mut label = text_at(0.0, 0.0, 50.0, 20.0);
    label.container_id = Some(rect.id.clone());

    let mut engine = engine_with_measure(vec![rect.clone(), label.clone()]);
    engine.set_element_text(&label.id, "");

    let live: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .collect();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, rect.id);
    assert!(live[0].bound_text_id.is_none());
}

/// A committed text emptied is deleted, not dropped: the tombstone is what tells the
/// server and the peers, and what undo stamps the text back above.
#[test]
fn emptying_a_committed_text_leaves_a_tombstone() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);

    engine.set_element_text(&label.id, "");

    let tombstone = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == label.id)
        .expect("a tombstone");
    assert!(tombstone.is_deleted);
    assert_eq!(tombstone.version, label.version + 1);
}

#[test]
fn set_element_text_whitespace_only_deletes_label() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text(&label.id, "   \n\t  ");
    assert!(engine.get_scene().iter().all(|e| e.is_deleted));
}

#[test]
fn set_element_text_non_existent_id_is_noop() {
    let label = text_at(0.0, 0.0, 50.0, 20.0);
    let mut engine = engine_with_measure(vec![label.clone()]);
    engine.set_element_text("ghost_id", "nothing happens");
    assert_eq!(engine.get_scene().len(), 1);
}

#[test]
fn measure_text_empty_string_min_bounds() {
    let (w, h) = measure_text("", 20.0);
    assert!(w >= 4.0);
    assert!(h >= 20.0);
}

#[test]
fn measure_text_long_line_expands_width() {
    let (short_w, _) = measure_text("Hi", 20.0);
    let (long_w, _) = measure_text("This is a significantly longer string of text", 20.0);
    assert!(long_w > short_w);
}

/// Text is measured over **characters**, not bytes.
///
/// `str::len()` is the UTF-8 byte length, and the old estimate used it: "café" came out a
/// fifth too wide, "日本語" three times too wide, and every emoji four times. Everything
/// downstream inherits the error — the selection frame, the hit test, where the editing
/// overlay sits, how a bound label is laid out, and the exported SVG.
///
/// In the browser this estimate is replaced by a real `measureText` against the font the
/// painter draws with; this covers the host fallback, which has to be sane on its own.
#[test]
fn the_host_estimate_counts_characters_not_bytes() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);

    let width_of = |engine: &mut DrawEngine, text: &str| {
        let mut el = create_element_default(
            DrawElementType::Text,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 20.0,
            },
        );
        el.id = "t".into();
        el.font_size = Some(20.0);
        el.text = Some(String::new());
        engine.set_scene(Scene::new(vec![el]));
        engine.set_element_text("t", text);
        engine
            .get_scene()
            .into_iter()
            .find(|e| e.id == "t")
            .map(|e| e.width)
            .unwrap_or(0.0)
    };

    // Same number of characters, wildly different byte counts.
    let ascii = width_of(&mut engine, "abcd");
    let accented = width_of(&mut engine, "café");
    let cjk = width_of(&mut engine, "日本語で");
    assert_close(ascii, accented);
    assert_close(ascii, cjk);

    // And twice the characters is twice the width.
    assert_close(width_of(&mut engine, "abcdabcd"), ascii * 2.0);
}

/// An empty line still occupies a line's height, so a blank line in the middle of a
/// paragraph does not collapse.
#[test]
fn blank_lines_keep_their_height() {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    let mut el = create_element_default(
        DrawElementType::Text,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 20.0,
        },
    );
    el.id = "t".into();
    el.font_size = Some(20.0);
    el.text = Some(String::new());
    engine.set_scene(Scene::new(vec![el]));

    engine.set_element_text("t", "one\n\nthree");
    let height = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == "t")
        .map(|e| e.height)
        .unwrap();
    assert_close(height, 3.0 * 20.0 * TEXT_LINE_HEIGHT);
}
