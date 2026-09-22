//! The laser pointer.
//!
//! A trail that fades is unusual to test because its output depends on a clock, so every
//! test here drives that clock explicitly rather than sleeping. That is only possible
//! because the engine takes the time from its host instead of reading one itself, and it
//! is the reason the fade is reproducible at all.

mod common;
use common::*;
use draw_engine::*;

fn span_x(outline: &[LaserPoint]) -> f64 {
    let min = outline.iter().fold(f64::MAX, |a, p| a.min(p.x));
    let max = outline.iter().fold(f64::MIN, |a, p| a.max(p.x));
    max - min
}

fn span_y(outline: &[LaserPoint]) -> f64 {
    let min = outline.iter().fold(f64::MAX, |a, p| a.min(p.y));
    let max = outline.iter().fold(f64::MIN, |a, p| a.max(p.y));
    max - min
}

/// Half the width of a stroke drawn along the x axis.
///
/// For a horizontal stroke the y extent *is* the beam, while the x extent is how far the
/// pointer travelled — measuring the bounding box would report the stroke's length.
fn beam_radius(outline: &[LaserPoint]) -> f64 {
    span_y(outline) / 2.0
}

/// The fade never quite reaches full size, so nothing here compares against an exact
/// radius.
///
/// At the head of a trail the length fade is `easeOut(0.98)`, not `easeOut(1)`: the last
/// point is one step short of the end of the queue by construction, because the mapping
/// counts how many points come *after* it. That leaves every measurement about 1.6e-7
/// under nominal — far inside a pixel, and far outside the 1e-9 the other suites use.
fn assert_near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "expected {actual} within {tolerance} of {expected}"
    );
}

/// A stroke drawn left to right, one sample every `step` world units.
fn drawn(points: usize, step: f64, start_time: f64, per_point_ms: f64) -> LaserStroke {
    let mut stroke = LaserStroke::new(LaserOptions::default());
    for i in 0..points {
        stroke.add_point(i as f64 * step, 0.0, start_time + i as f64 * per_point_ms);
    }
    stroke
}

// ------------------------------------------------------------------ the fade curve

#[test]
fn ease_out_is_quartic_and_pinned_at_both_ends() {
    // Excalidraw's `easeOut`, which is `1 - (1 - k)^4`. Pinned at the ends because a
    // curve that did not reach 1 would leave the head of a fresh trail permanently thin.
    assert_close(ease_out(0.0), 0.0);
    assert_close(ease_out(1.0), 1.0);
    assert_close(ease_out(0.5), 0.9375);
    assert_close(ease_out(0.25), 1.0 - 0.75_f64.powi(4));
}

#[test]
fn a_point_is_full_size_when_it_is_fresh_and_at_the_head() {
    // Not exactly 1: the length fade counts the points that come *after* this one, and
    // the head of a queue of 50 still has one behind it, so this is `easeOut(0.98)`.
    assert_near(size_mapping(0.0, 49.0, 50.0), 1.0, 1e-6);
    assert!(size_mapping(0.0, 49.0, 50.0) <= 1.0, "the fade overshot 1");
}

#[test]
fn a_point_older_than_the_decay_time_has_no_size_at_all() {
    // Both halves of the mapping have to agree for a trail to disappear: this is the
    // time half, and it is what makes a laser held still vanish rather than sit there.
    assert_eq!(size_mapping(LASER_DECAY_TIME_MS, 49.0, 50.0), 0.0);
    assert_eq!(size_mapping(LASER_DECAY_TIME_MS * 2.0, 49.0, 50.0), 0.0);
}

#[test]
fn a_point_further_back_than_the_decay_length_has_no_size_either() {
    // The length half. Counted in points rather than distance, so this is about how many
    // samples ago the point was, not how far away it is.
    let total = 200.0;
    assert_eq!(size_mapping(0.0, total - LASER_DECAY_LENGTH, total), 0.0);
    assert_eq!(size_mapping(0.0, 0.0, total), 0.0);
    assert!(size_mapping(0.0, total - 1.0, total) > 0.0);
}

#[test]
fn the_fade_never_grows_as_a_point_ages() {
    // Monotonic, which is the property that makes a trail read as decaying rather than
    // flickering. A non-monotonic mapping would still fade to nothing at the end.
    let mut previous = f64::MAX;
    for age in 0..=1000 {
        let size = size_mapping(age as f64, 49.0, 50.0);
        assert!(
            size <= previous + 1e-12,
            "size grew at age {age}: {size} after {previous}"
        );
        previous = size;
    }
}

