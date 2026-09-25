//! Snapping to other elements while moving: off unless asked for.
//!
//! It used to be always on. Every drag near another shape was pulled a few pixels onto
//! its edge or centre, and the only way out was a modifier held through the whole drag.
//! Excalidraw ships it **off** (`appState.ts@1118751f:129`, `objectsSnapModeEnabled: false`), with
//! `Alt+S` to turn it on and Ctrl/Cmd to invert it for one gesture
//! (`snapping.ts@1118751f:180-183`):
//!
//! ```text
//! snap = (objectsSnap && !ctrlOrCmd) || (!objectsSnap && ctrlOrCmd && !gridMode)
//! ```
//!
//! `snap_move` itself — which edge wins, the threshold — is pinned by `ci_snapping.rs`.
//! This is about *when* it runs.

mod common;
use common::*;
use draw_engine::*;

/// Two filled boxes on one row, 100px apart. A drag of the right one 97px left leaves it
/// 3px short of the left one's right edge — inside the 6px snap distance.
fn two_boxes() -> (DrawEngine, String) {
    let still = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let moving = filled(box_at(200.0, 0.0, 100.0, 80.0));
    let id = moving.id.clone();
    (engine_with_scene(vec![still, moving]), id)
}

fn x_of(engine: &DrawEngine, id: &str) -> f64 {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
        .x
}

/// Presses in the middle of the moving box and drags it 97px left, with Ctrl/Cmd held
/// or not for every move. Returns whether guides were showing just before release.
fn nudge_left(engine: &mut DrawEngine, ctrl: bool) -> bool {
    engine.begin_pointer(250.0, 40.0, false, false);
    for step in 1..=4 {
        let t = step as f64 / 4.0;
        engine.move_pointer(250.0 - 97.0 * t, 40.0, false, ctrl);
    }
    let guides = !engine.snap_guides().is_empty();
    engine.end_pointer();
    guides
}

#[test]
fn it_is_off_by_default() {
    let (engine, _) = two_boxes();
    assert!(!engine.objects_snap());
    assert!(!engine.debug_state().interaction.objects_snap);
}

/// The reported behaviour: a drag lands where the pointer put it.
#[test]
fn by_default_a_drag_lands_where_the_pointer_let_go() {
    let (mut engine, id) = two_boxes();

    let guides = nudge_left(&mut engine, false);

    assert_close(x_of(&engine, &id), 103.0);
    assert!(!guides, "no alignment guides when nothing snapped");
}

#[test]
fn holding_ctrl_snaps_for_that_drag() {
    let (mut engine, id) = two_boxes();

    let guides = nudge_left(&mut engine, true);

    assert_close(x_of(&engine, &id), 100.0);
    assert!(guides, "the guide shows what it snapped to");
    assert!(!engine.objects_snap(), "and the preference is untouched");
}

#[test]
fn turned_on_it_snaps() {
    let (mut engine, id) = two_boxes();
    engine.set_objects_snap(true);

    nudge_left(&mut engine, false);

    assert_close(x_of(&engine, &id), 100.0);
    assert!(engine.debug_state().interaction.objects_snap);
}

#[test]
fn turned_on_holding_ctrl_does_not_snap() {
    let (mut engine, id) = two_boxes();
    engine.set_objects_snap(true);

    nudge_left(&mut engine, true);

    assert_close(x_of(&engine, &id), 103.0);
}

/// Grid and object snapping give different answers, and a result that depends on which
/// won by a pixel is worse than either. The grid wins — over the preference, and over
/// Ctrl/Cmd turning object snapping on for a drag, which is the case Excalidraw's
/// `!isGridModeEnabled` term exists for.
#[test]
fn a_snapping_grid_wins_over_objects() {
    for (preference, ctrl) in [(true, false), (false, true)] {
        let (mut engine, _) = two_boxes();
        engine.set_objects_snap(preference);
        engine.set_grid(GridSettings {
            enabled: true,
            snap: true,
            ..engine.grid()
        });

        let guides = nudge_left(&mut engine, ctrl);

        assert!(
            !guides,
            "no object guides while the grid snaps (preference {preference}, ctrl {ctrl})"
        );
    }
}

/// Turning it off mid-drag must not leave a guide painted that nothing will clear.
#[test]
fn turning_it_off_clears_the_guides() {
    let (mut engine, _) = two_boxes();
    engine.set_objects_snap(true);
    engine.begin_pointer(250.0, 40.0, false, false);
    engine.move_pointer(153.0, 40.0, false, false);
    assert!(
        !engine.snap_guides().is_empty(),
        "setup: snapping shows a guide"
    );

    engine.set_objects_snap(false);

    assert!(engine.snap_guides().is_empty());
    engine.end_pointer();
}
