//! Recognising the shape someone meant to draw.
//!
//! A classifier is only as good as the strokes it is shown, so these are drawn rather
//! than asserted: each test synthesises the path a hand would trace — with wobble, with
//! overshoot, closed sloppily — and asks what it was taken for. Testing the statistics in
//! isolation would pass while the thing people actually do still failed.

mod common;
use common::*;
use draw_engine::*;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

/// A deterministic wobble, so a "hand-drawn" stroke is reproducible.
///
/// A real hand is not random noise, but it is not a straight edge either, and the whole
/// point of the recogniser is to survive the difference.
fn wobble(i: usize, amount: f64) -> f64 {
    let t = i as f64;
    (t * 12.9898).sin() * amount
}

/// Walk from `a` to `b` in `steps` samples, with a little tremor across the path.
fn edge(a: Point, b: Point, steps: usize, tremor: f64) -> Vec<Point> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy).max(1e-9);
    let (nx, ny) = (-dy / len, dx / len);
    (0..steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let off = wobble(i, tremor);
            p(a.x + dx * t + nx * off, a.y + dy * t + ny * off)
        })
        .collect()
}

/// A hand-drawn rectangle, optionally left slightly open at the start.
fn drawn_rectangle(w: f64, h: f64, tremor: f64, overshoot: f64) -> Vec<Point> {
    let corners = [p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)];
    let mut path = Vec::new();
    for i in 0..4 {
        path.extend(edge(corners[i], corners[(i + 1) % 4], 20, tremor));
    }
    // Nobody stops exactly where they started.
    path.push(p(overshoot, 0.0));
    path
}

fn drawn_ellipse(rx: f64, ry: f64, tremor: f64) -> Vec<Point> {
    (0..80)
        .map(|i| {
            let t = i as f64 / 80.0 * std::f64::consts::TAU;
            let r = 1.0 + wobble(i, tremor) / rx.max(ry);
            p(rx + rx * r * t.cos(), ry + ry * r * t.sin())
        })
        .collect()
}

fn drawn_diamond(w: f64, h: f64, tremor: f64) -> Vec<Point> {
    let corners = [
        p(w / 2.0, 0.0),
        p(w, h / 2.0),
        p(w / 2.0, h),
        p(0.0, h / 2.0),
    ];
    let mut path = Vec::new();
    for i in 0..4 {
        path.extend(edge(corners[i], corners[(i + 1) % 4], 20, tremor));
    }
    path.push(p(w / 2.0, 0.0));
    path
}

fn drawn_line(len: f64, tremor: f64) -> Vec<Point> {
    edge(p(0.0, 0.0), p(len, 0.0), 40, tremor)
}

/// A line with an arrowhead: out to the tip, back to one barb, out again, back again.
fn drawn_arrow(len: f64) -> Vec<Point> {
    let mut path = edge(p(0.0, 0.0), p(len, 0.0), 40, 1.0);
    let tip = p(len, 0.0);
    let head = len * 0.12;
    path.extend(edge(tip, p(len - head, -head * 0.6), 6, 0.0));
    path.extend(edge(p(len - head, -head * 0.6), tip, 6, 0.0));
    path.extend(edge(tip, p(len - head, head * 0.6), 6, 0.0));
    path
}

// --------------------------------------------------------------- what it recognises

#[test]
fn a_hand_drawn_rectangle_is_a_rectangle() {
    assert_eq!(
        recognize_shape(&drawn_rectangle(300.0, 180.0, 3.0, 6.0), 1.0),
        RecognizedShape::Rectangle
    );
}

#[test]
fn a_rectangle_is_still_a_rectangle_when_it_is_drawn_badly() {
    // The case the recogniser exists for. A clean rectangle is easy; one with real
    // tremor and a corner missed by a few pixels is what people actually draw.
    for tremor in [1.0, 5.0, 9.0] {
        for overshoot in [-12.0, 0.0, 14.0] {
            assert_eq!(
                recognize_shape(&drawn_rectangle(320.0, 200.0, tremor, overshoot), 1.0),
                RecognizedShape::Rectangle,
                "tremor {tremor}, overshoot {overshoot}"
            );
        }
    }
}

#[test]
fn a_hand_drawn_ellipse_is_an_ellipse() {
    assert_eq!(
        recognize_shape(&drawn_ellipse(160.0, 100.0, 4.0), 1.0),
        RecognizedShape::Ellipse
    );
}

