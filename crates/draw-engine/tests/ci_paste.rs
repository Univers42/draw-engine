//! Where a paste lands, as distinct from Ctrl+D.
//!
//! The oracle centres the pasted selection's bounding box on the pointer
//! (`addElementsFromPasteOrLibrary` → `duplicateAtSceneCoords`,
//! `App.duplicate.ts@1118751f:79-111`) — `x - width/2` becomes the new left edge, snapped to
//! the grid when one is on (`getGridPoint`). Ctrl+D never reads the pointer at all: it
//! offsets by a fixed `DEFAULT_GRID_SIZE / 2`
//! (`actionDuplicateSelection.tsx@1118751f:74-82`). The two must stay distinct —
//! `duplicate_selection`'s own placement is `ci_duplicate.rs`; this file is `paste_json`'s.

mod common;
use common::*;
use draw_engine::*;

fn engine_with(elements: Vec<DrawElement>) -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine.set_scene(Scene::new(elements));
    engine
}

/// The element a paste just landed, found through the selection it leaves behind
/// (`paste_json` ends by selecting exactly what it pasted) rather than by scanning the
/// whole scene, which also holds whatever was copied from.
fn pasted(engine: &DrawEngine) -> DrawElement {
    let selected = engine.get_selection();
    assert_eq!(
        selected.len(),
        1,
        "expected exactly one pasted element selected"
    );
    engine
        .get_scene()
        .into_iter()
        .find(|e| e.id == selected[0])
        .expect("the selected paste must be in the scene")
}

/// A copy pasted at a scene point lands centred on that point — not offset from where it
/// was copied, which is what `duplicate_selection` does and paste must not.
#[test]
fn paste_at_cursor_centers_the_bounding_box_on_the_pointer() {
    let mut engine = engine_with(vec![box_at(10.0, 10.0, 20.0, 20.0)]);
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id]);
    let copied = engine.copy_selection();
    engine.clear_selection();

    assert!(engine.paste_json(copied.as_deref(), Some((300.0, 300.0))));

    let pasted: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted && e.x != 10.0)
        .collect();
    assert_eq!(pasted.len(), 1, "the paste, not the untouched original");
    let element = &pasted[0];
    assert_close(element.x + element.width / 2.0, 300.0);
    assert_close(element.y + element.height / 2.0, 300.0);
}

/// Pasting twice at two different points lands two different places, each centred on its
/// own point — the bug this guards against pasted every time at a single fixed offset from
/// the original, wherever the pointer was.
#[test]
fn two_pastes_at_two_points_land_in_two_different_places() {
    let mut engine = engine_with(vec![box_at(0.0, 0.0, 10.0, 10.0)]);
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id]);
    let copied = engine.copy_selection();

    assert!(engine.paste_json(copied.as_deref(), Some((100.0, 100.0))));
    assert!(engine.paste_json(copied.as_deref(), Some((500.0, 50.0))));

    let mut xs: Vec<f64> = engine.get_scene().into_iter().map(|e| e.x).collect();
    xs.sort_by(f64::total_cmp);
    // original at 0, then the two pastes' left edges (centre minus half-width = centre - 5).
    assert_close(xs[0], 0.0);
    assert_close(xs[1], 95.0);
    assert_close(xs[2], 495.0);
}

/// A grid that snaps holds a paste to it exactly as it holds a drag: the oracle rounds the
/// bounding box's new left edge, `x - width / 2`, to the nearest grid intersection before
/// placing it (`getGridPoint`, `App.duplicate.ts@1118751f:92-99`) — not the pointer itself,
/// and not the true centre of the copied bounds.
#[test]
fn paste_at_cursor_snaps_the_bounding_box_to_the_grid() {
    let mut engine = engine_with(vec![box_at(10.0, 10.0, 20.0, 20.0)]);
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id]);
    let copied = engine.copy_selection();
    engine.clear_selection();
    engine.set_grid(GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: true,
    });

    // half-width = half-height = 10; dx = 297 - 10 = 287 -> round(287/20)*20 = 280.
    // dy = 303 - 10 = 293 -> round(293/20)*20 = 300.
    assert!(engine.paste_json(copied.as_deref(), Some((297.0, 303.0))));

    let element = pasted(&engine);
    assert_close(element.x, 280.0);
    assert_close(element.y, 300.0);
}

/// Ctrl+D ignores the pointer and the grid alike: a fixed offset from the source, every
/// time, whether or not a grid is on (`actionDuplicateSelection.tsx@1118751f` reads no grid
/// setting at all). Paste-at-cursor above must not regress into this, and this must not
/// regress into that.
#[test]
fn duplicate_selection_ignores_the_grid_unlike_paste() {
    let mut engine = engine_with(vec![box_at(10.0, 10.0, 20.0, 20.0)]);
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id]);
    engine.set_grid(GridSettings {
        enabled: true,
        size: 20.0,
        step: 5,
        snap: true,
    });

    engine.duplicate_selection(7.0, 3.0);

    let copies: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|e| e.x != 10.0)
        .collect();
    assert_eq!(copies.len(), 1);
    assert_close(copies[0].x, 17.0);
    assert_close(copies[0].y, 13.0);
}
