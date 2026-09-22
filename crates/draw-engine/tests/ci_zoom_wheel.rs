//! Wheel zoom: how far one wheel event is allowed to move the camera.
//!
//! The question this file settles is *continuity*. A wheel is a stream of events, not a
//! command, and the host cannot know whether the next one arrives in 4ms or 400 — so the
//! only thing that keeps zooming from looking like teleporting is a bound on what a
//! single event may do. Without one the zoom is not slow or fast, it is *unpredictable*,
//! which is the part that makes it unusable.
//!
//! The bound is not a preference: Excalidraw clamps the wheel delta to `ZOOM_STEP * 100`
//! (`App.wheel.ts::zoomBy` at the SHA in `scripts/oracle-sha.txt`) and that clamp is
//! ported here unchanged.
//!
//! **What the step does with the clamped delta is a deliberate divergence.** The oracle
//! works in *linear* zoom units — a flat tenth of the scale — and then corrects the top
//! of the range with `log10(max(1, zoom))`. The bottom of the range gets no correction,
//! so zooming out moved in leaps: 10% to 20% to 30% is 2.0x then 1.5x, and that is the
//! range you are in when you are hunting for something. A geometric step is the same
//! share of the picture everywhere, needs no amplification, and makes zooming out the
//! exact inverse of zooming in. It also matches `zoom_in`/`zoom_out`, which have always
//! multiplied by 1.2 for the toolbar buttons.
//!
//! So the tests below come in two kinds: ones that pin the numbers this engine promises,
//! and ones that assert the continuity property those numbers produce. Where a number
//! differs from the oracle's, the comment says so.

mod common;
use common::*;
use draw_engine::*;

/// Wheel deltas a browser actually sends.
///
/// Not a sweep: each is a real device on a real platform, and they differ by two orders
/// of magnitude, which is the whole difficulty. A handler tuned for one of these is
/// violent or inert on the others.
const REAL_WORLD_DELTAS: [f64; 10] = [
    // Firefox reports lines, not pixels: one notch is 3.
    -3.0, 3.0, // Chrome/macOS trackpad, a gentle two-finger drag.
    -4.0, 4.0, // Chrome/Linux mouse notch.
    -53.0, 53.0, // Chrome/Windows mouse notch, and the most common single event there is.
    -100.0, 100.0, // A trackpad fling, or a wheel spun hard enough to coalesce ticks.
    -240.0, 240.0,
];

/// Zoom levels to test the bound at, spanning the full clamped range.
fn zoom_levels() -> Vec<f64> {
    vec![
        MIN_ZOOM, 0.13, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 2.159, 3.0, 5.0, 10.0, 20.0, 29.5, MAX_ZOOM,
    ]
}

/// How much a measured step ratio may differ from the exact one.
///
/// Not slack: `normalize_zoom` rounds to six decimal places, which is an *absolute*
/// correction of up to 5e-7, and a ratio divides it by the scale. At 50% zooming out,
/// 0.5/1.1 = 0.4545454… rounds to 0.454545 and the ratio reads 1.1000011 rather than
/// 1.1. That is the rounding doing its job — it is what keeps the toolbar from showing
/// 99% at rest — so the tolerance covers it and nothing more. Five orders of magnitude
/// tighter than the 2.0 the linear step needed.
const RATIO_TOLERANCE: f64 = 1e-5;

/// How far apart two scales are, as a ratio ≥ 1 whichever way the zoom went.
fn step_ratio(before: f64, after: f64) -> f64 {
    if after > before {
        after / before
    } else {
        before / after
    }
}

// ---------------------------------------------------------------------------
// The property: continuity
// ---------------------------------------------------------------------------