// --------------------------------------------------------------- capturing a stroke

#[test]
fn an_untouched_stroke_draws_nothing() {
    let stroke = LaserStroke::new(LaserOptions::default());
    assert!(stroke.outline(0.0, None).is_empty());
    assert!(!stroke.is_visible(0.0));
}

#[test]
fn a_single_point_is_a_dot_of_the_configured_size() {
    let mut stroke = LaserStroke::new(LaserOptions::default());
    stroke.add_point(10.0, 10.0, 0.0);
    let outline = stroke.outline(0.0, None);

    assert!(!outline.is_empty());
    // A circle, so both extents are the diameter.
    assert_near(span_x(&outline), LASER_SIZE * 2.0, 1e-5);
    assert_near(span_y(&outline), LASER_SIZE * 2.0, 1e-5);
}

#[test]
fn a_repeated_position_is_ignored() {
    // A stationary pointer keeps emitting events. Letting them through would fill the
    // trail with zero-length segments, whose direction is undefined — and a direction
    // vector of NaN poisons every coordinate downstream of it.
    let mut stroke = LaserStroke::new(LaserOptions::default());
    for _ in 0..50 {
        stroke.add_point(5.0, 5.0, 0.0);
    }
    let outline = stroke.outline(0.0, None);
    assert!(outline.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    // Fifty events at one position still describe a single dot.
    assert_near(span_x(&outline), LASER_SIZE * 2.0, 1e-5);
}

#[test]
fn every_outline_coordinate_is_finite() {
    // The guard that matters most in practice. A single NaN does not fail loudly: it
    // makes the canvas silently skip the path, so the laser just stops appearing.
    for points in [1, 2, 3, 5, 20, 120] {
        for step in [0.01, 1.0, 250.0] {
            let stroke = drawn(points, step, 0.0, 8.0);
            let outline = stroke.outline(points as f64 * 8.0, None);
            assert!(
                outline.iter().all(|p| p.x.is_finite() && p.y.is_finite()),
                "non-finite coordinate with {points} points at step {step}"
            );
        }
    }
}

#[test]
fn a_stroke_that_doubles_back_on_itself_still_produces_a_finite_outline() {
    // Reversing exactly puts the two edge directions back to back, so their sum has no
    // direction at all. That is the case the mitre code falls back to the incoming edge
    // for, and the one a hand-drawn flick actually hits.
    let mut stroke = LaserStroke::new(LaserOptions::default());
    for x in [0.0, 10.0, 20.0, 30.0, 20.0, 10.0, 0.0] {
        stroke.add_point(x, 0.0, 0.0);
    }
    let outline = stroke.outline(0.0, None);
    assert!(outline.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
}

#[test]
fn streamlining_pulls_a_new_point_back_towards_the_last_one() {
    // The first point is taken as given; every later one is dragged back by the
    // streamline factor, which is what takes the shiver out of a raw pointer stream.
    let mut stroke = LaserStroke::new(LaserOptions {
        streamline: 0.4,
        ..LaserOptions::default()
    });
    stroke.add_point(0.0, 0.0, 0.0);
    stroke.add_point(100.0, 0.0, 0.0);

    // Two points make a capsule; its span is the distance between the smoothed centres
    // plus a radius at each end. 100 units pulled back by 0.4 leaves 60.
    assert_near(
        span_x(&stroke.outline(0.0, None)),
        60.0 + 2.0 * LASER_SIZE,
        1e-4,
    );
}

// ----------------------------------------------------------------------- the fade

#[test]
fn a_finished_stroke_fades_out_completely() {
    let mut stroke = drawn(20, 5.0, 0.0, 10.0);
    stroke.close();

    assert!(
        !stroke.outline(200.0, None).is_empty(),
        "visible when fresh"
    );
    assert!(
        stroke.outline(LASER_DECAY_TIME_MS * 2.0, None).is_empty(),
        "still visible long after the decay time"
    );
}

#[test]
fn is_visible_agrees_with_whether_there_is_an_outline() {
    // `prune` decides what to drop using `is_visible`, which answers in constant time
    // rather than building the outline to find out. If the two ever disagree the engine
    // either keeps dead strokes forever or drops live ones mid-fade, so they are held
    // together here across the whole life of a stroke.
    let mut stroke = drawn(30, 4.0, 0.0, 10.0);
    stroke.close();

    let mut saw_both = (false, false);
    for age in (0..2400).step_by(25) {
        let now = 300.0 + age as f64;
        let drawable = !stroke.outline(now, None).is_empty();
        // The implication that matters, in the direction that matters: anything with an
        // outline must survive pruning. The converse is allowed to lag — see the note on
        // `is_visible` — because being a frame late is free and being early is not.
        if drawable {
            assert!(
                stroke.is_visible(now),
                "a stroke with an outline would have been pruned at now={now}"
            );
        }
        saw_both = (saw_both.0 || drawable, saw_both.1 || !drawable);
    }
    // Otherwise the loop above proves nothing: a stroke that was never drawable, or
    // never finished, would satisfy it vacuously.
    assert_eq!(
        saw_both,
        (true, true),
        "the stroke never lived, or never died"
    );
}

#[test]
fn the_tail_is_eaten_from_behind_while_the_head_keeps_up() {
    // The defining behaviour: an old point drops out of the outline entirely rather than
    // merely thinning, so the trail is a streak behind the cursor and not a growing blob.
    let mut stroke = LaserStroke::new(LaserOptions::default());
    for i in 0..60 {
        stroke.add_point(i as f64 * 10.0, 0.0, i as f64 * 10.0);
    }
    let now = 600.0;
    let outline = stroke.outline(now, None);
    let min_x = outline.iter().fold(f64::MAX, |a, p| a.min(p.x));

    // The oldest samples are more than `LASER_DECAY_LENGTH` points back, so the visible
    // stroke cannot still start at the origin.
    assert!(
        min_x > 0.0,
        "the trail still reaches its oldest point: min_x={min_x}"
    );
}

// ------------------------------------------------------------------ zoom behaviour

#[test]
fn the_beam_keeps_its_width_on_screen_at_every_zoom() {
    // The size override exists for this. Without it the beam would be a fixed number of
    // world units, so zooming out would thin it to nothing and zooming in would turn it
    // into a stripe.
    let mut trails = LaserTrails::default();
    trails.start(0.0, 0.0, 0.0);
    for i in 1..10 {
        trails.add(i as f64, 0.0, 0.0);
    }

    for scale in [0.5, 1.0, 2.0, 8.0] {
        let outlines = trails.outlines(0.0, scale);
        // World radius times the zoom is the radius in screen pixels, and that is what
        // has to stay put.
        let screen_radius = beam_radius(&outlines[0]) * scale;
        assert_near(screen_radius, LASER_SIZE, 1e-4);
    }
}

// ------------------------------------------------------------- several strokes at once

#[test]
fn a_new_flick_does_not_erase_the_one_still_fading() {
    let mut trails = LaserTrails::default();
    trails.start(0.0, 0.0, 0.0);
    trails.add(50.0, 0.0, 10.0);
    trails.add(100.0, 0.0, 20.0);
    trails.end();

    trails.start(0.0, 200.0, 30.0);
    trails.add(50.0, 200.0, 40.0);
    trails.add(100.0, 200.0, 50.0);

    assert_eq!(
        trails.outlines(60.0, 1.0).len(),
        2,
        "the finished stroke should still be on screen"
    );
}

#[test]
fn faded_strokes_are_dropped_and_live_ones_are_not() {
    // Without pruning, a presentation accumulates one dead stroke per flick and every
    // frame walks all of them to draw nothing.
    let mut trails = LaserTrails::default();
    trails.start(0.0, 0.0, 0.0);
    trails.add(50.0, 0.0, 10.0);
    trails.end();

    trails.prune(100.0);
    assert!(
        trails.is_active(100.0),
        "dropped a stroke that was still on screen"
    );

    trails.prune(LASER_DECAY_TIME_MS * 2.0);
    assert!(!trails.is_active(LASER_DECAY_TIME_MS * 2.0));
    assert!(trails.outlines(LASER_DECAY_TIME_MS * 2.0, 1.0).is_empty());
}

#[test]
fn a_stroke_in_progress_counts_as_active_however_old_it_is() {
    // The pointer is still down. Its points may all have faded, but the gesture has not
    // ended, so the next move must still find a stroke to extend.
    let mut trails = LaserTrails::default();
    trails.start(0.0, 0.0, 0.0);
    trails.prune(LASER_DECAY_TIME_MS * 10.0);
    assert!(trails.is_active(LASER_DECAY_TIME_MS * 10.0));
}

#[test]
fn clearing_takes_everything_immediately() {
    let mut trails = LaserTrails::default();
    trails.start(0.0, 0.0, 0.0);
    trails.add(10.0, 0.0, 0.0);
    trails.end();
    trails.start(0.0, 50.0, 0.0);

    trails.clear();
    assert!(!trails.is_active(0.0));
    assert!(trails.outlines(0.0, 1.0).is_empty());
}

// ----------------------------------------------------------- through the engine

#[test]
fn the_laser_is_reachable_by_its_own_key() {
    assert_eq!(tool_for_key("k"), Some(DrawTool::Laser));
    assert_eq!(tool_for_key("K"), Some(DrawTool::Laser));
}

#[test]
fn a_laser_stroke_leaves_nothing_in_the_scene() {
    // The whole point of the tool. A laser mark is a gesture, like pointing at a slide:
    // if it reached the scene it would reach the undo stack, the autosave, and every
    // other person's board.
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Laser);
    engine.set_now(0.0);

    engine.begin_pointer(10.0, 10.0, false, false);
    for i in 1..20 {
        engine.set_now(i as f64 * 8.0);
        engine.move_pointer(10.0 + i as f64 * 6.0, 10.0 + i as f64 * 3.0, false, false);
    }
    engine.end_pointer();

    assert_eq!(engine.get_scene().len(), 0);
}

#[test]
fn a_laser_stroke_does_not_enter_the_undo_stack() {
    // Draw something real, flick the laser over it, then undo once. If the laser had
    // pushed a snapshot, that undo would spend itself reversing the laser and the
    // rectangle would still be here — so one undo has to reach past the laser entirely.
    let mut engine = engine_with_scene(vec![]);
    engine.set_now(0.0);

    engine.set_tool(DrawTool::Rectangle);
    engine.begin_pointer(20.0, 20.0, false, false);
    engine.move_pointer(120.0, 90.0, false, false);
    engine.end_pointer();
    assert_eq!(engine.get_scene().len(), 1, "the rectangle was not drawn");

    engine.set_tool(DrawTool::Laser);
    engine.begin_pointer(10.0, 10.0, false, false);
    for i in 1..10 {
        engine.set_now(i as f64 * 8.0);
        engine.move_pointer(10.0 + i as f64 * 9.0, 10.0, false, false);
    }
    engine.end_pointer();

    engine.undo();
    assert!(
        engine.get_scene().iter().all(|el| el.is_deleted),
        "one undo did not reach past the laser stroke"
    );
}

#[test]
fn the_laser_paints_while_it_fades_and_then_stops() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Laser);
    engine.set_now(0.0);

    engine.begin_pointer(10.0, 10.0, false, false);
    for i in 1..12 {
        engine.set_now(i as f64 * 8.0);
        engine.move_pointer(10.0 + i as f64 * 9.0, 10.0, false, false);
    }
    engine.end_pointer();

    engine.set_now(150.0);
    assert!(
        !engine.paint_view().laser.is_empty(),
        "nothing to paint while the trail should still be visible"
    );

    engine.set_now(LASER_DECAY_TIME_MS * 2.0);
    assert!(
        engine.paint_view().laser.is_empty(),
        "still painting a trail that has finished fading"
    );
}

