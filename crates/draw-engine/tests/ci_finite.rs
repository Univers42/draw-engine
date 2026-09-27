//! Two places a non-finite number walks into the scene, pinned where they stand.
//!
//! Neither is fixed. Both are a Phase 1 item under BUNNY.md §3.4 ("if a test exposes a
//! bug, stop and report it"), and (b) is reachable by a user: opening a file puts a
//! non-finite camera on the board, which blanks the canvas and makes the shape that did
//! it unclickable. `~/bunny/reports/t2-nan-ingress.md` has the traced path.
//!
//! **The pinning form, and why it is this one.** These assert the CURRENT, defective
//! answer rather than the desired one, in a test that RUNS. That is vitest's `it.fails`,
//! which is the form this repo already uses for its one known failure
//! (`apps/web/src/lib/autosave/sceneDiff.merges.test.ts`, landed 3f-ii in root
//! `c5988b3`): "it cannot be deleted later as obsolete: it is green only for as long as
//! the divergence is there." Rust's stable libtest has no `it.fails`, so it is written
//! out by hand.
//!
//! The two alternatives, and why not:
//!
//! - `#[ignore = "reason"]`, which the brief proposed, and which is close. Its reason IS
//!   printed — MEASURED, because I expected otherwise and was wrong:
//!   `test name ... ignored, REASON STRING PROBE: does libtest print this?` — so the
//!   skip is not silent and the brief's reading of BUNNY.md's "skipped ≠ passed" holds
//!   on its own terms. What it does not do is RUN. The assertion never executes, so the
//!   number is never measured, and a guard landing in `camera.rs` or `json.rs` leaves the
//!   test ignored forever with nothing to notice — while the direction that actually
//!   matters, a fix arriving, is the one an ignore cannot report. That is the whole
//!   argument for the form above, and it is an argument about the fix, not about the
//!   skip.
//! - Both at once (an ignored specification plus a running ratchet) is the strongest
//!   form and is not taken only because it is four tests where the brief budgeted two.
//!   It is the right upgrade if the owner wants these to carry the specification.
//!
//! So each test below has three parts: the CONTROL (an ordinary camera, an ordinary file
//! — these pass today and must keep passing, so neither defect can be "fixed" by
//! rejecting everything), the MEASUREMENT (the defect, with the real number in the
//! failure message rather than a bare `is_finite()`), and the INVERSION (which assertion
//! to flip when the guard lands — flipping it is what turns this test red in the
//! meantime, and that red is the point).
//!
//! Measured on `engine` `886738e`. Reproduce with:
//! `cargo test -p draw-engine --test ci_finite -- --nocapture`

mod common;
use common::*;
use draw_engine::*;

/// `f64::MAX`, spelled the way a file has to spell it. The largest number JSON can carry
/// that a Rust `f64` will still hold: `1e400` is refused by the parser ("number out of
/// range") and `NaN` is not JSON at all, so overflow has to arrive as arithmetic.
const F64_MAX: &str = "1.7976931348623157e308";

/// `assert!(v.is_finite())` that says what it got. A bare assertion on a `NaN` tells the
/// next reader nothing about the shape of the failure they are looking at.
#[track_caller]
fn assert_finite(what: &str, got: f64) {
    assert!(
        got.is_finite(),
        "{what} is not finite — it is {got:?}. A non-finite coordinate here is not a \
         wrong answer, it is an absent one: it propagates through every bounds fold and \
         every paint projection from here."
    );
}

// ---------------------------------------------------------------------------------------
// (a) `Camera { scale: 0 }` → `screen_to_world` answers non-finite
// ---------------------------------------------------------------------------------------