#[test]
fn one_notch_is_the_same_share_of_the_picture_at_every_zoom() {
    // The property the linear step could not give, and the reason for the divergence
    // recorded at the top of this file.
    //
    // A flat tenth of *linear* zoom is a different thing to look at depending on where
    // you already are: at 10% it doubles the picture, at 100% it grows it a tenth, at
    // 1000% it is a hundredth. The `log10` amplification only ever applied above 100%,
    // so the whole zoomed-out half of the range stepped in leaps — 10% to 20% to 30% is
    // 2.0x then 1.5x — and that is the range you are in when you are looking for
    // something, which is when a violent step costs the most.
    //
    // A geometric step is the same proportion everywhere by construction.
    //
    // The scale itself is pinned exactly, through `normalize_zoom` so the six-place
    // rounding is part of the contract rather than something the test works around. The
    // *ratio* gets a tolerance, because that rounding is an absolute correction of up to
    // 5e-7 and dividing it by a small scale makes it a relative one: at 13% zooming out,
    // 0.13/1.1 = 0.1181818… rounds to 0.118182 and the ratio lands on 1.0999983. That is
    // the rounding working, not the step drifting, and 1e-5 is comfortably inside what
    // anyone could see while being far tighter than the 2.0 the linear step needed.
    let step = 1.0 + ZOOM_STEP;

    for scale in zoom_levels() {
        // The ends of the range clamp, and a clamped step is not a step — it is the
        // limit doing its job, asserted by its own test below.
        if scale * step <= MAX_ZOOM {
            let zoomed_in = wheel_zoom_scale(scale, -100.0);
            assert_close(zoomed_in, normalize_zoom(scale * step));
            assert!(
                (step_ratio(scale, zoomed_in) - step).abs() < RATIO_TOLERANCE,
                "zooming in at {scale} moved by {}x, not {step}x",
                step_ratio(scale, zoomed_in)
            );
        }
        if scale / step >= MIN_ZOOM {
            let zoomed_out = wheel_zoom_scale(scale, 100.0);
            assert_close(zoomed_out, normalize_zoom(scale / step));
            assert!(
                (step_ratio(scale, zoomed_out) - step).abs() < RATIO_TOLERANCE,
                "zooming out at {scale} moved by {}x, not {step}x",
                step_ratio(scale, zoomed_out)
            );
        }
    }
}

#[test]
fn zooming_in_and_back_out_returns_to_where_it_started() {
    // What a constant proportion buys beyond consistency: the step out is the exact
    // inverse of the step in, so a notch you did not mean to spin costs one notch back
    // rather than leaving the scale somewhere near where it was. The linear step was
    // reversible at 100% and nowhere else — from 0.5 it went 0.6 then 0.5, but from 2.0
    // it went 2.401 then 2.021.
    for scale in zoom_levels() {
        if scale * (1.0 + ZOOM_STEP) > MAX_ZOOM {
            continue;
        }
        let there = wheel_zoom_scale(scale, -100.0);
        let back = wheel_zoom_scale(there, 100.0);
        assert_close(back, scale);
    }
}

#[test]
fn no_single_wheel_event_moves_the_zoom_by_more_than_one_notch() {
    // The regression test, and it now has teeth in both directions.
    //
    // The handler this replaced was `exp(-delta * 0.01)`, which on a Chrome notch
    // (`delta = 100`) is `e¹ = 2.718` — the zoom nearly tripled per event, at every zoom
    // level, and a wheel spun twice left you somewhere unrecognisable.
    //
    // The bound used to be 2.0, which was the *linear* step's own worst case: a flat
    // tenth of scale at MIN_ZOOM takes 0.1 to 0.2. That made it a poor guard — a silent
    // return to the linear step would have passed it, and passed the 1.26 bound over the
    // working range too. One notch is the whole guarantee now, so it is what is asserted,
    // and the two looser tests this replaced are covered by it at every level and delta.
    for scale in zoom_levels() {
        for delta in REAL_WORLD_DELTAS {
            let next = wheel_zoom_scale(scale, delta);
            let ratio = step_ratio(scale, next);
            assert!(
                ratio <= 1.0 + ZOOM_STEP + RATIO_TOLERANCE,
                "wheel delta {delta} at zoom {scale} moved it to {next} — a {ratio}× jump"
            );
        }
    }
}

#[test]
fn spinning_the_wheel_walks_the_zoom_rather_than_teleporting_it() {
    // Continuity is a property of the *sequence*, not of one event: every step forward,
    // none of them a leap, and the run ends against the limit instead of overshooting.
    let mut scale = 1.0;
    let mut steps = Vec::new();
    for _ in 0..60 {
        let next = wheel_zoom_scale(scale, -100.0);
        assert!(
            next >= scale,
            "zooming in went backwards: {scale} -> {next}"
        );
        assert!(
            step_ratio(scale, next) <= 1.0 + ZOOM_STEP + RATIO_TOLERANCE,
            "a jump from {scale} to {next}"
        );
        steps.push(next);
        scale = next;
    }
    assert_close(scale, MAX_ZOOM);

    // And the same walking back down.
    for _ in 0..80 {
        let next = wheel_zoom_scale(scale, 100.0);
        assert!(
            next <= scale,
            "zooming out went forwards: {scale} -> {next}"
        );
        assert!(
            step_ratio(scale, next) <= 1.0 + ZOOM_STEP + RATIO_TOLERANCE,
            "a jump from {scale} to {next}"
        );
        scale = next;
    }
    assert_close(scale, MIN_ZOOM);
}

