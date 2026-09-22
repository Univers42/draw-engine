//! Merging a peer's edits into the scene.
//!
//! These exist because the absence of them cost a real board. Remote patches were fed
//! through `paste_json`, which mints a fresh id for every element — so each incoming
//! edit arrived as a *duplicate* rather than an update — and the resulting change was
//! then broadcast back, so two clients grew the document without bound. A four-element
//! board reached 8,273 elements and three frames a second.

mod common;

use common::engine_with_scene;
use draw_engine::{DrawElement, DrawElementType};

fn scene_json(elements: &[DrawElement]) -> String {
    draw_engine::scene_to_json(elements)
}

fn element(id: &str, x: f64, version: u32, nonce: u32) -> DrawElement {
    let mut e = common::box_at(0.0, 0.0, 100.0, 60.0);
    e.id = id.to_string();
    e.x = x;
    e.version = version;
    e.version_nonce = nonce;
    e
}

#[test]
fn a_remote_edit_updates_in_place_instead_of_duplicating() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1)]);

    let changed = engine.apply_remote_patch(&scene_json(&[element("a", 500.0, 2, 1)]));

    assert!(changed);
    let scene = engine.get_scene();
    assert_eq!(scene.len(), 1, "the element was merged, not copied");
    assert_eq!(scene[0].id, "a", "and it kept its id");
    assert_eq!(scene[0].x, 500.0);
}

#[test]
fn an_unseen_element_is_added() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1)]);
    engine.apply_remote_patch(&scene_json(&[element("b", 10.0, 1, 1)]));

    let ids: Vec<String> = engine.get_scene().into_iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"b".to_string()));
}

#[test]
fn a_stale_edit_is_ignored() {
    let mut engine = engine_with_scene(vec![element("a", 500.0, 5, 1)]);

    let changed = engine.apply_remote_patch(&scene_json(&[element("a", 0.0, 2, 1)]));

    assert!(!changed, "an older version must not overwrite a newer one");
    assert_eq!(engine.get_scene()[0].x, 500.0);
}

/// The tie-break has to be total, or two clients can disagree forever.
#[test]
fn equal_versions_are_broken_by_nonce() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 3, 10)]);

    engine.apply_remote_patch(&scene_json(&[element("a", 111.0, 3, 5)]));
    assert_eq!(engine.get_scene()[0].x, 0.0, "lower nonce loses");

    engine.apply_remote_patch(&scene_json(&[element("a", 222.0, 3, 99)]));
    assert_eq!(engine.get_scene()[0].x, 222.0, "higher nonce wins");
}

/// Order of arrival must not change the outcome — that is what makes the rule a
/// convergent merge rather than a race.
#[test]
fn the_result_does_not_depend_on_arrival_order() {
    let patches = [
        element("a", 1.0, 2, 1),
        element("a", 2.0, 5, 1),
        element("a", 3.0, 3, 9),
    ];

    let mut forward = engine_with_scene(vec![element("a", 0.0, 1, 1)]);
    for p in &patches {
        forward.apply_remote_patch(&scene_json(std::slice::from_ref(p)));
    }

    let mut reverse = engine_with_scene(vec![element("a", 0.0, 1, 1)]);
    for p in patches.iter().rev() {
        reverse.apply_remote_patch(&scene_json(std::slice::from_ref(p)));
    }

    assert_eq!(forward.get_scene()[0].x, reverse.get_scene()[0].x);
    assert_eq!(
        forward.get_scene()[0].x,
        2.0,
        "the highest version wins either way"
    );
}

/// Applying the same patch twice must be a no-op. Retries and reconnects replay.
#[test]
fn applying_the_same_patch_twice_changes_nothing() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1)]);
    let patch = scene_json(&[element("a", 500.0, 2, 1)]);

    assert!(engine.apply_remote_patch(&patch));
    assert!(
        !engine.apply_remote_patch(&patch),
        "the second is redundant"
    );
    assert_eq!(engine.get_scene().len(), 1);
}

/// A remote edit is not a step in *your* undo stack.
#[test]
fn a_remote_edit_does_not_enter_the_undo_history() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1)]);
    engine.apply_remote_patch(&scene_json(&[element("a", 500.0, 2, 1)]));

    engine.undo();
    assert_eq!(
        engine.get_scene()[0].x,
        500.0,
        "undo must not roll back someone else's edit"
    );
}

#[test]
fn malformed_input_is_rejected_without_touching_the_scene() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1)]);

    assert!(!engine.apply_remote_patch("not json"));
    assert!(!engine.apply_remote_patch("{}"));
    assert_eq!(engine.get_scene().len(), 1);
    assert_eq!(engine.get_scene()[0].x, 0.0);
}

/// The shape of the original bug: a burst of patches for elements already present must
/// leave the element count unchanged.
#[test]
fn a_storm_of_echoes_cannot_grow_the_scene() {
    let start: Vec<DrawElement> = (0..20)
        .map(|i| element(&format!("e{i}"), i as f64, 1, 1))
        .collect();
    let mut engine = engine_with_scene(start.clone());

    let echo = scene_json(&start);
    for _ in 0..50 {
        engine.apply_remote_patch(&echo);
    }

    assert_eq!(
        engine.get_scene().len(),
        20,
        "echoing a scene back at itself must not duplicate it"
    );
}

#[test]
fn a_freedraw_with_points_survives_the_round_trip() {
    let mut stroke = common::box_at(0.0, 0.0, 50.0, 50.0);
    stroke.id = "stroke".into();
    stroke.kind = DrawElementType::Freedraw;
    stroke.points = Some(vec![[0.0, 0.0], [10.0, 5.0], [20.0, 0.0]]);
    stroke.version = 2;

    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1)]);
    engine.apply_remote_patch(&scene_json(&[stroke]));

    let merged = engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == "stroke")
        .expect("the stroke merged");
    assert_eq!(merged.points.as_ref().map(Vec::len), Some(3));
}

#[test]
fn a_remote_tombstone_soft_deletes_the_local_element() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1)]);
    let mut gone = element("a", 0.0, 2, 1);
    gone.is_deleted = true;
    // scene_to_json drops tombstones — peers still send them on the live wire.
    let patch = serde_json::json!({
        "type": "osidraw",
        "version": 1,
        "elements": [gone],
    })
    .to_string();

    assert!(engine.apply_remote_patch(&patch));
    let scene = engine.get_scene();
    let a = scene.iter().find(|e| e.id == "a").expect("tombstone kept");
    assert!(a.is_deleted);
}

#[test]
fn a_remote_order_rewrites_z_order() {
    let mut engine = engine_with_scene(vec![element("a", 0.0, 1, 1), element("b", 10.0, 1, 1)]);
    let patch = serde_json::json!({
        "type": "osidraw",
        "version": 1,
        "elements": [],
        "order": ["b", "a"],
    })
    .to_string();

    assert!(engine.apply_remote_patch(&patch));
    let ids: Vec<String> = engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .map(|e| e.id)
        .collect();
    assert_eq!(ids, vec!["b".to_string(), "a".to_string()]);
}