#[test]
fn a_circle_is_an_ellipse_too() {
    assert_eq!(
        recognize_shape(&drawn_ellipse(120.0, 120.0, 3.0), 1.0),
        RecognizedShape::Ellipse
    );
}

#[test]
fn a_hand_drawn_diamond_is_a_diamond() {
    assert_eq!(
        recognize_shape(&drawn_diamond(280.0, 200.0, 3.0), 1.0),
        RecognizedShape::Diamond
    );
}

#[test]
fn a_rectangle_and_an_ellipse_are_not_confused_for_each_other() {
    // They are the pair most likely to collide, because a tapered rectangle fills about
    // as much of its box as an ellipse does. What separates them is where the turning
    // is: all at four corners, or spread along the whole outline.
    let rect = recognize_shape(&drawn_rectangle(300.0, 180.0, 6.0, 8.0), 1.0);
    let ellipse = recognize_shape(&drawn_ellipse(150.0, 90.0, 6.0), 1.0);
    assert_eq!(rect, RecognizedShape::Rectangle);
    assert_eq!(ellipse, RecognizedShape::Ellipse);
}

#[test]
fn a_straight_stroke_is_a_line() {
    assert_eq!(
        recognize_shape(&drawn_line(300.0, 4.0), 1.0),
        RecognizedShape::Line
    );
}

#[test]
fn a_stroke_with_a_head_on_it_is_an_arrow() {
    // An arrowhead is drawn by going out to the point and back, which piles points up at
    // one end. That lopsidedness is the whole signal.
    assert_eq!(
        recognize_shape(&drawn_arrow(320.0), 1.0),
        RecognizedShape::Arrow
    );
}

// --------------------------------------------------------- what it refuses to recognise

#[test]
fn a_scribble_stays_a_scribble() {
    // The important half. A recogniser that turns everything into a rectangle is worse
    // than none, because it changes drawings nobody asked it to change.
    // A crossing-out: back and forth, drifting sideways. Deliberately *not* a smooth
    // oscillation — a Lissajous curve is regular enough to read as an ellipse, and
    // fairly so, because it is one.
    let mut scribble = Vec::new();
    for i in 0..8 {
        let x = i as f64 * 40.0;
        let down = i % 2 == 0;
        scribble.extend(edge(
            p(x, if down { 0.0 } else { 200.0 }),
            p(x + 40.0, if down { 200.0 } else { 0.0 }),
            12,
            4.0,
        ));
    }
    assert_eq!(recognize_shape(&scribble, 1.0), RecognizedShape::Freedraw);
}

#[test]
fn an_elbow_is_not_a_line() {
    // Its point cloud is elongated and its statistics look line-like; only the distance
    // from its own chord gives it away. This is what `shaft_deviation_ratio` is for.
    let mut elbow = edge(p(0.0, 0.0), p(200.0, 0.0), 30, 0.0);
    elbow.extend(edge(p(200.0, 0.0), p(200.0, 160.0), 30, 0.0));
    assert_eq!(recognize_shape(&elbow, 1.0), RecognizedShape::Freedraw);
}

#[test]
fn an_arc_is_not_a_line() {
    let arc: Vec<Point> = (0..50)
        .map(|i| {
            let t = i as f64 / 50.0 * std::f64::consts::PI;
            p(150.0 - 150.0 * t.cos(), -90.0 * t.sin())
        })
        .collect();
    assert_eq!(recognize_shape(&arc, 1.0), RecognizedShape::Freedraw);
}

#[test]
fn a_stroke_too_small_to_be_meant_is_left_alone() {
    // A flick of a few pixels while reaching for something else is not a rectangle.
    let tiny = drawn_rectangle(8.0, 6.0, 0.5, 0.0);
    assert_eq!(recognize_shape(&tiny, 1.0), RecognizedShape::Freedraw);
}

#[test]
fn the_size_test_is_about_the_screen_and_not_the_board() {
    // The same physical gesture should behave the same however far in or out the board
    // is: a stroke that is small in world units but large on screen was still deliberate.
    let small_in_world = drawn_rectangle(12.0, 8.0, 0.3, 0.0);
    assert_eq!(
        recognize_shape(&small_in_world, 1.0),
        RecognizedShape::Freedraw,
        "tiny on screen at 1:1"
    );
    assert_eq!(
        recognize_shape(&small_in_world, 8.0),
        RecognizedShape::Rectangle,
        "the same stroke zoomed in is a deliberate rectangle"
    );
}