#[test]
fn a_fading_trail_keeps_asking_for_the_next_frame() {
    // Nothing else on the board changes without input, so if the engine did not hold
    // itself dirty here the trail would freeze part-faded until something unrelated
    // happened to repaint.
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Laser);
    engine.set_now(0.0);

    engine.begin_pointer(10.0, 10.0, false, false);
    engine.set_now(8.0);
    engine.move_pointer(60.0, 10.0, false, false);
    engine.end_pointer();

    engine.set_now(200.0);
    let mut painter = NoopPainter;
    engine.paint_if_dirty(&mut painter);
    assert!(engine.is_dirty(), "stopped animating mid-fade");
    assert!(
        engine.needs_frame(),
        "would not be scheduled for another frame"
    );

    engine.set_now(LASER_DECAY_TIME_MS * 2.0);
    engine.paint_if_dirty(&mut painter);
    assert!(
        !engine.is_dirty(),
        "kept animating after the trail was gone"
    );
}

#[test]
fn the_laser_stays_selected_after_a_stroke() {
    // Excalidraw keeps it selected, and a presentation tool that reverted to Select
    // after every flick would be unusable for presenting.
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Laser);
    engine.set_now(0.0);

    engine.begin_pointer(10.0, 10.0, false, false);
    engine.set_now(8.0);
    engine.move_pointer(60.0, 10.0, false, false);
    engine.end_pointer();

    assert_eq!(engine.get_tool(), DrawTool::Laser);
}

