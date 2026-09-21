//! Where a line or arrow actually is, and what happens to it when a shape it is bound
//! to moves.
//!
//! A linear element is anchored differently from a shape: `x`/`y` is the position of its
//! **first point**, not a corner of a box, and `width`/`height` are the size of the point
//! cloud rather than an offset from `x`. Reading `x + width` as its right edge is
//! therefore wrong for any arrow that does not run left-to-right, and it was wrong
//! everywhere — bounds, culling, the marquee and the centre it turns about.

mod common;
use common::*;
use draw_engine::*;

fn arrow_with(points: Vec<[f64; 2]>, x: f64, y: f64) -> DrawElement {
    let last = *points.last().unwrap();
    let mut el = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x,
            y,
            width: last[0],
            height: last[1],
        },
    );
    el.points = Some(points);
    el
}

/// The case that showed it: an arrow whose tail is dragged past its head.
#[test]
fn a_leftward_arrow_is_where_it_is_drawn() {
    // Runs from (800, 300) back to (600, 300).
    let arrow = arrow_with(vec![[0.0, 0.0], [-200.0, 0.0]], 800.0, 300.0);
    let b = element_bounds(&arrow);
    assert_close(b.min_x, 600.0);
    assert_close(b.max_x, 800.0);
}

/// A bent arrow reaches well outside the box its two ends span.
#[test]
fn a_bent_arrow_is_measured_by_all_of_its_points() {
    let arrow = arrow_with(
        vec![[0.0, 0.0], [100.0, -150.0], [200.0, 150.0], [300.0, 0.0]],
        200.0,
        300.0,
    );
    let b = element_bounds(&arrow);
    assert_close(b.min_x, 200.0);
    assert_close(b.max_x, 500.0);
    assert_close(b.min_y, 150.0);
    assert_close(b.max_y, 450.0);
}

/// The marquee has to find it where it is, and must not find it where it is not.
#[test]
fn the_marquee_finds_a_leftward_arrow() {
    let mut arrow = arrow_with(vec![[0.0, 0.0], [-200.0, 0.0]], 800.0, 300.0);
    arrow.id = "a".into();
    let scene = vec![arrow];

    assert_eq!(
        elements_in_marquee(&scene, marquee_rect(580.0, 270.0, 820.0, 330.0)),
        vec!["a"],
        "a marquee over the arrow must select it"
    );
    assert!(
        elements_in_marquee(&scene, marquee_rect(820.0, 270.0, 1060.0, 330.0)).is_empty(),
        "and one over the empty space beyond its tail must not"
    );
}

/// The pivot is the middle of the points. `x + width / 2` put it beyond the arrow's own
/// tip, so turning one swung it about a point in empty space beside it.
#[test]
fn a_leftward_arrow_turns_about_itself() {
    let arrow = arrow_with(vec![[0.0, 0.0], [-200.0, 0.0]], 800.0, 300.0);
    let c = rotation_center(&arrow);
    assert_close(c.x, 700.0);
    assert_close(c.y, 300.0);

    let bounds = element_bounds(&arrow);
    assert!(
        c.x >= bounds.min_x && c.x <= bounds.max_x,
        "the pivot has to be inside the arrow"
    );
}

/// Rotating cannot change how much room something takes up.
#[test]
fn turning_an_arrow_half_way_leaves_its_box_the_same_size() {
    let mut arrow = arrow_with(vec![[0.0, 0.0], [200.0, 80.0]], 100.0, 100.0);
    let before = element_rotated_bounds(&arrow);
    arrow.angle = std::f64::consts::PI;
    let after = element_rotated_bounds(&arrow);

    assert_close(after.max_x - after.min_x, before.max_x - before.min_x);
    assert_close(after.max_y - after.min_y, before.max_y - before.min_y);
    // A half turn about the centre maps the box onto itself.
    assert_close(after.min_x, before.min_x);
    assert_close(after.min_y, before.min_y);
}

/// The defect that loses work: binding used to rewrite the point list as a straight
/// pair, so every bend a user had put into an arrow vanished the moment the shape it
/// pointed at was nudged.
#[test]
fn re_anchoring_keeps_the_bends() {
    let mut shape = filled(box_at(500.0, 100.0, 120.0, 80.0));
    shape.id = "box".into();

    let mut arrow = arrow_with(
        vec![[0.0, 0.0], [120.0, -80.0], [240.0, 20.0], [400.0, -160.0]],
        100.0,
        300.0,
    );
    arrow.id = "arrow".into();
    arrow.end_binding = Some("box".into());

    let refreshed = refresh_bindings(&[shape, arrow]);
    let out = refreshed.iter().find(|e| e.id == "arrow").unwrap();
    let points = out.points.as_ref().unwrap();

    assert_eq!(points.len(), 4, "the bends must survive re-anchoring");
    assert_eq!(points[0], [0.0, 0.0], "the origin stays on the first point");
}

/// The interior points do not merely survive, they stay put on the board.
#[test]
fn re_anchoring_leaves_the_bends_where_they_were() {
    let mut shape = filled(box_at(500.0, 100.0, 120.0, 80.0));
    shape.id = "box".into();

    let mut arrow = arrow_with(
        vec![[0.0, 0.0], [120.0, -80.0], [240.0, 20.0], [400.0, -160.0]],
        100.0,
        300.0,
    );
    arrow.id = "arrow".into();
    arrow.end_binding = Some("box".into());

    // World position of the middle points before anything moves.
    let before: Vec<(f64, f64)> = arrow.points.as_ref().unwrap()[1..3]
        .iter()
        .map(|p| (arrow.x + p[0], arrow.y + p[1]))
        .collect();

    let refreshed = refresh_bindings(&[shape, arrow]);
    let out = refreshed.iter().find(|e| e.id == "arrow").unwrap();
    let after: Vec<(f64, f64)> = out.points.as_ref().unwrap()[1..3]
        .iter()
        .map(|p| (out.x + p[0], out.y + p[1]))
        .collect();

    for (b, a) in before.iter().zip(after.iter()) {
        assert_close(a.0, b.0);
        assert_close(a.1, b.1);
    }
}

/// A two-point arrow has nothing to preserve, and still attaches.
#[test]
fn a_straight_arrow_still_attaches() {
    let mut shape = filled(box_at(500.0, 100.0, 120.0, 80.0));
    shape.id = "box".into();
    let mut arrow = arrow_with(vec![[0.0, 0.0], [380.0, 0.0]], 100.0, 140.0);
    arrow.id = "arrow".into();
    arrow.end_binding = Some("box".into());

    let refreshed = refresh_bindings(&[shape, arrow]);
    let out = refreshed.iter().find(|e| e.id == "arrow").unwrap();
    let points = out.points.as_ref().unwrap();
    assert_eq!(points.len(), 2);

    // The tip lands on the shape's left edge, a gap short of it.
    let tip_x = out.x + points[1][0];
    assert_close(tip_x, 500.0 - BINDING_GAP);
}
