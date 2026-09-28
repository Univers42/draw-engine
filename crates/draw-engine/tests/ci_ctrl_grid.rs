//! Ctrl/Cmd releases the grid at the last two places a pointer still holds it, and at a
//! third one the sweep turned up that no report had named.
//!
//! # What was already there
//!
//! `snap_gesture` (`engine/mod.rs:522`) is where a **pointer gesture** lands: the grid,
//! unless Ctrl/Cmd is held. It is the oracle's
//! `getGridPoint(x, y, event[KEYS.CTRL_OR_CMD] ? null : gridSize)` and the `null` is a null
//! *grid*, which `getGridPoint` returns the point unchanged for
//! (`packages/common/src/points.ts@1118751f:74-80`). It is wired at the press
//! (`pointer.rs:34`) and the move (`pointer_move.rs:23`, `:28`).
//!
//! # The two sites this file was written for
//!
//! **Resize** — `resize_pointer`, `engine/pointer_move.rs:716`. The oracle's
//! `getGridPoint(pointerCoords.x - pointerDownState.resize.offset.x, ...)` is
//! `App.tsx@1118751f:13579`, in `maybeHandleResize` (`:13546`). That is reached from
//! `onPointerMoveFromPointerDownHandler` under `pointerDownState.resize.isResizing`
//! (`:10725`, call at `:10731`), and from `onKeyDown`/`onKeyUpFromPointerDownHandler`
//! (`:10593`, `:10606`) — so a key press mid-resize runs the same gate, which is why the
//! gate belongs to the site and not to the pointer-up. Ours called `snap`, the ungated
//! one, so Ctrl left a resize on the grid.
//!
//! **The hover** — `hover_pointer`, `engine/hover.rs:89`. **This is not a Ctrl question,
//! and the brief's premise for it was wrong.** The three lines it names
//! (`App.tsx@1118751f:9974`, `:10014`, `:10067`) are `insertIframeElement`,
//! `insertEmbeddableElement` and `newImagePlaceholder`: element *creation* from a paste, a
//! drop and the AI magic-frame button, gated on `this.lastPointerDownEvent?.[CTRL_OR_CMD]`
//! — the modifier of an earlier press, not of a hover. The hover hit test is elsewhere and
//! has **no grid at all**: `handleCanvasPointerMove` computes
//! `scenePointer = viewportCoordsToSceneCoords(event, this.state)`
//! (`common/src/utils.ts:317-337`, a pure camera transform), reads it at `:7822`, and
//! passes `pointFrom(scenePointerX, scenePointerY)` to `getHoveredElementForBinding`
//! (`:7947-7954`) with no `getGridPoint` anywhere between. Nor does the callee: `scene`
//! → `getBindingCandidates` (`:432`) reads no grid. The oracle's hover therefore reads the
//! **raw** pointer, snapped or not, Ctrl or not.
//!
//! So the fix here is not a gate. It is to stop snapping a read: the hit test is asked
//! "what is under the pointer", and rounding the pointer first answers a different
//! question — with the grid on, hovering a shape's edge a few units off an intersection
//! highlights whatever the intersection happens to be over. That is the same class of bug
//! as a gate missing, and `snap_gesture` would have half-fixed it: it would have taken
//! Ctrl off the grid and left every Ctrl-released hover still reading the wrong point.
//!
//! # The third site
//!
//! The property sweep found a fourth `self.snap` on a pointer path that no report had
//! named: `end_sticky`, `engine/pointer_end.rs:453`. The oracle is
//! `App.tsx@1118751f:11825` — a sticky note placed by a **click** (a gesture under the
//! drag threshold) takes the default square, centred on the raw press, and then
//! grid-snaps it, under the same `childEvent[KEYS.CTRL_OR_CMD] ? null : gridSize`. Ours
//! called `snap`. Fixed here for the same reason as the other two, and named in the
//! report.
//!
//! # The Q/R pairs
//!
//! A test that asserts *a snap happened* also passes when a snap happens **always**, so
//! every case here is a pair: the same gesture, the same pointer, the grid on, snapping
//! without Ctrl and landing on the **raw pointer** with it, to `1e-9`. The one exception
//! is the hover, whose oracle half is the raw pointer **both** ways — so its pair is
//! "raw with Ctrl" and "raw without", and the test that would have caught the bug is the
//! one holding no Ctrl at all.
//!
//! # The tie
//!
//! `getGridPoint` uses `Math.round`, which breaks a tie toward **+infinity**
//! (`Math.round(-0.5) === -0`); `f64::round` breaks it **away from zero**. On a 20-unit
//! grid the two disagree at every negative half-cell by a whole cell: the oracle puts
//! x = -10 on **0**, `f64::round` on **-20**. `GridSettings::snap_point`
//! (`render/paint.rs:141-150`) already has the ECMA answer, so the cases below are stated
//! on both sides of every tie, positive and negative, and a positive-only test could not
//! see the class at all.

mod common;
use common::*;
use draw_engine::*;

/// Excalidraw's default grid (`ci_grid.rs::the_grid_is_off_until_asked_for`). 20 has the
/// useful property that half a cell is 10 — a whole number, so a tie is exactly
/// representable and the negative side of it is reachable.
const GRID: f64 = 20.0;
/// What a world coordinate must match to be "the raw pointer".
const RAW: f64 = 1e-9;

/// The oracle's `getGridPoint`: `round(v / size) * size` per axis, `null` grid meaning
/// the point unchanged (`points.ts@1118751f:69-81`). Written out here rather than called
/// so the expectation is the oracle's arithmetic and not this engine's.
fn get_grid_point(x: f64, y: f64, size: Option<f64>) -> (f64, f64) {
    match size {
        None => (x, y),
        Some(size) => (
            (x / size + 0.5).floor() * size,
            (y / size + 0.5).floor() * size,
        ),
    }
}

fn grid_on() -> GridSettings {
    GridSettings {
        enabled: true,
        size: GRID,
        step: 5,
        snap: true,
    }
}

fn engine() -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine
}

/// A grid-snapping engine holding one selected rectangle, both box and press on
/// intersections so only the **resize** under test is off-grid.
fn selected_box() -> (DrawEngine, String) {
    let (x, y, w, h) = BOX_GEOMETRY;
    let box_ = box_at(x, y, w, h);
    let id = box_.id.clone();
    let mut engine = engine_with_scene(vec![box_]);
    engine.set_grid(grid_on());
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    (engine, id)
}

