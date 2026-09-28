//! Backspace and Delete on the selected points of a line or arrow — the real
//! `design.md:274` "Remove point".
//!
//! # What the oracle actually does
//!
//! `BACKSPACE` appears in exactly three places in Excalidraw at `1118751f`:
//! `packages/common/src/keys.ts:37` (the constant),
//! `packages/excalidraw/actions/actionDeleteSelected.tsx:306` (`keyTest`), and
//! `App.tsx:5954` (⌘/Ctrl+Backspace, clear canvas). `linearElementEditor.ts` contains no
//! `Backspace` at all. So deleting a *point* is not a key handler on the line editor — it
//! is the first branch of the ordinary "delete selected" action.
//! `BUNNY.md:768`'s "5.1 Backspace removes the last point while placing a multi-point
//! line" is a misreading of an indented feature list, and building it as written would
//! have manufactured a divergence from the oracle.
//!
//! `actionDeleteSelected.perform` (`actionDeleteSelected.tsx@1118751f:213-273`) tries three
//! things **in this order**, and the order is the spec:
//!
//! 1. `selectedPointsIndices == null` → `return false` (`:229-231`). This action deletes
//!    nothing and falls through to whole-element deletion, which is the common case. The
//!    comment above it says why: taking the whole element is "most likely a mistake"
//!    when a point was meant.
//! 2. `selectedPointsIndices.length >= linearElement.points.length` → delete the
//!    **element** (`:234-251`), `selectedLinearElement: null`,
//!    `CaptureUpdateAction.IMMEDIATELY`.
//! 3. otherwise `deletePoints(...)`, and re-map the selection to
//!    `[selectedPointsIndices[0] - 1]`, or `[0]` when the first index was `0`
//!    (`:253-272`).
//!
//! `deletePoints` (`linearElementEditor.ts@1118751f:1576-1618`) filters by index, then
//! keeps a **polygon** intact (`:1590-1603`): if the element is a polygon and index 0,
//! the last index, or the uncommitted point went, `nextPoints[0]` is rewritten to the
//! coordinates of `nextPoints[nextPoints.length - 1]` — the loop stays shut. Then
//! `getNormalizedPoints` → `offsetX/offsetY` → `_updatePoints`. This engine reuses the
//! normaliser placement already uses (`engine/multi_linear.rs`'s `reseat_points`) rather
//! than writing a second one.
//!
//! # How a point comes to be selected
//!
//! `selectedPointsIndices` is written in two places in the oracle. One of them is dead
//! code — `a_marquee_over_a_point_editor_cannot_run` below proves why. The other is live,
//! and it is a **click on a point**, not a marquee:
//! `LinearElementEditor.handlePointerDown` (`linearElementEditor.ts@1118751f:1204-1216`),
//!
//! ```ts
//! const nextSelectedPointsIndices =
//!   clickedPointIndex > -1 || event.shiftKey
//!     ? event.shiftKey ||
//!       linearElementEditor.selectedPointsIndices?.includes(clickedPointIndex)
//!       ? normalizeSelectedPoints([...(linearElementEditor.selectedPointsIndices || []), clickedPointIndex])
//!       : [clickedPointIndex]
//!     : null;
//! ```
//!
//! `normalizeSelectedPoints` (`:2367-2375`) drops `null`/`-1`, de-duplicates through a
//! `Set`, **sorts ascending**, and returns `null` for an empty set. A plain click
//! *replaces* the set with `[i]`; shift-click *accumulates* into it. It does not toggle a
//! point off — shift-clicking one that is already selected is idempotent, because the
//! `Set` throws the duplicate away. The constructor starts at `null` (`:199`).
//!
//! The elbow-arrow filter is `isPointHandle` (`:1424-1431`): for an elbow arrow only
//! index `0` and index `points.length - 1` are handles, because the router owns the
//! corners. The identical predicate appears a second time in the dead marquee
//! (`:290-299`).
//!
//! # The invariant these tests assert
//!
//! Not "a line always keeps two points". The oracle has no such guard: deleting one
//! point of a two-point line leaves a one-point line
//! (`a_two_point_line_can_be_reduced_to_one` records that). A test that forbade it would
//! be a divergence, not a safety net. What is asserted is that the arithmetic is honest —
//! every point that was not asked for is still there, in order, at the same world
//! position.

mod common;
use common::*;
use draw_engine::engine::ArrowType;
use draw_engine::selection::linear::{is_point_handle, normalize_selected_points};
use draw_engine::*;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A three-point line, selected, with its point editor open: (100,100) (300,100)
/// (100,300).
fn three_point_line() -> DrawEngine {
    engine_for(&[[0.0, 0.0], [200.0, 0.0], [0.0, 200.0]], false)
}

/// A closed four-point line, as `finish_multi_linear` leaves one.
fn polygon_line() -> DrawEngine {
    engine_for(
        &[
            [0.0, 0.0],
            [200.0, 0.0],
            [200.0, 200.0],
            [0.0, 200.0],
            [0.0, 0.0],
        ],
        true,
    )
}

/// A line with `points` placed at `(100,100) + p`, selected with its editor open.
fn engine_for(points: &[[f64; 2]], polygon: bool) -> DrawEngine {
    let line = line_at(points, polygon);
    let id = line.id.clone();
    let mut engine = engine_with_scene(vec![line]);
    open_editor(&mut engine, &id);
    engine
}

/// The same three points, **selected but not opened**.
///
/// Needed by every gate about the transform handles: the point editor being open is
/// itself gate two (`App.tsx@1118751f:9353`), so a fixture with it open answers "no" to
/// all of them and cannot tell one gate from another.
fn closed_three_point_line() -> DrawEngine {
    let line = line_at(&[[0.0, 0.0], [200.0, 0.0], [0.0, 200.0]], false);
    let id = line.id.clone();
    let mut engine = engine_with_scene(vec![line]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id]);
    assert_eq!(engine.selected_points(), None, "nothing held, nothing open");
    engine
}

/// A line whose points are given relative to `(100,100)`, already normalised: the first
/// point is at the origin, which is what `LinearElementEditor`'s constructor insists on
/// (`linearElementEditor.ts@1118751f:191-197`).
fn line_at(points: &[[f64; 2]], polygon: bool) -> DrawElement {
    let (min_x, min_y) = points.iter().fold((f64::MAX, f64::MAX), |(ax, ay), p| {
        (ax.min(p[0]), ay.min(p[1]))
    });
    let (max_x, max_y) = points.iter().fold((f64::MIN, f64::MIN), |(ax, ay), p| {
        (ax.max(p[0]), ay.max(p[1]))
    });
    let mut line = create_element_default(
        DrawElementType::Line,
        Geometry {
            x: 100.0 + min_x,
            y: 100.0 + min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        },
    );
    line.points = Some(
        points
            .iter()
            .map(|p| [p[0] - min_x, p[1] - min_y])
            .collect(),
    );
    line.polygon = Some(polygon);
    line
}