#[test]
fn too_few_points_to_judge_are_left_alone() {
    assert_eq!(recognize_shape(&[], 1.0), RecognizedShape::Freedraw);
    assert_eq!(
        recognize_shape(&[p(0.0, 0.0), p(100.0, 100.0)], 1.0),
        RecognizedShape::Freedraw
    );
}

#[test]
fn a_stroke_that_never_moved_is_left_alone() {
    // A press with a tremor in it. Every statistic is degenerate, and dividing by the
    // spread would produce NaN and a shape chosen at random.
    let still = vec![p(50.0, 50.0); 40];
    assert_eq!(recognize_shape(&still, 1.0), RecognizedShape::Freedraw);
}

// -------------------------------------------------------------------- the machinery

#[test]
fn resampling_evens_out_a_stroke_drawn_at_an_uneven_speed() {
    // Features are moments of the point set, so raw samples weight the statistics
    // towards wherever the hand slowed down and crowded points together. Asserted as
    // "much more even than it was" rather than "perfectly even": see the note on
    // `resample`, which is faithful to a sampler that only equalises properly for the
    // densely sampled strokes a pointer actually produces.
    let spread = |pts: &[Point]| {
        let gaps: Vec<f64> = pts
            .windows(2)
            .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
            .collect();
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        let variance = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / gaps.len() as f64;
        variance.sqrt() / mean
    };

    // One slow stretch and one quick one, both sampled per frame as a pointer would be.
    let mut uneven = Vec::new();
    for i in 0..120 {
        uneven.push(p(i as f64 * 0.4, 0.0));
    }
    for i in 1..80 {
        uneven.push(p(48.0 + i as f64 * 4.4, 0.0));
    }

    let before = spread(&uneven);
    let after = spread(&resample(&uneven, 64));
    assert!(
        after < before / 4.0,
        "resampling barely evened the stroke out: {before} before, {after} after"
    );
}

#[test]
fn resampling_always_returns_what_was_asked_for() {
    // Rounding at the end of the walk used to leave the last slot or two empty, and a
    // short array makes every downstream index arithmetic wrong.
    for points in [1usize, 2, 3, 7, 500] {
        let stroke: Vec<Point> = (0..points).map(|i| p(i as f64, 0.0)).collect();
        assert_eq!(resample(&stroke, 64).len(), 64, "{points} points in");
    }
    assert_eq!(
        resample(&[p(1.0, 1.0); 9], 64).len(),
        64,
        "no length at all"
    );
}

#[test]
fn the_convex_hull_of_a_box_is_its_corners() {
    let mut points = vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
    // Interior points must not survive.
    points.push(p(5.0, 5.0));
    points.push(p(3.0, 7.0));

    let hull = convex_hull(&points);
    assert_eq!(hull.len(), 4);
    assert_close(polygon_area(&hull), 100.0);
}

#[test]
fn a_hulls_area_never_exceeds_its_box() {
    // `hull_fill_ratio` is an area over a box area, so a hull larger than its own box
    // would push the ratio past 1 and drag a shape towards the rectangle prototype.
    let ellipse = drawn_ellipse(100.0, 60.0, 2.0);
    let hull = convex_hull(&ellipse);
    let min_x = ellipse.iter().fold(f64::MAX, |a, q| a.min(q.x));
    let max_x = ellipse.iter().fold(f64::MIN, |a, q| a.max(q.x));
    let min_y = ellipse.iter().fold(f64::MAX, |a, q| a.min(q.y));
    let max_y = ellipse.iter().fold(f64::MIN, |a, q| a.max(q.y));
    let box_area = (max_x - min_x) * (max_y - min_y);

    let ratio = polygon_area(&hull) / box_area;
    assert!(ratio <= 1.0, "hull fill ratio {ratio}");
    // A circle fills PI/4 of its box; a hand-drawn one should be close.
    assert!(
        (ratio - std::f64::consts::FRAC_PI_4).abs() < 0.1,
        "an ellipse should fill about PI/4 of its box, got {ratio}"
    );
}

