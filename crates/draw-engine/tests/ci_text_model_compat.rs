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