/// Select a line and open its point editor: the double click is the gesture the oracle
/// opens it with (`open_linear_points`, the same rule as `actionLinearEditor`).
///
/// On the middle of the *drawn* path, not on empty canvas beside it: a double click that
/// lands on nothing is a different gesture entirely, and one at the element's first point
/// is a point grab.
fn open_editor(engine: &mut DrawEngine, id: &str) {
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.to_string()]);
    let element = engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the fixture is in the scene");
    let points = element.points.as_ref().expect("a line has points");
    let middle = points[points.len() / 2];
    engine.handle_double_click(element.x + middle[0], element.y + middle[1]);
    assert_eq!(
        engine.debug_state().interaction.editing_linear.as_deref(),
        Some(id),
        "the double click on the path opened the point editor"
    );
    assert_eq!(
        engine.selected_points(),
        None,
        "opening the editor selects no point — the constructor's `{{ selectedPointsIndices:
        null }}` (`linearElementEditor.ts@1118751f:199`)"
    );
}

/// Three **collinear** points, so the curve through them is a straight line and a
/// coordinate between two of them is genuinely on the ink.
///
/// This is a fixture, not a convenience. `roundness` defaults to `Some(8.0)` and
/// `render/shape.rs` sends a rounded linear element through `generator::curve`, so on
/// `three_point_line`'s right angle the curve sags away from every chord — the midpoint
/// handle sits at (202.8, 88.4), and the chord's midpoint at (200,100) is about 12px off
/// the line. A press there hits nothing and starts an ordinary marquee, which is exactly how
/// the first version of the marquee fixture passed while testing nothing (the subject of
/// `ci_linear_midpoint.rs`).
fn straight_three_point_line() -> DrawEngine {
    engine_for(&[[0.0, 0.0], [100.0, 100.0], [200.0, 200.0]], false)
}

/// Four collinear points, for the same reason and one more notch: a band drawn from the
/// **stroke** between points 0 and 1 has to be able to reach points 1 and 2 while leaving
/// point 0 behind the press, and three points cannot do both.
fn straight_four_point_line() -> DrawEngine {
    engine_for(
        &[[0.0, 0.0], [100.0, 100.0], [200.0, 200.0], [300.0, 300.0]],
        false,
    )
}

/// Opens the point editor on an **arrow**.
///
/// This engine opens an arrow's points only with Ctrl held — its `isSimpleArrow` rule, so a
/// double click on an arrow can still reach the arrow's label
/// (`App.tsx@1118751f:7218-7226`). The oracle reaches `isEditing` on any press that lands on
/// the element (`:9591-9607`), so the difference is in how the editor is *opened*, not in
/// what the marquee needs: by the time the band is drawn, the only thing that matters is that
/// the editor is open, and the band itself never asks about Ctrl.
fn open_arrow_editor(engine: &mut DrawEngine, id: &str) {
    engine.set_ctrl_held(true);
    open_editor(engine, id);
    engine.set_ctrl_held(false);
}

/// A two-point line, which this engine edits by its points without a second click.
fn two_point_line() -> DrawEngine {
    let line = line_at(&[[0.0, 0.0], [200.0, 0.0]], false);
    let id = line.id.clone();
    let mut engine = engine_with_scene(vec![line]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id]);
    engine
}

/// An elbow arrow routed between two filled shapes, which is the only way one survives:
/// an elbow arrow with nothing to bind to is thrown away rather than left as a shape that
/// means nothing (`multi_linear.rs`'s `discard_invisible_elbow`).
///
/// The route is whatever the router lays out — the tests assert the filter against
/// `len - 1` rather than a fixed index, so a different route does not make them lie.
fn elbow_arrow() -> DrawEngine {
    let left = filled(box_at(0.0, 0.0, 100.0, 80.0));
    let right = filled(box_at(300.0, 200.0, 100.0, 80.0));
    let mut engine = engine_with_scene(vec![left, right]);
    engine.set_arrow_type(ArrowType::Elbow);
    engine.set_tool(DrawTool::Arrow);
    drag(&mut engine, (70.0, 30.0), (330.0, 250.0));
    let id = engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the drag placed an arrow")
        .id;
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id]);
    engine
}

fn drag(engine: &mut DrawEngine, from: (f64, f64), to: (f64, f64)) {
    engine.begin_pointer(from.0, from.1, false, false);
    engine.move_pointer(to.0, to.1, false, false);
    engine.end_pointer();
}

fn first_id(engine: &DrawEngine) -> String {
    engine.get_scene().into_iter().next().expect("a scene").id
}

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("element is gone")
}

fn points_of(engine: &DrawEngine, id: &str) -> Vec<[f64; 2]> {
    let el = get(engine, id);
    el.points
        .as_ref()
        .expect("a line has points")
        .iter()
        .map(|p| [el.x + p[0], el.y + p[1]])
        .collect()
}

/// A press and release in one spot, which is how a point comes to be selected.
fn click(engine: &mut DrawEngine, x: f64, y: f64, shift: bool) {
    engine.begin_pointer(x, y, shift, false);
    engine.end_pointer();
}

// ---------------------------------------------------------------------------
// 1. Branch one: no point selected → the whole element, the ordinary way.
// ---------------------------------------------------------------------------

/// `selectedPointsIndices == null` returns `false` and falls through to deleting whole
/// elements (`actionDeleteSelected.tsx@1118751f:225-231`).
///
/// This is the *default* state — the constructor sets it to `null`
/// (`linearElementEditor.ts@1118751f:199`) and a press that misses every point leaves it
/// null (`:1204`) — so it is the case that most needs proving on its own: the two
/// branches below it would also delete the element, and this test would pass for the
/// wrong reason.
#[test]
fn backspace_with_no_point_selected_deletes_the_whole_line() {
    let mut engine = three_point_line();
    let id = first_id(&engine);
    assert_eq!(engine.selected_points(), None);

    engine.delete_selection();

    assert!(
        get(&engine, &id).is_deleted,
        "branch one: no point selected, so the element goes"
    );
    assert_eq!(engine.get_selection(), Vec::<String>::new());
}

/// The fall-through is only ever about the element: the point branch does not run, so a
/// line that *survives* — because other things were selected too — keeps every point.
/// The fall-through is about the **element**, and it is the only branch that can run when
/// nothing is held: branch two needs `selectedPointsIndices` to be a list and branch three
/// needs it to be a short one. So with a second element held the whole selection goes,
/// line included, and no point is ever considered.
#[test]
fn branch_one_is_only_about_the_element() {
    let line = line_at(&[[0.0, 0.0], [200.0, 0.0], [0.0, 200.0]], false);
    let other = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 700.0,
            y: 700.0,
            width: 50.0,
            height: 50.0,
        },
    );
    let mut engine = engine_with_scene(vec![line, other]);
    engine.set_tool(DrawTool::Select);
    let both: Vec<String> = engine.get_scene().into_iter().map(|el| el.id).collect();
    engine.select(both);
    assert_eq!(
        engine.selected_points(),
        None,
        "no point is held, so `:229` returns false and nothing downstream can fire"
    );

    engine.delete_selection();

    assert!(
        engine.get_scene().iter().all(|el| el.is_deleted),
        "both held elements go, the line among them — as whole elements, not as points"
    );
    assert_eq!(engine.get_selection(), Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// 2. Branch two: every point selected → the element, not its points.
// ---------------------------------------------------------------------------

/// `selectedPointsIndices.length >= points.length` deletes the element rather than its
/// points (`actionDeleteSelected.tsx@1118751f:233-251`).
///
/// Reached by shift-clicking every point, which accumulates the whole index set
/// (`linearElementEditor.ts@1118751f:1204-1216`). Proved on its own: branch three would
/// have left a two-point line, so a test asserting only "gone" would pass for the wrong
/// reason — `a_partly_selected_line_stays_in_the_scene` is the control.
#[test]
fn backspace_with_every_point_selected_deletes_the_element() {
    let mut engine = three_point_line();
    let id = first_id(&engine);

    click(&mut engine, 100.0, 100.0, false);
    click(&mut engine, 300.0, 100.0, true);
    click(&mut engine, 100.0, 300.0, true);
    assert_eq!(
        engine.selected_points(),
        Some(vec![0, 1, 2]),
        "all three held"
    );

    engine.delete_selection();

    assert!(
        get(&engine, &id).is_deleted,
        "holding every point means deleting the line, not three points of it"
    );
    assert_eq!(engine.get_selection(), Vec::<String>::new());
    assert_eq!(
        engine.selected_points(),
        None,
        "`selectedLinearElement: null` (`actionDeleteSelected.tsx@1118751f:247`)"
    );
}

/// The control for the test above: the same line, one point held, survives. Without it
/// "the element is gone" says nothing about which branch fired.
#[test]
fn a_partly_selected_line_stays_in_the_scene() {
    let mut engine = three_point_line();
    let id = first_id(&engine);
    click(&mut engine, 300.0, 100.0, false);

    engine.delete_selection();

    assert!(!get(&engine, &id).is_deleted);
    assert_eq!(points_of(&engine, &id).len(), 2);
}

// ---------------------------------------------------------------------------
// 3. Branch three: some points selected → the points go, the line stays.
// ---------------------------------------------------------------------------

/// The ordinary case: one point of a three-point line is removed
/// (`actionDeleteSelected.tsx@1118751f:253-272`).
#[test]
fn backspace_on_one_point_removes_that_point() {
    let mut engine = three_point_line();
    let id = first_id(&engine);

    click(&mut engine, 300.0, 100.0, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));

    engine.delete_selection();

    assert!(!get(&engine, &id).is_deleted, "two points are still a line");
    assert_eq!(
        points_of(&engine, &id),
        vec![[100.0, 100.0], [100.0, 300.0]]
    );
}

/// The re-map: `selectedPointsIndices[0] > 0 ? [first - 1] : [0]`
/// (`actionDeleteSelected.tsx@1118751f:265-268`). With point 1 gone the selection lands
/// on index 0 — the point that took its place.
#[test]
fn the_selection_follows_the_point_that_took_the_deleted_ones_place() {
    let mut engine = three_point_line();
    click(&mut engine, 300.0, 100.0, false);
    engine.delete_selection();
    assert_eq!(engine.selected_points(), Some(vec![0]));
}

/// The same clause read the other way: with index 0 deleted the re-map cannot go below
/// the start, so it clamps to `[0]`.
#[test]
fn deleting_the_first_point_clamps_the_selection_to_the_first() {
    let mut engine = three_point_line();
    click(&mut engine, 100.0, 100.0, false);
    engine.delete_selection();
    assert_eq!(engine.selected_points(), Some(vec![0]));
    assert_eq!(
        points_of(&engine, &first_id(&engine)),
        vec![[300.0, 100.0], [100.0, 300.0]]
    );
}

/// The re-map at an index where the subtraction is not a no-op.
///
/// Deleting point 1 of three gives `[1 - 1] == [0]`, which is also what clamping to the
/// first point would give — so that case cannot tell the two arms of `:266-268` apart.
/// Deleting point 2 can: the answer is `[1]`, and an implementation that always answered
/// `[0]` would be wrong here. (It was, once: the suite was green with the `- 1` arm
/// deleted until this test was added.)
#[test]
fn deleting_the_last_point_lands_on_the_one_before_it() {
    let mut engine = three_point_line();
    click(&mut engine, 100.0, 300.0, false);
    assert_eq!(engine.selected_points(), Some(vec![2]));

    engine.delete_selection();

    assert_eq!(engine.selected_points(), Some(vec![1]), "`2 - 1`, not `0`");
    assert_eq!(
        points_of(&engine, &first_id(&engine)),
        vec![[100.0, 100.0], [300.0, 100.0]]
    );
}

/// Two points at once, and the re-map reads the **lowest** of them
/// (`selectedPointsIndices[0]`, with the set sorted — `normalizeSelectedPoints`,
/// `linearElementEditor.ts@1118751f:2373`). Clicking 2 then 1 must give the same answer
/// as clicking 1 then 2.
#[test]
fn deleting_two_points_at_once_re_maps_from_the_lowest_index() {
    let mut engine = three_point_line();
    click(&mut engine, 100.0, 300.0, false);
    click(&mut engine, 300.0, 100.0, true);
    assert_eq!(
        engine.selected_points(),
        Some(vec![1, 2]),
        "sorted, not clicked"
    );

    engine.delete_selection();

    assert_eq!(points_of(&engine, &first_id(&engine)), vec![[100.0, 100.0]]);
    // first index was 1, so 1 - 1 = 0, which is also the last point left.
    assert_eq!(engine.selected_points(), Some(vec![0]));
}

// ---------------------------------------------------------------------------
// 4. `normalizeSelectedPoints` — `linearElementEditor.ts@1118751f:2367-2375`
// ---------------------------------------------------------------------------

/// Drop `null`/`-1`, de-duplicate, sort ascending, `None` when empty.
#[test]
fn the_selected_point_list_is_deduplicated_and_sorted() {
    assert_eq!(normalize_selected_points(&[2, 0, 2]), Some(vec![0, 2]));
    assert_eq!(normalize_selected_points(&[1, 1, 1]), Some(vec![1]));
    assert_eq!(normalize_selected_points(&[]), None);
    assert_eq!(
        normalize_selected_points(&[-1, 2]),
        Some(vec![2]),
        "-1 is dropped"
    );
    assert_eq!(normalize_selected_points(&[-1, -1]), None);
}

/// Shift-clicking a point that is already held changes nothing — the `Set` at `:2371`,
/// not a toggle. This is why the oracle accumulates instead of removing.
#[test]
fn shift_clicking_a_point_already_held_is_idempotent() {
    let mut engine = three_point_line();
    click(&mut engine, 300.0, 100.0, false);
    click(&mut engine, 300.0, 100.0, true);
    assert_eq!(engine.selected_points(), Some(vec![1]));
}

/// A plain click **replaces** the set (`:1212`, the `[clickedPointIndex]` arm), which is
/// what makes the second click of a shift-free drag a selection and not an addition.
#[test]
fn a_plain_click_replaces_the_set_rather_than_adding_to_it() {
    let mut engine = three_point_line();
    click(&mut engine, 300.0, 100.0, false);
    click(&mut engine, 100.0, 300.0, false);
    assert_eq!(engine.selected_points(), Some(vec![2]));
}

// ---------------------------------------------------------------------------
// 5. `deletePoints` — the filter and the polygon rule.
// ---------------------------------------------------------------------------

/// The filter is by index and keeps everything else, in order
/// (`linearElementEditor.ts@1118751f:1586-1588`).
#[test]
fn only_the_named_indices_go_and_the_rest_keep_their_order() {
    let mut engine = three_point_line();
    let id = first_id(&engine);
    engine.delete_points(&id, &[0, 2]);

    assert_eq!(points_of(&engine, &id), vec![[300.0, 100.0]]);
}

/// **The polygon rule** (`linearElementEditor.ts@1118751f:1590-1603`): removing index 0
/// from a closed line would open the loop, so `nextPoints[0]` is rewritten to the
/// coordinates of the new last point and the shape stays shut.
#[test]
fn a_polygon_losing_its_first_point_stays_closed() {
    let mut engine = polygon_line();
    let id = first_id(&engine);
    let before = points_of(&engine, &id);
    assert_eq!(before.first(), before.last(), "a polygon is a closed loop");

    engine.delete_points(&id, &[0]);

    let after = points_of(&engine, &id);
    assert_eq!(after.len(), before.len() - 1, "one point was removed");
    assert_eq!(
        after.first(),
        after.last(),
        "the loop must still be shut, or the fill opens into a notch"
    );
    // The sharp half of the rule. Without it the first point would be `before[1]` — the
    // one that slid up into index 0 — and the loop would run from there round to the old
    // closing corner, which is not closed at all. `:1599-1602` puts the closing corner
    // back at the front instead.
    assert_ne!(
        after[0], before[1],
        "the new first point is the closing corner, NOT the point that slid up"
    );
    assert_eq!(
        after[0],
        *before.last().unwrap(),
        "it is where the old last point was"
    );
}

/// The same rule for the **last** index — the other end of the loop.
///
/// Index `len - 1`, not "the one before the closing point": on a closed loop of five
/// points the last index is 4, and 3 is an ordinary corner. The rule asks
/// `pointIndices.includes(element.points.length - 1)` (`linearElementEditor.ts:1597`), so
/// deleting 3 here exercises the *middle* arm and not this one — which is what the third
/// test below is for.
#[test]
fn a_polygon_losing_its_last_point_stays_closed() {
    let mut engine = polygon_line();
    let id = first_id(&engine);
    let before = points_of(&engine, &id);
    let last = before.len() - 1;
    assert_eq!(before[0], before[last], "the loop's two ends are one place");

    engine.delete_points(&id, &[last]);

    let after = points_of(&engine, &id);
    assert_eq!(after.len(), before.len() - 1);
    // The sharp assertion. Without the rewrite the first point would stay at the old
    // closing corner and the last would be the corner before it, and the two would differ
    // — which is an open loop, and a polygon whose fill no longer meets its own stroke.
    assert_eq!(after.first(), after.last(), "still a loop");
}

/// And **not** for a middle index, which opens nothing. Without this the rewrite would
/// look right in both tests above and be wrong here.
#[test]
fn a_polygon_losing_a_middle_point_is_untouched_by_the_rule() {
    let mut engine = polygon_line();
    let id = first_id(&engine);
    let before = points_of(&engine, &id);

    engine.delete_points(&id, &[2]);

    let after = points_of(&engine, &id);
    assert_eq!(after.len(), before.len() - 1);
    let mut expected = before.clone();
    expected.remove(2);
    assert_eq!(after, expected, "no rewrite, just the point gone");
    // The first point is still the *original* first point. On this fixture it happens to
    // sit where the last one did too — a closed loop's two ends are the same place — so
    // the loop stays shut and `after.first() == after.last()` here would prove nothing.
    // What proves the rule did not fire is that index 0 was left alone.
    assert_eq!(after[0], before[0], "index 0 was not rewritten");
    assert_ne!(
        after[0], before[2],
        "nor slid anywhere near the removed point"
    );
}

/// A line that is not a polygon is never rewritten, even at index 0.
#[test]
fn a_plain_line_losing_its_first_point_keeps_its_own_start() {
    let mut engine = three_point_line();
    let id = first_id(&engine);

    engine.delete_points(&id, &[0]);

    let after = points_of(&engine, &id);
    assert_eq!(after, vec![[300.0, 100.0], [100.0, 300.0]]);
}

/// The rule is keyed on `isLineElement(element) && element.polygon`, so an *arrow* that
/// somehow carries `polygon` is not rewritten either (`linearElementEditor.ts:1590`).
#[test]
fn the_rule_is_for_lines_only() {
    let mut arrow = line_at(
        &[[0.0, 0.0], [200.0, 0.0], [200.0, 200.0], [0.0, 0.0]],
        true,
    );
    arrow.kind = DrawElementType::Arrow;
    let id = arrow.id.clone();
    let mut engine = engine_with_scene(vec![arrow]);

    engine.delete_points(&id, &[0]);

    let after = points_of(&engine, &id);
    assert_eq!(
        after[0],
        [300.0, 100.0],
        "an arrow keeps its own first point"
    );
    assert_ne!(
        after[0],
        [100.0, 100.0],
        "which is the corner the rewrite would have put back"
    );
}

// ---------------------------------------------------------------------------
// 6. Property tests — the index arithmetic.
// ---------------------------------------------------------------------------

/// `before` with every index in `indices` taken out — de-duplicated, and in descending
/// order so that removing one does not renumber the next.
fn without(before: &[[f64; 2]], indices: &[usize]) -> Vec<[f64; 2]> {
    let mut expected = before.to_vec();
    let mut wanted: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|i| *i < before.len())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    for i in wanted.into_iter().rev() {
        expected.remove(i);
    }
    expected
}

/// The oracle filters with `pointIndices.includes(idx)`, so order, duplicates and
/// out-of-range values are immaterial: a point is removed if its index is *in* the list,
/// once. `deletePoints` cannot remove a point twice and cannot run off the end because
/// it walks the points and asks, rather than indexing with the list.
#[test]
fn the_index_list_is_a_set_so_order_and_repeats_change_nothing() {
    let base = three_point_line();
    let before = points_of(&base, &first_id(&base));

    for indices in [vec![1usize], vec![1, 1, 1], vec![1, 0], vec![0, 1]] {
        let mut engine = three_point_line();
        let id = first_id(&engine);
        engine.delete_points(&id, &indices);

        assert_eq!(
            points_of(&engine, &id),
            without(&before, &indices),
            "indices {indices:?}"
        );
    }
}

/// An index that names no point removes nothing at all — including `usize::MAX`, which
/// is what a `-1` would have become had the index list been `usize` in the first place.
#[test]
fn an_index_past_the_end_removes_nothing() {
    let mut engine = three_point_line();
    let id = first_id(&engine);
    let before = points_of(&engine, &id);

    engine.delete_points(&id, &[3, 7, 99, usize::MAX]);

    assert_eq!(points_of(&engine, &id), before);
    assert!(!get(&engine, &id).is_deleted);
}

/// Deleting *every* point through `deletePoints` is not a branch the action can take —
/// branch two catches it first (`length >= points.length`). The function itself still has
/// to answer, and it must answer without indexing an empty list (`:1599-1602` reads
/// `nextPoints[nextPoints.length - 1]`). This is the test that would panic on a naive
/// port.
#[test]
fn deleting_every_point_leaves_a_line_with_no_points_rather_than_panicking() {
    let mut engine = three_point_line();
    let id = first_id(&engine);

    engine.delete_points(&id, &[0, 1, 2]);

    assert_eq!(
        get(&engine, &id).points.as_deref(),
        Some(&[][..]),
        "the function filters to nothing; turning that into a deletion is the action's job"
    );
}

/// The invariant the oracle actually keeps, over every shape of input: a line that
/// survives holds exactly the points that were not asked for, in the same order, at the
/// same world position — and a line whose every point went is not a tombstone with a
/// list still on it.
#[test]
fn whatever_the_input_a_surviving_line_keeps_exactly_the_points_it_did_not_lose() {
    let base = three_point_line();
    let before = points_of(&base, &first_id(&base));

    let cases: Vec<Vec<usize>> = vec![
        vec![],
        vec![0],
        vec![1],
        vec![2],
        vec![0, 1],
        vec![0, 2],
        vec![1, 2],
        vec![0, 1, 2],
        vec![0, 0, 0],
        vec![2, 1, 0],
        vec![0, 1, 2, 3],
        vec![usize::MAX],
        vec![1, usize::MAX],
        vec![0, 1, 2, usize::MAX, usize::MAX],
    ];

    for indices in cases {
        let mut engine = three_point_line();
        let id = first_id(&engine);
        engine.delete_points(&id, &indices);

        assert_eq!(
            points_of(&engine, &id),
            without(&before, &indices),
            "{indices:?}: the surviving points must be the others, unchanged"
        );
    }
}

/// The all-points branch is a **comparison, not an equality**
/// (`selectedPointsIndices.length >= linearElement.points.length`,
/// `actionDeleteSelected.tsx@1118751f:234`).
///
/// It is a comparison, and that matters for the indices that name no point: a list of
/// *four* indices on a three-point line takes the element branch even though only three of
/// them could ever name a point, because the oracle never asks what the indices mean — only
/// how many there are. The decision is therefore pinned as the named predicate it is, which
/// also keeps the test honest about the fact that no click can produce such a list.
///
/// The count has to **exceed** the point count to tell `>=` from `==`; an equal count is
/// what shift-clicking every point produces, and that case cannot tell the two apart. (It
/// did, once: this test was first written with three indices against three points, and
/// changing `>=` to `==` in the engine left all 41 tests green.)
#[test]
fn the_element_branch_is_a_comparison_not_an_equality() {
    let engine = three_point_line();
    let el = first(&engine);

    // What shift-clicking every point produces, and what the action actually sees.
    assert!(engine.delete_takes_the_element(&el, &[0, 1, 2]));

    // One more index than there are points: `>=` says yes and `==` says no. Nothing can
    // click this — a click only ever names a point that is there — which is exactly why
    // the decision is pinned as a predicate rather than through a gesture.
    assert!(
        engine.delete_takes_the_element(&el, &[0, 1, 2, 9]),
        "4 indices against 3 points: `>=` takes the element, `==` would not"
    );
    assert!(
        engine.delete_takes_the_element(&el, &[0, 5, 9, 9]),
        "and a list of four that names one point still counts as four"
    );
    assert!(
        engine.delete_takes_the_element(&el, &[0, 5, 9]),
        "3 against 3 is `==` as well, so this case alone would not have caught it"
    );
    assert!(
        !engine.delete_takes_the_element(&el, &[0, 5]),
        "2 against 3 is the point branch, and that is the case a person reaches"
    );
}

/// The same predicate on the fixtures the branch behaves differently on.
#[test]
fn the_element_branch_compares_against_the_points_that_are_there() {
    let three = first(&three_point_line());
    let two = first(&two_point_line());
    assert!(!engine_of(&two).delete_takes_the_element(&two, &[0]));
    assert!(engine_of(&two).delete_takes_the_element(&two, &[0, 1]));
    assert!(!engine_of(&three).delete_takes_the_element(&three, &[0, 1]));
}

fn engine_of(el: &DrawElement) -> DrawEngine {
    let id = el.id.clone();
    let mut engine = engine_with_scene(vec![el.clone()]);
    engine.select(vec![id]);
    engine
}

/// The oracle's missing guard, recorded rather than fixed. `deletePoints` filters and
/// stops; a two-point line whose one selected point is deleted becomes a one-point line,
/// drawn as a dot, and stays in the scene. Adding a "≥ 2 points or gone" rule here would
/// be a divergence from `1118751f` — and it is the one place where the obvious invariant
/// is the wrong one.
#[test]
fn a_two_point_line_can_be_reduced_to_one() {
    let mut engine = two_point_line();
    let id = first_id(&engine);

    click(&mut engine, 300.0, 100.0, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));

    engine.delete_selection();

    let el = get(&engine, &id);
    assert!(!el.is_deleted, "one point of two is not all of them");
    assert_eq!(points_of(&engine, &id), vec![[100.0, 100.0]]);
    assert_eq!(
        engine.selected_points(),
        Some(vec![0]),
        "and the selection re-maps onto what is left"
    );
}

/// A one-point line — which the placement path never leaves behind
/// (`engine/multi_linear.rs`, the `points.len() < 2` discard) — reaches this only from the
/// test above. Deleting its single point is the `length >= points.length` branch: the
/// element goes.
#[test]
fn a_one_point_line_loses_its_element_not_a_point() {
    let mut engine = two_point_line();
    let id = first_id(&engine);
    engine.delete_points(&id, &[1]);
    assert_eq!(points_of(&engine, &id).len(), 1, "one point now");

    click(&mut engine, 100.0, 100.0, false);
    assert_eq!(engine.selected_points(), Some(vec![0]));
    engine.delete_selection();

    assert!(get(&engine, &id).is_deleted);
}

/// Points are stored relative to the element's origin, and a deletion can remove the
/// point that *was* the origin. `deletePoints` re-normalises
/// (`getNormalizedPoints` → `offsetX/offsetY`, `linearElementEditor.ts@1118751f:1605-1617`),
/// and this engine reuses the normaliser placement already uses, so the world positions
/// must not move while the box is reseated.
#[test]
fn deleting_the_origin_point_reseats_the_line_without_moving_it() {
    let mut engine = three_point_line();
    let id = first_id(&engine);

    engine.delete_points(&id, &[0]);

    let el = get(&engine, &id);
    assert_eq!(
        points_of(&engine, &id),
        vec![[300.0, 100.0], [100.0, 300.0]]
    );
    assert_eq!(
        (el.x, el.y, el.width, el.height),
        (100.0, 100.0, 200.0, 200.0),
        "the origin reseats onto the new first point and the box follows"
    );
    // The reseat is not a coincidence: the surviving points are all *below and right* of
    // the old origin in local terms, so without the reseat the box would not contain them.
    assert!(el
        .points
        .as_ref()
        .unwrap()
        .iter()
        .all(|p| p[0] >= 0.0 && p[1] >= 0.0));
}

/// `deletePoints` goes through the engine's commit path, so one Delete is one undo
/// (`CaptureUpdateAction.IMMEDIATELY`, `actionDeleteSelected.tsx@1118751f:271`).
#[test]
fn one_backspace_is_one_undo() {
    let mut engine = three_point_line();
    let id = first_id(&engine);
    click(&mut engine, 300.0, 100.0, false);

    engine.delete_selection();
    assert_eq!(points_of(&engine, &id).len(), 2);

    engine.undo();
    assert_eq!(
        points_of(&engine, &id).len(),
        3,
        "undo brings the point back"
    );
    assert!(!get(&engine, &id).is_deleted);
}

// ---------------------------------------------------------------------------
// 7. The elbow-arrow filter — `isPointHandle`.
// ---------------------------------------------------------------------------

/// `linearElementEditor.ts@1118751f:1424-1431`: for an elbow arrow only the first and
/// last point are handles, because the router owns the corners.
#[test]
fn an_elbow_arrow_offers_only_its_two_ends() {
    let engine = elbow_arrow();
    let el = arrow_of(&engine);
    let n = el.points.as_ref().unwrap().len();
    assert!(is_point_handle(&el, 0), "the tail is a handle");
    assert!(is_point_handle(&el, n as i64 - 1), "the head is a handle");
    assert!(
        (1..n as i64 - 1).all(|i| !is_point_handle(&el, i)),
        "and no corner in between is, on a route of {n} points"
    );
}

/// The control: on an ordinary line every point is a handle. Without it the two
/// assertions above could be passing because `is_point_handle` always said no.
#[test]
fn an_ordinary_line_offers_every_point() {
    let el = first(&three_point_line());
    assert!(is_point_handle(&el, 0));
    assert!(is_point_handle(&el, 1));
    assert!(is_point_handle(&el, 2));
}

/// `index >= 0` is part of the predicate (`linearElementEditor.ts@1118751f:1426`): `-1` is
/// "no point under the cursor" and must not read as the first point.
///
/// Note what the oracle does **not** say: `is_point_handle` short-circuits on
/// `!isElbowArrow(element)`, so on an ordinary line it answers `true` for *any* index at
/// or above zero, including one past the last point. It is a filter on the elbow arrow's
/// interior, not a bounds check — `getPointIndexUnderCursor` (`:1433-1459`) is what
/// guarantees the index is in range, and it never hands out an index it did not find a
/// point at. Asserting the bounds here would have been inventing a check the oracle does
/// not have.
#[test]
fn no_point_under_the_cursor_is_not_a_handle() {
    let el = first(&three_point_line());
    assert!(!is_point_handle(&el, -1));
    assert!(
        is_point_handle(&el, 3),
        "and past the end it says yes, because the elbow arm never runs"
    );

    // On an elbow arrow the arm does run, and the same out-of-range index is refused —
    // there `index === element.points.length - 1` is the whole of the rule.
    let el = arrow_of(&elbow_arrow());
    let n = el.points.as_ref().unwrap().len();
    assert!(!is_point_handle(&el, n as i64));
}

/// The rule names the **last** index rather than "the second one", so however many
/// corners the router put in the route, only `0` and `len - 1` are handles
/// (`linearElementEditor.ts@1118751f:1427-1429`).
#[test]
fn the_elbow_filter_names_the_last_index_not_the_second() {
    let el = arrow_of(&elbow_arrow());
    let n = el.points.as_ref().unwrap().len();
    assert!(
        n > 2,
        "the route has corners to refuse, or there is nothing to test"
    );
    assert!(is_point_handle(&el, 0), "the tail is a handle");
    assert!(!is_point_handle(&el, 1), "and the first corner is not");
    assert!(is_point_handle(&el, n as i64 - 1), "the head is");
    assert!(
        !is_point_handle(&el, n as i64 - 2),
        "and neither is the corner before it"
    );
}

/// The same filter, read through a press rather than through the function, because that
/// is how it is reached: an elbow arrow's interior corners are not handles, so a press on
/// one selects no point.
#[test]
fn an_elbow_arms_interior_corners_are_not_grabbable() {
    let engine = elbow_arrow();
    let el = arrow_of(&engine);
    let corner = el.points.as_ref().unwrap()[1];
    assert!(
        !is_point_handle(&el, 1),
        "the fixture's second point is a corner"
    );

    let mut engine = elbow_arrow();
    engine.begin_pointer(el.x + corner[0], el.y + corner[1], false, false);
    engine.end_pointer();

    assert_eq!(
        engine.selected_points(),
        None,
        "an elbow corner is not a point, so nothing is selected"
    );
}

/// The only linear element in a scene of one.
fn first(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| matches!(el.kind, DrawElementType::Line | DrawElementType::Arrow))
        .expect("a linear element")
}

/// The elbow arrow in a scene that also has the two shapes it is bound to.
fn arrow_of(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow)
        .expect("the elbow arrow")
}

// ---------------------------------------------------------------------------
// 8. The five gates on `handleSelectionOnPointerDown`.
// ---------------------------------------------------------------------------

/// `App.tsx@1118751f:9351-9364`. The five conditions decide whether a single selected
/// element is offered **transform handles** on a press, or whether the press goes to the
/// line editor instead (`:9427-9448`) — which is what selects the point. So they are the
/// gates that make this feature reachable, and each is a place a port drops one.
#[test]
fn gate_four_sends_a_two_point_line_to_its_points_instead_of_a_box() {
    let engine = two_point_line();
    let el = first(&engine);
    assert!(
        !engine.offers_transform_handles(&el),
        "`isLinearElement && points.length === 2` (`:9356-9358`) — a two-point line has no
        bounding box (`transformHandles.ts@1118751f:352`), so it is edited by its points"
    );
    assert!(
        engine.linear_points().len() == 3,
        "two points and the midpoint between them, which is the oracle's layout for a
        two-point line (`interactiveScene.ts@1118751f:1198`)"
    );
}

/// The control for gate four: the same call on a three-point line with its editor
/// **closed**, where the only difference is the point count.
#[test]
fn a_three_point_line_is_offered_the_box() {
    let engine = closed_three_point_line();
    assert!(engine.offers_transform_handles(&first(&engine)));
}

/// Gate three, `!isElbowArrow(...)` (`App.tsx@1118751f:9354`).
#[test]
fn gate_three_never_offers_an_elbow_arrow_a_box() {
    let engine = elbow_arrow();
    assert!(!engine.offers_transform_handles(&arrow_of(&engine)));
}

/// Gate one, `selectedElements.length === 1` (`App.tsx@1118751f:9352`).
#[test]
fn gate_one_needs_exactly_one_element_held() {
    let line = line_at(&[[0.0, 0.0], [200.0, 0.0], [0.0, 200.0]], false);
    let other = create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x: 700.0,
            y: 700.0,
            width: 50.0,
            height: 50.0,
        },
    );
    let mut engine = engine_with_scene(vec![line, other]);
    engine.set_tool(DrawTool::Select);
    let both: Vec<String> = engine.get_scene().into_iter().map(|el| el.id).collect();
    engine.select(both);

    assert_eq!(engine.get_selected_elements().len(), 2, "two are held");
    assert!(
        !engine.selection_offers_transform_handles(),
        "`:9352` fails, so the whole branch is skipped — a press that misses their handles
        is a marquee, and the points are not offered at all"
    );
}

/// The control for gate one: one element held, same question.
#[test]
fn gate_one_passes_on_a_single_element() {
    let engine = closed_three_point_line();
    assert!(engine.selection_offers_transform_handles());
}

/// Gate two, `!selectedLinearElement?.isEditing` (`App.tsx@1118751f:9353`): once the
/// point editor is open the press goes to the points, not to a resize.
///
/// The oracle reads this off `selectedLinearElement.isEditing`; here the editor is
/// `editing_linear`, and the predicate is the same question asked of the same state.
#[test]
fn gate_two_swallows_the_transform_handles_while_the_editor_is_open() {
    let el = line_at(&[[0.0, 0.0], [200.0, 0.0], [0.0, 200.0]], false);
    let id = el.id.clone();

    // Closed: the box is offered.
    let mut closed = engine_with_scene(vec![el.clone()]);
    closed.select(vec![id.clone()]);
    assert!(
        closed.offers_transform_handles(&el),
        "editor closed, so `:9353` passes"
    );

    // Open: the same element, the same question, the other answer.
    let mut open = engine_with_scene(vec![el.clone()]);
    open_editor(&mut open, &id);
    assert_eq!(
        open.debug_state().interaction.editing_linear.as_deref(),
        Some(id.as_str())
    );
    assert!(
        !open.offers_transform_handles(&el),
        "`:9353` now fails, and the press reaches `handlePointerDown` instead"
    );
}

/// Gate five, `!(selectedLinearElement && hoverPointIndex !== -1)`
/// (`App.tsx@1118751f:9360-9363`).
///
/// **This engine has no `hoverPointIndex` field, so the gate is trivially satisfied and
/// was not built.** The oracle's value is `-1` from the constructor
/// (`linearElementEditor.ts@1118751f:214`) and is only set while a point is being
/// dragged, so the gate exists to stop a press on a hovered point from being read as a
/// resize. Here the press that lands on a point goes to the point branch anyway — the
/// handle test at `pointer.rs` is what does it — so the outcome is the same without the
/// field, and no field was invented to satisfy a test.
#[test]
fn gate_five_is_trivially_true_because_there_is_no_hover_point_index() {
    let engine = closed_three_point_line();
    // The predicate the five gates are built from takes the element alone; there is no
    // hover argument to pass, because the state does not exist here.
    assert!(engine.offers_transform_handles(&first(&engine)));
    // And the press on a point still reaches the point branch, which is what gate five
    // exists to protect.
    let mut engine = three_point_line();
    click(&mut engine, 300.0, 100.0, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));
}
// ---------------------------------------------------------------------------
// 9. The marquee: Shift+drag over the line being edited.
// ---------------------------------------------------------------------------

/// The box is read from the rubber band, and a point is held when it is **inside it**.
///
/// `LinearElementEditor.handleBoxSelection`
/// (`packages/element/src/linearElementEditor.ts@1118751f:248-309`):
///
/// ```ts
/// const [selectionX1, selectionY1, selectionX2, selectionY2] =
///   getElementAbsoluteCoords(appState.selectionElement, elementsMap);
///
/// const pointsSceneCoords = LinearElementEditor.getPointsGlobalCoordinates(element, elementsMap);
///
/// const nextSelectedPoints = pointsSceneCoords
///   .reduce((acc: number[], point, index) => {
///     if (
///       (point[0] >= selectionX1 && point[0] <= selectionX2 &&
///        point[1] >= selectionY1 && point[1] <= selectionY2) ||
///       (event.shiftKey && selectedPointsIndices?.includes(index))
///     ) { acc.push(index); }
///     return acc;
///   }, [])
///   .filter((index) => { /* elbow: keep 0 and len-1 only */ });
///
/// setState({ selectedLinearElement: { ...selectedLinearElement,
///   selectedPointsIndices: nextSelectedPoints.length ? nextSelectedPoints : null } });
/// ```
///
/// # The tests are a Q/R pair on purpose
///
/// "The marquee selected some points" is satisfied by an implementation that ignores the box
/// entirely, so every case here states *which* points, and the two halves of the pair are
/// chosen to make a box-ignoring answer wrong. The Q half's band holds points 0 and 2 and
/// **excludes** point 1; the R half's holds **only** point 1. An implementation that returned
/// the whole list would answer `[0, 1, 2]` to both and fail both.
///
/// # Where the press has to land
///
/// On the line being edited — `:10898` compares the **element id**, not a handle, so a press
/// on the stroke works as well as one on a point. Most of these tests press on points,
/// because a point's own coordinates are known to be on the ink; the stroke case is
/// `a_box_drawn_from_the_stroke_between_two_points`, on a fixture whose curve really is
/// straight. That qualification is not decoration: this engine rounds a line by default, the
/// curve sags away from the chord, and a press at the chord's midpoint lands on nothing at
/// all — which is how the first version of this fixture passed for the wrong reason.
#[test]
fn a_box_over_two_of_three_points_holds_those_two() {
    let mut engine = three_point_line();

    // Press on point 0 at (100,100) and drag down the left edge. The band is x 100..100,
    // y 100..320: points 0 and 2 are on it, point 1 is 200px to the right.
    engine.begin_pointer(100.0, 100.0, true, false);
    engine.move_pointer(100.0, 320.0, true, false);

    assert_eq!(
        engine.selected_points(),
        Some(vec![0, 2]),
        "the box covers the first and third points and not the second"
    );
}

