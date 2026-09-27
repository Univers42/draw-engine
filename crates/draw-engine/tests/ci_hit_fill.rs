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

// -----------------------------------------------------------------------------
// closed lines: the polygon case
// -----------------------------------------------------------------------------
//
// A line element whose points come back to where they started and which paints a
// background is a polygon, and a polygon is solid. This is what the bucket produces —
// the fill it leaves behind *is* a closed line — so without this rule the paint is a
// ghost: visible, and clickable only along the edge it was traced from, where the
// stroke it was derived from is already competing for the same click.
//
// Excalidraw's rule verbatim, `packages/element/src/collision.ts@1118751f:85-105`:
//
//     if (element.type === "line") {
//       return isDraggableFromInside && isPathALoop(element.points);
//     }
//
// with `isDraggableFromInside` being a background that is not transparent, and an arrow
// excluded before either test is reached.

/// A closed line through the given world points, with the first repeated at the end.
///
/// Sharp, as the bucket fill leaves its paint (`engine/bucket.rs`), so its edges are the
/// straight runs between its points. A rounded loop is drawn, and hit, along the curve
/// through them instead (`ci_curved_linear.rs`).
fn closed_line(points: &[(f64, f64)]) -> DrawElement {
    let (ox, oy) = points[0];
    let mut element = create_element_default(
        DrawElementType::Line,
        Geometry {
            x: ox,
            y: oy,
            width: 0.0,
            height: 0.0,
        },
    );
    let mut ring: Vec<[f64; 2]> = points.iter().map(|&(x, y)| [x - ox, y - oy]).collect();
    ring.push([0.0, 0.0]);
    element.points = Some(ring);
    element.roundness = None;
    element
}

const SQUARE: [(f64, f64); 4] = [(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)];

#[test]
fn a_closed_filled_line_is_solid() {
    let polygon = filled(closed_line(&SQUARE));
    assert!(
        hit_test_element(&polygon, 100.0, 100.0, T),
        "the middle of the paint is the paint"
    );
    assert!(
        hit_test_element(&polygon, 20.0, 180.0, T),
        "and so is any other point inside it"
    );
    assert!(
        !hit_test_element(&polygon, 260.0, 100.0, T),
        "outside is still outside"
    );
}

#[test]
fn a_closed_line_without_a_background_is_only_its_outline() {
    let polygon = closed_line(&SQUARE);
    assert!(
        is_transparent(&polygon.background_color),
        "the default, and what makes this the other half of the rule"
    );
    assert!(
        !hit_test_element(&polygon, 100.0, 100.0, T),
        "nothing is painted there, so the click belongs to what is behind it"
    );
    assert!(
        hit_test_element(&polygon, 100.0, 0.0, T),
        "the edge is still the line"
    );
}

/// The loop is what makes it a polygon. An open path with a background paints nothing
/// enclosed, so there is no interior to click.
#[test]
fn an_open_filled_line_is_not_solid() {
    let mut open = filled(closed_line(&SQUARE));
    // Pull the closing point away, past the threshold that decides closure.
    open.points = Some(vec![
        [0.0, 0.0],
        [200.0, 0.0],
        [200.0, 200.0],
        [0.0, 200.0],
        [0.0, 40.0],
    ]);
    assert!(
        !hit_test_element(&open, 100.0, 100.0, T),
        "a path that does not come back is not a region"
    );
}

/// Eight pixels, and it is the renderer's own threshold rather than a hit-testing one:
/// whether a path paints its background and whether its middle can be clicked have to be
/// the same question, or paint appears that cannot be grabbed.
#[test]
fn the_loop_closes_within_eight_pixels() {
    let nearly = |gap: f64| {
        let mut element = filled(closed_line(&SQUARE));
        element.points = Some(vec![
            [0.0, 0.0],
            [200.0, 0.0],
            [200.0, 200.0],
            [0.0, 200.0],
            [0.0, gap],
        ]);
        element
    };
    assert!(
        hit_test_element(&nearly(8.0), 100.0, 100.0, T),
        "a gap of exactly the threshold still closes"
    );
    assert!(
        !hit_test_element(&nearly(9.0), 100.0, 100.0, T),
        "a gap wider than it does not"
    );
}

/// An arrow is excluded before the background is even looked at. It points at something;
/// its middle is not a region, however closed and however filled.
#[test]
fn an_arrow_is_never_solid() {
    let mut arrow = filled(closed_line(&SQUARE));
    arrow.kind = DrawElementType::Arrow;
    assert!(
        !hit_test_element(&arrow, 100.0, 100.0, T),
        "an arrow is its shaft, never its enclosure"
    );
    assert!(
        hit_test_element(&arrow, 100.0, 0.0, T),
        "the shaft itself still answers"
    );
}