#[test]
fn a_bigger_delta_never_zooms_less_than_a_smaller_one() {
    // Monotonicity in the input. Without it the zoom responds to how hard you spin in a
    // way you cannot predict, which reads as the same unreliability as a jump.
    for scale in zoom_levels() {
        let mut previous = scale;
        for delta in [1.0, 4.0, 10.0, 53.0, 100.0, 240.0, 1000.0] {
            let out = wheel_zoom_scale(scale, delta);
            assert!(
                out <= previous + EPS,
                "delta {delta} at zoom {scale} zoomed out less ({out}) than a smaller one ({previous})"
            );
            previous = out;
        }
    }
}

// ---------------------------------------------------------------------------
// The oracle's exact numbers
// ---------------------------------------------------------------------------

#[test]
fn a_notch_at_one_hundred_percent_is_a_tenth_in_and_the_same_tenth_back_out() {
    // In is `x1.1`; out is `/1.1`, which is 0.909090… and NOT the oracle's 0.9.
    //
    // That asymmetry is the point rather than a rounding artefact: 0.9 is a tenth of
    // where you *were*, so zooming out and back in under the linear rule landed at 0.99.
    // A tenth more picture and a tenth less have to be inverses or the scale drifts
    // every time you change your mind.
    assert_close(wheel_zoom_scale(1.0, -100.0), 1.1);
    assert_close(wheel_zoom_scale(1.0, 100.0), normalize_zoom(1.0 / 1.1));
    assert_close(wheel_zoom_scale(1.0, 100.0), 0.909091);
}

#[test]
fn the_delta_is_clamped_so_a_fling_is_worth_one_notch() {
    // This clamp is the entire fix. Every delta past `ZOOM_STEP * 100` is the same step,
    // so a trackpad fling and a mouse notch land in the same place rather than the fling
    // leaving the board.
    let notch = wheel_zoom_scale(1.0, -100.0);
    assert_close(wheel_zoom_scale(1.0, -240.0), notch);
    assert_close(wheel_zoom_scale(1.0, -10_000.0), notch);
    assert_close(wheel_zoom_scale(1.0, -10.0), notch);

    // Just under the clamp still scales with the delta, so a trackpad stays continuous.
    assert!(wheel_zoom_scale(1.0, -9.0) < notch);
}

#[test]
fn the_step_needs_no_amplification_because_it_is_already_proportional() {
    // What replaced `log10(max(1, zoom))`. That term existed to stop a flat linear step
    // from crawling once you were zoomed in; a proportional step has nothing to correct,
    // so the whole amplification — and the `min(1, |delta| / 20)` fade that kept it from
    // making a slow trackpad drag out-zoom a fast flick — is gone.
    //
    // Pinned at the zoom levels the old amplification changed the answer at, so deleting
    // it cannot quietly come back.
    let step = 1.0 + ZOOM_STEP;
    for scale in [0.5, 1.0, 2.0, 5.0, 10.0] {
        assert_close(
            wheel_zoom_scale(scale, -100.0),
            normalize_zoom(scale * step),
        );
    }

    // 10.0 is the case that shows the difference plainly: the oracle's amplified step
    // was `10 + 0.1 + log10(10)` = 11.1, and a proportional tenth is 11.0.
    assert_close(wheel_zoom_scale(10.0, -100.0), 11.0);
}

#[test]
fn a_delta_under_the_clamp_is_a_proportional_fraction_of_a_notch() {
    // A trackpad sends a stream of small deltas where a mouse sends one large one, so a
    // sub-clamp delta has to be worth a fraction of a notch rather than a whole one —
    // otherwise a gentle two-finger drag zooms in notches and is unusable for framing.
    //
    // 4 of the clamp's 10 is four tenths of a notch, so the factor is `1.1^0.4`.
    assert_close(
        wheel_zoom_scale(5.0, -4.0),
        normalize_zoom(5.0 * (1.0 + ZOOM_STEP).powf(0.4)),
    );

    // And it really is smaller than the notch it is a fraction of.
    assert!(wheel_zoom_scale(5.0, -4.0) < wheel_zoom_scale(5.0, -100.0));
}

#[test]
fn a_firefox_line_delta_is_a_small_step_rather_than_a_wild_one() {
    // Firefox reports `deltaMode: 1` — one notch is 3 *lines*, not ~100 pixels.
    //
    // `host/wheel.ts` now normalises that to pixels before it ever reaches here, so a
    // real Firefox notch arrives as 120 and is worth a whole notch. This pins what the
    // bare function does with a raw 3 regardless: three tenths of the clamp, so three
    // tenths of a notch. Continuous either way, which is what the function owes; the
    // unit is the host's problem and is tested there.
    assert_close(
        wheel_zoom_scale(1.0, -3.0),
        normalize_zoom((1.0 + ZOOM_STEP).powf(0.3)),
    );
    assert_close(
        wheel_zoom_scale(1.0, 3.0),
        normalize_zoom((1.0 + ZOOM_STEP).powf(-0.3)),
    );
}