/// The R half: a band holding **only** point 1 — the one the Q half excluded. A marquee that
/// ignored the box would answer `[0, 1, 2]` here and pass the Q half by accident.
#[test]
fn a_box_elsewhere_holds_only_the_point_it_covers() {
    let mut engine = three_point_line();

    // Press on point 1 at (300,100), drag a little past it.
    engine.begin_pointer(300.0, 100.0, true, false);
    engine.move_pointer(330.0, 130.0, true, false);

    assert_eq!(
        engine.selected_points(),
        Some(vec![1]),
        "the band has moved, and it contains only the point it was drawn from"
    );
}

/// The press can be on the **stroke**, not on a handle — `:10898` compares the element, and
/// this is the ordinary way to draw a band that does not start at a vertex.
///
/// The fixture is four collinear points, so the curve through them is a straight line and
/// (150,150) is genuinely on the ink. The band runs from there to (350,350), which covers
/// points 1 and 2 and **not** point 0 — the one behind the press, so a band-ignoring port
/// would answer differently here too.
#[test]
fn a_box_drawn_from_the_stroke_between_two_points_selects_points() {
    let mut engine = straight_four_point_line();

    engine.begin_pointer(150.0, 150.0, true, false);
    engine.move_pointer(350.0, 350.0, true, false);

    assert_eq!(
        engine.selected_points(),
        Some(vec![1, 2]),
        "a press on the ink, not on a handle, is still a press on the element (`:10898`)"
    );
}