/// A region with something inside it becomes one polygon with a keyhole, and the hole is
/// a hole for clicking too — otherwise the paint swallows every click meant for the thing
/// it was drawn around.
///
/// The hole is spliced with the winding *opposed* to the outer ring, which is what makes
/// the answer independent of the fill rule: non-zero — what `ctx.fill()` uses — and
/// even-odd agree about an opposed hole, and they do not agree about a hole wound the
/// same way. So this pins the behaviour without pinning the rule.
#[test]
fn a_keyhole_is_a_hole_to_click_through_as_well() {
    let mut polygon = filled(closed_line(&SQUARE));
    polygon.points = Some(vec![
        // the outer ring, clockwise, broken open at the left edge
        [0.0, 0.0],
        [200.0, 0.0],
        [200.0, 200.0],
        [0.0, 200.0],
        [0.0, 100.0],
        // in along the seam, once round the island the other way, and back out
        [50.0, 100.0],
        [50.0, 150.0],
        [150.0, 150.0],
        [150.0, 50.0],
        [50.0, 50.0],
        [50.0, 100.0],
        [0.0, 100.0],
        [0.0, 0.0],
    ]);
    assert!(
        !hit_test_element(&polygon, 100.0, 100.0, T),
        "the middle of the island is unpainted, so the click belongs to the island"
    );
    assert!(
        hit_test_element(&polygon, 25.0, 25.0, T),
        "the paint around it is still paint"
    );
}

// ---------------------------------------------------------------------- content kinds
//
// `design.md:1728` (Sticky) and `design.md:1730` (Embeds), two of the eleven kinds §43
// "Hit-testing engine" lists under `hitTest(point, scene)`.
//
// The rule is `has_solid_interior` (`scene/geometry.rs:479-493`): a frame is hollow
// because its middle belongs to its contents, a rectangle/diamond/ellipse/figure is
// hollow when its background is transparent, and **everything else is solid whatever its
// background is**. Text, freedraw and image are already in `content_elements_are_never_hollow`
// above; a sticky note and an embed are the two content kinds that list does not name, so
// nothing has ever hit-tested either of them.
//
// The oracle decides the same way, and by name rather than by falling through:
// `shouldTestInside` returns true for `isIframeLikeElement` — `iframe` or `embeddable`
// (`packages/element/src/typeChecks.ts@1118751f:59-65`) — whatever the background is
// (`packages/element/src/collision.ts@1118751f:85-105`), and `hasBackground` lists
// `stickynote` and `embeddable` among the kinds that have a background at all
// (`packages/element/src/comparisons.ts@1118751f:3-14`), so a note is draggable from
// inside for the same reason a filled rectangle is.

/// Both kinds, on their interiors. The point is the **middle**: the outline is hit either
/// way, so a test that only tried the edge would pass even if the interior were hollow.
#[test]
fn a_sticky_note_is_clickable_in_its_middle() {
    let note = create_element_default(
        DrawElementType::StickyNote,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
        },
    );
    assert!(
        hit_test_element(&note, 100.0, 100.0, T),
        "the middle of a note is the note"
    );
    // Off-centre too: a note is a rectangle to the geometry, so a corner of the pad is as
    // much its interior as the dead centre.
    assert!(hit_test_element(&note, 150.0, 150.0, T));
    assert!(
        !hit_test_element(&note, 260.0, 100.0, T),
        "and it stops at its own edge, tolerance or not"
    );
}

#[test]
fn an_embed_is_clickable_in_its_middle() {
    let embed = create_element_default(
        DrawElementType::Embed,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 180.0,
        },
    );
    assert!(
        hit_test_element(&embed, 160.0, 90.0, T),
        "the middle of an embed is the embed"
    );
    assert!(hit_test_element(&embed, 10.0, 170.0, T));
    assert!(!hit_test_element(&embed, 400.0, 90.0, T));
}

/// The part that makes it a decision rather than a fall-through: a **transparent** note or
/// embed is still solid, where a transparent rectangle is hollow. This is the oracle's
/// `isIframeLikeElement` arm of `shouldTestInside` and it is the whole difference — a
/// background of "transparent" does not make a note or an embed hollow, because what is
/// inside them is content, not canvas showing through.
#[test]
fn a_transparent_sticky_note_or_embed_is_still_solid_where_a_transparent_rectangle_is_not() {
    let mut note = create_element_default(
        DrawElementType::StickyNote,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
        },
    );
    note.background_color = "transparent".to_string();
    assert!(
        hit_test_element(&note, 100.0, 100.0, T),
        "hollowing a note would make its label unclickable and the note unreachable"
    );

    let mut embed = create_element_default(
        DrawElementType::Embed,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 180.0,
        },
    );
    embed.background_color = "transparent".to_string();
    assert!(hit_test_element(&embed, 160.0, 90.0, T));

    // The contrast, so the assertion above is about the kind and not about the colour
    // having been ignored: the same colour on a rectangle *is* hollow.
    let mut rect = box_at(0.0, 0.0, 200.0, 200.0);
    rect.background_color = "transparent".to_string();
    assert!(
        !hit_test_element(&rect, 100.0, 100.0, T),
        "a rectangle with no fill is only its outline"
    );
}

/// Through the engine, because that is the claim that matters: a click in the middle of a
/// note selects the note, and a click just past its edge does not. `hit_test_element` is
/// the primitive; this is `hitTest → selection`, which is what §43 says the two are for.
#[test]
fn clicking_the_middle_of_a_note_selects_it() {
    let note = create_element_default(
        DrawElementType::StickyNote,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
        },
    );
    let id = note.id.clone();
    let mut engine = engine_with_scene(vec![note]);
    engine.set_tool(DrawTool::Select);

    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();
    assert_eq!(
        engine.get_selection(),
        vec![id.clone()],
        "the dead centre of the note"
    );

    engine.select(vec![]);
    engine.begin_pointer(260.0, 100.0, false, false);
    engine.end_pointer();
    assert!(
        engine.get_selection().is_empty(),
        "and 60px past its right edge is the canvas"
    );
}