#[test]
fn the_laser_ignores_the_grid() {
    // Snapping is for things that stay on the board. A beam that jumped between grid
    // intersections while the hand it follows moved smoothly would look broken.
    //
    // Asserted as "snapping changes nothing" rather than against a hand-computed
    // coordinate: streamlining moves the end point too, so a literal expected value would
    // encode the smoothing constant as well as the grid behaviour and would need
    // rewriting whenever either changed.
    let flick = |snap: bool| {
        let mut engine = engine_with_scene(vec![]);
        engine.set_tool(DrawTool::Laser);
        engine.set_grid(GridSettings {
            enabled: true,
            snap,
            size: 100.0,
            ..GridSettings::default()
        });
        engine.set_now(0.0);
        engine.begin_pointer(13.0, 7.0, false, false);
        for i in 1..6 {
            engine.set_now(i as f64 * 8.0);
            engine.move_pointer(13.0 + i as f64 * 27.0, 7.0 + i as f64 * 11.0, false, false);
        }
        engine.paint_view().laser[0].clone()
    };

    let free = flick(false);
    let snapped = flick(true);
    assert_eq!(free.len(), snapped.len());
    for (a, b) in free.iter().zip(snapped.iter()) {
        assert_close(a.x, b.x);
        assert_close(a.y, b.y);
    }

    // And confirm that grid really was on, by checking it still moves a tool that does
    // respect it — otherwise the comparison above could be two identically broken runs.
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Rectangle);
    engine.set_grid(GridSettings {
        enabled: true,
        snap: true,
        size: 100.0,
        ..GridSettings::default()
    });
    engine.begin_pointer(13.0, 7.0, false, false);
    engine.move_pointer(147.0, 61.0, false, false);
    engine.end_pointer();
    let rect = &engine.get_scene()[0];
    assert_close(rect.x, 0.0);
    assert_close(rect.width, 100.0);
}

