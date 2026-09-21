//! Whether a shape's middle belongs to it.
//!
//! Excalidraw's rule, and it is not an implementation detail — it decides whether a big
//! transparent rectangle drawn around a diagram swallows every click meant for the
//! diagram. Verified against excalidraw.com on a clean board holding exactly one
//! transparent rectangle: clicking its dead centre selects nothing, clicking its border
//! selects it.

mod common;
use common::*;
use draw_engine::*;

const T: f64 = 10.0;

#[test]
fn transparent_is_the_default() {
    // If this ever changes, every assertion below is testing the wrong thing.
    assert!(is_transparent(
        &box_at(0.0, 0.0, 10.0, 10.0).background_color
    ));
}

#[test]
fn a_transparent_shape_is_hollow() {
    let rect = box_at(0.0, 0.0, 400.0, 250.0);
    assert!(
        !hit_test_element(&rect, 200.0, 125.0, T),
        "the middle is the canvas showing through"
    );
    assert!(
        !hit_test_element(&rect, 60.0, 60.0, T),
        "and so is anywhere else clear of the outline"
    );
    assert!(hit_test_element(&rect, 200.0, 0.0, T), "the top edge is it");
    assert!(
        hit_test_element(&rect, 0.0, 125.0, T),
        "the left edge is it"
    );
    assert!(hit_test_element(&rect, 400.0, 250.0, T), "and the corner");
}

#[test]
fn a_filled_shape_is_solid() {
    let rect = filled(box_at(0.0, 0.0, 400.0, 250.0));
    assert!(hit_test_element(&rect, 200.0, 125.0, T));
    assert!(hit_test_element(&rect, 200.0, 0.0, T));
    assert!(!hit_test_element(&rect, 200.0, -40.0, T), "still bounded");
}

/// The band reaches the same distance inwards and outwards.
#[test]
fn the_outline_band_is_symmetric() {
    let rect = box_at(0.0, 0.0, 400.0, 250.0);
    assert!(hit_test_element(&rect, 200.0, 8.0, T), "8 inside");
    assert!(hit_test_element(&rect, 200.0, -8.0, T), "8 outside");
    assert!(!hit_test_element(&rect, 200.0, 12.0, T), "12 inside misses");
    assert!(
        !hit_test_element(&rect, 200.0, -12.0, T),
        "12 outside misses"
    );
}

#[test]
fn hollowness_follows_the_shape_not_its_box() {
    let ellipse = ellipse_at(0.0, 0.0, 200.0, 200.0);
    assert!(!hit_test_element(&ellipse, 100.0, 100.0, T), "centre");
    assert!(hit_test_element(&ellipse, 100.0, 0.0, T), "top of the arc");
    assert!(
        !hit_test_element(&ellipse, 5.0, 5.0, T),
        "the box corner is outside the ellipse entirely"
    );

    let diamond = diamond_at(0.0, 0.0, 200.0, 200.0);
    assert!(!hit_test_element(&diamond, 100.0, 100.0, T));
    assert!(hit_test_element(&diamond, 100.0, 0.0, T), "the top point");
    assert!(!hit_test_element(&diamond, 10.0, 10.0, T), "the box corner");
}

/// A shape too thin to have an interior is all outline, so it stays grabbable.
#[test]
fn a_shape_thinner_than_the_band_stays_solid() {
    let sliver = box_at(0.0, 0.0, 400.0, 6.0);
    assert!(hit_test_element(&sliver, 200.0, 3.0, T));
}

/// Hex with a zero alpha paints nothing, so it is transparent too.
#[test]
fn zero_alpha_hex_counts_as_transparent() {
    assert!(is_transparent("transparent"));
    assert!(is_transparent("TRANSPARENT"));
    assert!(is_transparent("#ffffff00"));
    assert!(is_transparent("#1E1E1E00"));
    assert!(is_transparent("#fff0"));
    assert!(!is_transparent("#ffffff"));
    assert!(!is_transparent("#ffffff01"));
    assert!(!is_transparent("#ffec99"));
    assert!(!is_transparent(""));
}

/// Text, freehand and images are content, not outlines. Their background is transparent
/// by default and that says nothing about whether their middle can be clicked.
#[test]
fn content_elements_are_never_hollow() {
    // A frame used to be in this list, by falling through rather than by decision. It is
    // the one element whose middle belongs to something else: see
    // `a_frame_is_hollow_because_its_middle_belongs_to_its_contents` below.
    for kind in [
        DrawElementType::Text,
        DrawElementType::Freedraw,
        DrawElementType::Image,
    ] {
        let element = create_element_default(
            kind,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 100.0,
            },
        );
        assert!(
            hit_test_element(&element, 100.0, 50.0, T),
            "{kind:?} must be clickable in the middle"
        );
    }
}

