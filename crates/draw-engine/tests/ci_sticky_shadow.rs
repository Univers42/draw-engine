//! A sticky note as the host builds it (`apps/web/src/lib/notes/stickyNotes.ts`): the
//! note, and a filled, locked shadow three units down and right, in one group.
//!
//! Three rules meet on it, each tested alone elsewhere. An arrow never binds a locked
//! shape, but a filled one still hides what is under it (`ci_binding_dense.rs`); align
//! carries a locked group member (`ci_group_locks_frames.rs`); flip takes what a drag
//! moves, with no lock filter (`ci_flip.rs`). Here they run together, with the binding
//! refresh after each commit re-routing the arrow bound to the note though the arrow is
//! not selected.
//!
//! ponytail: this goes with the two-rectangle note. When the note is one element with a
//! painted shadow, as Excalidraw's `stickynote`, the shadow cases here have nothing left
//! to test.

mod common;
use common::*;
use draw_engine::*;

const SIZE: f64 = 220.0;

/// The note `tag` at (x, y), shadow first so it is drawn under the note.
fn note(tag: &str, x: f64, y: f64) -> [DrawElement; 2] {
    let group = vec![format!("{tag}-group")];
    let mut shadow = box_at(x + 3.0, y + 3.0, SIZE, SIZE);
    shadow.id = format!("{tag}-shadow");
    shadow.background_color = "#000000".into();
    shadow.fill_style = FillStyle::Solid;
    shadow.opacity = 16.0;
    shadow.roundness = Some(12.0);
    shadow.locked = Some(true);
    shadow.group_ids = group.clone();
    let mut note = box_at(x, y, SIZE, SIZE);
    note.id = format!("{tag}-note");
    note.background_color = "#ffdf6b".into();
    note.fill_style = FillStyle::Solid;
    note.roundness = Some(12.0);
    note.group_ids = group;
    [shadow, note]
}

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .unwrap_or_else(|| panic!("{id} is gone"))
}

fn arrow_of(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow && !el.is_deleted)
        .expect("an arrow was drawn")
}

fn end_point(arrow: &DrawElement) -> (f64, f64) {
    let last = arrow
        .points
        .as_ref()
        .and_then(|p| p.last())
        .expect("points");
    (arrow.x + last[0], arrow.y + last[1])
}

/// Notes A at (0, 0) and B at (400, 100), and an arrow dragged from the empty space
/// right of A to two units outside A's right edge — a point one unit inside A's shadow,
/// whose outline is the nearer of the two.
fn two_notes_and_an_arrow() -> DrawEngine {
    let mut scene = note("A", 0.0, 0.0).to_vec();
    scene.extend(note("B", 400.0, 100.0));
    let mut engine = engine_with_scene(scene);
    engine.set_viewport(1600.0, 1200.0, 1.0);
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale: 1.0,
    });
    engine.set_tool(DrawTool::Arrow);
    engine.begin_pointer(320.0, 110.0, false, false);
    for k in 1..=10 {
        engine.move_pointer(320.0 - 9.8 * k as f64, 110.0, false, false);
    }
    engine.end_pointer();
    engine.set_tool(DrawTool::Select);
    engine.clear_selection();
    engine
}

fn click(engine: &mut DrawEngine, x: f64, y: f64, additive: bool) {
    engine.begin_pointer(x, y, additive, false);
    engine.end_pointer();
}

/// The shadow is nearer, but locked: the arrow takes the note.
#[test]
fn an_arrow_aimed_beside_a_note_binds_the_note() {
    let engine = two_notes_and_an_arrow();
    assert_eq!(arrow_of(&engine).end_binding.as_deref(), Some("A-note"));
}

/// Aligned by their bottoms, both notes take their shadows along, and the arrow bound to
/// A follows it down though only the notes were selected.
#[test]
fn aligning_notes_carries_their_shadows_and_the_arrow() {
    let mut engine = two_notes_and_an_arrow();
    let before = end_point(&arrow_of(&engine));
    click(&mut engine, 110.0, 110.0, false);
    click(&mut engine, 510.0, 210.0, true);

    engine.align_selection(AlignMode::Bottom);

    // Each unit is note and shadow: A's reached y = 323 with B's, so A came down 100.
    assert_close(get(&engine, "A-note").y, 100.0);
    assert_close(get(&engine, "A-shadow").y, 103.0);
    assert_close(get(&engine, "B-note").y, 100.0);
    assert_close(get(&engine, "B-shadow").y, 103.0);
    let arrow = arrow_of(&engine);
    assert_eq!(arrow.end_binding.as_deref(), Some("A-note"));
    // Still the same gap off A's right side, now level with A where it went: the free
    // start stayed, so the end aims at the anchor from there and lands short of +100.
    let after = end_point(&arrow);
    assert!(
        (after.0 - before.0).abs() < 1e-6 && after.1 > before.1 + 50.0 && after.1 < 320.0,
        "the end followed A down: {before:?} → {after:?}"
    );
}

/// Flipped, a note mirrors with its shadow about their common box (x 0..223): the note
/// lands at x = 3 and the shadow at 0. The arrow, not selected, follows the note's right
/// side three units right.
#[test]
fn flipping_a_note_carries_its_shadow_and_the_arrow() {
    let mut engine = two_notes_and_an_arrow();
    let before = end_point(&arrow_of(&engine));
    click(&mut engine, 110.0, 110.0, false);

    engine.flip_selection(FlipAxis::Horizontal);

    assert_close(get(&engine, "A-note").x, 3.0);
    assert_close(get(&engine, "A-shadow").x, 0.0);
    let arrow = arrow_of(&engine);
    assert_eq!(arrow.end_binding.as_deref(), Some("A-note"));
    let after = end_point(&arrow);
    assert!(
        (after.0 - before.0 - 3.0).abs() < 1e-6 && (after.1 - before.1).abs() < 1e-6,
        "the end followed A's right side: {before:?} → {after:?}"
    );
}
