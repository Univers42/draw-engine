//! Hit testing a turned element.
//!
//! Everything about an element is stored unrotated and turned about its centre when it is
//! drawn. The hit test has to undo that turn, or it answers for a shape that is not on
//! screen: selectable from empty space, and dead where it is actually painted.

mod common;
use common::*;
use draw_engine::*;

const QUARTER: f64 = std::f64::consts::FRAC_PI_2;
const EIGHTH: f64 = std::f64::consts::FRAC_PI_4;

/// A long rectangle turned upright is hit where it now is, not where it was.
#[test]
fn a_quarter_turn_moves_the_hit_area_with_the_shape() {
    // Filled: this is about where the shape is, not about whether its middle counts,
    // which `ci_hit_fill` covers.
    let mut element = filled(box_at(0.0, 40.0, 200.0, 20.0));
    element.angle = QUARTER;
    // Centre is (100, 50); upright, the bar now runs from y=-50 to y=150 at x=100.
    assert!(hit_test_element(&element, 100.0, 0.0, 0.0), "now covered");
    assert!(hit_test_element(&element, 100.0, 140.0, 0.0), "now covered");
    assert!(
        !hit_test_element(&element, 10.0, 50.0, 0.0),
        "the old horizontal extent is empty space once the bar is upright"
    );
}

/// The case that reads as broken: a square turned 45 degrees.
#[test]
fn a_square_turned_45_degrees_is_hit_on_its_points_not_its_old_corners() {
    let mut element = filled(box_at(0.0, 0.0, 100.0, 100.0));
    element.angle = EIGHTH;
    // Centre (50, 50). The diamond's points reach ~70.7 from the centre along the axes.
    assert!(
        hit_test_element(&element, 50.0, -15.0, 0.0),
        "the top point is part of the shape"
    );
    assert!(
        !hit_test_element(&element, 2.0, 2.0, 0.0),
        "the old flat corner is now outside the shape"
    );
}

/// A full turn is the identity, and a half turn is symmetric — cheap guards against a
/// sign error in the rotation.
#[test]
fn whole_turns_change_nothing() {
    let plain = filled(box_at(0.0, 0.0, 80.0, 40.0));
    let mut turned = plain.clone();
    turned.angle = std::f64::consts::TAU;
    for (x, y) in [(5.0, 5.0), (79.0, 39.0), (-5.0, 20.0), (40.0, 20.0)] {
        assert_eq!(
            hit_test_element(&plain, x, y, 0.0),
            hit_test_element(&turned, x, y, 0.0),
            "({x}, {y}) differs after a full turn"
        );
    }
}

/// An ellipse stays elliptical when turned: the test must run in the element's own frame,
/// not against a circle or the enclosing box.
#[test]
fn a_turned_ellipse_keeps_its_shape() {
    let mut element = filled(ellipse_at(0.0, 0.0, 200.0, 40.0));
    element.angle = QUARTER;
    // Centre (100, 20); upright it is 40 wide and 200 tall.
    assert!(hit_test_element(&element, 100.0, -70.0, 0.0), "tall now");
    assert!(
        !hit_test_element(&element, 10.0, 20.0, 0.0),
        "no longer wide"
    );
}

/// A line's points are relative to its origin, so it needs the same treatment.
#[test]
fn a_turned_arrow_is_hit_along_its_painted_path() {
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 0.0,
        },
    );
    arrow.points = Some(vec![[0.0, 0.0], [100.0, 0.0]]);
    arrow.angle = QUARTER;
    // Centre (50, 0); upright the arrow runs from (50, -50) to (50, 50).
    assert!(hit_test_element(&arrow, 50.0, 40.0, 4.0));
    assert!(!hit_test_element(&arrow, 95.0, 0.0, 4.0));
}

/// The marquee measures the box a shape really occupies.
///
/// A 200x20 bar turned upright occupies x 90..110, y -50..150. On the *unrotated* box it
/// would have looked like x 0..200, y 40..60 — so a rectangle enclosing where the bar now
/// stands would have missed it, and one enclosing where it used to lie would have taken
/// it. Both are checked.
#[test]
fn the_marquee_uses_the_rotated_box() {
    let mut element = box_at(0.0, 40.0, 200.0, 20.0);
    element.angle = QUARTER;
    let id = element.id.clone();
    let elements = vec![element];

    let round_where_it_stands = marquee_rect(80.0, -60.0, 120.0, 160.0);
    assert_eq!(
        elements_in_marquee(&elements, round_where_it_stands),
        vec![id]
    );

    let round_where_it_used_to_lie = marquee_rect(-10.0, 30.0, 210.0, 70.0);
    assert!(
        elements_in_marquee(&elements, round_where_it_used_to_lie).is_empty(),
        "the bar is no longer horizontal, so it is not inside its old footprint"
    );
}

/// The rotated box of an unturned element is just its box — the fast path must agree with
/// the general one.
#[test]
fn rotated_bounds_match_plain_bounds_without_rotation() {
    let element = box_at(-30.0, 12.0, 140.0, 70.0);
    let plain = element_bounds(&element);
    let rotated = element_rotated_bounds(&element);
    assert_close(plain.min_x, rotated.min_x);
    assert_close(plain.min_y, rotated.min_y);
    assert_close(plain.max_x, rotated.max_x);
    assert_close(plain.max_y, rotated.max_y);
}

/// A square turned 45 degrees spans its diagonal.
#[test]
fn rotated_bounds_span_the_diagonal() {
    let mut element = filled(box_at(0.0, 0.0, 100.0, 100.0));
    element.angle = EIGHTH;
    let b = element_rotated_bounds(&element);
    assert_close(b.max_x - b.min_x, 100.0 * std::f64::consts::SQRT_2);
    assert_close(b.max_y - b.min_y, 100.0 * std::f64::consts::SQRT_2);
}
