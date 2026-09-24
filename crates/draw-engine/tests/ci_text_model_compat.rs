//! The text model's wire format: what the host is sent to open an editor, and what a
//! scene carries once text has a source, a family and a line height of its own.
//!
//! Two promises are pinned here. The editor request is camelCase, like every other
//! struct the host reads — it was snake_case, so the host read `fontSize`, `textAlign`
//! and `containerId` as `undefined` and drew the overlay in the browser's default 13px
//! font, left-aligned, with no wrapping for labels. And a scene saved before the new
//! fields existed comes back out byte for byte: an old board must not change because the
//! engine learned new words.

mod common;
use common::*;
use draw_engine::*;

/// A scene exported by the engine before `originalText`, `fontFamily`, `lineHeight` and
/// `wrap` existed (engine 888669c): a labelled rectangle and a right-aligned fixed-width
/// free text, grouped. Printed by `export_json`, with `autoResize` added by hand in the
/// place the struct puts it.
const LEGACY_SCENE: &str = r##"{
  "type": "osidraw",
  "version": 1,
  "elements": [
    {
      "id": "el-200494509-40788086",
      "type": "rectangle",
      "x": 0.0,
      "y": 0.0,
      "width": 200.0,
      "height": 100.0,
      "angle": 0.0,
      "strokeColor": "#1e1e1e",
      "backgroundColor": "transparent",
      "fillStyle": "hachure",
      "strokeWidth": 2.0,
      "strokeStyle": "solid",
      "roughness": 1.0,
      "opacity": 100.0,
      "roundness": 8.0,
      "seed": 1703960886,
      "boundTextId": "el-566577900-1316748153",
      "groupIds": [
        "el-1434828149-1802586959"
      ],
      "version": 3,
      "versionNonce": 1219402265,
      "updated": 0.0,
      "isDeleted": false
    },
    {
      "id": "el-566577900-1316748153",
      "type": "text",
      "x": 8.0,
      "y": 8.0,
      "width": 184.0,
      "height": 25.0,
      "angle": 0.0,
      "strokeColor": "#1e1e1e",
      "backgroundColor": "transparent",
      "fillStyle": "hachure",
      "strokeWidth": 2.0,
      "strokeStyle": "solid",
      "roughness": 1.0,
      "opacity": 100.0,
      "roundness": 8.0,
      "seed": 1458107087,
      "text": "hello world",
      "fontSize": 20.0,
      "verticalAlign": "top",
      "containerId": "el-200494509-40788086",
      "groupIds": [
        "el-1434828149-1802586959"
      ],
      "version": 4,
      "versionNonce": 1828741485,
      "updated": 0.0,
      "isDeleted": false
    },
    {
      "id": "el-1456938755-765923837",
      "type": "text",
      "x": 400.0,
      "y": 300.0,
      "width": 120.0,
      "height": 50.0,
      "angle": 0.0,
      "strokeColor": "#1e1e1e",
      "backgroundColor": "transparent",
      "fillStyle": "hachure",
      "strokeWidth": 2.0,
      "strokeStyle": "solid",
      "roughness": 1.0,
      "opacity": 100.0,
      "roundness": 8.0,
      "seed": 1446740126,
      "text": "free\ntext",
      "fontSize": 20.0,
      "textAlign": "right",
      "verticalAlign": "top",
      "autoResize": false,
      "groupIds": [
        "el-1434828149-1802586959"
      ],
      "version": 4,
      "versionNonce": 439458543,
      "updated": 0.0,
      "isDeleted": false
    }
  ]
}"##;

/// The request the engine sends when a label editor opens, as the JSON the host parses.
fn label_edit_request() -> serde_json::Value {
    let rect = box_at(0.0, 0.0, 200.0, 100.0);
    let mut engine = engine_with_measure(vec![rect]);
    engine.handle_double_click(100.0, 50.0);
    let request = engine
        .drain_events()
        .text_edit
        .expect("double-clicking a shape opens its label editor");
    serde_json::to_value(&request).expect("the request serialises")
}

#[test]
fn the_text_edit_request_is_camel_case() {
    let json = label_edit_request();
    let object = json.as_object().expect("an object");
    // The three the host reads by these exact names (engine/src/types.ts TextEditRequest).
    assert_eq!(object.get("fontSize"), Some(&serde_json::json!(20.0)));
    assert_eq!(object.get("textAlign"), Some(&serde_json::json!("center")));
    assert!(
        object.get("containerId").is_some_and(|id| id.is_string()),
        "a label's request names its container: {json}"
    );
    let snake: Vec<&String> = object.keys().filter(|key| key.contains('_')).collect();
    assert!(snake.is_empty(), "snake_case keys {snake:?} in {json}");
}

