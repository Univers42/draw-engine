//! Free-form selection.
//!
//! All of it is geometry, so all of it is tested here rather than through a browser. A
//! host reports where the pointer went and paints the path it is handed back; what the
//! loop catches is the engine's answer, and must be the same on every platform.

mod common;
use common::*;
use draw_engine::*;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

/// A closed loop around the given box, as a hand would roughly draw it.
fn loop_around(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Vec<Point> {
    vec![
        p(min_x, min_y),
        p(max_x, min_y),
        p(max_x, max_y),
        p(min_x, max_y),
    ]
}

fn ids(elements: &[DrawElement], path: &[Point], mode: LassoMode) -> Vec<String> {
    elements_in_lasso(elements.iter(), path, mode)
}

// ---------------------------------------------------------------- point in polygon

#[test]
fn a_square_encloses_its_middle_and_not_its_outside() {
    let square = loop_around(0.0, 0.0, 100.0, 100.0);
    assert!(polygon_contains_point(&square, p(50.0, 50.0)));
    assert!(!polygon_contains_point(&square, p(150.0, 50.0)));
    assert!(!polygon_contains_point(&square, p(-10.0, 50.0)));
    assert!(!polygon_contains_point(&square, p(50.0, 150.0)));
}

#[test]
fn a_degenerate_polygon_encloses_nothing() {
    assert!(!polygon_contains_point(&[], p(0.0, 0.0)));
    assert!(!polygon_contains_point(&[p(0.0, 0.0)], p(0.0, 0.0)));
    assert!(!polygon_contains_point(
        &[p(0.0, 0.0), p(10.0, 0.0)],
        p(5.0, 0.0)
    ));
}

/// The reason for winding rather than even-odd.
///
/// A lasso is drawn by hand and crosses itself constantly. Under even-odd, a loop that
/// doubles back punches a hole in its own middle, so scribbling around a diagram would
/// take some of it and leave the rest.
#[test]
fn a_self_crossing_loop_has_no_hole_in_it() {
    // A "pretzel": a big loop with an extra crossing lobe inside it.
    let path = vec![
        p(0.0, 0.0),
        p(100.0, 0.0),
        p(100.0, 100.0),
        p(0.0, 100.0),
        p(0.0, 0.0),
        p(50.0, 20.0),
        p(80.0, 50.0),
        p(50.0, 80.0),
        p(20.0, 50.0),
    ];
    // The doubly-wound centre is still inside.
    assert!(polygon_contains_point(&path, p(50.0, 50.0)));
    assert!(!polygon_contains_point(&path, p(200.0, 200.0)));
}

/// Winding is signed, so the direction the loop is drawn in must not matter.
#[test]
fn drawing_the_loop_the_other_way_round_is_the_same_loop() {
    let clockwise = loop_around(0.0, 0.0, 100.0, 100.0);
    let mut anticlockwise = clockwise.clone();
    anticlockwise.reverse();

    for probe in [p(50.0, 50.0), p(1.0, 1.0), p(150.0, 50.0), p(-5.0, -5.0)] {
        assert_eq!(
            polygon_contains_point(&clockwise, probe),
            polygon_contains_point(&anticlockwise, probe),
            "({}, {}) differs by winding direction",
            probe.x,
            probe.y
        );
    }
}

/// A concave loop must not behave like its bounding box.
#[test]
fn a_concave_loop_excludes_the_bite_taken_out_of_it() {
    // A "C": the notch on the right is outside the loop but inside its box.
    let c = vec![
        p(0.0, 0.0),
        p(100.0, 0.0),
        p(100.0, 30.0),
        p(40.0, 30.0),
        p(40.0, 70.0),
        p(100.0, 70.0),
        p(100.0, 100.0),
        p(0.0, 100.0),
    ];
    assert!(polygon_contains_point(&c, p(20.0, 50.0)), "the spine");
    assert!(!polygon_contains_point(&c, p(80.0, 50.0)), "the notch");
}

// ------------------------------------------------------------------- simplification

#[test]
fn simplifying_keeps_the_ends() {
    let path: Vec<Point> = (0..50).map(|i| p(i as f64, 0.0)).collect();
    let simple = simplify_path(&path, 1.0);
    assert_eq!(simple.first().map(|q| q.x), Some(0.0));
    assert_eq!(simple.last().map(|q| q.x), Some(49.0));
}

#[test]
fn simplifying_collapses_a_straight_run() {
    // A hundred points along a line describe a line, and two points say it just as well.
    let path: Vec<Point> = (0..100).map(|i| p(i as f64, 0.0)).collect();
    assert_eq!(simplify_path(&path, 1.0).len(), 2);
}

#[test]
fn simplifying_keeps_a_corner() {
    let path = vec![p(0.0, 0.0), p(50.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)];
    let simple = simplify_path(&path, 1.0);
    // The collinear middle point goes, the corner stays.
    assert_eq!(simple.len(), 3);
    assert_close(simple[1].x, 100.0);
    assert_close(simple[1].y, 0.0);
}

#[test]
fn simplifying_a_short_path_changes_nothing() {
    for path in [vec![], vec![p(1.0, 2.0)], vec![p(1.0, 2.0), p(3.0, 4.0)]] {
        assert_eq!(simplify_path(&path, 5.0).len(), path.len());
    }
    // A zero tolerance is a request to keep everything.
    let dense: Vec<Point> = (0..20).map(|i| p(i as f64, 0.0)).collect();
    assert_eq!(simplify_path(&dense, 0.0).len(), 20);
}

/// The simplification must not move the loop, or it would select different things.
#[test]
fn simplifying_does_not_shift_the_path() {
    let jitter: Vec<Point> = (0..200)
        .map(|i| {
            let t = i as f64;
            p(t, (t * 0.7).sin() * 0.4)
        })
        .collect();
    let simple = simplify_path(&jitter, 2.0);
    assert!(simple.len() < jitter.len(), "it did simplify");
    for q in &simple {
        assert!(q.y.abs() <= 0.5, "a kept point moved to y={}", q.y);
    }
}

// ------------------------------------------------------------------------ selection

#[test]
fn a_loop_around_a_shape_takes_it() {
    let mut shape = box_at(100.0, 100.0, 100.0, 80.0);
    shape.id = "s".into();
    let scene = vec![shape];
    assert_eq!(
        ids(
            &scene,
            &loop_around(50.0, 50.0, 300.0, 300.0),
            LassoMode::Contain
        ),
        vec!["s"]
    );
}

#[test]
fn a_loop_beside_a_shape_takes_nothing() {
    let mut shape = box_at(100.0, 100.0, 100.0, 80.0);
    shape.id = "s".into();
    let scene = vec![shape];
    assert!(ids(
        &scene,
        &loop_around(400.0, 400.0, 600.0, 600.0),
        LassoMode::Contain
    )
    .is_empty());
}

/// `Contain` means contained. Half an element is not an element.
#[test]
fn a_loop_over_half_a_shape_takes_nothing_when_containing() {
    let mut shape = box_at(100.0, 100.0, 200.0, 80.0);
    shape.id = "s".into();
    let scene = vec![shape];
    // Covers the left half only.
    let path = loop_around(50.0, 50.0, 200.0, 250.0);
    assert!(ids(&scene, &path, LassoMode::Contain).is_empty());
    // ...but intersecting takes it.
    assert_eq!(ids(&scene, &path, LassoMode::Intersect), vec!["s"]);
}

/// The case a bounding-box test gets wrong: a concave loop whose *box* contains the
/// element but whose *shape* does not.
#[test]
fn a_concave_loop_does_not_take_what_sits_in_its_notch() {
    let mut shape = box_at(300.0, 200.0, 60.0, 60.0);
    shape.id = "s".into();
    let scene = vec![shape];

    // A "C" open to the right; the shape sits in the mouth, inside the loop's box.
    let c = vec![
        p(0.0, 0.0),
        p(400.0, 0.0),
        p(400.0, 150.0),
        p(200.0, 150.0),
        p(200.0, 350.0),
        p(400.0, 350.0),
        p(400.0, 500.0),
        p(0.0, 500.0),
    ];
    assert!(
        ids(&scene, &c, LassoMode::Contain).is_empty(),
        "the notch is outside the loop even though it is inside its box"
    );
}

#[test]
fn an_ellipse_is_tested_against_its_curve() {
    let mut round = ellipse_at(100.0, 100.0, 200.0, 200.0);
    round.id = "e".into();
    let scene = vec![round];
    // A loop snug around the circle takes it.
    assert_eq!(
        ids(
            &scene,
            &loop_around(95.0, 95.0, 305.0, 305.0),
            LassoMode::Contain
        ),
        vec!["e"]
    );
}

#[test]
fn a_line_is_tested_against_its_path() {
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 0.0,
        },
    );
    arrow.id = "a".into();
    arrow.points = Some(vec![[0.0, 0.0], [200.0, 0.0]]);
    let scene = vec![arrow];

    assert_eq!(
        ids(
            &scene,
            &loop_around(50.0, 50.0, 350.0, 150.0),
            LassoMode::Contain
        ),
        vec!["a"]
    );
    // A loop crossing it but not containing it only counts when intersecting.
    let crossing = loop_around(150.0, 60.0, 250.0, 140.0);
    assert!(ids(&scene, &crossing, LassoMode::Contain).is_empty());
    assert_eq!(ids(&scene, &crossing, LassoMode::Intersect), vec!["a"]);
}