#[test]
fn a_camera_with_no_scale_answers_screen_to_world_with_a_non_finite_point() {
    // --- the control. Three ordinary cameras, and both ends of the real zoom range, so
    // this test cannot be satisfied by a guard that rejects everything. `MIN_ZOOM` and
    // `MAX_ZOOM` are the clamps `zoom_to` (`camera.rs:166`) enforces, so a guard placed
    // there is inside the range and these three still round-trip.
    for scale in [1.0, MIN_ZOOM, MAX_ZOOM] {
        let camera = Camera {
            x: 12.3456,
            y: -78.9101,
            scale,
        };
        let back = screen_to_world(camera, 300.0, 200.0);
        assert_finite(&format!("a camera at scale {scale} answering x"), back.x);
        assert_finite(&format!("a camera at scale {scale} answering y"), back.y);
    }

    // --- the measurement. `Camera` derives `Deserialize` with no validator
    // (`camera.rs:104`) and `screen_to_world` divides by `scale` (`camera.rs:154-159`).
    // The only clamps are on the path that SETS a scale — `zoom_to` (`camera.rs:166`) and
    // `normalize_zoom` (`camera.rs:69-73`) — never on the path that ACCEPTS one.
    //
    // MEASURED, and the shape matters: `0 / 0` is a `NaN` and `n / 0` is a signed
    // infinity, so the answer is NOT uniformly `NaN`. `screen_to_world(zero, 0.0, 0.0)`
    // is `NaN` — that is the `0/0` case, and it is the one the architect's probe
    // happened to print. Any other point is `±inf`. `visible_world_rect` calls
    // `screen_to_world` at two corners, so one call returns BOTH: its top-left corner
    // sits on the camera and is `NaN`, its bottom-right is `inf`.
    //
    // These four are the ratchet. When the guard lands they go red, and the fix is to
    // assert the finite answer instead — `assert_finite("…", away.x)` in place of the
    // `assert_eq!` below, and the same for the two after it.
    //
    let zero = Camera {
        x: 0.0,
        y: 0.0,
        scale: 0.0,
    };

    // A point off the camera: the signed infinity, not a NaN.
    let away = screen_to_world(zero, 100.0, 100.0);
    assert_eq!(
        (away.x, away.y),
        (f64::INFINITY, f64::INFINITY),
        "MEASURED on 886738e: a zero scale answers 100/0 = +inf on both axes. If this \
         changed, the ingress moved — a guard, or a different division — and the \
         ratchet below has to be re-measured, not assumed."
    );

    // A point ON the camera: the 0/0 case, the one that is a genuine NaN.
    let on = screen_to_world(zero, 0.0, 0.0);
    assert!(
        on.x.is_nan() && on.y.is_nan(),
        "MEASURED on 886738e: a point sitting on a zero-scale camera answers 0/0 = NaN, \
         not an infinity. Got ({}, {}) instead.",
        on.x,
        on.y
    );

    // And the amplification, which is the part that makes it worth a Phase 1 ticket: one
    // frame at scale 0 turns the camera's own easing into a NaN that no later frame can
    // undo, because `interpolate_camera` (`camera.rs:270`) computes `to.scale /
    // from.scale` and a zero denominator is an infinity in the exponent.
    let eased = interpolate_camera(
        zero,
        Camera {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        },
        0.5,
    );
    assert!(
        eased.scale.is_nan(),
        "MEASURED on 886738e: one frame at scale 0 poisons the easing permanently — \
         `interpolate_camera` gives NaN for x, y AND scale. Got {eased:?} instead, which \
         means a guard now exists somewhere on the zoom path and this ratchet is stale."
    );

    // The reachable half, and the part that makes this worth a ticket at all rather than
    // a note about a public field. Nothing above needs a caller to write `scale: 0`: a
    // scene whose bounds are not finite is enough, because `fit_bounds` clamps its SCALE
    // and does not clamp its CENTRE. `camera.rs:191-192` computes `(min_x + max_x) / 2`
    // with no guard, so the camera it returns carries the non-finiteness straight into
    // `width / 2 - center_x * scale` at `:195` — which is the same `0/0`-shaped hole as
    // `screen_to_world`, arrived at from the other side. `DrawEngine::fit` and
    // `zoom_to_fit` (`engine/style.rs:684-690`, `:823`) both go through it, so pressing
    // zoom-to-fit on a board like the one `ci_finite` describes below is enough.
    let fitted = fit_bounds(
        WorldBounds {
            min_x: f64::NAN,
            min_y: f64::NAN,
            max_x: f64::NAN,
            max_y: f64::NAN,
        },
        800.0,
        600.0,
        20.0,
    );
    assert!(
        fitted.x.is_nan() && fitted.y.is_nan(),
        "MEASURED on 886738e: fit_bounds clamps the scale (30.0, from MIN/MAX_ZOOM) and \
         still hands back a NaN camera, because the centre is not clamped. Got {fitted:?} \
         instead — a guard on the centre has landed and this ratchet is stale."
    );
}

