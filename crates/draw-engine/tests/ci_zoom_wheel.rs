//! Wheel zoom: how far one wheel event is allowed to move the camera.
//!
//! The question this file settles is *continuity*. A wheel is a stream of events, not a
//! command, and the host cannot know whether the next one arrives in 4ms or 400 — so the
//! only thing that keeps zooming from looking like teleporting is a bound on what a
//! single event may do. Without one the zoom is not slow or fast, it is *unpredictable*,
//! which is the part that makes it unusable.
//!
//! The bound is not a preference. Excalidraw clamps the wheel delta to `ZOOM_STEP * 100`
//! and works in linear zoom units, and this is a port of that arithmetic
//! (`App.wheel.ts::zoomBy` at the SHA in `scripts/oracle-sha.txt`), so the tests below
//! come in two kinds: ones that pin the oracle's exact numbers, and ones that assert the
//! continuity property those numbers produce.

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
fn no_single_wheel_event_more_than_doubles_the_zoom() {
    // The regression test. The handler this replaced was `exp(-delta * 0.01)`, which on
    // a Chrome notch (`delta = 100`) is `e¹ = 2.718` — the zoom nearly tripled per event,
    // at every zoom level, and a wheel spun twice left you somewhere unrecognisable.
    //
    // Two is the oracle's own worst case rather than a target: below 100% the step is a
    // flat tenth of *linear* zoom, so it is proportionally largest at MIN_ZOOM, where
    // 0.1 → 0.2 doubles it. Everywhere else it is far smaller — see the test below.
    for scale in zoom_levels() {
        for delta in REAL_WORLD_DELTAS {
            let next = wheel_zoom_scale(scale, delta);
            let ratio = step_ratio(scale, next);
            assert!(
                ratio <= 2.0 + EPS,
                "wheel delta {delta} at zoom {scale} moved it to {next} — a {ratio}× jump"
            );
        }
    }
}

#[test]
fn a_notch_in_the_everyday_zoom_range_moves_it_by_about_a_quarter_at_most() {
    // 50%–2000% is where a board is read and edited. The bound here is what the zoom
    // actually feels like; the 2× above is a corner of the range nobody works in.
    //
    // 1.26 rather than a round quarter because the worst case is 1.2519, zooming out at
    // 216% — that is where `(ZOOM_STEP + log10 z) / z`, the amplification's share of the
    // step, peaks. Stated as a computed bound rather than rounded up to a nice number so
    // that a change to the amplification moves this test instead of hiding under it.
    for scale in zoom_levels() {
        if !(0.5..=20.0).contains(&scale) {
            continue;
        }
        for delta in REAL_WORLD_DELTAS {
            let next = wheel_zoom_scale(scale, delta);
            let ratio = step_ratio(scale, next);
            assert!(
                ratio <= 1.26,
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
            step_ratio(scale, next) <= 1.25 + EPS,
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
            step_ratio(scale, next) <= 2.0 + EPS,
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
fn a_notch_at_one_hundred_percent_is_a_tenth_either_way() {
    // `delta / 100` after the clamp to `ZOOM_STEP * 100`, with no amplification because
    // the amplification only applies above 100%.
    assert_close(wheel_zoom_scale(1.0, -100.0), 1.1);
    assert_close(wheel_zoom_scale(1.0, 100.0), 0.9);
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
fn zooming_in_past_one_hundred_percent_takes_larger_steps() {
    // `log10(max(1, zoom))`: a tenth of linear zoom is a fifth of the picture at 50% and
    // a three-hundredth at 3000%, so without this the zoom crawls once you are in close.
    // The amplification is what keeps the *felt* step roughly constant.
    // Through `normalize_zoom` because the engine rounds to six places and the long-hand
    // arithmetic does not; the two differ in the ninth decimal, which is the rounding
    // doing its job rather than a discrepancy.
    assert_close(
        wheel_zoom_scale(2.0, -100.0),
        normalize_zoom(2.0 + 0.1 + 2.0_f64.log10()),
    );
    assert_close(wheel_zoom_scale(10.0, -100.0), 10.0 + 0.1 + 1.0);

    // Below 100% there is none: `max(1, zoom)` floors it.
    assert_close(wheel_zoom_scale(0.5, -100.0), 0.6);
    assert_close(wheel_zoom_scale(0.5, 100.0), 0.4);
}

#[test]
fn a_small_trackpad_delta_is_amplified_less() {
    // `min(1, |delta| / 20)`. A trackpad sends a stream of small deltas; giving each one
    // the full `log10` amplification would make a slow drag zoom faster than a fast one.
    let damped = wheel_zoom_scale(5.0, -4.0);
    assert_close(
        damped,
        normalize_zoom(5.0 + 0.04 + 5.0_f64.log10() * (4.0 / 20.0)),
    );

    // Undamped it would be this instead — a 15% jump out of a 4px nudge.
    let undamped = 5.0 + 0.04 + 5.0_f64.log10();
    assert!(damped < undamped - 0.5, "{damped} vs {undamped}");
}

#[test]
fn a_firefox_line_delta_is_a_small_step_rather_than_a_wild_one() {
    // Firefox reports `deltaMode: 1` — one notch is 3 *lines*, not ~100 pixels. The
    // oracle does not normalise for it, and it does not need to: 3 is well under the
    // clamp, so it lands as a 3% step. Slower than Chrome, but continuous, which is the
    // property that matters. Pinned so that adding a `deltaMode` normalisation later is
    // a deliberate divergence rather than an accident.
    assert_close(wheel_zoom_scale(1.0, -3.0), 1.03);
    assert_close(wheel_zoom_scale(1.0, 3.0), 0.97);
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
    // And a limit is reachable from the other side, not merely approached.
    assert_close(wheel_zoom_scale(MIN_ZOOM, -100.0), 0.2);
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