#[test]
fn a_legacy_scene_round_trips_byte_identical() {
    let mut engine = DrawEngine::new();
    assert!(engine.load_scene(LEGACY_SCENE));
    assert_eq!(engine.export_json(), LEGACY_SCENE);
}

#[test]
fn a_legacy_text_resolves_exactly_as_before() {
    let elements = elements_from_json(LEGACY_SCENE).expect("the legacy scene parses");
    for text in elements
        .iter()
        .filter(|el| el.kind == DrawElementType::Text)
    {
        assert_eq!(text.original_text, None);
        assert_eq!(text.font_family, None);
        assert_eq!(text.line_height, None);
        assert_eq!(text.wrap, None);
        // No source of its own: the source is what is drawn.
        assert_eq!(source_text(text), text.text.as_deref().unwrap());
        // No family: today's system stack, at today's line height.
        assert_eq!(resolved_font_family(text), None);
        assert_eq!(resolved_line_height(text), TEXT_LINE_HEIGHT);
    }
}

/// A label wrapped inside its shape, carrying every new field.
fn modern_label() -> DrawElement {
    let mut label = text_at(8.0, 8.0, 60.0, 50.0);
    label.text = Some("hello\nworld".into());
    label.original_text = Some("hello world".into());
    label.font_family = Some(5);
    label.line_height = Some(1.15);
    label.wrap = Some(false);
    label.container_id = Some("el-box".into());
    label
}

#[test]
fn the_new_text_fields_round_trip_under_their_wire_names() {
    let label = modern_label();
    let json = scene_to_json(std::slice::from_ref(&label));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let wire = &value["elements"][0];
    assert_eq!(wire["originalText"], "hello world");
    assert_eq!(wire["fontFamily"], 5);
    assert_eq!(wire["lineHeight"], 1.15);
    assert_eq!(wire["wrap"], false);

    let mut engine = DrawEngine::new();
    assert!(engine.load_scene(&json));
    assert_eq!(engine.get_scene(), vec![label]);
    assert_eq!(engine.export_json(), json, "a second trip changes nothing");
}

#[test]
fn source_text_prefers_the_text_as_typed() {
    let label = modern_label();
    assert_eq!(source_text(&label), "hello world");

    let mut blank = text_at(0.0, 0.0, 10.0, 10.0);
    blank.text = None;
    assert_eq!(source_text(&blank), "");
}

#[test]
fn each_known_family_resolves_with_the_oracles_line_height() {
    // `packages/common/src/constants.ts:133-144` for the ids,
    // `packages/common/src/font-metadata.ts:35-104` for the line heights.
    let families = [
        (1, 1.25), // Virgil
        (2, 1.15), // Helvetica
        (3, 1.2),  // Cascadia
        (5, 1.25), // Excalifont
        (6, 1.25), // Nunito
        (7, 1.15), // Lilita One
        (8, 1.25), // Comic Shanns
        (9, 1.15), // Liberation Sans
    ];
    for (id, line_height) in families {
        let mut text = text_at(0.0, 0.0, 10.0, 10.0);
        text.font_family = Some(id);
        assert_eq!(resolved_font_family(&text), Some(id), "family {id}");
        assert_eq!(resolved_line_height(&text), line_height, "family {id}");
    }
}

#[test]
fn an_explicit_line_height_wins_over_the_familys() {
    let mut text = text_at(0.0, 0.0, 10.0, 10.0);
    text.font_family = Some(3);
    text.line_height = Some(2.0);
    assert_eq!(resolved_line_height(&text), 2.0);
    text.font_family = None;
    assert_eq!(resolved_line_height(&text), 2.0);
}

/// Values the engine cannot draw with are ignored, never clamped: an unknown family is
/// drawn with the system stack, an out-of-range line height is the family's own. An id
/// the contract allows but the engine does not know (4, 10, 64) is kept as it came, so a
/// newer client's font survives a trip through this one; what the contract refuses never
/// gets in (`a_value_the_contract_refuses_is_dropped_where_it_comes_in`).
#[test]
fn odd_values_are_ignored_not_trusted() {
    for id in [0, 4, 10, 64, 99, 255] {
        let mut text = text_at(0.0, 0.0, 10.0, 10.0);
        text.font_family = Some(id);
        assert_eq!(resolved_font_family(&text), None, "family {id}");
        assert_eq!(resolved_line_height(&text), TEXT_LINE_HEIGHT, "family {id}");
    }
    for line_height in [
        0.0,
        -1.0,
        0.49,
        4.01,
        1e308,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let mut text = text_at(0.0, 0.0, 10.0, 10.0);
        text.font_family = Some(7);
        text.line_height = Some(line_height);
        assert_eq!(
            resolved_line_height(&text),
            1.15,
            "line height {line_height}"
        );
    }
}