/// The two corners are normalised, so a drag that goes **up and to the left** encloses the
/// same kind of region as one that goes down and to the right. `marquee_rect` does it here
/// and `getElementAbsoluteCoords` does it in the oracle.
///
/// The case is chosen so an un-normalised box selects nothing: the band is x 250..300,
/// y 40..100, and point 1 is at (300,100) — on both edges. Tested as `x1 >= 300 && x1 <= 250`
/// it is false, so the test would see `None`.
#[test]
fn a_box_dragged_up_and_left_encloses_its_region_too() {
    let mut engine = three_point_line();

    engine.begin_pointer(300.0, 100.0, true, false);
    engine.move_pointer(250.0, 40.0, true, false);

    assert_eq!(
        engine.selected_points(),
        Some(vec![1]),
        "the band is the region between the two corners, whichever order they came in"
    );
}

/// The box is **axis-aligned over global coordinates**, and the y bounds are compared in
/// the same min-then-max order as the x bounds — `getElementAbsoluteCoords` returns
/// `[x1, y1, x2, y2]` with `y1 = minY` (`LinearElementEditor.getElementAbsoluteCoords`,
/// `:2247-2251`), so `:281-282` is `point[1] >= y1 && point[1] <= y2` and not the comparison
/// the other way round.
///
/// The band below is 120 wide and 240 tall and holds point 2 at (100,300) on both axes.
/// Swapped, `point[1] >= 340` is false for every point on this line and the band holds
/// nothing.
#[test]
fn the_box_compares_both_axes_the_same_way_round() {
    let mut engine = three_point_line();

    // Press on the line at (100,100), drag to (220,340): band x 100..220, y 100..340.
    engine.begin_pointer(100.0, 100.0, true, false);
    engine.move_pointer(220.0, 340.0, true, false);

    assert_eq!(
        engine.selected_points(),
        Some(vec![0, 2]),
        "y1 is the top edge and y2 the bottom, exactly as x1 is the left and x2 the right"
    );
}