#[test]
fn a_locked_element_is_not_taken() {
    let mut shape = box_at(100.0, 100.0, 100.0, 80.0);
    shape.id = "s".into();
    shape.locked = Some(true);
    let scene = vec![shape];
    assert!(ids(
        &scene,
        &loop_around(0.0, 0.0, 400.0, 400.0),
        LassoMode::Contain
    )
    .is_empty());
}

#[test]
fn a_deleted_element_is_not_taken() {
    let mut shape = box_at(100.0, 100.0, 100.0, 80.0);
    shape.id = "s".into();
    shape.is_deleted = true;
    let scene = vec![shape];
    assert!(ids(
        &scene,
        &loop_around(0.0, 0.0, 400.0, 400.0),
        LassoMode::Contain
    )
    .is_empty());
}

/// A label is carried by its container, never taken alone.
#[test]
fn a_bound_label_is_left_to_its_container() {
    let mut shape = box_at(100.0, 100.0, 200.0, 100.0);
    shape.id = "s".into();
    let mut label = create_element_default(
        DrawElementType::Text,
        Geometry {
            x: 120.0,
            y: 130.0,
            width: 80.0,
            height: 25.0,
        },
    );
    label.id = "label".into();
    label.container_id = Some("s".into());

    let got = ids(
        &[label, shape],
        &loop_around(0.0, 0.0, 500.0, 500.0),
        LassoMode::Contain,
    );
    assert_eq!(got, vec!["s"]);
}