#[test]
fn needs_frame_is_the_only_question_a_host_has_to_ask() {
    // The browser loop had been spelling out `dirty || in_motion` itself, so a trail
    // that the engine's own loop faded correctly froze part-way in the browser.
    // Anything that animates without input has to be visible through one predicate, or
    // every frontend has to be taught about it separately — and will be taught late.
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Laser);
    engine.set_now(0.0);
    engine.begin_pointer(10.0, 10.0, false, false);
    engine.set_now(8.0);
    engine.move_pointer(60.0, 10.0, false, false);
    engine.end_pointer();

    // Clear `dirty`, so only the fade is left to justify another frame.
    engine.set_now(300.0);
    engine.take_dirty();
    assert!(!engine.is_dirty());
    assert!(
        engine.needs_frame(),
        "a host asking only `is_dirty` would stop here, mid-fade"
    );

    engine.set_now(LASER_DECAY_TIME_MS * 3.0);
    engine.take_dirty();
    assert!(!engine.needs_frame());
}

#[test]
fn a_disposed_engine_asks_for_nothing() {
    // `needs_frame` gates the rAF loop, so a stale `true` after teardown is a loop that
    // never stops.
    let mut engine = engine_with_scene(vec![]);
    engine.set_tool(DrawTool::Laser);
    engine.set_now(0.0);
    engine.begin_pointer(10.0, 10.0, false, false);
    engine.set_now(8.0);
    engine.move_pointer(60.0, 10.0, false, false);

    assert!(engine.needs_frame());
    engine.destroy();
    assert!(
        !engine.needs_frame(),
        "kept the frame loop alive after destroy"
    );
}