/// Shift **accumulates** through the drag, and re-reads the set on every move
/// (`:283`, `event.shiftKey && selectedPointsIndices?.includes(index)`).
///
/// Read the expression carefully, because the De Morgan slip is easy and this engine had it
/// in its click path: a point is kept when `shift && already-held`, so a point already held
/// **stays** held for the rest of the drag, while a point the band has left and that was
/// never latched is dropped. The whole set is rebuilt from the previous set on every move,
/// which is what makes the latch work across moves rather than within one.
#[test]
fn a_point_already_held_stays_held_after_the_box_leaves_it() {
    let mut engine = three_point_line();

    // A band holding only point 1.
    engine.begin_pointer(300.0, 100.0, true, false);
    engine.move_pointer(330.0, 130.0, true, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));

    // Drag it away, still with shift. Point 1 is no longer inside, but the latch keeps it,
    // and the new position adds what it now covers.
    engine.move_pointer(100.0, 320.0, true, false);
    assert_eq!(
        engine.selected_points(),
        Some(vec![0, 1, 2]),
        "`:283` — point 1 is latched, and points 0 and 2 are newly covered"
    );

    // Back to a band over nothing at all. **Nothing** is dropped, and that is the point:
    // the latch reads the *previous* set (`:283` compares against
    // `selectedPointsIndices`), so by now all three have been held and all three are
    // latched. A shift-drag can only ever grow.
    engine.move_pointer(300.0, 100.0, true, false);
    engine.move_pointer(301.0, 101.0, true, false);
    assert_eq!(
        engine.selected_points(),
        Some(vec![0, 1, 2]),
        "a shift-drag only ever grows: `:283` latches against the previous set"
    );
}

