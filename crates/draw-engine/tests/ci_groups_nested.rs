//! Groups nest, and a double click steps into them.
//!
//! The contract is transcribed in `docs/reference/groups.md`, verified twice: read in the
//! pinned oracle source, and observed on excalidraw.com by grouping three rectangles and
//! reading the result back out of `localStorage`.
//!
//! ```text
//! after grouping A+B      A [g1]        B [g1]        C []
//! after grouping all      A [g1, g2]    B [g1, g2]    C [g2]
//! ```
//!
//! The array is **innermost → outermost**, and it *is* the nesting — there is no group
//! entity, no tree, no parent pointer. A group is an id that several elements carry.
//!
//! We could not express any of that: `group_id` was a single `Option<String>`, so a group
//! inside a group was unrepresentable, and `group_patches` overwrote the field, so
//! grouping a group destroyed the inner one.
//!
//! The selection rule is five lines and everything else falls out of it:
//!
//! ```text
//! ids = element.group_ids
//! if editing: ids = ids[..position_of(editing)]   // only what is strictly inside it
//! select( ids.last() or the element itself )
//! ```

mod common;
use common::*;
use draw_engine::*;

/// Three filled boxes in a row. Filled so the middle of each is a hit target — a
/// transparent shape is hit on its outline only, which is correct and tested elsewhere,
/// and would make every case here about the fill rule instead.
fn three() -> (DrawEngine, String, String, String) {
    let a = filled(box_at(0.0, 0.0, 80.0, 80.0));
    let b = filled(box_at(150.0, 0.0, 80.0, 80.0));
    let c = filled(box_at(300.0, 0.0, 80.0, 80.0));
    let (ida, idb, idc) = (a.id.clone(), b.id.clone(), c.id.clone());
    let mut engine = engine_with_scene(vec![a, b, c]);
    engine.set_tool(DrawTool::Select);
    (engine, ida, idb, idc)
}

/// The middle of the box at `index`.
fn middle(index: usize) -> (f64, f64) {
    (40.0 + 150.0 * index as f64, 40.0)
}

fn click(engine: &mut DrawEngine, index: usize) {
    let (x, y) = middle(index);
    engine.begin_pointer(x, y, false, false);
    engine.end_pointer();
}

fn double_click(engine: &mut DrawEngine, index: usize) {
    let (x, y) = middle(index);
    engine.handle_double_click(x, y);
}

fn group_ids_of(engine: &DrawEngine, id: &str) -> Vec<String> {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .map(|el| el.group_ids)
        .unwrap_or_default()
}

fn selection(engine: &DrawEngine) -> std::collections::HashSet<String> {
    engine.get_selection().into_iter().collect()
}

fn holds(engine: &DrawEngine, ids: &[&String]) -> bool {
    let actual = selection(engine);
    actual.len() == ids.len() && ids.iter().all(|id| actual.contains(*id))
}