#[test]
fn the_features_of_a_rectangle_are_the_ones_the_prototype_expects() {
    // The prototypes are constants with no derivation next to them. This pins what they
    // are supposed to describe, so a change to the feature code that quietly shifts them
    // fails here rather than in a misrecognised drawing.
    let features = extract_features(&drawn_rectangle(300.0, 200.0, 2.0, 2.0));
    assert!(
        features.gap_ratio < CLOSED_GAP_MAX_RATIO,
        "a rectangle is a closed stroke: {}",
        features.gap_ratio
    );
    assert!(
        features.hull_fill_ratio > 0.9,
        "a rectangle fills its box: {}",
        features.hull_fill_ratio
    );
    assert!(
        features.corner_turn_share > 0.8,
        "a rectangle turns at its corners: {}",
        features.corner_turn_share
    );
}

#[test]
fn the_features_of_a_line_are_the_ones_the_open_rule_expects() {
    let features = extract_features(&drawn_line(300.0, 2.0));
    assert!(
        features.gap_ratio > CLOSED_GAP_MAX_RATIO,
        "a line is an open stroke: {}",
        features.gap_ratio
    );
    assert!(
        features.elongation < LINEAR_MAX_ELONGATION,
        "a line has almost no spread across itself: {}",
        features.elongation
    );
    assert!(
        features.shaft_deviation_ratio < LINEAR_MAX_SHAFT_DEVIATION,
        "a line stays near its own chord: {}",
        features.shaft_deviation_ratio
    );
}

#[test]
fn recognition_does_not_depend_on_where_the_stroke_was_drawn() {
    // Everything is measured from the centroid, so a rectangle drawn a mile from the
    // origin is the same rectangle. This is the property that makes a whole-board test
    // unnecessary.
    let here = drawn_rectangle(300.0, 200.0, 3.0, 4.0);
    let far: Vec<Point> = here
        .iter()
        .map(|q| p(q.x + 50_000.0, q.y - 31_000.0))
        .collect();
    assert_eq!(recognize_shape(&here, 1.0), recognize_shape(&far, 1.0));
}

#[test]
fn a_square_turned_forty_five_degrees_is_a_diamond() {
    // Deliberately not rotation invariant, and Excalidraw's own note says the same: a
    // square on its point *is* a diamond, and pretending otherwise would make the
    // diamond tool unreachable by drawing.
    let square = drawn_rectangle(200.0, 200.0, 2.0, 2.0);
    let (sin, cos) = std::f64::consts::FRAC_PI_4.sin_cos();
    let turned: Vec<Point> = square
        .iter()
        .map(|q| {
            let (dx, dy) = (q.x - 100.0, q.y - 100.0);
            p(100.0 + dx * cos - dy * sin, 100.0 + dx * sin + dy * cos)
        })
        .collect();

    assert_eq!(recognize_shape(&square, 1.0), RecognizedShape::Rectangle);
    assert_eq!(recognize_shape(&turned, 1.0), RecognizedShape::Diamond);
}

#[test]
fn an_arrows_tip_is_where_it_points_and_not_where_the_pen_stopped() {
    // An arrowhead is drawn out to the point and back, so the stroke ends on a barb.
    // Taking the last point would put the arrow's head slightly short and off to one
    // side of where it was aimed.
    let stroke = drawn_arrow(300.0);
    let (start, tip) = arrow_endpoints(&stroke).expect("an arrow has endpoints");
    assert_close(start.x, stroke[0].x);
    assert!(
        tip.x > 290.0,
        "the tip should be at the far end, not on a barb: {}",
        tip.x
    );
    assert!(
        tip.x > stroke[stroke.len() - 1].x,
        "the pen stopped at {} but the arrow points to {}",
        stroke[stroke.len() - 1].x,
        tip.x
    );
}

// -------------------------------------------------------------- through the engine

/// Draw a stroke through the engine with the auto-shape tool.
fn draw_with_autoshape(engine: &mut DrawEngine, stroke: &[Point]) -> Option<DrawElement> {
    engine.set_tool(DrawTool::AutoShape);
    engine.begin_pointer(stroke[0].x, stroke[0].y, false, false);
    for point in &stroke[1..] {
        engine.move_pointer(point.x, point.y, false, false);
    }
    engine.end_pointer();
    engine.get_scene().into_iter().find(|el| !el.is_deleted)
}