fn the_box(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the rectangle is gone")
}

/// How far outside the outline a handle's centre is drawn, in world units at a given
/// camera scale.
///
/// The engine's own figure is `HandleLayout::screen`: `(FRAME_MARGIN_PX + HANDLE_PX / 2) /
/// scale` — a **screen** size, so the offset shrinks in world units as you zoom in
/// (`selection/handles.rs:145-160`). Hard-coding `8.0` is right only at 1:1: at 2× the
/// press lands four world units past the handle, misses it, and drags the shape instead —
/// which reads as a resize that ignored its own gate. The two constants are repeated
/// because they are private to the crate; the formula is the part that has to match.
fn handle_offset(scale: f64) -> f64 {
    (4.0 + 8.0 / 2.0) / scale
}

/// The handle's **drawn** centre and the box **edge** it stands for.
///
/// The two differ by [`HANDLE_OFFSET`] on the axes the handle moves, and a resize takes
/// off where in its handle it was grabbed (`getResizeOffsetXY`,
/// `packages/element/src/resizeElements.ts@1118751f:497-554`) — so dragging a handle's
/// drawn centre by `delta` puts the **edge** at `edge + delta`, snapped or not. Every
/// expectation below is that sentence, and none of them is a literal.
fn handle_geometry(engine: &DrawEngine, id: &str, kind: HandleKind) -> ((f64, f64), (f64, f64)) {
    let el = the_box(engine, id);
    geometry_of((el.x, el.y, el.width, el.height), kind, engine.camera.scale)
}

/// The one place the offset arithmetic lives, for a box and for a group's union alike.
///
/// `handle_local_point` is given the half-extent **plus** the offset, so `local` is already
/// the distance out to where the handle is *drawn*; the edge it stands for is `half`, which
/// is the same distance with the offset taken back **off the outside** — `+8` for a west or
/// north-west handle, `-8` for an east one. Only the axes the handle moves are adjusted,
/// which is why a side handle's other coordinate is the element's own and not a corner's.
///
/// Two copies of this arithmetic, one of them with the sign the other way round, is how the
/// first version of this file came to expect the corner of one box on the other and report
/// a green suite over a red engine.
fn geometry_of(
    geometry: (f64, f64, f64, f64),
    kind: HandleKind,
    scale: f64,
) -> ((f64, f64), (f64, f64)) {
    let (x, y, w, h) = geometry;
    let pad = handle_offset(scale);
    let local = handle_local_point(kind, w / 2.0 + pad, h / 2.0 + pad);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let inward = |offset: f64, moved: bool| {
        if moved {
            offset - offset.signum() * pad
        } else {
            offset
        }
    };
    let edge = (
        cx + inward(local.x, moves_x(kind)),
        cy + inward(local.y, moves_y(kind)),
    );
    ((cx + local.x, cy + local.y), edge)
}

/// The box's geometry, on a 120×80 box whose **centre is itself on the 20-grid** — so the
/// cardinal handles' non-moving coordinate is on an intersection and the tie cases cannot
/// be confused by a centre that lands on one.
const BOX_GEOMETRY: (f64, f64, f64, f64) = (100.0, 100.0, 120.0, 80.0);

/// Whether the handle moves the box on this axis. The cardinal handles move one, the
/// corners two.
fn moves_x(kind: HandleKind) -> bool {
    matches!(
        kind,
        HandleKind::Nw
            | HandleKind::Ne
            | HandleKind::Se
            | HandleKind::Sw
            | HandleKind::E
            | HandleKind::W
    )
}

fn moves_y(kind: HandleKind) -> bool {
    matches!(
        kind,
        HandleKind::Nw
            | HandleKind::Ne
            | HandleKind::Se
            | HandleKind::Sw
            | HandleKind::N
            | HandleKind::S
    )
}

/// Presses the handle `kind` and drags it by `(dx, dy)`, releasing there. The pointer
/// takes **screen** coordinates, so a camera other than 1:1 goes through the round trip
/// and the unswept cases would not pass with that step missing.
///
/// The grab is offset to the middle of the handle's own box, not to its edge point, so
/// the press lands on the handle the painter drew rather than on the line under it —
/// `HANDLE_HIT_PX` is 10 world units at 1:1, and a press on the very edge of that is a
/// coin toss.
fn drag_handle(engine: &mut DrawEngine, id: &str, kind: HandleKind, delta: (f64, f64)) {
    let (drawn, _) = handle_geometry(engine, id, kind);
    drag_handle_from(engine, drawn, delta, false)
}

/// Drags from the handle's **drawn** centre by `delta`, with Ctrl held or not.
///
/// The press takes the handle where the painter draws it, and the move is offset from that
/// same point, so the edge follows `edge + delta` — which is the oracle's "the corner goes
/// exactly as far as the pointer" (`ci_group_resize.rs` says the same of its own drags).
fn drag_handle_from(engine: &mut DrawEngine, drawn: (f64, f64), delta: (f64, f64), ctrl: bool) {
    let press = world_to_screen(engine.camera, drawn.0, drawn.1);
    let to = world_to_screen(engine.camera, drawn.0 + delta.0, drawn.1 + delta.1);
    engine.set_ctrl_held(ctrl);
    engine.begin_pointer(press.x, press.y, false, false);
    engine.move_pointer(to.x, to.y, false, false);
    engine.set_ctrl_held(false);
    engine.end_pointer();
}

/// The same drag, with Ctrl held for the whole of it. `set_ctrl_held` is a latch, so the
/// press carries it too — the oracle's `event[CTRL_OR_CMD]` is read on the pointer move
/// (`App.tsx@1118751f:10722`) but the press origin is read from
/// `lastPointerDownEvent` (`:10443`), so both halves of the gesture must agree.
fn drag_handle_with_ctrl(engine: &mut DrawEngine, id: &str, kind: HandleKind, delta: (f64, f64)) {
    let (drawn, _) = handle_geometry(engine, id, kind);
    drag_handle_from(engine, drawn, delta, true)
}

fn assert_at(where_: &str, got: (f64, f64), want: (f64, f64)) {
    assert!(
        (got.0 - want.0).abs() < RAW && (got.1 - want.1).abs() < RAW,
        "{where_}: expected {want:?}, got {got:?} — the snap side answered, not the raw one"
    );
}