/// The rule survives rotation: it is applied in the element's own frame.
#[test]
fn a_turned_hollow_shape_is_still_hollow() {
    let mut rect = box_at(0.0, 0.0, 400.0, 250.0);
    rect.angle = std::f64::consts::FRAC_PI_4;
    assert!(
        !hit_test_element(&rect, 200.0, 125.0, T),
        "centre is centre"
    );

    // The top edge midpoint, carried round by the rotation about (200, 125).
    let (sin, cos) = rect.angle.sin_cos();
    let (dx, dy) = (0.0_f64, -125.0_f64);
    let x = 200.0 + dx * cos - dy * sin;
    let y = 125.0 + dx * sin + dy * cos;
    assert!(hit_test_element(&rect, x, y, T), "the turned top edge");
}

/// The practical consequence, and the reason the rule exists: a frame drawn round a
/// diagram must not eat the clicks meant for the diagram.
#[test]
fn a_hollow_shape_does_not_swallow_what_is_inside_it() {
    let mut outer = box_at(0.0, 0.0, 600.0, 400.0);
    outer.id = "outer".into();
    let mut inner = filled(box_at(250.0, 170.0, 100.0, 60.0));
    inner.id = "inner".into();

    // `outer` is last, so it is on top and would win any overlap.
    let scene = vec![inner, outer];
    let hit = hit_test(&scene, 300.0, 200.0, T);
    assert_eq!(
        hit.map(|el| el.id.as_str()),
        Some("inner"),
        "the click belongs to the shape that is actually there"
    );
}

/// The marquee takes what it **encloses**, not what it brushes past.
///
/// Confirmed against excalidraw.com: a rectangle drawn fully around a shape selects it,
/// while one that clips its corner, crosses its middle or touches its edge selects
/// nothing. On overlap, a small drag across a large background shape quietly picked the
/// background up too.
#[test]
fn the_marquee_requires_containment() {
    let mut shape = box_at(500.0, 300.0, 300.0, 200.0);
    shape.id = "s".into();
    let scene = vec![shape];

    let take = |r| elements_in_marquee(&scene, r);
    assert_eq!(take(marquee_rect(450.0, 250.0, 860.0, 560.0)), vec!["s"]);
    assert!(
        take(marquee_rect(430.0, 240.0, 600.0, 380.0)).is_empty(),
        "clips a corner"
    );
    assert!(
        take(marquee_rect(400.0, 380.0, 900.0, 420.0)).is_empty(),
        "crosses the middle"
    );
    assert!(
        take(marquee_rect(780.0, 480.0, 900.0, 600.0)).is_empty(),
        "touches a corner"
    );
    // Exactly coincident counts as enclosed.
    assert_eq!(take(marquee_rect(500.0, 300.0, 800.0, 500.0)), vec!["s"]);
}

/// A label is carried by its container, so a marquee must not pick it up on its own —
/// dragging it alone would pull the text out of the shape it labels.
#[test]
fn the_marquee_leaves_bound_labels_to_their_container() {
    let mut label = create_element_default(
        DrawElementType::Text,
        Geometry {
            x: 520.0,
            y: 380.0,
            width: 100.0,
            height: 25.0,
        },
    );
    label.id = "label".into();
    label.container_id = Some("s".into());
    let mut shape = box_at(500.0, 300.0, 300.0, 200.0);
    shape.id = "s".into();

    let got = elements_in_marquee(&[label, shape], marquee_rect(450.0, 250.0, 860.0, 560.0));
    assert_eq!(got, vec!["s"]);
}

/// A frame is the one element whose middle is not its own.
///
/// Every other element with no fill to leave out — text, a freehand stroke, an image —
/// *is* its content, so its middle is clickable. A frame is a boundary drawn around
/// other people's content, so its middle belongs to them. Treated like the rest it
/// swallows every click that lands inside it, and framing a diagram makes the diagram
/// unselectable.
#[test]
fn a_frame_is_hollow_because_its_middle_belongs_to_its_contents() {
    let frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        },
    );
    assert!(
        !hit_test_element(&frame, 100.0, 50.0, T),
        "a click in the middle of a frame must fall through to what is inside it"
    );
    assert!(
        hit_test_element(&frame, 0.0, 50.0, T),
        "a frame must still be grabbable by its border"
    );
}
