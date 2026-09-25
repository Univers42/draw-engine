//! What drawing together must not lose, and what it must not send twice.
//!
//! People joining a board saw pictures as empty frames, and a photo being moved went to
//! every screen whole, megabytes at a time, twenty times a second. Pictures are now sent
//! once per image and never lost to a copy without one; a patch no longer fails whole
//! over one element it cannot read; and a text shows on other screens while it is typed.

mod common;
use common::*;
use draw_engine::*;

const PICTURE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

fn image(id: &str, version: u32) -> DrawElement {
    let mut element = box_at(100.0, 100.0, 120.0, 80.0);
    element.kind = DrawElementType::Image;
    element.id = id.into();
    element.version = version;
    element.version_nonce = 7;
    element.data_url = Some(PICTURE.into());
    element
}

fn patch(elements: &[DrawElement]) -> String {
    serde_json::json!({ "type": "osidraw", "elements": elements }).to_string()
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("in the scene")
}

#[test]
fn an_edit_sent_without_the_picture_keeps_the_one_here() {
    let mut engine = engine_with_scene(vec![image("photo", 1)]);
    let mut moved = image("photo", 2);
    moved.x = 400.0;
    moved.data_url = None;

    assert!(engine.apply_remote_patch(&patch(&[moved])));

    let photo = element(&engine, "photo");
    assert_close(photo.x, 400.0);
    assert_eq!(
        photo.data_url.as_deref(),
        Some(PICTURE),
        "the picture was lost"
    );
}

#[test]
fn a_copy_with_the_picture_fills_in_one_that_arrived_without() {
    // The move overtook the picture's first arrival: same stamp, no picture here.
    let mut bare = image("photo", 3);
    bare.data_url = None;
    let mut engine = engine_with_scene(vec![bare]);

    assert!(engine.apply_remote_patch(&patch(&[image("photo", 3)])));
    assert_eq!(
        element(&engine, "photo").data_url.as_deref(),
        Some(PICTURE),
        "a tied stamp kept the copy with no picture"
    );
}

#[test]
fn a_deleted_image_is_not_given_its_picture_back() {
    let mut engine = engine_with_scene(vec![image("photo", 1)]);
    let mut tombstone = image("photo", 2);
    tombstone.is_deleted = true;
    tombstone.data_url = None;

    engine.apply_remote_patch(&patch(&[tombstone]));

    let photo = element(&engine, "photo");
    assert!(photo.is_deleted);
    assert_eq!(photo.data_url, None, "a tombstone carries no picture");
}

#[test]
fn an_older_copy_with_a_picture_changes_nothing_else() {
    let mut engine = engine_with_scene(vec![image("photo", 5)]);
    let mut stale = image("photo", 2);
    stale.x = 900.0;
    assert!(!engine.apply_remote_patch(&patch(&[stale])));
    assert_close(element(&engine, "photo").x, 100.0);
}

/// What a migrated sticky's shadow and date rely on: a scene loaded with an id already
/// tombstoned (`+page.svelte` seeds `Scene` with `[...elements, ...migrated.removed]`,
/// which reaches here through `engine.setScene`/`set_scene_json`, both keeping tombstones
/// in the `Vec` — `scene/store.rs`) refuses a stale peer's live copy of that id on stamp
/// alone, the same as any other tombstone. Without this, `apply_remote_patch_step` had
/// never heard of the id (`self.scene.get(&element.id) => None => true`) and took it
/// unconditionally — which is what let a peer tab still holding the pre-migration shadow
/// resurrect it on a migrated page (`docs/reference/sticky.md` › Open).
#[test]
fn a_tombstone_loaded_at_boot_refuses_a_stale_remote_copy() {
    let mut tombstone = box_at(3.0, 3.0, 220.0, 220.0);
    tombstone.id = "shadow".into();
    tombstone.version = 4;
    tombstone.version_nonce = 40;
    tombstone.is_deleted = true;

    let mut engine = engine_with_scene(vec![tombstone]);

    // The stale peer's copy, at the version the shadow had before it was migrated away.
    let mut stale = box_at(3.0, 3.0, 220.0, 220.0);
    stale.id = "shadow".into();
    stale.version = 3;
    stale.version_nonce = 30;

    assert!(!engine.apply_remote_patch(&patch(&[stale])));
    let shadow = element(&engine, "shadow");
    assert!(
        shadow.is_deleted,
        "the boot-time tombstone must outrank the stale copy"
    );
}