// ---------------------------------------------------------------------------
// Site 1 — resize (`resize_pointer`, pointer_move.rs:716)
// ---------------------------------------------------------------------------

/// The eight handles, and the four corners among them. The oracle's site is one line
/// serving all of them (`App.tsx@1118751f:13579`), so a gate that reached three of them
/// would still be a bug a reader would not see.
const HANDLES: [(HandleKind, &str); 8] = [
    (HandleKind::Nw, "north-west"),
    (HandleKind::N, "north"),
    (HandleKind::Ne, "north-east"),
    (HandleKind::E, "east"),
    (HandleKind::Se, "south-east"),
    (HandleKind::S, "south"),
    (HandleKind::Sw, "south-west"),
    (HandleKind::W, "west"),
];

/// The handle's **own** corner in the finished box: north-west is `(x, y)`, north-east
/// `(x + w, y)`, and a cardinal handle reports the coordinate it moves with the centre on
/// the other, which does not move.
///
/// Reporting one corner for every handle is what made the first version of this file fail
/// for the wrong reason: a north-west drag moved nothing at `(x + w, y + h)`, so the
/// expectation and the reading were about two different points and the pair "agreed".
fn landed(engine: &DrawEngine, id: &str, kind: HandleKind) -> (f64, f64) {
    let el = the_box(engine, id);
    let (x, y) = (el.x, el.y);
    let (ex, ey) = (x + el.width, y + el.height);
    let (cx, cy) = (x + el.width / 2.0, y + el.height / 2.0);
    match kind {
        HandleKind::Nw => (x, y),
        HandleKind::N => (cx, y),
        HandleKind::Ne => (ex, y),
        HandleKind::E => (ex, cy),
        HandleKind::Se => (ex, ey),
        HandleKind::S => (cx, ey),
        HandleKind::Sw => (x, ey),
        HandleKind::W => (x, cy),
        // The rotation handle moves no edge, and no case in `HANDLES` uses it.
        HandleKind::Rotate => (cx, cy),
    }
}

/// The edge a drag of `delta` should reach: the handle's own edge plus the drag, through
/// the oracle's `getGridPoint` or through the `null` grid Ctrl hands it.
/// The oracle snaps both axes (`getGridPoint` takes a point), but a side handle's
/// non-moving coordinate is discarded by the resize maths — so the *expectation* for it is
/// the element's own value, not a rounded one. Rounding it would put a tie on an axis that
/// never moved and blame the snap for a number the grid had no part in.
fn want_edge(
    engine: &DrawEngine,
    id: &str,
    kind: HandleKind,
    delta: (f64, f64),
    ctrl: bool,
) -> (f64, f64) {
    let (_, edge) = handle_geometry(engine, id, kind);
    let grid = (!ctrl).then_some(GRID);
    (
        if moves_x(kind) {
            get_grid_point(edge.0 + delta.0, 0.0, grid).0
        } else {
            edge.0
        },
        if moves_y(kind) {
            get_grid_point(0.0, edge.1 + delta.1, grid).1
        } else {
            edge.1
        },
    )
}

/// Q. With the grid on, a resize lands its moving edge on the nearest intersection.
/// `App.tsx@1118751f:13579` — the grab first (`pointerDownState.resize.offset`), then
/// the grid.
#[test]
fn resizing_lands_the_moving_edge_on_the_grid() {
    let (mut engine, id) = selected_box();
    let kind = HandleKind::Se;
    let delta = (37.0, 23.0);
    let want = want_edge(&engine, &id, kind, delta, false);
    drag_handle(&mut engine, &id, kind, delta);
    assert_at(
        "Q: the grid snap did not happen",
        landed(&engine, &id, kind),
        want,
    );
}

/// R. The same press, same drag, same grid, Ctrl held: the oracle passes a `null` grid and
/// the edge lands under the pointer.
///
/// This is the half that can fail. A test asserting only Q passes if the edge snapped to
/// anything, to everything, always.
#[test]
fn holding_ctrl_takes_a_resize_off_the_grid() {
    let (mut engine, id) = selected_box();
    let kind = HandleKind::Se;
    let delta = (37.0, 23.0);
    let want = want_edge(&engine, &id, kind, delta, true);
    drag_handle_with_ctrl(&mut engine, &id, kind, delta);
    assert_at(
        "R: the grid still snapped under Ctrl",
        landed(&engine, &id, kind),
        want,
    );
}

/// Both halves in one test, so a regression cannot satisfy one and break the other, and
/// so the pair is shown to actually differ — a Q and an R that agree prove nothing.
#[test]
fn a_resize_snaps_to_the_grid_unless_ctrl_is_held() {
    resize_pair(HandleKind::Se, (37.0, 23.0)).assert_both("a south-east resize");
}

/// The Q/R pair's two runs: the same board, the same selection, the same gesture, once
/// with Ctrl and once without. Both expectations are read off the board they will be
/// checked against, so a pair can never pass by comparing a run to a literal that has
/// drifted from the geometry.
/// One Q/R pair, read back as a record rather than a four-tuple: clippy counts
/// `((f64, f64), (f64, f64), (f64, f64), (f64, f64))` as too complex a type, and four
/// positional pairs at a call site are four ways to swap the halves.
#[derive(Clone, Copy, Debug)]
struct Pair {
    /// Where the gesture landed with the grid on and no Ctrl.
    snapped: (f64, f64),
    /// Where it should have landed: the oracle's `getGridPoint`.
    want_snapped: (f64, f64),
    /// Where it landed with Ctrl held.
    free: (f64, f64),
    /// Where it should have landed: the oracle's `null` grid.
    want_free: (f64, f64),
}

impl Pair {
    /// Asserts both halves, and that the pair actually differs — a Q and an R that agree
    /// prove nothing, and the whole instrument rests on their differing.
    fn assert_both(&self, where_: &str) {
        assert_ne!(
            self.want_snapped, self.want_free,
            "{where_}: the two expectations are equal, so nothing is under test"
        );
        let at = |got: (f64, f64), want: (f64, f64), what: &str| {
            assert_at(&format!("{where_}: {what}"), got, want);
        };
        at(
            self.snapped,
            self.want_snapped,
            "the grid snap did not happen",
        );
        at(
            self.free,
            self.want_free,
            "Ctrl did not take it off the grid",
        );
        assert_ne!(
            self.snapped, self.free,
            "{where_}: the pair must differ, or one side is not answering"
        );
    }
}

