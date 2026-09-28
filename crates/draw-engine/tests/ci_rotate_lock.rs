//! Shift while turning, and how it is *not* the same lock as Shift while dragging a line.
//!
//! # One constant, one rounding — and a range that matters more than either
//!
//! Both locks are driven by the same `shiftKey`
//! (`packages/common/src/keys.ts@1118751f:151-153`, `shouldRotateWithDiscreteAngle`) and
//! both step by `SHIFT_LOCKING_ANGLE` (`packages/common/src/constants.ts@1118751f:31`,
//! 15 degrees). In the oracle they are also the **same rounding**:
//!
//! - a linear drag is `Math.round(angle / step) * step`
//!   (`sizeHelpers.ts@1118751f:198-199`) — to the nearest, tie toward +infinity;
//! - a rotation is `angle += step / 2; angle -= angle % step`
//!   (`resizeElements.ts@1118751f:230-231`) — which looks like a *floor*, and is not
//!   quite, because `%` truncates toward zero.
//!
//! The floor only behaves like a floor because of how the raw angle is built:
//! `5 * PI / 2 + atan2(..)` (`resizeElements.ts@1118751f:227`), two and a half turns up,
//! so the number being floored is in `(3PI/2, 7PI/2]` and is **never negative**.
//! `2 * PI` is 24 whole steps, so normalising an angle into `[0, 2PI)` moves it by a whole
//! number of steps and cannot change which cell it is in — which is what makes the two
//! rules the same rule, and what [`crate::math::shift_locked_angle`] relies on.
//!
//! Feed that floor a *negative* angle and `%` opens a cell **twice as wide** around zero:
//! everything within 7.5 degrees either side of straight up folds onto 0. Our
//! `rotate_element` builds its raw angle as `PI / 2 + atan2(..)`
//! (`selection/transform.rs:293`) — the same angle, a whole turn smaller — so it *does* go
//! negative, and a Shift-locked turn used to snap the entire lower-left quadrant to 0
//! instead of 345, 330, 315. `a_turn_and_a_line_agree_on_the_negative_side` is that bug's
//! test, swept across the whole quadrant.
//!
//! So: one constant, one rounding primitive (`math::round_half_up`), and two functions
//! because the rotation also normalises and the drag does not. The linear lock's own
//! geometry — the projection, which is the half a "just change the constant" fix leaves
//! behind — is held in `ci_end_snap.rs`.
//!
//! # What was missing here
//!
//! `rotate_element` took only the pointer position, so a turn was not quantised at all.
//! Shift arrived at the gesture and went unread: `move_pointer` already carries it as
//! `square` (`engine/src/host/pointerInput.ts:27`, `event.shiftKey`) and the rotate branch
//! ignored it. `shouldRotateWithDiscreteAngle` is a parameter in the oracle too
//! (`resizeElements.ts@1118751f:215`), so the port keeps it a parameter.

mod common;
use common::*;
use draw_engine::*;

/// How far the pointer sits from the element's centre, in world units. The lock is on the
/// angle, so any distance works, but a long one keeps the geometry clear of its own
/// rounding: a pointer a pixel from the centre would put every case in the noise.
const REACH: f64 = 120.0;

/// Tolerance in degrees. Every comparison is between two pipelines ending in the same
/// `to_degrees()`, so the only slack is the round trip through the pointer.
const NEAR: f64 = 1e-9;

/// A rectangle selected and ready to turn, at a viewport 1:1 with nothing to snap to.
fn selected_rect() -> (DrawEngine, String) {
    let rect = box_at(300.0, 200.0, 120.0, 80.0);
    let id = rect.id.clone();
    let mut engine = engine_with_scene(vec![rect]);
    engine.set_tool(DrawTool::Select);
    engine.select(vec![id.clone()]);
    (engine, id)
}

fn the_rect(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .expect("the rectangle vanished")
}

/// The world position of the rotation handle, which is where a turn has to be pressed.
fn rotate_handle(engine: &DrawEngine, id: &str) -> Point {
    let element = the_rect(engine, id);
    let view = engine.paint_view();
    selection_handles(&element, view.handle_layout)
        .into_iter()
        .find(|h| h.kind == HandleKind::Rotate)
        .map(|h| Point { x: h.x, y: h.y })
        .expect("a selected rectangle offers a rotation handle")
}

/// The centre `pointer_for` aims from, so a reference and a gesture can be handed the same
/// two numbers.
fn centre_of(element: &DrawElement) -> (f64, f64) {
    // The oracle's centre, from the element's absolute coords rather than the engine's own
    // pivot — the reason `ci_line_multipoint.rs` and `ci_image.rs` both say so.
    let box_ = element_bounds(element);
    (
        (box_.min_x + box_.max_x) / 2.0,
        (box_.min_y + box_.max_y) / 2.0,
    )
}