/// A+B grouped, then all three grouped — the exact scene observed on excalidraw.com.
fn nested() -> (DrawEngine, String, String, String) {
    let (mut engine, a, b, c) = three();
    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();
    engine.select(vec![a.clone(), b.clone(), c.clone()]);
    engine.group_selection();
    (engine, a, b, c)
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

#[test]
fn grouping_two_elements_gives_them_one_shared_id() {
    let (mut engine, a, b, _c) = three();
    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();

    let ga = group_ids_of(&engine, &a);
    assert_eq!(ga.len(), 1);
    assert_eq!(ga, group_ids_of(&engine, &b));
}

/// The case the old model could not represent at all.
#[test]
fn grouping_a_group_appends_an_outer_id_and_keeps_the_inner_one() {
    let (engine, a, b, c) = nested();

    let ga = group_ids_of(&engine, &a);
    assert_eq!(
        ga.len(),
        2,
        "inner then outer, not one replaced by the other"
    );
    assert_eq!(ga, group_ids_of(&engine, &b));

    let gc = group_ids_of(&engine, &c);
    assert_eq!(gc.len(), 1, "C was never in the inner group");
    assert_eq!(
        gc[0], ga[1],
        "and the one it has is the outer one — the array is innermost → outermost"
    );
}

/// "A group can inherit another group indefinitely." Nothing in the model caps it.
///
/// Each level brings something new in, because wrapping the *same* set again is a no-op —
/// that is the guard against Ctrl+G minting a fresh id over an unchanged group, and it is
/// the oracle's behaviour too.
#[test]
fn nesting_is_not_limited_to_two_levels() {
    let (mut engine, a, b, c) = three();
    let d = filled(box_at(450.0, 0.0, 80.0, 80.0));
    let id_d = d.id.clone();
    let mut scene = engine.get_scene();
    scene.push(d);
    engine.set_scene(Scene::new(scene));

    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();
    engine.select(vec![a.clone(), b.clone(), c.clone()]);
    engine.group_selection();
    engine.select(vec![a.clone(), b.clone(), c.clone(), id_d.clone()]);
    engine.group_selection();

    assert_eq!(group_ids_of(&engine, &a).len(), 3, "three levels deep");
    assert_eq!(group_ids_of(&engine, &c).len(), 2);
    assert_eq!(group_ids_of(&engine, &id_d).len(), 1);
}

// ---------------------------------------------------------------------------
// Selecting
// ---------------------------------------------------------------------------

#[test]
fn clicking_a_member_selects_the_whole_outermost_group() {
    let (mut engine, a, b, c) = nested();
    engine.clear_selection();

    click(&mut engine, 0);

    assert!(
        holds(&engine, &[&a, &b, &c]),
        "with no group entered, a click takes the outermost group: {:?}",
        selection(&engine)
    );
    assert_eq!(engine.editing_group_id(), None);
}

#[test]
fn a_double_click_steps_into_the_outer_group() {
    let (mut engine, a, b, c) = nested();
    engine.clear_selection();
    click(&mut engine, 0);

    double_click(&mut engine, 0);

    assert!(
        engine.editing_group_id().is_some(),
        "the double click should have entered a group"
    );
    assert!(
        holds(&engine, &[&a, &b]),
        "inside the outer group, the inner group is what a member selects to: {:?}",
        selection(&engine)
    );
    assert!(!selection(&engine).contains(&c));
}

/// The recursion. A second double click goes one level further, to the element itself.
#[test]
fn a_second_double_click_reaches_the_element() {
    let (mut engine, a, _b, _c) = nested();
    engine.clear_selection();
    click(&mut engine, 0);
    double_click(&mut engine, 0);

    double_click(&mut engine, 0);

    assert!(
        holds(&engine, &[&a]),
        "nothing is left inside the inner group, so the element itself is selected: {:?}",
        selection(&engine)
    );
}

#[test]
fn a_click_outside_the_edited_group_leaves_it() {
    let (mut engine, _a, _b, c) = nested();
    engine.clear_selection();
    click(&mut engine, 0);
    double_click(&mut engine, 0);
    double_click(&mut engine, 0);
    assert!(engine.editing_group_id().is_some());

    // C is in the outer group but not the inner one being edited.
    click(&mut engine, 2);

    assert_eq!(
        engine.editing_group_id(),
        None,
        "clicking something outside the edited group steps back out"
    );
    assert!(selection(&engine).contains(&c));
}

#[test]
fn escape_leaves_the_group() {
    let (mut engine, _a, _b, _c) = nested();
    engine.clear_selection();
    click(&mut engine, 0);
    double_click(&mut engine, 0);
    assert!(engine.editing_group_id().is_some());

    engine.cancel_pointer();

    assert_eq!(engine.editing_group_id(), None);
}

/// Entering a group must not be a trap: the next thing you select has to behave normally.
#[test]
fn leaving_a_group_restores_whole_group_selection() {
    let (mut engine, a, b, c) = nested();
    engine.clear_selection();
    click(&mut engine, 0);
    double_click(&mut engine, 0);
    engine.cancel_pointer();

    click(&mut engine, 0);

    assert!(holds(&engine, &[&a, &b, &c]), "{:?}", selection(&engine));
}

// ---------------------------------------------------------------------------
// Grouping and ungrouping
// ---------------------------------------------------------------------------

/// Observed on excalidraw.com: ungrouping removes only the selected level, and the inner
/// group survives it.
#[test]
fn ungrouping_removes_only_the_outer_level() {
    let (mut engine, a, _b, c) = nested();
    engine.select(vec![a.clone(), c.clone()]);
    let inner = group_ids_of(&engine, &a)[0].clone();

    engine.ungroup_selection();

    assert_eq!(
        group_ids_of(&engine, &a),
        vec![inner],
        "the inner group must survive an outer ungroup"
    );
    assert!(group_ids_of(&engine, &c).is_empty());
}

/// The toggle, which is what was asked for. A deliberate divergence: Excalidraw's Ctrl+G
/// on an already-grouped selection is a **no-op** — observed, not assumed — which leaves
/// the key with no inverse and no way out of a group using the key you reached for.
#[test]
fn ctrl_g_groups_when_ungrouped_and_ungroups_when_grouped() {
    let (mut engine, a, b, _c) = three();
    engine.select(vec![a.clone(), b.clone()]);

    engine.toggle_group_selection();
    assert_eq!(group_ids_of(&engine, &a).len(), 1, "first press groups");

    engine.toggle_group_selection();
    assert!(
        group_ids_of(&engine, &a).is_empty(),
        "second press undoes it — the whole point of a toggle"
    );
}

/// On a nested selection the toggle peels one level, not all of them. Anything else would
/// make it impossible to undo one grouping without losing the structure beneath it.
#[test]
fn the_toggle_peels_one_level_at_a_time() {
    let (mut engine, a, b, c) = nested();
    engine.select(vec![a.clone(), b.clone(), c.clone()]);

    engine.toggle_group_selection();

    assert_eq!(group_ids_of(&engine, &a).len(), 1);
    assert!(group_ids_of(&engine, &c).is_empty());
}

/// Grouping a selection that is already exactly that group must not wrap it again. This
/// is what made Ctrl+G look like it did nothing: it minted a fresh id over the same
/// elements every press, so the group changed identity without ever changing shape.
#[test]
fn grouping_an_existing_group_does_not_wrap_it_twice() {
    let (mut engine, a, b, _c) = three();
    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();
    let first = group_ids_of(&engine, &a);

    engine.group_selection();

    assert_eq!(group_ids_of(&engine, &a), first);
}

#[test]
fn one_undo_takes_a_grouping_away() {
    let (mut engine, a, b, _c) = three();
    engine.select(vec![a.clone(), b.clone()]);
    engine.group_selection();
    assert_eq!(group_ids_of(&engine, &a).len(), 1);

    engine.undo();

    assert!(group_ids_of(&engine, &a).is_empty());
}

// ---------------------------------------------------------------------------
// The wire format
// ---------------------------------------------------------------------------

/// Boards saved before this change carry a single `groupId`. They have to keep opening,
/// and the group has to survive the trip.
#[test]
fn a_legacy_group_id_loads_as_a_one_element_array() {
    let scene = r##"{
      "type": "osidraw", "version": 1, "source": "test",
      "elements": [
        {"id":"one","type":"rectangle","x":0,"y":0,"width":10,"height":10,"angle":0,
         "strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid",
         "strokeWidth":1,"strokeStyle":"solid","roughness":1,"opacity":100,"roundness":null,
         "seed":1,"groupId":"legacy-group","version":1,"versionNonce":1,"updated":1,
         "isDeleted":false}
      ]
    }"##;
    let elements = elements_from_json(scene).expect("the scene should load");
    assert_eq!(elements.len(), 1);
    assert_eq!(
        elements[0].group_ids,
        vec!["legacy-group".to_string()],
        "a legacy single groupId folds into the array"
    );
}

/// Copying a nested group has to regenerate every id in the array while preserving which
/// elements share which — otherwise the copy is either joined to the original or has its
/// structure flattened.
#[test]
fn copying_a_nested_group_keeps_its_structure_with_fresh_ids() {
    let (mut engine, a, b, c) = nested();
    let original = group_ids_of(&engine, &a);

    engine.select(vec![a.clone(), b.clone(), c.clone()]);
    engine.duplicate_selection(500.0, 0.0);

    let scene = engine.get_scene();
    let copies: Vec<&DrawElement> = scene
        .iter()
        .filter(|el| !el.is_deleted && el.id != a && el.id != b && el.id != c)
        .collect();
    assert_eq!(copies.len(), 3, "three copies");

    let deep: Vec<&&DrawElement> = copies.iter().filter(|el| el.group_ids.len() == 2).collect();
    assert_eq!(deep.len(), 2, "two copies keep both levels");
    assert_eq!(
        deep[0].group_ids, deep[1].group_ids,
        "and they still share the same two groups"
    );
    assert_ne!(
        deep[0].group_ids, original,
        "with fresh ids, or the copy would be joined to the original"
    );
}