#[test]
fn a_path_too_short_to_enclose_anything_selects_nothing() {
    let mut shape = box_at(100.0, 100.0, 100.0, 80.0);
    shape.id = "s".into();
    let scene = vec![shape];
    for path in [
        vec![],
        vec![p(0.0, 0.0)],
        vec![p(0.0, 0.0), p(400.0, 400.0)],
    ] {
        assert!(ids(&scene, &path, LassoMode::Contain).is_empty());
    }
}

#[test]
fn a_turned_element_is_measured_where_it_is() {
    let mut shape = box_at(100.0, 190.0, 200.0, 20.0);
    shape.id = "s".into();
    shape.angle = std::f64::consts::FRAC_PI_2;
    let scene = vec![shape];

    // Upright it occupies roughly x 190..210, y 100..300.
    assert_eq!(
        ids(
            &scene,
            &loop_around(180.0, 90.0, 220.0, 310.0),
            LassoMode::Contain
        ),
        vec!["s"],
        "a loop around where it now stands"
    );
    assert!(
        ids(
            &scene,
            &loop_around(90.0, 180.0, 310.0, 220.0),
            LassoMode::Contain
        )
        .is_empty(),
        "and not one around where it used to lie"
    );
}

#[test]
fn several_elements_come_back_in_scene_order() {
    let mut a = box_at(0.0, 0.0, 50.0, 50.0);
    a.id = "a".into();
    let mut b = box_at(100.0, 0.0, 50.0, 50.0);
    b.id = "b".into();
    let mut c = box_at(200.0, 0.0, 50.0, 50.0);
    c.id = "c".into();

    let got = ids(
        &[a, b, c],
        &loop_around(-20.0, -20.0, 300.0, 100.0),
        LassoMode::Contain,
    );
    assert_eq!(got, vec!["a", "b", "c"]);
}

// ------------------------------------------------------------------ through the engine

#[test]
fn the_tool_round_trips_through_its_name() {
    assert_eq!(DrawTool::Lasso.as_str(), "lasso");
}

#[test]
fn drawing_a_loop_selects_what_it_encloses() {
    let mut a = box_at(100.0, 100.0, 80.0, 60.0);
    a.id = "a".into();
    let mut far = box_at(600.0, 400.0, 80.0, 60.0);
    far.id = "far".into();

    let mut engine = DrawEngine::new();
    engine.set_viewport(900.0, 700.0, 1.0);
    engine.set_scene(Scene::new(vec![a, far]));
    engine.set_tool(DrawTool::Lasso);

    // A loop around the first shape only.
    engine.begin_pointer(60.0, 60.0, false, false);
    for (x, y) in [(260.0, 60.0), (260.0, 220.0), (60.0, 220.0)] {
        engine.move_pointer(x, y, false, false);
    }
    engine.end_pointer();

    assert_eq!(engine.get_selection(), vec!["a".to_string()]);
}

/// Holding the additive modifier adds to the selection rather than replacing it.
#[test]
fn an_additive_loop_keeps_what_was_already_selected() {
    let mut a = box_at(100.0, 100.0, 80.0, 60.0);
    a.id = "a".into();
    let mut b = box_at(400.0, 100.0, 80.0, 60.0);
    b.id = "b".into();

    let mut engine = DrawEngine::new();
    engine.set_viewport(900.0, 700.0, 1.0);
    engine.set_scene(Scene::new(vec![a, b]));
    engine.select(vec!["b".to_string()]);
    engine.set_tool(DrawTool::Lasso);

    engine.begin_pointer(60.0, 60.0, true, false);
    for (x, y) in [(260.0, 60.0), (260.0, 220.0), (60.0, 220.0)] {
        engine.move_pointer(x, y, true, false);
    }
    engine.end_pointer();

    let mut got = engine.get_selection();
    got.sort();
    assert_eq!(got, vec!["a".to_string(), "b".to_string()]);
}