/// The single element's Q/R pair, exactly as the oracle's one `getGridPoint` at
/// `App.tsx@1118751f:13579` serves every handle: the same board and the same gesture, once
/// with Ctrl and once without, both expectations read off the board they are checked
/// against rather than off a literal that can drift from the geometry.
fn resize_pair(kind: HandleKind, delta: (f64, f64)) -> Pair {
    let (mut snapped, snap_id) = selected_box();
    let want_snapped = want_edge(&snapped, &snap_id, kind, delta, false);
    drag_handle(&mut snapped, &snap_id, kind, delta);

    let (mut free, free_id) = selected_box();
    let want_free = want_edge(&free, &free_id, kind, delta, true);
    drag_handle_with_ctrl(&mut free, &free_id, kind, delta);

    Pair {
        snapped: landed(&snapped, &snap_id, kind),
        want_snapped,
        free: landed(&free, &free_id, kind),
        want_free,
    }
}

/// Every handle, not one. The bug class is "the gate exists but this call site does not
/// use it", and a test that drives only the south-east handle passes while the north-west
/// one is still wrong — or while a future edit reorders the match and drops one.
#[test]
fn every_handle_answers_to_ctrl() {
    for (kind, name) in HANDLES {
        resize_pair(kind, (37.0, 23.0)).assert_both(name);
    }
}

/// A group resize runs the **same** `resize_pointer` (`pointer_move.rs:89`, against
/// `:264` for a single element), and the oracle's one site serves both — so a gate on one
/// call site and not the other is invisible to a test that only resizes a shape.
#[test]
fn a_group_resize_answers_to_ctrl_too() {
    group_pair(HandleKind::Se, (37.0, 23.0)).assert_both("a group's south-east handle");
}

/// Every handle of a group, for the same reason as the single element's eight: the call
/// site is shared but the two call sites into it are not, and a corner group handle takes
/// a different branch than a side one.
#[test]
fn every_handle_of_a_group_answers_to_ctrl() {
    for (kind, name) in HANDLES {
        group_pair(kind, (37.0, 23.0)).assert_both(&format!("a group's {name} handle"));
    }
}

/// The group's Q/R pair, exactly as the single element's: same board, same gesture, once
/// with Ctrl and once without, both expectations read off the board they are checked
/// against.
fn group_pair(kind: HandleKind, delta: (f64, f64)) -> Pair {
    let (mut snapped, _) = group_of_two();
    let want_snapped = group_want_edge(&snapped, kind, delta, false);
    drag_group_handle(&mut snapped, kind, delta, false);
    let (mut free, _) = group_of_two();
    let want_free = group_want_edge(&free, kind, delta, true);
    drag_group_handle(&mut free, kind, delta, true);
    Pair {
        snapped: group_landed(&snapped, kind),
        want_snapped,
        free: group_landed(&free, kind),
        want_free,
    }
}

/// Two rectangles side by side, selected together: union (100,100)-(300,200).
fn group_of_two() -> (DrawEngine, String) {
    let a = box_at(100.0, 100.0, 100.0, 100.0);
    let b = box_at(200.0, 100.0, 100.0, 100.0);
    let (id, bid) = (a.id.clone(), b.id.clone());
    let mut engine = engine_with_scene(vec![a, b]);
    engine.set_grid(grid_on());
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone(), bid]);
    (engine, id)
}

/// The union's corners. The group's frame is the union of what is selected, so the handle
/// is drawn on that and not on the first element's own box.
const GROUP_UNION: (f64, f64, f64, f64) = (100.0, 100.0, 200.0, 100.0);

/// Where the union's handle `kind` is drawn, and the union edge it stands for — the same
/// two-points-and-an-offset shape as the single element's [`handle_geometry`].
fn group_handle_geometry(engine: &DrawEngine, kind: HandleKind) -> ((f64, f64), (f64, f64)) {
    geometry_of(GROUP_UNION, kind, engine.camera.scale)
}

fn drag_group_handle(engine: &mut DrawEngine, kind: HandleKind, delta: (f64, f64), ctrl: bool) {
    let (drawn, _) = group_handle_geometry(engine, kind);
    drag_handle_from(engine, drawn, delta, ctrl);
}

fn group_want_edge(
    engine: &DrawEngine,
    kind: HandleKind,
    delta: (f64, f64),
    ctrl: bool,
) -> (f64, f64) {
    let (_, edge) = group_handle_geometry(engine, kind);
    let grid = (!ctrl).then_some(GRID);
    (
        if moves_x(kind) {
            get_grid_point(edge.0 + delta.0, 0.0, grid).0
        } else {
            edge.0
        },
        if moves_y(kind) {
            get_grid_point(0.0, edge.1 + delta.1, grid).1
        } else {
            edge.1
        },
    )
}

/// The union's moving corner, read back off the two elements and matched to the handle —
/// the same corner-per-handle rule as [`landed`], so a north-west group drag is read at
/// the top-left rather than at the bottom-right that never moved.
fn group_landed(engine: &DrawEngine, kind: HandleKind) -> (f64, f64) {
    let scene = engine.get_scene();
    let min_x = scene.iter().map(|el| el.x).fold(f64::MAX, f64::min);
    let min_y = scene.iter().map(|el| el.y).fold(f64::MAX, f64::min);
    let max_x = scene
        .iter()
        .map(|el| el.x + el.width)
        .fold(f64::MIN, f64::max);
    let max_y = scene
        .iter()
        .map(|el| el.y + el.height)
        .fold(f64::MIN, f64::max);
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    match kind {
        HandleKind::Nw => (min_x, min_y),
        HandleKind::N => (cx, min_y),
        HandleKind::Ne => (max_x, min_y),
        HandleKind::E => (max_x, cy),
        HandleKind::Se => (max_x, max_y),
        HandleKind::S => (cx, max_y),
        HandleKind::Sw => (min_x, max_y),
        HandleKind::W => (min_x, cy),
        HandleKind::Rotate => (cx, cy),
    }
}