/// The corollary, and the reason the latch is a latch: a point the band has **left** and
/// that was held is still held, and the only way to let one go is a drag that is not a
/// marquee at all.
///
/// This is worth pinning because it is the opposite of what the band looks like it does. An
/// implementation that rebuilt the set purely from containment would answer `None` here and
/// pass every other test in this file.
#[test]
fn the_latched_point_outlives_the_band_that_selected_it() {
    let mut engine = three_point_line();

    engine.begin_pointer(300.0, 100.0, true, false);
    engine.move_pointer(330.0, 130.0, true, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));

    // The band now holds nothing: (500,500) to (600,600) is nowhere near the line.
    engine.move_pointer(500.0, 500.0, true, false);
    engine.move_pointer(600.0, 600.0, true, false);

    assert_eq!(
        engine.selected_points(),
        Some(vec![1]),
        "the band covers nothing, and point 1 is still held"
    );
}

/// The other half of the latch, on its own: a point the band picks up on the way is
/// **added** to the ones already held, not substituted for them. This is the `||` at `:283`
/// meeting the accumulate in `normalizeSelectedPoints`.
#[test]
fn a_box_picks_up_more_points_as_it_grows() {
    let mut engine = three_point_line();

    engine.begin_pointer(300.0, 100.0, true, false);
    engine.move_pointer(330.0, 130.0, true, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));

    engine.move_pointer(100.0, 320.0, true, false);
    assert_eq!(
        engine.selected_points(),
        Some(vec![0, 1, 2]),
        "every point the band has covered during the drag is held"
    );
}