#[test]
fn one_element_that_cannot_be_read_costs_only_itself() {
    let mut engine = engine_with_scene(vec![box_at(0.0, 0.0, 10.0, 10.0)]);
    let good = box_at(300.0, 300.0, 50.0, 50.0);
    let good_id = good.id.clone();
    let mut unreadable = serde_json::to_value(box_at(0.0, 0.0, 5.0, 5.0)).unwrap();
    unreadable["type"] = "hologram".into();
    let json = serde_json::json!({
        "type": "osidraw",
        "elements": [unreadable, good],
    });

    assert!(engine.apply_remote_patch(&json.to_string()));
    assert!(
        engine
            .get_scene()
            .iter()
            .any(|element| element.id == good_id),
        "the readable edit was thrown away with the unreadable one"
    );
}

#[test]
fn an_order_with_something_odd_in_it_still_applies() {
    let a = box_at(0.0, 0.0, 10.0, 10.0);
    let b = box_at(50.0, 0.0, 10.0, 10.0);
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    let json = serde_json::json!({
        "type": "osidraw",
        "elements": [],
        "order": [b_id, 42, a_id],
    });

    assert!(engine.apply_remote_patch(&json.to_string()));
    let order: Vec<String> = engine.get_scene().into_iter().map(|e| e.id).collect();
    assert_eq!(order, vec![b_id, a_id]);
}

#[test]
fn a_photo_being_moved_is_streamed_without_its_picture() {
    let mut engine = engine_with_scene(vec![filled(image("photo", 1))]);
    engine.begin_pointer(160.0, 140.0, false, false);
    engine.move_pointer(200.0, 180.0, false, false);

    let sent = engine.gesture_elements();
    assert_eq!(sent.len(), 1);
    assert_close(sent[0].x, 140.0);
    assert_eq!(sent[0].data_url, None, "megabytes a frame to every peer");
    // Only the copy sent: the scene keeps it.
    assert_eq!(element(&engine, "photo").data_url.as_deref(), Some(PICTURE));
    engine.end_pointer();
}

#[test]
fn a_gesture_that_came_to_nothing_takes_a_peers_copy_with_the_picture() {
    let mut engine = engine_with_scene(vec![filled(image("photo", 1))]);
    engine.begin_pointer(160.0, 140.0, false, false);
    engine.move_pointer(200.0, 180.0, false, false);
    let mut theirs = image("photo", 2);
    theirs.x = 600.0;
    theirs.data_url = None;
    engine.apply_remote_patch(&patch(&[theirs]));
    // Back where it started: nothing of ours to commit.
    engine.move_pointer(160.0, 140.0, false, false);
    engine.end_pointer();

    let photo = element(&engine, "photo");
    assert_close(photo.x, 600.0);
    assert_eq!(photo.data_url.as_deref(), Some(PICTURE));
}

#[test]
fn a_text_being_typed_can_be_shown_before_it_is_committed() {
    let mut text = text_at(100.0, 100.0, 10.0, 25.0);
    text.text = Some("Hi".into());
    let id = text.id.clone();
    let mut engine = engine_with_measure(vec![text]);
    let before = element(&engine, &id);

    let preview = engine
        .text_preview(&id, "Hello there")
        .expect("a text has a preview");
    assert_eq!(preview.text.as_deref(), Some("Hello there"));
    assert!(preview.width > before.width, "measured as it will be drawn");
    assert_eq!(preview.version, before.version, "a preview is not an edit");
    assert_eq!(element(&engine, &id), before, "and the scene is untouched");

    // And it is what the commit will be, but for the stamp.
    engine.set_element_text(&id, "Hello there");
    let committed = element(&engine, &id);
    assert_eq!(committed.text, preview.text);
    assert_close(committed.width, preview.width);
    assert_close(committed.height, preview.height);
}

#[test]
fn only_a_text_has_a_text_preview() {
    let shape = box_at(0.0, 0.0, 10.0, 10.0);
    let id = shape.id.clone();
    let engine = engine_with_measure(vec![shape]);
    assert!(engine.text_preview(&id, "x").is_none());
    assert!(engine.text_preview("nobody", "x").is_none());
}