/// The tie, on both sides and on the negative half-cell where `f64::round` and
/// `Math.round` disagree by a whole cell. A resize whose handle ends on a tie is the only
/// way a gesture reaches one, so the case is driven as a gesture and not as a call into
/// the grid.
#[test]
fn a_resize_on_a_tie_breaks_it_towards_positive_infinity() {
    for delta in [(10.0, 0.0), (-10.0, 0.0), (0.0, 10.0), (0.0, -10.0)] {
        let (mut engine, id) = selected_box();
        let want = want_edge(&engine, &id, HandleKind::Se, delta, false);
        drag_handle(&mut engine, &id, HandleKind::Se, delta);
        assert_at(
            &format!("tie at {delta:?}"),
            landed(&engine, &id, HandleKind::Se),
            want,
        );
    }
}

/// The same tie on the **negative** half-cell, where the two languages part company by a
/// whole cell: `Math.round(-0.5)` is `-0`, `f64::round(-0.5)` is `-1.0`. A test using only
/// positive coordinates cannot see this class at all.
/// The same tie on the **negative** half-cell, where the two languages part company by a
/// whole cell: `Math.round(-0.5)` is `-0`, `f64::round(-0.5)` is `-1.0`. A test using only
/// positive coordinates cannot see this class at all.
///
/// The box's east edge is put on -120 and dragged by +10, so the edge is asked to land on
/// exactly -110 — a negative half-cell, the one value where the two disagree.
#[test]
fn a_resize_on_a_negative_tie_breaks_towards_positive_infinity() {
    let (mut engine, id) = negative_box();
    let delta = (10.0, 0.0);
    let want = want_edge(&engine, &id, HandleKind::E, delta, false);
    drag_handle(&mut engine, &id, HandleKind::E, delta);
    let got = landed(&engine, &id, HandleKind::E);
    assert_at("negative tie", got, want);
    assert_eq!(
        want.0, -100.0,
        "the edge should be asked to break the tie at -110, and Math.round(-0.5) === -0"
    );
}

/// A box whose east edge sits on -120, so a +10 drag lands the edge on the negative
/// half-cell at -110.
fn negative_box() -> (DrawEngine, String) {
    let box_ = box_at(-240.0, 100.0, 120.0, 80.0);
    let id = box_.id.clone();
    let mut engine = engine_with_scene(vec![box_]);
    engine.set_grid(grid_on());
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    (engine, id)
}

// ---------------------------------------------------------------------------
// Site 2 — the hover hit test (hover.rs:89)
//
// The oracle's pair is the **raw pointer both ways**: `handleCanvasPointerMove` reads
// `scenePointer` (`:7822`) and hands it to `getHoveredElementForBinding` (`:7947-7954`)
// with no `getGridPoint` on the way, and neither does the callee. So "Ctrl off the grid"
// is not the rule here; "off the grid" is.
// ---------------------------------------------------------------------------

/// A filled rectangle with the arrow tool up. `B` sits with its right edge at x = 400, so
/// a pointer five units to its right is five units outside the outline — inside the
/// binding tolerance, and outside every grid intersection on a 10-unit grid.
fn hover_board(grid: bool) -> DrawEngine {
    let shape = filled(box_at(300.0, 300.0, 105.0, 90.0));
    let mut engine = engine_with_scene(vec![shape]);
    if grid {
        engine.set_grid(GridSettings {
            enabled: true,
            size: 10.0,
            step: 5,
            snap: true,
        });
    }
    engine.set_tool(DrawTool::Arrow);
    engine
}

/// Where the hover cases aim, and why this number.
///
/// The shape spans x 300–405, so its right edge is 10 units from the 410 grid line. A
/// pointer at (415, 345) is **10 units** from that edge — inside the binding tolerance of
/// 15 (`max_binding_distance`, `scene/binding.rs:148-152`) — and the grid's nearest
/// intersection is **420**, which is 15 from the edge and just outside it.
///
/// So the two readings cannot agree: the raw pointer finds the shape and the snapped one
/// does not. That is the whole reason this file needed a shape whose edge is not on a grid
/// line — with an edge at 400 the tolerance (15) is three times the grid's reach (5) and
/// every point finds the shape either way, so a snapping hover passes uncaught. The
/// numbers here are measured, not chosen: a probe over edge ∈ {400..405} and pointer
/// ∈ {412..418} found exactly this family.
const HOVER_AT: (f64, f64) = (415.0, 345.0);
/// The midpoint the shape's right side carries, and the point the hover must mark.
const HOVER_MIDPOINT: (f64, f64) = (405.0, 345.0);

/// R. Ctrl held: the oracle's hover has no `getGridPoint` at all, so the modifier does not
/// change *where* it reads — it suppresses the suggestion outright
/// (`App.tsx@1118751f:5754-5762`, `isBindingEnabled`). Stated as its own case so the
/// pair below is not the only thing covering it, and so the reason Ctrl is absent from the
/// grid pair is written down rather than inferred.
#[test]
fn ctrl_suppresses_the_suggestion_without_moving_the_read() {
    let mut engine = hover_board(true);
    engine.set_ctrl_held(true);
    engine.hover_pointer(HOVER_AT.0, HOVER_AT.1);
    assert!(
        engine.paint_view().binding_highlight.is_none(),
        "Ctrl held and the suggestion is up, so this is not the oracle's Ctrl behaviour"
    );
}

/// The half that could not fail before, and now can: with the grid on and **no** Ctrl, the
/// hover must still read the raw pointer. The oracle's site has no gate at all.
///
/// Before the fix this snapped 415 → 420, which is outside the tolerance, and the shape
/// under the pointer went dark — a hover that stops working the moment the grid is turned
/// on, for a point the cursor is plainly on.
#[test]
fn a_hover_reads_the_raw_pointer_with_the_grid_on_and_no_ctrl() {
    let mut engine = hover_board(true);
    engine.hover_pointer(HOVER_AT.0, HOVER_AT.1);
    let id = the_shape_id(&engine);
    assert_eq!(
        engine
            .paint_view()
            .binding_highlight
            .map(|el| el.id.as_str()),
        Some(id.as_str()),
        "the raw pointer is 10 units from the outline, inside the tolerance of 15"
    );
}

