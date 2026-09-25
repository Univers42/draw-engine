//! Track B: the command palette's "Add rectangle / diamond / ellipse" — a keyboard/palette
//! route to a new shape that never drags a pointer. See `engine/insert.rs`.

mod common;
use common::*;
use draw_engine::*;

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("element exists")
}

#[test]
fn inserting_a_rectangle_centres_it_on_the_point_and_selects_it() {
    let mut engine = engine_with_scene(Vec::new());
    let id = engine
        .insert_default_shape(DrawElementType::Rectangle, 500.0, 300.0)
        .expect("rectangle is a supported kind");

    let rect = element(&engine, &id);
    assert_eq!(rect.kind, DrawElementType::Rectangle);
    assert_close(rect.x + rect.width / 2.0, 500.0);
    assert_close(rect.y + rect.height / 2.0, 300.0);
    assert_eq!(engine.get_selection(), vec![id]);
}

/// `sx`/`sy` are screen coordinates, like `insert_image`/`insert_embed` — a panned,
/// zoomed-in camera must not land the shape where it would under the identity camera the
/// other tests use.
#[test]
fn the_point_is_read_in_screen_space_not_world_space() {
    let mut engine = engine_with_scene(Vec::new());
    engine.set_camera(Camera {
        x: -100.0,
        y: -50.0,
        scale: 2.0,
    });
    // Screen (400, 300) under this camera is world ((400 - -100) / 2, (300 - -50) / 2).
    let id = engine
        .insert_default_shape(DrawElementType::Rectangle, 400.0, 300.0)
        .expect("inserted");
    let rect = element(&engine, &id);
    assert_close(rect.x + rect.width / 2.0, 250.0);
    assert_close(rect.y + rect.height / 2.0, 175.0);
}

#[test]
fn diamond_and_ellipse_are_supported_too() {
    let mut engine = engine_with_scene(Vec::new());
    for kind in [DrawElementType::Diamond, DrawElementType::Ellipse] {
        let id = engine
            .insert_default_shape(kind, 0.0, 0.0)
            .unwrap_or_else(|| panic!("{kind:?} is a supported kind"));
        assert_eq!(element(&engine, &id).kind, kind);
    }
}

#[test]
fn an_unsupported_kind_inserts_nothing() {
    let mut engine = engine_with_scene(Vec::new());
    let before = engine.get_scene().len();
    let id = engine.insert_default_shape(DrawElementType::Text, 0.0, 0.0);
    assert_eq!(id, None);
    assert_eq!(engine.get_scene().len(), before);
}

#[test]
fn insertion_is_one_step_of_undo() {
    let mut engine = engine_with_scene(Vec::new());
    engine
        .insert_default_shape(DrawElementType::Rectangle, 0.0, 0.0)
        .expect("inserted");
    assert!(engine.debug_state().scene.can_undo, "one step recorded");

    engine.undo();
    let live: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|el| !el.is_deleted)
        .collect();
    assert!(live.is_empty(), "the single undo removes it entirely");
    assert!(!engine.debug_state().scene.can_undo, "nothing left to undo");
}

#[test]
fn insertion_takes_the_current_default_style() {
    let mut engine = engine_with_scene(Vec::new());
    engine.set_next_style(DrawElementStylePatch {
        stroke_color: Some("#123456".into()),
        ..Default::default()
    });
    let id = engine
        .insert_default_shape(DrawElementType::Rectangle, 0.0, 0.0)
        .expect("inserted");
    assert_eq!(element(&engine, &id).stroke_color, "#123456");
}