#[test]
fn drawing_a_rectangle_freehand_leaves_a_rectangle() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let element = draw_with_autoshape(&mut engine, &drawn_rectangle(300.0, 200.0, 3.0, 4.0))
        .expect("nothing was drawn");

    assert_eq!(element.kind, DrawElementType::Rectangle);
    // A shape is described by its box. Leaving the path behind would make every later
    // hit test and resize read points that no longer mean anything.
    assert!(element.points.is_none(), "the stroke's path survived");
    assert!(element.width > 250.0 && element.height > 150.0);
}

#[test]
fn drawing_a_scribble_freehand_leaves_the_scribble() {
    // The tool has to be safe to leave on. The worst it may do is nothing.
    let mut scribble = Vec::new();
    for i in 0..8 {
        let x = i as f64 * 40.0;
        let down = i % 2 == 0;
        scribble.extend(edge(
            p(x, if down { 0.0 } else { 200.0 }),
            p(x + 40.0, if down { 200.0 } else { 0.0 }),
            12,
            4.0,
        ));
    }

    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let element = draw_with_autoshape(&mut engine, &scribble).expect("nothing was drawn");

    assert_eq!(element.kind, DrawElementType::Freedraw);
    assert!(element.points.is_some(), "the stroke lost its path");
}

#[test]
fn drawing_an_arrow_freehand_leaves_an_arrow_pointing_the_right_way() {
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let element = draw_with_autoshape(&mut engine, &drawn_arrow(320.0)).expect("nothing was drawn");

    assert_eq!(element.kind, DrawElementType::Arrow);
    let points = element.points.as_ref().expect("an arrow has points");
    assert_eq!(points.len(), 2);
    // Pointing at the tip, not back at the barb the pen happened to stop on.
    assert!(points[1][0] > 280.0, "the arrow is short: {:?}", points[1]);
}

#[test]
fn converting_a_stroke_is_one_undoable_step() {
    // The stroke is never committed on its own, so one undo takes the shape away rather
    // than turning it back into a scribble nobody drew.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    draw_with_autoshape(&mut engine, &drawn_rectangle(300.0, 200.0, 3.0, 4.0));

    engine.undo();
    assert!(
        engine.get_scene().iter().all(|el| el.is_deleted),
        "one undo left something behind"
    );
}

#[test]
fn converting_keeps_the_elements_id() {
    // Replaced in place, so a selection or a binding still refers to it afterwards.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    let element = draw_with_autoshape(&mut engine, &drawn_rectangle(300.0, 200.0, 3.0, 4.0))
        .expect("nothing was drawn");
    assert_eq!(engine.get_selection(), vec![element.id]);
}

#[test]
fn converting_keeps_the_style_that_was_chosen_before_drawing() {
    // Someone who picked a colour meant it for whatever the stroke became.
    let mut engine = engine_with_scene(vec![]);
    engine.set_viewport(1200.0, 900.0, 1.0);
    engine.set_next_style(stroke_patch("#e03131"));

    let element = draw_with_autoshape(&mut engine, &drawn_rectangle(300.0, 200.0, 3.0, 4.0))
        .expect("nothing was drawn");
    assert_eq!(element.kind, DrawElementType::Rectangle);
    assert_eq!(element.stroke_color, "#e03131");
}

#[test]
fn the_auto_shape_tool_is_reachable_by_its_own_chord() {
    // Shift+X, which is Excalidraw's. `G` was invented here because the keymap took a key
    // and no modifier, so a chord could not be expressed at all — see ci_shortcuts.rs,
    // where the whole table is pinned against the oracle.
    assert_eq!(tool_for_chord("x", true), Some(DrawTool::AutoShape));
    assert_eq!(tool_for_key("g"), None);
}

#[test]
fn converting_something_that_is_not_a_stroke_does_nothing() {
    // `convert_to_shape` is public, so it can be pointed at anything.
    let rect = box_at(0.0, 0.0, 100.0, 80.0);
    let id = rect.id.clone();
    let mut engine = engine_with_scene(vec![rect]);
    assert_eq!(engine.convert_to_shape(&id), RecognizedShape::Freedraw);
    assert_eq!(
        engine.convert_to_shape("no-such-element"),
        RecognizedShape::Freedraw
    );
}