#[test]
fn an_unknown_family_loads_edits_and_exports_without_panicking() {
    let mut label = modern_label();
    label.font_family = Some(10);
    label.line_height = Some(4.0);
    let json = scene_to_json(std::slice::from_ref(&label));

    let mut engine = engine_with_measure(vec![]);
    assert!(engine.load_scene(&json));
    assert_eq!(engine.export_json(), json, "kept as it came");
    assert!(engine.export_svg(10.0).is_some());
    engine.select(vec![label.id.clone()]);
    assert!(engine.edit_selected_text());
    engine.set_element_text(&label.id, "still editable");
    engine.set_font_size(28.0);
}

/// The label of [`LEGACY_SCENE`] carrying `key: raw`, as a file or a peer could send it.
fn legacy_scene_with(key: &str, raw: &str) -> String {
    LEGACY_SCENE.replacen(
        "\"fontSize\": 20.0,",
        &format!("\"fontSize\": 20.0, \"{key}\": {raw},"),
        1,
    )
}

/// What a text may carry is the contract's rule (`packages/contract/src/element.ts`:
/// `fontFamily` an integer in 1..=64, `lineHeight` a number in 0.5..=4). A value outside
/// it can still come from a file or a peer, and the server refuses every patch that
/// carries one, so a board holding it would never save again. It is dropped where it
/// comes in, as these keys were dropped before the engine knew them, and never refuses
/// the rest of the document.
#[test]
fn a_value_the_contract_refuses_is_dropped_where_it_comes_in() {
    let label_id = "el-566577900-1316748153";
    let cases: [(&str, &str, Option<serde_json::Value>); 20] = [
        ("fontFamily", "99", None),
        ("fontFamily", "1", Some(serde_json::json!(1))),
        ("fontFamily", "64", Some(serde_json::json!(64))),
        ("fontFamily", "5.0", Some(serde_json::json!(5))),
        ("fontFamily", "0", None),
        ("fontFamily", "65", None),
        ("fontFamily", "255", None),
        ("fontFamily", "300", None),
        ("fontFamily", "-1", None),
        ("fontFamily", "1.5", None),
        ("fontFamily", "\"5\"", None),
        ("fontFamily", "null", None),
        ("lineHeight", "0.5", Some(serde_json::json!(0.5))),
        ("lineHeight", "4", Some(serde_json::json!(4.0))),
        ("lineHeight", "0.49", None),
        ("lineHeight", "4.01", None),
        ("lineHeight", "0.01", None),
        ("lineHeight", "-1", None),
        ("lineHeight", "\"1.2\"", None),
        ("lineHeight", "null", None),
    ];
    for (key, raw, kept) in cases {
        let json = legacy_scene_with(key, raw);

        // A file, or the board the server sends.
        let mut engine = DrawEngine::new();
        assert!(engine.load_scene(&json), "{key}: {raw} refused the scene");
        let exported: serde_json::Value = serde_json::from_str(&engine.export_json()).unwrap();
        let label = exported["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|el| el["id"] == label_id)
            .expect("the label loads");
        assert_eq!(label.get(key), kept.as_ref(), "{key}: {raw} from a file");

        // A peer's patch, which is read element by element.
        let mut engine = DrawEngine::new();
        assert!(engine.apply_remote_patch(&json), "{key}: {raw} from a peer");
        let label = engine
            .get_scene()
            .into_iter()
            .find(|el| el.id == label_id)
            .expect("the peer's label arrives");
        let wire = serde_json::to_value(&label).unwrap();
        assert_eq!(wire.get(key), kept.as_ref(), "{key}: {raw} from a peer");
    }
}

/// The oracle's `originalText || text` (`packages/excalidraw/data/restore.ts:572`): an
/// empty string is no source, and must not hide the text that is drawn.
#[test]
fn an_empty_source_falls_back_to_the_text_drawn() {
    let mut label = modern_label();
    label.original_text = Some(String::new());
    assert_eq!(source_text(&label), "hello\nworld");
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the element is in the scene")
}

/// A free text a newer client wrapped: `text` has the soft break baked in, and
/// `original_text` is what was typed. Fixed width, so it wraps.
fn wrapped_column() -> DrawElement {
    let mut text = text_at(0.0, 0.0, 60.0, 50.0);
    text.text = Some("hello\nworld".into());
    text.original_text = Some("hello world".into());
    text.auto_resize = Some(false);
    text
}

/// An edit is typed over the source, so it becomes the source. Left as it was, the
/// source would still read what the text said before the edit, and a client laying text
/// out from it would put the old words back.
#[test]
fn an_edit_keeps_the_source_in_step() {
    let column = wrapped_column();
    let mut engine = engine_with_measure(vec![column.clone()]);
    engine.set_element_text(&column.id, "goodbye");
    let edited = element(&engine, &column.id);
    // Drawn wrapped to the column, kept as typed. The break is the ported `wrapWord`'s
    // (`crate::text`): it sums each char measured alone, 14 under this hook (10 + its 4),
    // so four fit in 60.
    assert_eq!(edited.text.as_deref(), Some("good\nbye"));
    assert_eq!(edited.original_text.as_deref(), Some("goodbye"));
    assert_eq!(source_text(&edited), "goodbye");

    // A text saved before sources existed gains one when it is edited: every writer
    // keeps the two in step, as the oracle's `originalText` always is
    // (`newElement.ts@1118751f` `updateTextElement`). Left without, the next layout — a
    // font change, a resize of its shape — would re-wrap the drawn lines instead.
    // Loaded and not edited, it keeps exactly the fields it came with.
    let mut legacy = text_at(0.0, 200.0, 60.0, 25.0);
    legacy.text = Some("hello".into());
    let mut engine = engine_with_measure(vec![legacy.clone()]);
    assert_eq!(element(&engine, &legacy.id).original_text, None);
    engine.set_element_text(&legacy.id, "goodbye");
    assert_eq!(
        element(&engine, &legacy.id).original_text.as_deref(),
        Some("goodbye")
    );
}

/// The editor opens on what was typed, as the oracle's does
/// (`packages/excalidraw/wysiwyg/textWysiwyg.tsx:488`). Opened on `text`, every soft
/// break a newer client wrapped at would come back from the edit as a hard one.
#[test]
fn the_editor_opens_on_the_source() {
    let column = wrapped_column();
    let mut engine = engine_with_measure(vec![column.clone()]);
    engine.select(vec![column.id.clone()]);
    assert!(engine.edit_selected_text());
    let request = engine.drain_events().text_edit.expect("the editor opens");
    assert_eq!(request.text, "hello world");
}

/// A column given a new width re-wraps what was typed, not what was drawn at the old
/// width: the soft break goes where the new width puts it, and the source is untouched.
#[test]
fn a_resized_column_rewraps_its_source() {
    let column = wrapped_column();
    let mut engine = engine_with_measure(vec![column.clone()]);
    engine.set_text_box_width(&column.id, 1000.0);
    let resized = element(&engine, &column.id);
    assert_eq!(resized.text.as_deref(), Some("hello world"));
    assert_eq!(resized.original_text.as_deref(), Some("hello world"));
}

/// The editor is sent the box the canvas lays the label out in, so the text wraps at the
/// same width on both and does not jump when the edit is committed. An arrow's label is
/// as wide as its text, centred on the arrow's middle, while its lines wrap at the
/// oracle's arrow width — `max(0.7 × width, 11 × fontSize)` (`getBoundTextMaxWidth`,
/// `textElement.ts@1118751f:515-520`): sent the label's own box, the editor wrapped
/// every word onto a line of its own and the canvas drew them on one.
#[test]
fn an_arrow_labels_editor_is_the_box_its_lines_wrap_in() {
    let arrow = connector(0.0, 0.0, 400.0, 0.0, DrawElementType::Arrow);
    let mut engine = engine_with_measure(vec![arrow.clone()]);
    engine.select(vec![arrow.id.clone()]);
    assert!(engine.edit_selected_text());
    let request = engine
        .drain_events()
        .text_edit
        .expect("the label editor opens");

    let wrap = (0.7_f64 * 400.0).max(11.0 * 20.0);
    assert_close(request.width.expect("a label has a width"), wrap);
    // Centred where the canvas centres the label's lines: the arrow's middle.
    let middle = world_to_screen(engine.camera, 200.0, 0.0);
    assert_close(request.x + wrap / 2.0, middle.x);

    // And what is committed wraps at exactly that width: a line that fits it stays one.
    let line = "the quick brown fox jumps";
    assert!(measure_text(line, 20.0).0 <= wrap);
    engine.set_element_text(&request.id, line);
    let label = element(&engine, &request.id);
    assert_eq!(label.text.as_deref(), Some(line));
}

/// A shape's label is edited in the box its lines wrap in: the shape less its padding
/// (`getContainerCoords` / `getBoundTextMaxWidth`, `textElement.ts@1118751f:396-417`,
/// `:511-540`), whatever the width of the label's own text.
#[test]
fn a_shapes_label_editor_is_its_padded_box() {
    let rect = box_at(0.0, 0.0, 40.0, 120.0);
    let mut engine = engine_with_measure(vec![rect.clone()]);
    engine.select(vec![rect.id.clone()]);
    assert!(engine.edit_selected_text());
    let request = engine
        .drain_events()
        .text_edit
        .expect("the label editor opens");
    let label = element(&engine, &request.id);
    assert_close(request.width.unwrap(), 40.0 - 2.0 * LABEL_PADDING);
    assert_close(
        request.x,
        world_to_screen(engine.camera, LABEL_PADDING, label.y).x,
    );
}