/// The marked midpoint is where the arrow's tail would anchor, and it is computed from the
/// same point the hit test used — so it is the second, independent witness that the read
/// was raw. Before the fix there was no mark at all, for the same reason there was no
/// highlight.
#[test]
fn a_hover_marks_the_midpoint_it_actually_read() {
    let mut engine = hover_board(true);
    engine.hover_pointer(HOVER_AT.0, HOVER_AT.1);
    let (mark, _snaps) = engine
        .paint_view()
        .binding_midpoint
        .expect("a hovered shape marks its midpoint");
    assert_at("the marked midpoint", (mark.x, mark.y), HOVER_MIDPOINT);
}

/// The hover's Q/R pair, which is not Ctrl at all: **the same pointer on the same board**,
/// once with the grid and once without. One engine, so the element ids are the same
/// element's and the only variable is the grid — two engines would mint two elements and
/// the comparison would prove nothing.
///
/// The oracle reads the raw pointer both ways, so the two must agree exactly: in what it
/// found **and** where it marked it.
#[test]
fn a_hover_reads_the_same_point_with_and_without_the_grid() {
    let mut engine = hover_board(false);
    let id = the_shape_id(&engine);
    engine.hover_pointer(HOVER_AT.0, HOVER_AT.1);
    let without = (
        engine
            .paint_view()
            .binding_highlight
            .map(|el| el.id.clone()),
        engine
            .paint_view()
            .binding_midpoint
            .map(|(p, _)| (p.x, p.y)),
    );

    engine.set_grid(GridSettings {
        enabled: true,
        size: 10.0,
        step: 5,
        snap: true,
    });
    engine.hover_pointer(HOVER_AT.0, HOVER_AT.1);
    let with = (
        engine
            .paint_view()
            .binding_highlight
            .map(|el| el.id.clone()),
        engine
            .paint_view()
            .binding_midpoint
            .map(|(p, _)| (p.x, p.y)),
    );

    assert_eq!(
        without.0.as_deref(),
        Some(id.as_str()),
        "the raw read found nothing"
    );
    assert_eq!(with.0, without.0, "the grid changed what a hover found");
    assert_eq!(
        with.1, without.1,
        "the grid changed where a hover marked its midpoint"
    );
}

/// The same rule swept: every pointer in a band around the tolerance edge must give the
/// same answer with the grid on as with it off. One engine per point, the grid toggled
/// between the two reads.
#[test]
fn a_hover_ignores_the_grid_across_the_tolerance_edge() {
    let mut engine = hover_board(false);
    for x in [404.0, 408.0, 412.0, 415.0, 418.0, 420.0, 422.0] {
        for y in [310.0, 345.0, 380.0] {
            engine.hover_pointer(x, y);
            let off_hit = engine
                .paint_view()
                .binding_highlight
                .map(|el| el.id.clone());
            let off_mark = engine
                .paint_view()
                .binding_midpoint
                .map(|(p, _)| (p.x, p.y));
            engine.set_grid(GridSettings {
                enabled: true,
                size: 10.0,
                step: 5,
                snap: true,
            });
            engine.hover_pointer(x, y);
            let on_hit = engine
                .paint_view()
                .binding_highlight
                .map(|el| el.id.clone());
            let on_mark = engine
                .paint_view()
                .binding_midpoint
                .map(|(p, _)| (p.x, p.y));
            assert_eq!(
                on_hit, off_hit,
                "at ({x}, {y}) the grid changed what the hover found: {off_hit:?} became {on_hit:?}"
            );
            assert_eq!(
                on_mark, off_mark,
                "at ({x}, {y}) the grid moved the marked midpoint"
            );
        }
    }
}

/// Nothing is highlighted far from the outline, with the grid on and off alike — so "the
/// grid changed nothing" above is not passing because everything matches everything.
#[test]
fn a_hover_still_finds_nothing_when_it_is_far_away() {
    for grid in [true, false] {
        let mut engine = hover_board(grid);
        engine.hover_pointer(700.0, 100.0);
        assert!(
            engine.paint_view().binding_highlight.is_none(),
            "grid {grid}: something was highlighted a long way off"
        );
    }
}

fn the_shape_id(engine: &DrawEngine) -> String {
    engine
        .get_scene()
        .into_iter()
        .next()
        .expect("one shape on the board")
        .id
}

// ---------------------------------------------------------------------------
// Site 3 — the sticky note placed by a click (pointer_end.rs:453)
//
// Found by the sweep, named by nobody. `App.tsx@1118751f:11825`: a gesture under the
// drag threshold is a click, the default square is centred on the raw press, and only
// then is it snapped, under the same `childEvent[CTRL_OR_CMD] ? null : gridSize`.
// ---------------------------------------------------------------------------

/// A sticky-note tool, the grid on. A **click** — no move — is what takes this path: the
/// note keeps the drag threshold's own "too small to be a drag" test, so a move would
/// take the other branch and never reach the line under test.
fn sticky_board() -> DrawEngine {
    let mut engine = engine();
    engine.set_grid(grid_on());
    engine.set_tool(DrawTool::StickyNote);
    engine
}

fn the_note(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::StickyNote)
        .expect("no sticky note was placed")
}

/// The default square's side, as the oracle sizes a clicked note
/// (`DEFAULT_STICKY_NOTE_SIZE`). Read through the placed note rather than a literal, so
/// the test states the landing and not the constant.
fn note_side(engine: &DrawEngine) -> f64 {
    let note = the_note(engine);
    assert_eq!(
        note.width, note.height,
        "a clicked note is the default square"
    );
    note.width
}

/// A sticky-note click, with Ctrl held throughout — the press **and** the release, because
/// the oracle reads the modifier on the event that *ends* the gesture
/// (`childEvent[KEYS.CTRL_OR_CMD]`, `App.tsx@1118751f:11825`). Letting Ctrl go before the
/// release would be a different gesture: the key came up before the button did.
fn clicked_note(ctrl: bool) -> DrawEngine {
    let mut engine = sticky_board();
    engine.set_ctrl_held(ctrl);
    engine.begin_pointer(137.0, 123.0, false, false);
    engine.end_pointer();
    engine.set_ctrl_held(false);
    engine
}

/// Q. A clicked sticky note lands centred on the press and then on the grid, half a size
/// before the snap — so the corner is not simply `press` rounded.
#[test]
fn a_clicked_sticky_note_lands_on_the_grid() {
    let engine = clicked_note(false);
    let side = note_side(&engine);
    let note = the_note(&engine);
    assert_at(
        "Q: the grid snap did not happen",
        (note.x, note.y),
        get_grid_point(137.0 - side / 2.0, 123.0 - side / 2.0, Some(GRID)),
    );
}

