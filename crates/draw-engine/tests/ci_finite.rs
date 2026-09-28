//! Two ways a non-finite number walks into the scene, and where the paint stops.
//!
//! Nothing here is fixed. Every one of these is a Phase 1 item under BUNNY.md §3.4 ("a
//! test that exposes a bug stops and reports it"), and the reachable one is a user
//! opening a file. `~/bunny/reports/t2-nan-ingress.md` has the traced path and the
//! owner's decision.
//!
//! **The pinning form.** Each test asserts ONE defective answer in a test that RUNS —
//! the hand-written equivalent of vitest's `it.fails`, which is what this repo already
//! uses for its one known failure
//! (`apps/web/src/lib/autosave/sceneDiff.merges.test.ts`, 3f-ii, root `c5988b3`):
//! "it cannot be deleted later as obsolete: it is green only for as long as the
//! divergence is there." Stable `libtest` has no `it.fails`.
//!
//! **One assertion per red is the point, and it is why this file is many small tests
//! rather than two.** The 3f-ii comment says why, and round 1 of this file broke it:
//! *"It carries ONE assertion, so a red here can only be the resurrection."* Eight
//! ratchets in one test means a red names only the *first* one that changed. So each
//! ratchet is its own `#[test]`, named for the behaviour it pins, and each keeps the
//! control that makes it mean something — an ordinary camera, an ordinary board, or an
//! ordinary file — in the same test, running first. A red therefore names exactly one
//! cause, and the control says the cause is not "everything is rejected now".
//!
//! The two alternatives, and why not:
//!
//! - `#[ignore = "reason"]`, which the brief proposed. MEASURED on `rustc 1.98.1`, and
//!   the opposite of what I expected: the reason IS printed —
//!   `test name ... ignored, REASON STRING: …`. So the skip is not silent. What it
//!   cannot do is RUN, and that is the whole argument: a number nobody evaluates is a
//!   comment, and a guard landing in `camera.rs` or `json.rs` leaves it skipped forever
//!   with nothing to notice. The direction that matters is a *fix* arriving, and an
//!   ignore cannot report it.
//! - An ignored specification beside each ratchet. The strongest form, and not taken
//!   only because it doubles a file this long for a spec the doc comment above
//!   already carries.
//!
//! Two more conventions worth stating, because this file has broken both:
//!
//! - **A ratchet names its own cause.** When the fix lands, the message says which
//!   line to look at, so a red is a to-do rather than a mystery.
//! - **The cause is measured, not assumed — and there are three of them.** Round 1
//!   asserted a NaN camera makes the poisoned shape unclickable, then, when that was
//!   falsified, that the shape's own `rotation_center` had overflowed. The second was
//!   falsified the same way, and in the way that matters most: with `angle: 0.0`,
//!   `to_element_local` returns at `geometry.rs:212-214` and NEVER READS the centre —
//!   and the shape is still unhittable. The mechanism is `within_shape`, which computes
//!   a centre of its own: `geometry.rs:502-503` is `x + width / 2`, which overflows to
//!   +inf; `:510-511` divides the probe by it and answers `-inf`; `:524` compares that
//!   infinity against `1.0`. The Q/R pair inside the test below is what pins that
//!   rather than narrates it: same `x`, same `angle: 0.0`, same camera, same probe, and
//!   only `width` between them — Q is hit, R is missed.
//!   A NaN camera is a third cause, independent of both — there is a test for that too —
//!   so no single line is "the" cause, and pinning one you have not isolated is how a
//!   fix gets aimed at the wrong line. This file is the argument: it shipped the wrong
//!   line twice, and a fixer who trusted either sentence would have left the real one
//!   untouched and this ratchet green.
//!
//! Measured on `engine` `886738e`. Reproduce with:
//! `cargo test -p draw-engine --test ci_finite -- --nocapture`

mod common;
use common::*;
use draw_engine::*;

/// `f64::MAX`, spelled the way a file has to spell it: the largest number JSON can
/// carry that an `f64` still holds. `1e400` is refused by the parser as out of range
/// and `NaN` is not JSON at all, so an overflow has to arrive as arithmetic.
const F64_MAX: &str = "1.7976931348623157e308";