/// Where the pointer has to be for the turn to report `degrees`.
///
/// `rotate_element` is `atan2(pointer - centre) + PI / 2` (`selection/transform.rs:292-294`),
/// so the reported angle is the pointer's own bearing turned a quarter: a pointer due
/// **right** of the centre reads 90 degrees. Derived rather than tabulated, so a change to
/// the pivot shows up here as a wrong angle rather than as a stale constant.
fn pointer_for(element: &DrawElement, degrees: f64) -> Point {
    let centre = centre_of(element);
    let bearing = degrees.to_radians() - std::f64::consts::FRAC_PI_2;
    Point {
        x: centre.0 + REACH * bearing.cos(),
        y: centre.1 + REACH * bearing.sin(),
    }
}

fn angle_of(engine: &DrawEngine, id: &str) -> f64 {
    the_rect(engine, id).angle.to_degrees().rem_euclid(360.0)
}

/// A press on the handle, a move, a release — the whole of a turn.
fn turn_to(engine: &mut DrawEngine, handle: Point, to: Point, shifted: bool) {
    engine.begin_pointer(handle.x, handle.y, false, false);
    engine.move_pointer(to.x, to.y, shifted, false);
    engine.end_pointer();
}

/// Every twentieth of a degree across `from..to`, **missing every tie**.
///
/// The half-way points sit at 7.5 + 15k, and a sweep on a plain 0.1 grid lands on all of
/// them. A pointer is built with `cos`/`sin` and read back with `atan2`, so such an angle
/// comes back within an ulp of half way and lands on either neighbouring step depending on
/// which way that ulp fell — for the oracle exactly as much as for this engine. Half a
/// hundredth of a degree off every tie keeps the sweep away from the knife edge; the ties
/// themselves are stated exactly, in `a_tie_breaks_toward_positive_infinity`.
fn sweep_degrees(from: f64, to: f64) -> impl Iterator<Item = f64> {
    let steps = ((to - from) * 20.0).round() as i32;
    (0..steps).map(move |n| from + (f64::from(n) + 0.5) / 20.0)
}

/// The oracle's whole rotation lock, transcribed from the **pointer** and not from a
/// nominal angle: `resizeElements.ts@1118751f:227-233` — the raw angle, the floored step
/// when Shift is down, and `normalizeRadians` (`packages/math/src/angle.ts@1118751f:11-14`).
///
/// Two details in here are the test. The input is the **pointer**, because
/// `rotate_element` works from `atan2(..) + PI / 2` and never sees a wrapped angle at all —
/// a reference fed "the -360 degrees I asked for" would be comparing against an input the
/// engine was never given, and would disagree with it by whole steps. And the offset is
/// the oracle's **`5 * PI / 2`**, not the engine's `PI / 2`: with the smaller offset this
/// transcription agrees with the engine on every positive angle and quietly hides the
/// double-wide zero cell it exists to catch.
fn oracle_turn(to: Point, centre: (f64, f64)) -> f64 {
    let raw = (to.y - centre.1).atan2(to.x - centre.0) + 5.0 * std::f64::consts::FRAC_PI_2;
    let step = std::f64::consts::PI / 12.0;
    let shifted = raw + step / 2.0;
    (shifted - shifted % step).to_degrees().rem_euclid(360.0)
}

/// **Q** — the half that can fail. A turn with Shift lands on **exactly** 15, 30, 165 and
/// 345 degrees, and on nothing between. Each raw angle is 5 degrees off its step, so a
/// build that ignored Shift, or that stepped by 45, would report the raw angle itself.
///
/// Asserted twice over: against the numbers written out, which is what a person reads, and
/// against the oracle's own lock fed the same pointer, which is what stops the two drifting.
#[test]
fn shift_turns_to_exactly_15_30_165_and_345_degrees() {
    for (raw, want) in [(20.0, 15.0), (35.0, 30.0), (160.0, 165.0), (-20.0, 345.0)] {
        let (mut engine, id) = selected_rect();
        let element = the_rect(&engine, &id);
        let centre = centre_of(&element);
        let to = pointer_for(&element, raw);
        let handle = rotate_handle(&engine, &id);
        turn_to(&mut engine, handle, to, true);
        let got = angle_of(&engine, &id);
        assert!(
            (got - want).abs() < NEAR,
            "a turn to {raw} degrees landed on {got}, not {want}"
        );
        let oracle = oracle_turn(to, centre);
        assert!(
            (got - oracle).abs() < NEAR,
            "a turn to {raw} degrees landed on {got}, the oracle says {oracle}"
        );
    }
}

/// **R** — the other half, the identical gesture with nothing held: the angle is the raw
/// one, to the last decimal. A lock applied unconditionally fails here, which is what makes
/// the pair a pair.
#[test]
fn without_shift_a_turn_is_the_pointer_angle_exactly() {
    for raw in [20.0, 35.0, 160.0, -20.0] {
        let (mut engine, id) = selected_rect();
        let element = the_rect(&engine, &id);
        let handle = rotate_handle(&engine, &id);
        turn_to(&mut engine, handle, pointer_for(&element, raw), false);
        let want = raw.rem_euclid(360.0);
        let got = angle_of(&engine, &id);
        assert!(
            (got - want).abs() < NEAR,
            "an unshifted turn to {raw} degrees landed on {got}, not {want}"
        );
    }
}

