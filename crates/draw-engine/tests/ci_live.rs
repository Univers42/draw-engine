//! Which elements the painter draws fresh during a gesture, and when its cached picture
//! of everything else is still good.
//!
//! The painter caches everything outside the live set and draws only the live elements
//! each frame. Two properties keep that honest:
//!
//! - **The live set is complete** — everything the gesture moves is in it. An element
//!   left out would change without the cache knowing, and show at its old place.
//! - **`static_revision` moves exactly when the cached part changes** — not on a live
//!   element's change, which would repaint the whole board every frame of a drag, and
//!   always on anything else's, such as a peer's edit landing mid-drag.

mod common;
use common::*;
use draw_engine::*;

fn live(engine: &DrawEngine) -> Vec<String> {
    let mut ids: Vec<String> = engine.debug_live().into_iter().collect();
    ids.sort();
    ids
}

#[test]
fn nothing_is_live_between_gestures() {
    let engine = engine_with_scene(vec![filled(box_at(100.0, 100.0, 80.0, 60.0))]);
    assert!(live(&engine).is_empty());
}

#[test]
fn a_stroke_being_drawn_is_live_and_the_rest_stays_cached() {
    let mut engine = engine_with_scene(vec![filled(box_at(400.0, 400.0, 80.0, 60.0))]);
    engine.set_tool(DrawTool::Freedraw);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.move_pointer(110.0, 105.0, false, false);
    let stroke = engine.get_scene().last().unwrap().id.clone();
    assert_eq!(live(&engine), vec![stroke]);

    let cached = engine.debug_static_revision();
    for step in 2..20 {
        engine.move_pointer(100.0 + f64::from(step) * 10.0, 105.0, false, false);
    }
    assert_eq!(
        engine.debug_static_revision(),
        cached,
        "only the stroke changed, so the cached picture of the rest is still good"
    );

    engine.end_pointer();
    assert!(
        live(&engine).is_empty(),
        "and the stroke goes into the picture"
    );
}

#[test]
fn dragging_a_shape_makes_its_label_and_its_arrows_live() {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let mut right = filled(box_at(350.0, 0.0, 100.0, 80.0));
    let right_id = right.id.clone();
    let mut label = text_at(370.0, 30.0, 60.0, 20.0);
    label.text = Some("label".into());
    label.container_id = Some(right_id.clone());
    let label_id = label.id.clone();
    right.bound_text_id = Some(label_id.clone());
    let mut engine = engine_with_measure(vec![left, right, label]);
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(50.0, 40.0, false, false);
    engine.move_pointer(200.0, 40.0, false, false);
    engine.move_pointer(400.0, 40.0, false, false);
    engine.end_pointer();
    let arrow = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the arrow was drawn")
        .id;

    engine.select(vec![right_id.clone()]);
    engine.begin_pointer(440.0, 70.0, false, false);
    engine.move_pointer(440.0, 120.0, false, false);

    let mut expected = vec![right_id, label_id, arrow];
    expected.sort();
    assert_eq!(live(&engine), expected);
    engine.end_pointer();
}

#[test]
fn a_peers_edit_mid_drag_invalidates_the_cached_picture() {
    let mine = filled(box_at(100.0, 100.0, 80.0, 60.0));
    let mut theirs = filled(box_at(400.0, 100.0, 80.0, 60.0));
    let mut engine = engine_with_scene(vec![mine, theirs.clone()]);
    engine.begin_pointer(140.0, 130.0, false, false);
    engine.move_pointer(160.0, 150.0, false, false);
    let cached = engine.debug_static_revision();

    theirs.x = 500.0;
    theirs.version = 2;
    engine.apply_remote_patch(&scene_to_json(&[theirs]));

    assert_ne!(engine.debug_static_revision(), cached);
    engine.end_pointer();
}

#[test]
fn an_alt_drag_makes_the_copy_live() {
    let mut engine = engine_with_scene(vec![filled(box_at(100.0, 100.0, 80.0, 60.0))]);
    engine.begin_pointer(140.0, 130.0, false, true);
    engine.move_pointer(200.0, 180.0, false, false);

    let moving = live(&engine);
    assert_eq!(moving.len(), 1);
    let dragged = engine
        .get_scene()
        .into_iter()
        .find(|el| moving.contains(&el.id))
        .expect("the live element exists");
    assert!(
        dragged.x > 150.0,
        "it is the copy being dragged: {}",
        dragged.x
    );
    engine.end_pointer();
}

#[test]
fn a_path_placed_point_by_point_is_live_between_clicks() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Line);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();
    engine.move_pointer(200.0, 150.0, false, false);

    assert_eq!(
        live(&engine).len(),
        1,
        "the path follows the pointer, no button held"
    );
    engine.finish_linear();
    assert!(live(&engine).is_empty());
}