/// `assert!(v.is_finite())` that says what it got. A bare assertion on a `NaN` tells
/// the next reader nothing about the shape of the failure in front of them.
#[track_caller]
fn assert_finite(what: &str, got: f64) {
    assert!(
        got.is_finite(),
        "{what} is not finite — it is {got:?}. A non-finite number here is not a wrong \
         answer, it is an absent one: it propagates through every bounds fold and every \
         paint projection from here. Invert this assertion when the guard lands."
    );
}

/// Every `f64` in a box, so "is this box finite" is one question with one answer.
fn box_is_finite(bounds: WorldBounds) -> bool {
    [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .all(|v| v.is_finite())
}

/// A box with no finite corner, which is what a poisoned scene's bounds look like once
/// one element has overflowed.
fn non_finite_bounds() -> WorldBounds {
    WorldBounds {
        min_x: f64::NAN,
        min_y: f64::NAN,
        max_x: f64::NAN,
        max_y: f64::NAN,
    }
}

/// The app's own export for one 100×5 box at the origin, with `field` respelled as
/// `replacement`. Built by exporting rather than hand-written, because `DrawElement`
/// has no defaults: a literal is rejected for missing fields, which would make the
/// accept/refuse table measure nothing at all.
fn document_with(field: &str, replacement: &str) -> String {
    let json = scene_to_json(&[box_at(0.0, 0.0, 100.0, 5.0)]);
    assert!(
        json.contains(field),
        "the exported document no longer spells {field:?}, so this test is measuring a \
         replacement that never landed. Re-read the export before trusting it."
    );
    json.replace(field, replacement)
}

/// A syntactically valid `.osidraw` file whose geometry is at the top of the `f64`
/// range on every axis. Each field is a legal JSON number and a legal `f64`; their SUM
/// is neither, and the sum is what the engine measures with.
fn overflowing_document() -> String {
    document_with("\"x\": 0.0", &format!("\"x\": {F64_MAX}"))
        .replace("\"width\": 100.0", &format!("\"width\": {F64_MAX}"))
        .replace("\"height\": 5.0", &format!("\"height\": {F64_MAX}"))
        .replace("\"angle\": 0.0", "\"angle\": 0.001")
}

/// An engine on an 800×600 board, which is the viewport every test here measures.
fn board() -> DrawEngine {
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    engine
}

/// A board holding one ordinary, solid 200×100 rectangle under the middle of the
/// canvas — the thing a hit test is supposed to find.
fn board_with_a_shape_under_the_pointer() -> DrawEngine {
    let mut engine = board();
    engine.load_scene(&scene_to_json(&[filled(box_at(
        300.0, 200.0, 200.0, 100.0,
    ))]));
    engine
}

/// A camera with no scale at all. Nothing in the engine produces one; it is written
/// here because `DrawEngine::camera` is a `pub` field (`engine/mod.rs:101`) and
/// `set_camera` takes a `Camera` unchecked (`engine/style.rs:675-679`), and `Camera`
/// derives `Deserialize` with no validator (`camera.rs:104`).
fn scaleless_camera() -> Camera {
    Camera {
        x: 0.0,
        y: 0.0,
        scale: 0.0,
    }
}

// ---------------------------------------------------------------------------------------
// (a) The camera. The guard belongs on the WRITE side — `fit_bounds` and
// `zoom_to_fit_bounds` (the two `center_x` computations), `set_camera`, and `zoom_to`'s
// `ratio` — and not on `screen_to_world`, which is a pure read of a value the host is
// about to trust. Guarding the read would silently rewrite geometry and break the
// oracle comparison the camera tests exist to police.
// ---------------------------------------------------------------------------------------

#[test]
fn an_ordinary_camera_roundtrips_at_scale_one_and_at_both_ends_of_the_zoom_range() {
    // The control every ratchet below is measured against, and the reason none of them
    // can be "fixed" by refusing cameras. `MIN_ZOOM` and `MAX_ZOOM` are what `zoom_to`
    // (`camera.rs:166`) clamps to, so a guard sitting inside that range leaves all three
    // of these alone.
    for scale in [1.0, MIN_ZOOM, MAX_ZOOM] {
        let camera = Camera {
            x: 12.3456,
            y: -78.9101,
            scale,
        };
        let back = screen_to_world(camera, 300.0, 200.0);
        assert_finite(&format!("a camera at scale {scale}, answering x"), back.x);
        assert_finite(&format!("a camera at scale {scale}, answering y"), back.y);
        assert_finite(
            &format!("a camera at scale {scale}, fitting a box"),
            fit_bounds(
                WorldBounds {
                    min_x: 0.0,
                    min_y: 0.0,
                    max_x: 100.0,
                    max_y: 50.0,
                },
                800.0,
                600.0,
                20.0,
            )
            .x,
        );
    }
}

#[test]
fn a_point_off_a_camera_with_no_scale_is_answered_with_a_signed_infinity() {
    // `screen_to_world` divides by `scale` (`camera.rs:154-159`) and `100 / 0` is an
    // infinity, not a `NaN`. Pinning the infinity as its own value is the point: the
    // two answers are different numbers and a test that says "non-finite" without
    // saying which has not pinned either.
    let away = screen_to_world(scaleless_camera(), 100.0, 100.0);
    assert_eq!(
        (away.x, away.y),
        (f64::INFINITY, f64::INFINITY),
        "MEASURED on 886738e: a point off a zero-scale camera is 100 / 0 = +inf on both \
         axes. If this changed, the division moved — invert this assertion and assert the \
         finite answer once a guard exists."
    );
}

#[test]
fn the_signed_infinity_a_scaleless_camera_answers_keeps_the_sign_of_its_input() {
    // −7 / 0 is −inf, and 3 / 0 is +inf, in the same call. The 3g builder expected a
    // `NaN` here and this is the case that says why it does not get one: the numerator
    // is not zero, so the division never reaches `0 / 0`.
    let mixed = screen_to_world(scaleless_camera(), -7.0, 3.0);
    assert_eq!(
        mixed,
        Point {
            x: f64::NEG_INFINITY,
            y: f64::INFINITY
        },
        "MEASURED on 886738e: a point at (-7, 3) on a zero-scale camera is (-inf, +inf) \
         — signed per axis, not NaN. Invert when the guard lands."
    );
}

#[test]
fn a_point_sitting_on_a_camera_with_no_scale_is_answered_with_a_nan() {
    // The `0 / 0` case, and the only one: both numerators are zero, so this is the one
    // place a scaleless camera answers a NaN rather than an infinity.
    let on = screen_to_world(scaleless_camera(), 0.0, 0.0);
    assert!(
        on.x.is_nan() && on.y.is_nan(),
        "MEASURED on 886738e: a point sitting exactly on a zero-scale camera is 0 / 0 = \
         NaN. Got ({}, {}) instead. Invert when the guard lands.",
        on.x,
        on.y
    );
}

#[test]
fn one_visible_world_rect_call_answers_both_a_nan_and_an_infinity() {
    // The most-quoted sentence about this defect lived in a comment for a whole round.
    // It is arithmetic and it is pinnable: `visible_world_rect` (`camera.rs:200-209`)
    // converts two corners, the top-left one sits on the camera and is `0 / 0`, and the
    // bottom-right is `width / 0`. One call, both answers — which is why "the value is
    // NaN" and "the value is infinite" were both right and neither was the whole story.
    let visible = visible_world_rect(scaleless_camera(), 800.0, 600.0);
    assert!(
        visible.min_x.is_nan() && visible.max_x == f64::INFINITY,
        "MEASURED on 886738e: a zero-scale camera's visible rect is NaN at the top-left \
         and +inf at the bottom-right, in the same call. Got {visible:?} instead. Invert \
         when the guard lands."
    );
}

#[test]
fn one_frame_at_no_scale_poisons_the_camera_easing_for_good() {
    // `interpolate_camera` (`camera.rs:270`) computes `to.scale / from.scale`, so a zero
    // denominator is an infinity in the exponent and the frame after the bad one is
    // already a NaN — for x, y AND scale. A later frame cannot undo it, which makes this
    // worse than a single bad frame and is why an eased camera is the reachable shape of
    // the defect rather than a corner case.
    let eased = interpolate_camera(
        scaleless_camera(),
        Camera {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        },
        0.5,
    );
    assert!(
        eased.scale.is_nan(),
        "MEASURED on 886738e: one frame at scale 0 poisons the easing permanently — x, y \
         and scale all NaN. Got {eased:?} instead, which means a guard now exists \
         somewhere on the zoom path and this ratchet is stale."
    );
}

#[test]
fn zooming_a_camera_that_has_no_scale_leaves_it_at_a_clamped_scale_and_a_nan_position() {
    // The table in the report said "`zoom_to` cannot produce 0 or NaN". That was true of
    // the SCALE and false of the camera: `zoom_to` (`camera.rs:166-172`) clamps the
    // scale it is given, but computes `ratio = scale / camera.scale` from the
    // *unclamped* current one, so a zero there is an infinity in `x` and `y`. Measured.
    let zoomed = zoom_to(scaleless_camera(), 0.0, 0.0, 2.0);
    // The two halves are separate assertions on purpose: `NaN != NaN`, so an
    // `assert_eq!` over a tuple containing one can never hold and would be a test that
    // is red for a reason that is not the defect. The scale, which is finite and is the
    // part `zoom_to` did clamp, is the half worth comparing exactly.
    assert!(
        zoomed.x.is_nan() && zoomed.y.is_nan(),
        "MEASURED on 886738e: zoom_to returns a NaN position, because the ratio is taken \
         from the unclamped current scale (camera.rs:167) and 2 / 0 is an infinity. Got \
         ({}, {}) instead. Invert when the guard lands.",
        zoomed.x,
        zoomed.y
    );
    assert_eq!(
        zoomed.scale, 2.0,
        "MEASURED on 886738e: the scale itself IS clamped, to the 2.0 that was asked for. \
         This is the half that is safe, and it is the half a reader checking this camera \
         would look at first — which is why the NaN needs its own assertion."
    );
}

#[test]
fn fitting_a_box_with_no_finite_corner_gives_a_camera_with_a_nan_offset() {
    // WRITE SIDE, and the reachable half of (a): `fit_bounds` clamps its scale and does
    // not clamp its centre. `camera.rs:191-192` computes `(min_x + max_x) / 2` with no
    // guard and `camera.rs:195` carries it into `x`. So the engine manufactures a
    // non-finite camera out of a poisoned scene, with no public-field write and nothing
    // from the host. `DrawEngine::fit` (`style.rs:687-692`) is one of the callers.
    let fitted = fit_bounds(non_finite_bounds(), 800.0, 600.0, 20.0);
    assert!(
        fitted.x.is_nan() && fitted.y.is_nan(),
        "MEASURED on 886738e: fit_bounds clamps the scale (to MAX_ZOOM, 30.0) and still \
         hands back a NaN camera, because the centre is not clamped. Got {fitted:?} \
         instead — a guard on the centre has landed and this ratchet is stale."
    );
}

#[test]
fn zoom_to_fit_on_a_box_with_no_finite_corner_gives_a_different_camera_and_also_a_nan() {
    // The second of the two centres, and round 1 named only the first. `fit_bounds` and
    // `zoom_to_fit_bounds` are not the same function and do not even agree on the scale
    // they answer: on identical non-finite bounds this one gives `1.0` where `fit_bounds`
    // gives `30.0`, because it divides the room by the extent (`camera.rs:245`) and
    // `normalize_zoom`s the result, while `fit_bounds` clamps a ratio. The unguarded
    // centre is a second copy of the same line at `camera.rs:250-251`, and exactly TWO
    // callers reach it: `reveal_if_hidden` (`style.rs:798`) and `fit_to_view`
    // (`style.rs:861`), which all three Shift+fits enter — `zoom_to_fit` (`:823`),
    // `zoom_to_fit_selection_in_viewport` (`:829`), `zoom_to_fit_selection` (`:835`).
    // `fit_bounds` has its own two, `fit` (`:689`) and `zoom_to_selection` (`:708`), and
    // a guard on `:191` does cover both. So a guard on `:191` alone leaves
    // `reveal_if_hidden` and the three Shift+fits open — two call sites, four user paths —
    // which is what the owner's Option B has to reach.
    let fitted = zoom_to_fit_bounds(
        non_finite_bounds(),
        800.0,
        600.0,
        Offsets::default(),
        ViewportFit::ScaleDown,
    );
    assert!(
        fitted.x.is_nan() && fitted.y.is_nan(),
        "MEASURED on 886738e: zoom_to_fit_bounds answers scale 1.0 on a non-finite box \
         where fit_bounds answers 30.0, and both answer a NaN camera. Got {fitted:?} \
         instead — a guard on `camera.rs:250-251` has landed and this ratchet is stale."
    );
}

// ---------------------------------------------------------------------------------------
// The THREE ways a hit test stops answering, all measured and all different. Non-finite
// bounds on their own are SUFFICIENT, a NaN camera is an INDEPENDENT second cause, and
// the `within_shape` centre overflow is the NECESSARY one. Round 1 blamed the first
// cause it found, and then the second, and the file's own rule — pin a cause you have
// isolated, or a fix gets aimed at the wrong line — is what the Q/R pair below exists to
// keep honest. No single line is "the" cause, which argues harder for not aiming a fix at
// any one of them, not less.
// ---------------------------------------------------------------------------------------

#[test]
fn an_ordinary_shape_is_hit_with_an_ordinary_camera() {
    // The control for both hit-test ratchets below.
    let engine = board_with_a_shape_under_the_pointer();
    assert!(
        engine.hit_test(400.0, 250.0, 10.0).is_some(),
        "MEASURED on 886738e: a solid 200×100 rectangle at (300, 200) is hit at the \
         middle of an 800×600 canvas under the identity camera. If this fails, the \
         control is broken and the two tests after it are measuring nothing."
    );
}

#[test]
fn a_nan_camera_alone_stops_an_ordinary_shape_from_being_hit() {
    // The camera IS a sufficient cause, on its own, on a board with nothing wrong with
    // it: every world point becomes NaN, and a NaN compares false against everything.
    // Round 1's report claimed this was the cause of the unclickable shape; it is a
    // cause, but not THE cause, and the test that says so is the one that pins it.
    let mut engine = board_with_a_shape_under_the_pointer();
    engine.camera = Camera {
        x: f64::NAN,
        y: 0.0,
        scale: 1.0,
    };
    let hits = (0..41)
        .flat_map(|i| (0..41).map(move |j| (i, j)))
        .filter(|(i, j)| {
            engine
                .hit_test(380.0 + *i as f64, 230.0 + *j as f64, 10.0)
                .is_some()
        })
        .count();
    assert_eq!(
        hits, 0,
        "MEASURED on 886738e: a 41×41 sweep finds 0 of 1681 points on an ordinary board \
         once the camera's x is NaN. If it now finds some, the camera is no longer the \
         cause and this ratchet is stale."
    );
}

#[test]
fn a_shape_whose_own_centre_overflowed_is_not_hit_even_with_a_perfect_finite_camera() {
    // The NECESSARY cause, and the one rounds 1 and 2 each misattributed. The camera is
    // FINITE and deliberately perfect: `x = 400 - f64::MAX` at scale 1 maps screen
    // (400, 300) to world `(f64::MAX, f64::MAX)`, which is inside the element's own box
    // — the control above proves it. The miss is therefore the element's, not the
    // camera's. It is not the rotation centre either: `within_shape` computes a centre
    // of its own, and at `scene/geometry.rs:502-503` that is `x + width / 2`, which is
    // `f64::MAX + 8.988e307` = +inf. `:510-511` then answers `nx = (f64::MAX - inf) / rx`
    // = -inf, and `:524` compares `nx.abs() <= 1.0` — false, so `hit_test_element`
    // returns at `:651`.
    let mut engine = board();
    let overflowing = overflowing_document();
    assert!(engine.load_scene(&overflowing), "the file did not load");
    engine.camera = Camera {
        x: 400.0 - f64::MAX,
        y: 300.0 - f64::MAX,
        scale: 1.0,
    };
    let world = engine.screen_to_world(400.0, 300.0);
    let plain = element_bounds(&engine.get_scene()[0]);
    assert!(
        plain.min_x <= world.x && world.x <= plain.max_x,
        "CONTROL FAILED: the pointer is not over the element even in world space ({world:?} \
         against {plain:?}), so a miss below would prove nothing. The camera needs \
         re-aiming before this test means anything."
    );
    // Q and R, ONE VARIABLE APART, and this pair is the whole point of the comment
    // above: same x, same height, same `angle: 0.0` — so `to_element_local` returns at
    // `geometry.rs:212-214` and the rotation centre is never read — same finite camera,
    // same probe, same screen point. Only `width` differs. Q's `cx` at `:502` is
    // `f64::MAX + 0`, finite, `:510` answers 0, `:524` says yes: HIT. R's is
    // `f64::MAX + f64::MAX / 2` = +inf, `:510` answers -inf, `:524` says no: MISS.
    // A distance of f64::MAX on its own is hittable, so it is the INFINITY and not the
    // distance that does it — and without this pair the sentence above is a claim.
    let qr = |width: &str| {
        let mut qr_engine = board();
        let qr_doc = document_with("\"x\": 0.0", &format!("\"x\": {F64_MAX}"))
            .replace("\"width\": 100.0", &format!("\"width\": {width}"));
        assert!(qr_engine.load_scene(&qr_doc), "the file did not load");
        qr_engine.camera = Camera {
            x: 400.0 - f64::MAX,
            y: 297.5,
            scale: 1.0,
        };
        qr_engine.hit_test(400.0, 300.0, 10.0).is_some()
    };
    assert!(
        qr("0.0") && !qr(F64_MAX),
        "CONTROL FAILED: the Q/R pair no longer isolates `width`, so the ratchet below \
         would be measuring a difference this test can no longer account for. Q is an \
         element at x = f64::MAX with width 0, probed at its own centre: it is hit. R is \
         the same element with width f64::MAX: it is missed, because the centre at \
         `scene/geometry.rs:502` overflows. Both run at angle 0.0, where the rotation \
         centre is never read, under a finite camera."
    );
    assert!(
        engine.hit_test(400.0, 300.0, 10.0).is_none(),
        "MEASURED on 886738e: the hit test misses with a finite, correctly aimed camera, \
         because `within_shape` overflows a centre of its OWN — `x + width / 2` is +inf at \
         `scene/geometry.rs:502-503`, `nx` is -inf at `:510-511`, and `:524` compares an \
         infinity against 1.0. It is the ELEMENT that is unhittable, and not the rotation \
         centre: at angle 0 `to_element_local` never reads that. The Q/R pair above is \
         what pins the line. If this now hits, `:502` has been guarded and the ratchet is \
         stale."
    );
}

// ---------------------------------------------------------------------------------------
// (b) The file door. Guarded by a RANGE check where the wire is guarded by a
// FINITENESS check, and neither of them looks at arithmetic.
// ---------------------------------------------------------------------------------------

#[test]
fn an_ordinary_file_measures_back_to_a_finite_and_ordered_box() {
    // The control for everything below: the document this app writes is already well
    // formed, so nothing downstream is the parser's doing.
    let ordinary = document_with("\"x\": 0.0", "\"x\": 0.0");
    let elements = elements_from_json(&ordinary).expect("the app's own export must load");
    let bounds = element_bounds(&elements[0]);
    assert!(
        box_is_finite(bounds) && bounds.min_x <= bounds.max_x && bounds.min_y <= bounds.max_y,
        "an ordinary element's box is neither finite nor ordered: {bounds:?}"
    );
}

#[test]
fn the_file_door_refuses_every_spelling_of_a_number_that_does_not_fit_an_f64() {
    // MEASURED, and it is the correction that matters: the door is guarded, just not by
    // the thing anyone assumed. `elements_from_json` (`export/json.rs:27-42`) validates
    // nothing itself, but `serde_json`'s parser refuses a literal `NaN` (not JSON) and
    // an out-of-range `1e400` ("number out of range") before any of that code runs. So
    // the file is protected by a RANGE check where the wire is protected by a
    // FINITENESS check (`packages/contract/src/element.ts:99-107`) — and neither of
    // them looks at arithmetic, which is where the file still gets through.
    for (spelling, replacement) in [
        ("a bare NaN", "\"width\": NaN"),
        ("1e400", "\"width\": 1e400"),
        ("-1e400", "\"width\": -1e400"),
        ("NaN in the angle", "\"angle\": NaN"),
    ] {
        assert!(
            elements_from_json(&document_with("\"width\": 100.0", replacement)).is_none(),
            "MEASURED on 886738e: {spelling} is refused by serde_json before \
             elements_from_json sees it. A red here means the door started ACCEPTING a \
             spelling it used to refuse — the OPPOSITE of the sanctioned fix, which makes \
             it refuse more and so leaves this green straight through. The parser changed."
        );
    }
}

#[test]
fn a_file_whose_geometry_outranges_f64_loads_through_three_of_the_four_doors_of_one_parser() {
    // FOUR call sites, one parser, and validation in none of them of its own. A guard has
    // to go on the door at `export/json.rs:27` and that one place covers all four, because
    // all four call it: `load_scene` (`engine/clipboard.rs:631`) — the door Open reaches
    // — `materialize_elements` (`edit/clipboard.rs:204`), which BOTH `insert_json` and
    // `paste_json` enter through `place_json` at `engine/clipboard.rs:424` and `:442`, and
    // `set_scene_json` (`wasm/input.rs:112`), live from `engine.ts:79`. Three of the four
    // are measurable from a native test; the fourth is `#[cfg(target_arch = "wasm32")]`
    // (`lib.rs:16-17`) and its whole body is `elements_from_json` then `set_scene`, so it
    // is counted here rather than asserted.
    let overflowing = overflowing_document();
    let mut engine = board();
    assert!(
        elements_from_json(&overflowing).is_some()
            && engine.load_scene(&overflowing)
            && engine.insert_json(&overflowing, None)
            && engine.paste_json(Some(&overflowing), None),
        "MEASURED on 886738e: three of the four accept a file whose every coordinate is \
         f64::MAX, and the fourth hands the same string to the same parser. If one refuses \
         now, the fix has landed — it is a behaviour change on a public format and needs \
         the owner's sign-off, so these ratchets are stale."
    );
}

#[test]
fn an_overflowing_geometry_measures_a_box_whose_far_edge_is_an_infinity() {
    // The unturned box. `normalize_rect` (`scene/geometry.rs:14-21`) is one addition
    // past the range: x at `f64::MAX` plus a width at `f64::MAX` is +inf.
    let elements = elements_from_json(&overflowing_document()).expect("it loads");
    let bounds = element_bounds(&elements[0]);
    assert_eq!(
        bounds.max_x,
        f64::INFINITY,
        "MEASURED on 886738e: the far edge of the box is +inf, and `WorldBounds::width()` \
         is {}. If it is finite now the overflow moved and this ratchet is stale.",
        bounds.width()
    );
}

#[test]
fn a_turned_overflowing_box_measures_a_nan_where_its_rotation_centre_overflowed() {
    // Where the `NaN` is born, and it is one specific line: `rotation_center`
    // (`scene/geometry.rs:108`) computes `element.x + width / 2`, which overflows to
    // +inf, and `element_rotated_bounds` then answers `c.x - ex` at
    // `scene/geometry.rs:244` — `inf - inf` is `NaN`. The far edge on the next line is
    // `inf + inf`, which is only an infinity. So a `NaN` needs no `NaN` anywhere in the
    // file: three fields at the top of the range and an angle of 0.001 is enough.
    let elements = elements_from_json(&overflowing_document()).expect("it loads");
    let turned = element_rotated_bounds(&elements[0]);
    assert!(
        turned.min_x.is_nan(),
        "MEASURED on 886738e: a turned overflowing box's min_x is NaN, from `inf - inf` at \
         `scene/geometry.rs:244`. Got {turned:?} instead, which means the arithmetic \
         moved and this ratchet is stale."
    );
}

#[test]
fn fitting_a_board_that_contains_one_leaves_the_camera_at_negative_infinity() {
    // The reachable chain, step one. `DrawEngine::fit` (`style.rs:687-692`) reads the
    // poisoned bounds and hands `fit_bounds` a box whose far edge is infinite, so the
    // camera's offset is `-inf` and its scale is MIN_ZOOM: the board is framed as a speck
    // at an infinite offset.
    let mut engine = board();
    engine.load_scene(&overflowing_document());
    engine.fit(20.0);
    for now_ms in [16.0, 400.0] {
        engine.set_now(now_ms);
    }
    assert_eq!(
        engine.camera.x,
        f64::NEG_INFINITY,
        "MEASURED on 886738e: fitting a board that contains an overflowing element leaves \
         the camera's x at -inf. If it is finite now, this ratchet is stale — the camera \
         was {:?}.",
        engine.camera
    );
}

#[test]
fn one_frame_of_that_fit_turns_the_camera_into_a_nan_that_never_lifts() {
    // Step two, and the arithmetic is the reviewer's, not round 1's: the easing's TARGET
    // is the NaN camera, and at the first frame `interpolate_camera` takes `m = factor`
    // because the two scales match, so `x` is `1 * -inf + 0 * NaN` — `0 * NaN` is NaN,
    // and `-inf + NaN` is NaN. Round 1 wrote `t * -inf`, which is the wrong term and
    // would have been the wrong explanation to hand a fixer. Every later frame is NaN
    // too, so the canvas is painted at NaN from here on.
    let mut engine = board();
    engine.load_scene(&overflowing_document());
    engine.fit(20.0);
    for now_ms in [16.0, 400.0] {
        engine.set_now(now_ms);
    }
    engine.zoom_to_fit();
    for now_ms in [500.0, 2000.0] {
        engine.set_now(now_ms);
    }
    assert!(
        engine.camera.x.is_nan(),
        "MEASURED on 886738e: one frame of the zoom-to-fit easing turns the -inf camera \
         into a NaN, because the target is already NaN and 0 * NaN is NaN. The camera is \
         then NaN permanently. Got {:?} instead.",
        engine.camera
    );
}

#[test]
fn a_board_containing_one_exports_an_svg_with_an_infinite_width() {
    // What the user gets to keep. `export_svg` sizes the canvas from the scene's bounds,
    // and the width of those bounds is the infinite far edge, so the file a renderer is
    // handed begins `width="inf"`.
    let mut engine = board();
    engine.load_scene(&overflowing_document());
    let svg = svg_of(&engine, 20.0);
    assert!(
        svg.contains("inf"),
        "MEASURED on 886738e: export_svg writes `width=\"inf\"`. If it is finite now, the \
         ratchet is stale. The file began: {}",
        svg.chars().take(120).collect::<String>()
    );
}

#[test]
fn the_exporter_re_emits_the_poison_so_a_saved_board_stays_poisoned_for_the_next_person() {
    // The persistence half, and the reason this is worse than a bad render: the value
    // survives the round trip exactly, so the board that comes back is the same poisoned
    // board — and `load_scene` fires `events.scene_json` with it
    // (`engine/clipboard.rs:638`), so the host is handed the poison on OPEN, not only on
    // save. Every collaborator who opens that board gets it too.
    let mut engine = board();
    engine.load_scene(&overflowing_document());
    let re_exported = engine.export_json();
    assert!(
        re_exported.contains("1.7976931348623157e+308"),
        "MEASURED on 886738e: the exporter re-emits f64::MAX, so the file the user saves \
         is as poisoned as the one they opened. The export began: {}",
        re_exported.chars().take(200).collect::<String>()
    );
}