/// A press that names no point and holds no shift clears the list, and an empty set is
/// `null` — not `[]` (`:304-306`). The test carries the consequence rather than the
/// spelling, because the spelling alone is satisfied by a marquee that does not exist.
///
/// A shift-drag can never *empty* the selection, because the oracle's latch is
/// `event.shiftKey && includes` (`:283`) and shift is what makes the marquee: once a point is
/// held it stays held for the rest of that drag. The release is the other writer — `:1204`,
/// where the `clickedPointIndex > -1` guard sits **outside** the ternary, so a press that
/// names no point and holds no shift writes `null`. That is how `:304-306` is reachable at
/// all, and it is the state the delete action's first branch keys on.
#[test]
fn a_press_that_names_no_point_releases_the_held_one() {
    let mut engine = three_point_line();
    let id = first_id(&engine);

    engine.begin_pointer(300.0, 100.0, true, false);
    engine.move_pointer(330.0, 130.0, true, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));
    engine.end_pointer();

    // A click on empty canvas beside the line.
    engine.begin_pointer(500.0, 500.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.selected_points(), None, "`:304-306` — empty is null");

    // And the consequence, which is why the distinction is a type rather than a length: the
    // delete action's first branch is `== null` (`:229-231`), so a Backspace now takes the
    // whole element rather than a point.
    engine.select(vec![id.clone()]);
    engine.delete_selection();
    assert!(
        get(&engine, &id).is_deleted,
        "branch one, which is what `null` means"
    );
}

/// The elbow filter, on the marquee's own copy of it (`:290-299`) — the same predicate as
/// `is_point_handle` (`:1424-1431`), which the *click* path uses.
///
/// A band that encloses an elbow arrow's interior corners takes the arrow's two ends and
/// nothing else, because the corners are the router's and are not selectable. Without the
/// filter this test answers `[0, 1, 2, 3]` on a four-point route.
#[test]
fn a_box_over_an_elbow_arrow_takes_only_its_two_ends() {
    let mut engine = elbow_arrow();
    // The arrow's own id, not the first element in the scene: the scene also holds the two
    // shapes it is bound to, and those have no points at all.
    let id = arrow_of(&engine).id;
    // The editor has to be open, or there is no point editor to box over at all — the same
    // `isEditing` term the marquee's reachability turns on, and the reason this test would
    // otherwise just see a point drag (which holds exactly one index: the tail).
    open_arrow_editor(&mut engine, &id);
    let el = arrow_of(&engine);
    let n = el.points.as_ref().unwrap().len() as i64;
    assert!(
        n > 2,
        "the route has interior corners for the filter to refuse"
    );

    // Press on the tail and drag past the head, so the whole route is inside the band. The
    // tail's own coordinates are on the ink.
    let tail = el.points.as_ref().unwrap()[0];
    engine.begin_pointer(el.x + tail[0], el.y + tail[1], true, false);
    engine.move_pointer(el.x + el.width + 30.0, el.y + el.height + 30.0, true, false);

    assert_eq!(
        engine.selected_points(),
        Some(vec![0, n as usize - 1]),
        "`:290-299` — only the ends of an elbow arrow are selectable"
    );
}