/// R. The same click on the same board with Ctrl held: the oracle passes a `null` grid, so
/// the square stays centred on the raw press.
#[test]
fn holding_ctrl_takes_a_clicked_sticky_note_off_the_grid() {
    let engine = clicked_note(true);
    let side = note_side(&engine);
    let note = the_note(&engine);
    assert_at(
        "R: the grid still snapped under Ctrl",
        (note.x, note.y),
        (137.0 - side / 2.0, 123.0 - side / 2.0),
    );
}

/// Both halves, and the pair must differ.
#[test]
fn a_clicked_sticky_note_snaps_unless_ctrl_is_held() {
    let snapped = the_note(&clicked_note(false));
    let free = the_note(&clicked_note(true));
    let (a, b) = ((snapped.x, snapped.y), (free.x, free.y));
    let raw = (137.0 - snapped.width / 2.0, 123.0 - snapped.width / 2.0);
    assert_at("snapped", a, get_grid_point(raw.0, raw.1, Some(GRID)));
    assert_at("ctrl held", b, raw);
    assert_ne!(a, b, "the pair must differ, or one side is not answering");
}

// ---------------------------------------------------------------------------
// The duplicate — the one call site that is NOT a Ctrl site
//
// `App.duplicate.ts@1118751f:95-99` passes `getEffectiveGridSize()` **bare**: no
// modifier test at all. Duplicating grid-snaps, Ctrl held or not, and that is deliberate
// in both trees.
// ---------------------------------------------------------------------------

/// Duplicates the selection **at a point**, the oracle's `duplicateAtSceneCoords` — which
/// is the path that carries the grid. The engine reaches it through
/// `paste_json(json, Some(at))` (`clipboard.rs:372` → `place_json:420` → `snap:433`); the
/// `duplicate_selection` entry point is Ctrl+D, which offsets by a fixed amount instead
/// and has no grid in it at all.
fn duplicate_at(engine: &mut DrawEngine, at: (f64, f64)) -> DrawElement {
    let json = engine.copy_selection().expect("something to copy");
    let before = engine.get_scene().len();
    assert!(
        engine.paste_json(Some(&json), Some(at)),
        "the duplicate did not land"
    );
    let scene = engine.get_scene();
    assert_eq!(
        scene.len(),
        before + 1,
        "the duplicate did not add one element"
    );
    scene.into_iter().last().expect("the copy is last").clone()
}

/// Q. Duplicating at a point **without** Ctrl still snaps: the oracle passes
/// `getEffectiveGridSize()` bare (`App.duplicate.ts@1118751f:95-99`).
#[test]
fn duplicating_at_a_point_snaps_to_the_grid() {
    let (mut engine, _id) = selected_box();
    let copy = duplicate_at(&mut engine, (237.0, 223.0));
    let (_, half) = {
        let el = the_box(&engine, engine.get_scene()[0].id.as_str());
        ((el.width, el.height), (el.width / 2.0, el.height / 2.0))
    };
    let want = get_grid_point(237.0 - half.0, 223.0 - half.1, Some(GRID));
    assert_at("duplicate Q", (copy.x, copy.y), want);
}

/// R. Duplicating at a point **with** Ctrl **also** snaps — the deliberate exception. If
/// this ever fails, the gate has leaked into a call site the oracle leaves bare, which is
/// the failure 5.2's sibling function was shaped to make impossible: a boolean threaded
/// through would have had to be threaded into this one too.
#[test]
fn duplicating_at_a_point_still_snaps_under_ctrl() {
    let (mut engine, _id) = selected_box();
    let el = the_box(&engine, engine.get_scene()[0].id.as_str());
    let half = (el.width / 2.0, el.height / 2.0);
    engine.set_ctrl_held(true);
    let copy = duplicate_at(&mut engine, (237.0, 223.0));
    engine.set_ctrl_held(false);
    let want = get_grid_point(237.0 - half.0, 223.0 - half.1, Some(GRID));
    assert_at(
        "Ctrl took the duplicate off the grid, but App.duplicate.ts passes it bare",
        (copy.x, copy.y),
        want,
    );
}

// ---------------------------------------------------------------------------
// The property sweep
//
// One property, over every gesture, so a call site added later is caught by the sweep
// rather than by a reviewer's eye: **for every pointer gesture in the engine, Ctrl held
// means the raw pointer and Ctrl released means `getGridPoint`** — except the hover,
// which the oracle reads raw both ways, and duplicate, which the oracle snaps both ways.
// Both exceptions are stated as themselves, not folded in.
// ---------------------------------------------------------------------------

/// Every point of the cell, of the half-cell and of the tie, on both sides, negative
/// included. `lattice()` is the brief's "whole cells, quarter cells, both sides of every
/// tie, negative coordinates" made concrete: for a 20-unit grid that is 0, 5, 10, 15, 20
/// in every sign, and the offsets that land on a tie (10) and just either side of one
/// (10 ± 0.5, ± 1).
fn lattice() -> Vec<f64> {
    let mut out = Vec::new();
    for base in [0.0, 20.0, 40.0, 60.0, 80.0] {
        for offset in [0.0, 0.5, 5.0, 9.5, 10.0, 10.5, 15.0, 19.5] {
            for sign in [1.0, -1.0] {
                out.push(sign * (base + offset));
            }
        }
    }
    out
}

/// 1×, 2×, 0.5× and 3×, each with a fractional pan, so the pointer's screen coordinate
/// is not the world one and the camera's own arithmetic is inside every case.
fn cameras() -> Vec<Camera> {
    [1.0, 2.0, 0.5, 3.0]
        .into_iter()
        .flat_map(|scale| {
            [
                Camera {
                    x: 0.0,
                    y: 0.0,
                    scale,
                },
                Camera {
                    x: 13.7,
                    y: -7.25,
                    scale,
                },
            ]
        })
        .collect()
}

fn on_grid(v: f64) -> bool {
    (v / GRID).fract().abs() < RAW
}

