//! A rounded line or arrow is the curve it is drawn as, not the points it is drawn
//! through.
//!
//! Both halves of that used to be the points: a click was measured against the straight
//! segments between them, so the bow of a curved arrow — everything but its ends and its
//! corners — was a dead zone; and a longer path's points could only be reached through
//! the line editor, which a double click on an arrow never opens (it opens the label).

mod common;

use common::*;
use draw_engine::scene::distance_to_segment;
use draw_engine::selection::LinearHandle;
use draw_engine::*;

/// The arrow the user drew: three clicks, a peak in the middle, rounded by default.
fn peaked(kind: DrawElementType) -> DrawElement {
    let mut element = create_element_default(
        kind,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 120.0,
        },
    );
    element.points = Some(vec![[0.0, 120.0], [150.0, 0.0], [300.0, 120.0]]);
    assert!(element.roundness.is_some(), "setup: rounded by default");
    element
}

/// Where the drawn curve is half way along its first segment.
fn on_the_bow(element: &DrawElement) -> [f64; 2] {
    let world: Vec<[f64; 2]> = element
        .points
        .as_deref()
        .unwrap()
        .iter()
        .map(|p| [element.x + p[0], element.y + p[1]])
        .collect();
    let cubics = catmull_rom_cubics(&world, 0.0);
    bezier_point_at_fraction(&cubics[0], 0.5)
}

#[test]
fn a_click_on_a_curved_arrows_bow_hits_it() {
    let arrow = peaked(DrawElementType::Arrow);
    let [x, y] = on_the_bow(&arrow);
    // Setup: the bow is well clear of the chord from (0, 120) to (150, 0), or this
    // would pass against the straight segments too.
    let chord = distance_to_segment(x, y, 0.0, 120.0, 150.0, 0.0);
    assert!(chord > 10.0, "setup: the bow is {chord} from its chord");

    assert!(
        hit_test_element(&arrow, x, y, 1.0),
        "the curve at ({x}, {y})"
    );
}

#[test]
fn a_sharp_arrow_is_still_its_straight_segments() {
    let mut arrow = peaked(DrawElementType::Arrow);
    arrow.roundness = None;
    let [x, y] = on_the_bow(&arrow);
    assert!(
        !hit_test_element(&arrow, x, y, 1.0),
        "a sharp arrow is not drawn along that curve"
    );
    assert!(
        hit_test_element(&arrow, 75.0, 60.0, 1.0),
        "the chord middle"
    );
}

/// Inside the curve but outside the triangle its points make: the bulge is painted, so
/// it is solid.
#[test]
fn a_click_in_the_bulge_of_a_filled_rounded_loop_hits_it() {
    let mut line = create_element_default(
        DrawElementType::Line,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: 180.0,
        },
    );
    line.points = Some(vec![[0.0, 0.0], [220.0, 0.0], [110.0, 180.0], [0.0, 0.0]]);
    line.background_color = "#ffec99".into();
    assert!(line.roundness.is_some(), "setup: rounded by default");
    // The right edge runs from (220, 0) to (110, 180); at y = 101 it is at x ≈ 158, and
    // the curve bows out to x ≈ 186 there.
    let (x, y) = (175.0, 101.0);
    assert!(
        distance_to_segment(x, y, 220.0, 0.0, 110.0, 180.0) > 10.0,
        "setup: well outside the straight edge"
    );

    assert!(
        hit_test_element(&line, x, y, 1.0),
        "inside the painted bulge"
    );
    assert!(
        !hit_test_element(&line, 200.0, 101.0, 1.0),
        "and not beyond the curve"
    );
}

#[test]
fn a_selected_longer_arrow_offers_its_points_beside_its_box() {
    let arrow = peaked(DrawElementType::Arrow);
    let id = arrow.id.clone();
    let mut engine = engine_with_scene(vec![arrow]);
    engine.select(vec![id]);

    let offered = engine.linear_points();
    assert_eq!(
        offered
            .iter()
            .map(|handle| handle.handle)
            .collect::<Vec<_>>(),
        vec![
            LinearHandle::Point(0),
            LinearHandle::Point(1),
            LinearHandle::Point(2)
        ],
        "every point, and no midpoints until the editor is open"
    );
}

/// The user's report: the peak could not be moved, the whole arrow went with it.
#[test]
fn dragging_the_peak_of_a_selected_arrow_moves_the_peak() {
    let arrow = peaked(DrawElementType::Arrow);
    let id = arrow.id.clone();
    let mut engine = engine_with_scene(vec![arrow]);
    engine.select(vec![id.clone()]);

    engine.begin_pointer(150.0, 0.0, false, false);
    engine.move_pointer(180.0, -40.0, false, false);
    engine.end_pointer();

    let after = engine.get_scene().into_iter().find(|e| e.id == id).unwrap();
    let world: Vec<[f64; 2]> = after
        .points
        .as_deref()
        .unwrap()
        .iter()
        .map(|p| [after.x + p[0], after.y + p[1]])
        .collect();
    assert_eq!(world[0], [0.0, 120.0], "the start stayed");
    assert_eq!(world[1], [180.0, -40.0], "the peak followed the pointer");
    assert_eq!(world[2], [300.0, 120.0], "the end stayed");
}

#[test]
fn a_locked_arrow_offers_no_points() {
    let mut arrow = peaked(DrawElementType::Arrow);
    arrow.locked = Some(true);
    let id = arrow.id.clone();
    let mut engine = engine_with_scene(vec![arrow]);
    engine.select(vec![id]);
    assert!(engine.linear_points().is_empty());
}