/// Without shift there is no point marquee: the press goes to the point handle and the drag
/// **moves that point**, which is `isSelectingPointsInLineEditor`'s `event.shiftKey` term
/// (`App.tsx@1118751f:10895-10899`).
///
/// The control for every test above: without it, "shift+drag holds points" could be passing
/// because any drag on the line does. Here the drag starts **on** point 1 and ends 50px from
/// where it was, and the assertion is that the point followed the pointer.
#[test]
fn without_shift_a_drag_on_a_point_moves_it_instead_of_box_selecting() {
    let mut engine = three_point_line();
    let id = first_id(&engine);

    engine.begin_pointer(300.0, 100.0, false, false);
    engine.move_pointer(350.0, 150.0, false, false);
    engine.end_pointer();

    assert_eq!(
        engine.selected_points(),
        Some(vec![1]),
        "one point is held — the one the press named, which is all the click path ever holds"
    );
    assert_eq!(
        points_of(&engine, &id),
        vec![[100.0, 100.0], [350.0, 150.0], [100.0, 300.0]],
        "and it followed the pointer, so no box was drawn"
    );
}

/// The other control: the same press **without** shift, starting on the stroke between two
/// points rather than on one, is a drag of the whole line. Without this, a port that began a
/// box on any press over the line would pass every test above.
#[test]
fn without_shift_a_drag_on_the_stroke_moves_the_whole_line() {
    let mut engine = straight_three_point_line();
    let id = first_id(&engine);
    let before = points_of(&engine, &id);

    engine.begin_pointer(150.0, 150.0, false, false);
    engine.move_pointer(200.0, 210.0, false, false);
    engine.end_pointer();

    assert_eq!(engine.selected_points(), None, "a move holds no point");
    assert_ne!(
        points_of(&engine, &id),
        before,
        "and the line moved, so this really was a drag of the element"
    );
}

/// The gesture is a **Shift+drag on the line being edited**, and the reachability is not
/// obvious — this comment is here so the next reader does not have to re-derive it, having
/// already been told once that it was dead.
///
/// The call graph, at `1118751f`:
///
/// - `handleBoxSelection` needs `isEditing` **and** `selectionElement`
///   (`linearElementEditor.ts:255-259`), and its only call site is `App.tsx:11275`, behind
///   `:11274 if (this.state.selectedLinearElement?.isEditing)`;
/// - `selectionElement` is assigned in exactly one place, `App.tsx:10497`, inside
///   `createGenericElementOnPointerDown` (`:10439`) under `if (element.type ===
///   "selection")` (`:10495`) — **not** in `maybeDragNewGenericElement` (`:13330`), which
///   only *reads* it (`:13335`). `createGenericElementOnPointerDown` has two call sites:
///   `:8985` on pointer-down with `this.state.activeTool.type`, so the selection tool makes
///   the band on **every** press, and `:11158` with the literal `"selection"` in the lasso
///   branch;
/// - so on a *plain* box-drag the band exists and `isEditing` exists — but
///   `handleSelectionOnPointerDown` (`:9427-9448`) and then `:9591-9607` turn `isEditing`
///   **off** on any press that does not land on the element, and a press that does land on it
///   starts a drag of the element at `:10901-11132`, which `return`s at `:11132` before
///   `:11268`. Neither reaches `:11274` with both true;
/// - the branch that *is* skipped for a line being edited is `:10901-10904`, whose guard
///   reads `!isSelectingPointsInLineEditor`, where (`:10895-10899`)
///
///   ```ts
///   const isSelectingPointsInLineEditor =
///     this.state.selectedLinearElement?.isEditing &&
///     event.shiftKey &&
///     this.state.selectedLinearElement.elementId === pointerDownState.hit.element?.id;
///   ```
///
///   With **shift held on a press that lands on the line being edited**, the
///   drag-the-element branch is skipped, `:11136`'s `if (this.state.selectionElement)` is
///   true (the band was made on pointer-down at `:8985`), `:11154` grows it, and control
///   falls through to `:11268` and `:11274`. The marquee runs.
///
/// So: **shift-click adds points one at a time, shift-drag box-selects them**, and the press
/// only has to land on the line — on its stroke or on a handle, since `:10898` compares the
/// element id. The `:11155-11158` lasso round-trip is a real path to a recreated band, but
/// it is not the way in: `:11140` sits inside `:11136`, which is only reached when the
/// `:10901` branch was skipped, which needs shift.
#[test]
fn the_gesture_is_a_shift_drag_on_the_line_being_edited() {
    // A plain click on a point still selects that point, without shift.
    let mut engine = three_point_line();
    click(&mut engine, 300.0, 100.0, false);
    assert_eq!(
        engine.selected_points(),
        Some(vec![1]),
        "the click path, unchanged"
    );

    // Shift-click accumulates, as `normalizeSelectedPoints` has it.
    click(&mut engine, 100.0, 100.0, true);
    assert_eq!(engine.selected_points(), Some(vec![0, 1]));

    // And shift-drag box-selects: the Q half of the pair above.
    let mut engine = three_point_line();
    engine.begin_pointer(100.0, 100.0, true, false);
    engine.move_pointer(100.0, 320.0, true, false);
    assert_eq!(engine.selected_points(), Some(vec![0, 2]));
}

// 10. `keyTest` — Backspace and Delete, and never ⌘/Ctrl.
// ---------------------------------------------------------------------------

/// `actionDeleteSelected.tsx@1118751f:305-307`:
/// `(event.key === BACKSPACE || event.key === DELETE) && !event[CTRL_OR_CMD]`.
///
/// The host already refuses to pass a modified key: `engine/src/host/keys.ts:300` returns
/// before the plain keys whenever ⌘/Ctrl is held, and the oracle binds ⌘/Ctrl+Backspace
/// to *clear the canvas* (`App.tsx:5954`), which is a different action entirely. So the
/// exclusion is structural, there is no modifier test inside `delete_selection`, and none
/// was added — a second one would be a place the two could disagree.
#[test]
fn the_delete_key_ignores_a_command_or_control_modifier() {
    let mut engine = three_point_line();
    let id = first_id(&engine);
    click(&mut engine, 300.0, 100.0, false);
    assert_eq!(engine.selected_points(), Some(vec![1]));

    // What the engine guarantees is the ordering, not the modifier: the point branch is
    // decided inside `delete_selection`, and nothing else can pre-empt it.
    engine.delete_selection();
    assert_eq!(points_of(&engine, &id).len(), 2);
}

// ---------------------------------------------------------------------------
// 11. The state as a debug view reads it.
// ---------------------------------------------------------------------------

/// `debug_state()` reports the element and the indices as two fields, the way the oracle
/// carries them on one object (`linearElementEditor.ts@1118751f:196-197`), and reports
/// **no point held** as absent rather than as an empty list.
///
/// The last half is the point of the test: the delete action's first branch turns on
/// `selectedPointsIndices == null` against an empty array (`:229-231`), so a debug view
/// that flattened the two would make the state that decides everything invisible.
#[test]
fn the_debug_view_separates_no_point_held_from_an_empty_list() {
    let mut engine = three_point_line();
    assert_eq!(
        engine.debug_state().interaction.selected_points,
        None,
        "nothing held"
    );

    click(&mut engine, 300.0, 100.0, false);
    let held = engine
        .debug_state()
        .interaction
        .selected_points
        .expect("a point is held");
    assert_eq!(held.element_id, first_id(&engine));
    assert_eq!(held.indices, vec![1]);

    // Shift-accumulated, and the element is named rather than implied.
    click(&mut engine, 100.0, 300.0, true);
    let held = engine
        .debug_state()
        .interaction
        .selected_points
        .expect("two points are held");
    assert_eq!(
        held.indices,
        vec![1, 2],
        "sorted, as `normalizeSelectedPoints` keeps them"
    );

    // Deselecting closes the editor, and with it the held points: a fresh
    // `LinearElementEditor` over the next element starts at `null` (`:199`).
    engine.clear_selection();
    assert_eq!(engine.debug_state().interaction.selected_points, None);
}