/// Where a box dragged from `from` to `to` puts its top-left corner, through the oracle's
/// `getGridPoint` on **both** ends and then the smaller of the two per axis — a drag to
/// the left or up has its origin at the release, not the press, and reading the press
/// alone makes a correct engine look wrong.
fn drawn_origin(from: (f64, f64), to: (f64, f64), ctrl: bool) -> (f64, f64) {
    let grid = (!ctrl).then_some(GRID);
    let a = get_grid_point(from.0, from.1, grid);
    let b = get_grid_point(to.0, to.1, grid);
    (a.0.min(b.0), a.1.min(b.1))
}

/// The property, for one camera and one pair of points: draw a rectangle from `from` to
/// `to`, and the same with Ctrl. Q's origin is the grid corner and R's the raw one.
fn sweep_draw(camera: Camera, from: (f64, f64), to: (f64, f64)) {
    for (ctrl, label) in [(false, "Q"), (true, "R")] {
        let mut engine = engine();
        engine.set_camera(camera);
        engine.set_grid(grid_on());
        engine.set_tool(DrawTool::Rectangle);
        let p = world_to_screen(camera, from.0, from.1);
        let q = world_to_screen(camera, to.0, to.1);
        engine.set_ctrl_held(ctrl);
        engine.begin_pointer(p.x, p.y, false, false);
        engine.move_pointer(q.x, q.y, false, false);
        engine.set_ctrl_held(false);
        engine.end_pointer();
        let el = engine
            .get_scene()
            .into_iter()
            .next()
            .expect("a box was drawn");
        // The expectation is built in **world** coordinates and the reading is in world
        // coordinates too, so the camera cancels — which is the point of a camera sweep:
        // a bug that only appears at 2× is a bug in the round trip, and this compares what
        // the pointer meant rather than what its screen coordinate was.
        let want = drawn_origin(from, to, ctrl);
        assert_at(
            &format!("draw {label} {from:?}->{to:?} on {camera:?}"),
            (el.x, el.y),
            want,
        );
    }
}

/// The property, for one camera and one delta: drag a selected box's south-east handle,
/// snapped and raw. The press takes the handle where it is drawn and the expectation is
/// read off the board, so a camera other than 1:1 goes through the round trip and the
/// unswept cases would not pass with that step missing.
fn sweep_resize(camera: Camera, delta: (f64, f64)) {
    for (ctrl, label) in [(false, "Q"), (true, "R")] {
        let (mut engine, id) = selected_box();
        engine.set_camera(camera);
        let kind = HandleKind::Se;
        let want = want_edge(&engine, &id, kind, delta, ctrl);
        if ctrl {
            drag_handle_with_ctrl(&mut engine, &id, kind, delta);
        } else {
            drag_handle(&mut engine, &id, kind, delta);
        }
        assert_at(
            &format!("resize {label} by {delta:?} on {camera:?}"),
            landed(&engine, &id, kind),
            want,
        );
    }
}

/// The whole sweep: every gesture, every camera, every point of the lattice, as a Q/R
/// pair. This is the test that catches a call site nobody named.
#[test]
fn every_gesture_answers_to_ctrl_over_the_whole_lattice() {
    for camera in cameras() {
        for &x in &lattice() {
            for &y in &lattice() {
                sweep_draw(camera, (x, y), (x + 37.0, y + 23.0));
                sweep_resize(camera, (37.0, 23.0));
                // The resize's deltas sweep the lattice's own values too, so a tie reaches
                // the resize as well as the press — and the resize reads a *different*
                // point from the press (it subtracts the grab first), so a tie broken
                // wrongly at one of them need not be wrong at the other.
                sweep_resize(camera, (10.0, -10.0));
                sweep_resize(camera, (-10.0, 10.0));
                sweep_resize(camera, (0.0, 0.0));
                sweep_resize(camera, (100.0, 100.0));
            }
        }
    }
}

/// The tie on both sides, for both gestures, at every camera. Split out from the sweep
/// because the lattice's ties are already in it and this names them: the case that
/// distinguishes `Math.round` from `f64::round` and is invisible to positive-only input.
#[test]
fn a_gesture_on_a_tie_breaks_it_towards_positive_infinity() {
    for camera in cameras() {
        for &sign in &[1.0, -1.0] {
            for delta in [10.0, 10.5, 9.5, -10.0, -10.5, -9.5] {
                // 30 on a 20-grid is a half-cell: 30 / 20 = 1.5, and `Math.round(1.5)` is
                // 2, so the oracle puts it on 40. `f64::round(1.5)` agrees here — the sign
                // only matters on the negative side, which `sign` supplies.
                let from = (sign * 30.0, sign * 30.0);
                let to = (from.0 + delta, from.1 + 40.0);
                let mut engine = engine();
                engine.set_camera(camera);
                engine.set_grid(grid_on());
                engine.set_tool(DrawTool::Rectangle);
                let p = world_to_screen(camera, from.0, from.1);
                let q = world_to_screen(camera, to.0, to.1);
                engine.begin_pointer(p.x, p.y, false, false);
                engine.move_pointer(q.x, q.y, false, false);
                engine.end_pointer();
                let el = engine
                    .get_scene()
                    .into_iter()
                    .next()
                    .expect("a drag this long is kept");
                assert_at(
                    &format!("tie {from:?} + {delta} on {camera:?}"),
                    (el.x, el.y),
                    drawn_origin(from, to, false),
                );
            }
        }
    }
}

/// The negative half-cell, named: x = -10 on a 20-grid belongs on 0. The oracle's
/// `Math.round(-0.5) === -0`; `f64::round` would say -20, a whole cell away, and only a
/// negative case can see it.
#[test]
fn a_negative_half_cell_belongs_to_zero() {
    for camera in cameras() {
        for &v in &[-10.0, -30.0, -50.0] {
            let mut engine = engine();
            engine.set_camera(camera);
            engine.set_grid(grid_on());
            engine.set_tool(DrawTool::Rectangle);
            let p = world_to_screen(camera, v, v);
            engine.begin_pointer(p.x, p.y, false, false);
            engine.end_pointer();
            let drawn = engine.get_scene();
            let el = match drawn.first() {
                Some(el) => el,
                None => continue, // too small to keep: a click, not a drag
            };
            assert!(
                on_grid(el.x) && on_grid(el.y),
                "({v}, {v}) landed at ({}, {}), which is off the grid",
                el.x,
                el.y
            );
        }
    }
}