// ---------------------------------------------------------------------------------------
// (b) a file whose geometry outranges `f64` loads, and measures a box with `NaN` in it
// ---------------------------------------------------------------------------------------

/// The app's own export for one 100×5 box at the origin, with `field` respelled as
/// `replacement`. Built by exporting rather than hand-written, because `DrawElement`
/// has no defaults: a literal is rejected for missing fields, which would make this
/// table measure nothing.
fn document_with(field: &str, replacement: &str) -> String {
    let json = scene_to_json(&[box_at(0.0, 0.0, 100.0, 5.0)]);
    assert!(
        json.contains(field),
        "the exported document no longer spells {field:?}, so this test is measuring a \
         replacement that never landed. Re-read the export before trusting it."
    );
    json.replace(field, replacement)
}

/// Every `f64` in a box, so "is this box finite" is one question with one answer.
fn box_is_finite(bounds: WorldBounds) -> bool {
    [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .all(|v| v.is_finite())
}

#[test]
fn a_file_whose_geometry_outranges_f64_loads_and_measures_a_box_with_nan_in_it() {
    // --- the control. A file this app wrote, measured back, is finite and ordered.
    // A no-op replacement: the point is that the document the app itself writes is
    // already well formed, so nothing below it is the parser's doing.
    let ordinary = document_with("\"x\": 0.0", "\"x\": 0.0");
    let elements = elements_from_json(&ordinary).expect("the app's own export must load");
    let bounds = element_bounds(&elements[0]);
    assert!(
        box_is_finite(bounds),
        "an ordinary element's box is not finite: {bounds:?}"
    );
    assert!(
        bounds.min_x <= bounds.max_x && bounds.min_y <= bounds.max_y,
        "an ordinary element's box is not ordered: {bounds:?}"
    );

    // --- which entry points accept it and which already refuse. MEASURED, and the
    // answer is not the one the brief assumed.
    //
    // The brief's claim was that the FILE path is unguarded while the WIRE is guarded
    // (`packages/contract/src/element.ts:99-107`, a zod `.refine(Number.isFinite)`).
    // Half of that is right and half is wrong, and the wrong half is the interesting
    // one: `elements_from_json` (`export/json.rs:27-42`) really does validate NOTHING
    // of its own, but `serde_json`'s parser refuses both spellings of a non-finite
    // number before any of that code runs. `NaN` is not JSON. `1e400` is a valid JSON
    // number that does not fit an `f64`, and the parser says so.
    //
    // So the asymmetry is not file-vs-wire. It is that the file parser is a *range*
    // check and the wire is a *finiteness* check, and neither of them looks at
    // arithmetic. A number that fits, twice added, does not.
    for (spelling, replacement) in [
        ("a bare NaN", "\"width\": NaN"),
        ("1e400", "\"width\": 1e400"),
        ("-1e400", "\"width\": -1e400"),
    ] {
        assert!(
            elements_from_json(&document_with("\"width\": 100.0", replacement)).is_none(),
            "MEASURED on 886738e: {spelling} is refused by serde_json before \
             elements_from_json sees it. If it now loads, the parser changed and this \
             table is stale."
        );
    }
    // And one that is accepted and silently becomes something else: 1e-400 underflows to
    // 0.0, which loads as a zero-width element. Not this test's subject; noted because
    // it is the third way the same door answers, and it is silent.

    // --- the measurement. Every field at the top of the `f64` range, which the parser
    // accepts because each one is a legal number, and which the engine adds together.
    let hostile = document_with("\"x\": 0.0", &format!("\"x\": {F64_MAX}"))
        .replace("\"width\": 100.0", &format!("\"width\": {F64_MAX}"))
        .replace("\"height\": 5.0", &format!("\"height\": {F64_MAX}"))
        .replace("\"angle\": 0.0", "\"angle\": 0.001");

    // All three doors into `elements_from_json`, because a guard has to go on the one
    // they share and the report has to say which is which. MEASURED: all three accept.
    let Some(elements) = elements_from_json(&hostile) else {
        panic!(
            "the hostile document was REJECTED. That is the fix landing: it is a \
             behaviour change on a public format and needs the owner's sign-off, so the \
             ratchet assertions below are now stale and this test should be rewritten to \
             assert the refusal."
        )
    };
    let mut engine = DrawEngine::new();
    engine.set_viewport(800.0, 600.0, 1.0);
    assert!(
        engine.load_scene(&hostile),
        "DrawEngine::load_scene refused a document elements_from_json accepted"
    );
    assert!(
        engine.insert_json(&hostile, None),
        "DrawEngine::insert_json — the paste door — refused it too"
    );

    // The unturned box, whose far edge is an infinity: `x + width` in
    // `normalize_rect` (`geometry.rs:14-21`) is one addition past the range.
    let bounds = element_bounds(&elements[0]);
    assert_eq!(
        bounds.max_x,
        f64::INFINITY,
        "MEASURED on 886738e: the far edge of the box is +inf, one addition past the \
         range. If it is finite now, the overflow moved and this ratchet is stale — \
         re-measure rather than assume. The whole box was {bounds:?}."
    );

    // The turned box, which is where the NaN is born and it is born in one specific
    // place: `rotation_center` (`geometry.rs:108`) computes `element.x + width / 2`,
    // which overflows to +inf, and `element_rotated_bounds` then answers
    // `c.x - ex` — `inf - inf` — at `geometry.rs:246`. A `NaN` therefore needs no NaN
    // anywhere in the file: an overflowing extent and an angle of 0.001 is enough.
    let turned = element_rotated_bounds(&elements[0]);
    assert!(
        turned.min_x.is_nan(),
        "MEASURED on 886738e: a turned box's min_x is NaN — the rotation centre \
         overflowed to inf and inf - inf is NaN. Got {turned:?} instead, which means \
         the arithmetic moved and this ratchet is stale."
    );

    // Why it is a Phase 1 item and not a curiosity: the NaN does not stay in the
    // element. Fit the board and the CAMERA goes non-finite, and the canvas is then
    // asked to paint at NaN. MEASURED end to end through `DrawEngine::load_scene`,
    // `fit` and `zoom_to_fit` on this same document.
    engine.fit(20.0);
    for now_ms in [16.0, 400.0] {
        engine.set_now(now_ms);
    }
    assert_eq!(
        engine.camera.x,
        f64::NEG_INFINITY,
        "MEASURED on 886738e: fitting a board that contains it leaves the camera's x at \
         -inf, and its scale at MIN_ZOOM, so the board is framed as a speck at an \
         infinite offset. If it is finite now, this ratchet is stale — the camera was {:?}.",
        engine.camera
    );
    engine.zoom_to_fit();
    for now_ms in [500.0, 2000.0] {
        engine.set_now(now_ms);
    }
    assert!(
        engine.camera.x.is_nan(),
        "MEASURED on 886738e: one frame of the zoom-to-fit easing turns that -inf into \
         a NaN, because (1 - t) * -inf + t * -inf is -inf + NaN at t = 0. The camera is \
         then NaN permanently. Got {:?} instead.",
        engine.camera
    );

    // And the user cannot click their way out: every projection of a NaN camera is NaN,
    // so nothing is ever hit, so the element that did this cannot be selected and
    // deleted. MEASURED, and this is why the only escape is Open, which throws the
    // board away.
    assert!(
        engine.hit_test(400.0, 300.0, 10.0).is_none(),
        "MEASURED on 886738e: a NaN camera makes hit_test miss everything, so the \
         poisoned element cannot be clicked. If this now hits, the ratchet above is stale."
    );

    // What the user does get to keep: an SVG with an infinity in its width attribute,
    // from the export that is supposed to produce a file they can open.
    let svg = engine.export_svg(20.0).unwrap_or_default();
    assert!(
        svg.contains("inf"),
        "MEASURED on 886738e: export_svg writes `width=\"inf\"`, because the scene's \
         width is the infinite far edge of the poisoned box. If it is finite now, the \
         ratchet is stale."
    );
}