/// A turn and a line offered the **same raw angle** must land on the same step, on both
/// sides of the top. They are the same rounding in the oracle, and the only way they can
/// differ is a negative angle handed to a floor that never sees one — which snaps the whole
/// lower-left quadrant to 0 rather than to 345, 330, 315.
///
/// A whole quadrant is swept, not the four numbers in the pair above: this is not a
/// boundary case, it is an entire region.
#[test]
fn a_turn_and_a_line_agree_on_the_negative_side() {
    for raw in sweep_degrees(-180.0, 0.0) {
        let r = raw.to_radians();
        let (dx, dy) = constrain_to_angle(200.0 * r.cos(), 200.0 * r.sin());
        let dragged = dy.atan2(dx).to_degrees().rem_euclid(360.0);

        let (mut engine, id) = selected_rect();
        let element = the_rect(&engine, &id);
        let handle = rotate_handle(&engine, &id);
        turn_to(&mut engine, handle, pointer_for(&element, raw), true);
        let turned = angle_of(&engine, &id);

        assert!(
            (dragged - turned).abs() < NEAR,
            "a raw {raw} degrees is {dragged} as a line drag and {turned} as a turn"
        );
    }
}

/// Swept, not sampled: every twentieth of a degree across the whole turn, the locked angle
/// is the oracle's, fed the identical pointer. This is the property that holds the step, the
/// rounding and the normalisation at once — a 45-degree step and a build that ignored Shift
/// both pass the pair above on four numbers and fail here.
#[test]
fn every_raw_turn_lands_on_the_oracles_step() {
    let (mut engine, id) = selected_rect();
    let element = the_rect(&engine, &id);
    let centre = centre_of(&element);
    for raw in sweep_degrees(-360.0, 360.0) {
        let to = pointer_for(&element, raw);
        let handle = rotate_handle(&engine, &id);
        turn_to(&mut engine, handle, to, true);
        let want = oracle_turn(to, centre);
        let got = angle_of(&engine, &id);
        assert!(
            (got - want).abs() < NEAR,
            "a turn to {raw} degrees landed on {got}, the oracle says {want}"
        );
    }
}

/// **The tie rule, where it can actually be held.** `Math.round` breaks a tie **toward
/// +infinity** — `Math.round(-0.5) === -0` — and `f64::round` breaks it *away from zero*,
/// so a port that used `.round()` is a whole 15-degree step out on every negative half-step.
/// Both Shift locks are built on this one rounding, so it is the root of the rule and it is
/// exact in floating point: a half is a half.
///
/// The **composed** result cannot be tested this way, and the reason is worth recording. An
/// angle exactly half way between two steps is not representable once it is in radians:
/// `337.5deg.to_radians() + 7.5deg.to_radians()` lands an ulp either side of
/// `345deg.to_radians()`, and which side decides between 345 and 330. The oracle is knife-
/// edged in the same place — its `angle` comes from `atan2`, so it is not exactly half way
/// either. `sweep_degrees` steps around every tie for this reason, and the best that can be
/// said about one is that either neighbour is defensible.
#[test]
fn a_tie_breaks_toward_positive_infinity() {
    for (tie, want) in [
        (-0.5, 0.0),
        (0.5, 1.0),
        (-1.5, -1.0),
        (1.5, 2.0),
        (-2.5, -2.0),
    ] {
        let got = round_half_up(tie);
        assert_eq!(got, want, "a tie at {tie} went to {got}, not {want}");
    }
    // And that this is the disagreement it exists to prevent: `f64::round` is the other
    // answer, and on every negative tie it is a whole step away.
    assert_eq!(
        (-0.5f64).round(),
        -1.0,
        "f64::round is expected to differ here"
    );
    assert_eq!(round_half_up(-0.5), 0.0, "and to be the one we do not want");
}

/// The lock is **idempotent**, which is what lets a turn be locked on every pointer move of
/// the drag rather than only its first: a lock that kept re-rounding would walk the angle
/// along instead of holding it.
///
/// A fresh engine each time, deliberately. Reusing one would move the rotation handle out
/// from under the next press, and a press that misses the handle starts some other gesture
/// — the angle then silently keeps its old value and the assertion fails on a test that was
/// never measuring the lock.
#[test]
fn locking_an_already_locked_turn_changes_nothing() {
    for locked in [0.0, 15.0, 30.0, 90.0, 165.0, 345.0] {
        let (mut engine, id) = selected_rect();
        let element = the_rect(&engine, &id);
        let handle = rotate_handle(&engine, &id);
        turn_to(&mut engine, handle, pointer_for(&element, locked), true);
        let once = angle_of(&engine, &id);
        assert!(
            (once - locked).abs() < NEAR,
            "an already-locked {locked} degrees moved to {once}"
        );
        turn_to(&mut engine, handle, pointer_for(&element, once), true);
        let twice = angle_of(&engine, &id);
        assert!(
            (once - twice).abs() < NEAR,
            "locking {locked} degrees twice moved it: {once} then {twice}"
        );
    }
}
