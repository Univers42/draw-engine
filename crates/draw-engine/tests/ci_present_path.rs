//! The presentation path: each frame's step, written by the path editor in one go.

mod common;
use common::*;
use draw_engine::*;

fn frame_at(x: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Frame,
        Geometry {
            x,
            y: 0.0,
            width: 100.0,
            height: 80.0,
        },
    )
}

fn step(engine: &DrawEngine, id: &str) -> Option<u32> {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element in scene")
        .path_step
}

#[test]
fn the_listed_frames_are_numbered_in_order_as_one_stamped_step() {
    let frames: Vec<_> = (0..3).map(|i| frame_at(f64::from(i) * 200.0)).collect();
    let ids: Vec<String> = frames.iter().map(|f| f.id.clone()).collect();
    let mut engine = engine_with_scene(frames);

    let order = vec![ids[2].clone(), ids[0].clone(), ids[1].clone()];
    engine.set_presentation_path(&order);
    assert_eq!(step(&engine, &ids[2]), Some(0));
    assert_eq!(step(&engine, &ids[0]), Some(1));
    assert_eq!(step(&engine, &ids[1]), Some(2));
    let delta = engine
        .drain_events()
        .scene_delta
        .expect("it syncs to peers and autosave like any edit");
    assert_eq!(delta.updated.len(), 3);

    engine.undo();
    assert!(
        ids.iter().all(|id| step(&engine, id).is_none()),
        "one undo takes the whole order back"
    );
}

#[test]
fn only_frames_whose_step_moved_are_written() {
    let frames: Vec<_> = (0..3).map(|i| frame_at(f64::from(i) * 200.0)).collect();
    let ids: Vec<String> = frames.iter().map(|f| f.id.clone()).collect();
    let mut engine = engine_with_scene(frames);
    engine.set_presentation_path(&ids);
    engine.drain_events();

    // The last two swap; the first keeps step 0 and its stamp.
    engine.set_presentation_path(&[ids[0].clone(), ids[2].clone(), ids[1].clone()]);
    let delta = engine.drain_events().scene_delta.expect("a change");
    let mut written: Vec<_> = delta.updated.iter().map(|el| el.id.clone()).collect();
    written.sort();
    let mut moved = vec![ids[1].clone(), ids[2].clone()];
    moved.sort();
    assert_eq!(written, moved);

    engine.set_presentation_path(&[ids[0].clone(), ids[2].clone(), ids[1].clone()]);
    assert!(
        engine.drain_events().scene_delta.is_none(),
        "the same order again is not a step"
    );
}

#[test]
fn what_is_not_a_live_frame_takes_no_number() {
    let frame = frame_at(0.0);
    let shape = create_element_default(DrawElementType::Rectangle, Geometry::default());
    let (frame_id, shape_id) = (frame.id.clone(), shape.id.clone());
    let mut engine = engine_with_scene(vec![frame, shape]);

    engine.set_presentation_path(&["missing".into(), shape_id.clone(), frame_id.clone()]);
    assert_eq!(step(&engine, &shape_id), None);
    assert_eq!(
        step(&engine, &frame_id),
        Some(0),
        "the first frame listed is step 0"
    );
}

#[test]
fn a_step_survives_the_scene_document() {
    let frame = frame_at(0.0);
    let id = frame.id.clone();
    let mut engine = engine_with_scene(vec![frame]);
    engine.set_presentation_path(std::slice::from_ref(&id));

    let json = engine.export_json();
    assert!(json.contains("\"pathStep\""), "written as pathStep: {json}");
    let back = elements_from_json(&json).expect("reads back");
    assert_eq!(back[0].path_step, Some(0));
    let plain = scene_to_json(&[frame_at(0.0)]);
    assert!(
        !plain.contains("pathStep"),
        "absent on a frame nobody placed"
    );
}

#[test]
fn a_duplicated_frame_starts_off_the_path() {
    let frame = frame_at(0.0);
    let id = frame.id.clone();
    let mut engine = engine_with_scene(vec![frame]);
    engine.set_presentation_path(std::slice::from_ref(&id));

    engine.select(vec![id.clone()]);
    engine.duplicate_selection(10.0, 10.0);
    let copies: Vec<_> = engine
        .get_scene()
        .into_iter()
        .filter(|el| is_frame(el) && el.id != id)
        .collect();
    assert_eq!(copies.len(), 1, "one copy");
    assert_eq!(
        copies[0].path_step, None,
        "the copy is not given the original's step"
    );
    assert_eq!(step(&engine, &id), Some(0), "the original keeps its own");
}