#[test]
fn a_wheel_event_with_no_vertical_delta_leaves_the_zoom_alone() {
    // `Math.sign(0)` is 0, but Rust's `f64::signum(0.0)` is *1.0* — take the sign from
    // `signum` and a horizontal-only wheel (a tilt wheel, a sideways two-finger scroll)
    // silently applies the amplification term and drifts the zoom while you scroll
    // sideways. The scale has to come out untouched, bit for bit.
    for scale in zoom_levels() {
        assert_eq!(
            wheel_zoom_scale(scale, 0.0),
            scale,
            "a zero delta moved the zoom at {scale}"
        );
        assert_eq!(wheel_zoom_scale(scale, -0.0), scale);
    }
}

#[test]
fn the_zoom_stops_at_the_limits_instead_of_wrapping_or_drifting() {
    assert_close(wheel_zoom_scale(MAX_ZOOM, -100.0), MAX_ZOOM);
    assert_close(wheel_zoom_scale(MIN_ZOOM, 100.0), MIN_ZOOM);
    // And a limit is reachable from the other side, not merely approached. A tenth more
    // than MIN_ZOOM rather than the linear step's 0.2, which doubled the picture in one
    // event at exactly the zoom where that is most disorienting.
    assert_close(wheel_zoom_scale(MIN_ZOOM, -100.0), 0.11);
    assert!(wheel_zoom_scale(MAX_ZOOM, 100.0) < MAX_ZOOM);
}

#[test]
fn the_scale_is_rounded_so_it_does_not_accumulate_float_dust() {
    // `getNormalizedZoom` rounds to six places. Without it a long run of notches leaves
    // the scale a hair off every round number, and the zoom readout in the toolbar shows
    // 99% at rest.
    assert_eq!(normalize_zoom(1.000000499), 1.0);
    assert_eq!(normalize_zoom(0.30000000000000004), 0.3);
    // Clamping is part of normalising, so nothing downstream has to re-check.
    assert_eq!(normalize_zoom(1e9), MAX_ZOOM);
    assert_eq!(normalize_zoom(-5.0), MIN_ZOOM);
}

// ---------------------------------------------------------------------------
// Through the engine
// ---------------------------------------------------------------------------

#[test]
fn a_wheel_zoom_keeps_the_point_under_the_cursor_under_the_cursor() {
    // The reason zoom is anchored at all. If this drifts, zooming in on a detail walks it
    // off the screen and you chase it with the pan — which is the same complaint as a
    // jump, arriving by a different route.
    let mut engine = DrawEngine::new();
    let cursor = Point { x: 317.0, y: 224.0 };
    let before = screen_to_world(engine.camera, cursor.x, cursor.y);

    for delta in [-100.0, -100.0, 53.0, -4.0, 240.0, -3.0] {
        engine.wheel_zoom(cursor.x, cursor.y, delta);
        let after = screen_to_world(engine.camera, cursor.x, cursor.y);
        assert_point_close(before, after);
    }
}

#[test]
fn a_wheel_zoom_moves_the_camera_the_same_way_the_maths_says() {
    // The engine method must be the pure function plus an anchor, with no second opinion
    // about the step — otherwise the tests above pin arithmetic nothing calls.
    let mut engine = DrawEngine::new();
    let mut scale = engine.camera.scale;
    for delta in [-100.0, -53.0, 3.0, -240.0, 100.0] {
        scale = wheel_zoom_scale(scale, delta);
        engine.wheel_zoom(400.0, 300.0, delta);
        assert_close(engine.camera.scale, scale);
    }
}

#[test]
fn a_wheel_tick_at_the_limit_does_not_redraw() {
    // At MAX_ZOOM every further tick is a no-op, but a trackpad keeps sending them for as
    // long as the fingers move. Publishing a camera event per tick would repaint the
    // whole scene, dozens of times a second, to produce the identical picture.
    let mut engine = DrawEngine::new();
    engine.set_camera(zoom_to(engine.camera, 400.0, 300.0, MAX_ZOOM));
    let _ = engine.drain_events();

    engine.wheel_zoom(400.0, 300.0, -100.0);

    let events = engine.drain_events();
    assert!(
        events.camera.is_none(),
        "a tick at the zoom limit published a camera change"
    );
    assert_close(engine.camera.scale, MAX_ZOOM);
}

#[test]
fn a_wheel_zoom_does_publish_a_camera_change_when_it_moves() {
    // The counterpart, so the no-op above cannot be satisfied by never publishing at all.
    let mut engine = DrawEngine::new();
    let _ = engine.drain_events();
    engine.wheel_zoom(400.0, 300.0, -100.0);
    let events = engine.drain_events();
    assert!(events.camera.is_some());
}
