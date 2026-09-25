//! Bucket fill: the region under a click.
//!
//! The tests are grouped by the stage of the pipeline they pin, because a fill that comes
//! out wrong is almost always wrong at one identifiable stage, and the failure name should
//! say which.

mod common;

use common::*;
use draw_engine::*;

// -----------------------------------------------------------------------------
// helpers
// -----------------------------------------------------------------------------

/// A line element through the given world points.
///
/// `x`/`y` is the first point and `points` are offsets from it, which is how the engine
/// stores every point-based element.
fn poly(points: &[(f64, f64)]) -> DrawElement {
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
    element.points = Some(points.iter().map(|&(x, y)| [x - ox, y - oy]).collect());
    element.stroke_color = "#1e1e1e".into();
    element
}

/// A closed ring: the first point repeated at the end, which is what makes a line element
/// read as a polygon.
fn ring(points: &[(f64, f64)]) -> DrawElement {
    let mut closed = points.to_vec();
    closed.push(points[0]);
    poly(&closed)
}

/// A rectangle with a visible stroke, so it is an eligible boundary.
fn stroked_box(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    let mut element = box_at(x, y, w, h);
    element.stroke_color = "#1e1e1e".into();
    element
}

/// A rectangle that paints an opaque background, so it covers what is under it.
fn opaque_box(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    let mut element = stroked_box(x, y, w, h);
    element.background_color = "#ffec99".into();
    element.fill_style = FillStyle::Solid;
    element
}

/// A next-style patch that only sets the background, which is what the bucket paints with.
fn style_background(color: &str) -> DrawElementStylePatch {
    DrawElementStylePatch {
        background_color: Some(color.to_string()),
        ..Default::default()
    }
}

/// A rectangle cut in half by a line, and a point in the left half.
///
/// The bucket colours a *shape's own background* when the region under the click is
/// exactly that shape's inside, so a test about the polygon it makes otherwise has to ask
/// for a region that is not one. Half a rectangle is the smallest such region.
fn divided_box(w: f64, h: f64) -> Vec<DrawElement> {
    vec![
        stroked_box(0.0, 0.0, w, h),
        poly(&[(w / 2.0, 0.0), (w / 2.0, h)]),
    ]
}

fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn fill(elements: &[DrawElement], point: Point) -> Result<BucketFill, BucketFillFailure> {
    let refs: Vec<&DrawElement> = elements.iter().collect();
    compute_bucket_fill(point, &refs, &BucketFillOptions::default())
}

/// The axis-aligned box of a ring, so a test can say where the fill landed without
/// asserting every vertex of a simplified polygon.
fn ring_bounds(points: &[Point]) -> (f64, f64, f64, f64) {
    points.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
    )
}

fn assert_bounds_near(points: &[Point], expected: (f64, f64, f64, f64), tolerance: f64) {
    let got = ring_bounds(points);
    for (a, b, name) in [
        (got.0, expected.0, "min x"),
        (got.1, expected.1, "min y"),
        (got.2, expected.2, "max x"),
        (got.3, expected.3, "max y"),
    ] {
        assert!(
            (a - b).abs() <= tolerance,
            "{name}: expected {b}, got {a} (full bounds {got:?})"
        );
    }
}

// -----------------------------------------------------------------------------
// polygon primitives
//
// The whole tool hangs off the sign convention here, so it is pinned directly rather than
// only through the behaviour it produces.
// -----------------------------------------------------------------------------

#[test]
fn signed_area_is_positive_for_counter_clockwise_winding() {
    // y-down, so this order is counter-clockwise on screen.
    let ccw = [at(0.0, 0.0), at(10.0, 0.0), at(10.0, 10.0), at(0.0, 10.0)];
    assert_close(polygon_signed_area(&ccw, 0.0), 100.0);

    let cw: Vec<Point> = ccw.iter().rev().copied().collect();
    assert_close(polygon_signed_area(&cw, 0.0), -100.0);
}

#[test]
fn signed_area_ignores_a_repeated_closing_vertex() {
    // A ring may arrive open or closed; both describe the same polygon, so both must
    // measure the same. The face walk hands back open rings and `finalize_polygon`
    // hands back closed ones.
    let open = [at(0.0, 0.0), at(4.0, 0.0), at(4.0, 4.0), at(0.0, 4.0)];
    let mut closed = open.to_vec();
    closed.push(open[0]);
    assert_close(
        polygon_signed_area(&open, 0.0),
        polygon_signed_area(&closed, 0.0),
    );
}

#[test]
fn even_odd_and_non_zero_disagree_about_a_keyhole() {
    // This is why there are two containment rules rather than one. A keyhole ring walks
    // into a hole and back out along the same line; even-odd — the rule the renderer
    // fills with — says the hole is unpainted, which is the whole point of the technique.
    let outer = [at(0.0, 0.0), at(10.0, 0.0), at(10.0, 10.0), at(0.0, 10.0)];
    let hole = [at(3.0, 3.0), at(3.0, 7.0), at(7.0, 7.0), at(7.0, 3.0)];
    let keyhole: Vec<Point> = outer
        .iter()
        .copied()
        .chain(hole.iter().copied())
        .chain(std::iter::once(hole[0]))
        .chain(std::iter::once(outer[0]))
        .collect();

    let middle = at(5.0, 5.0);
    assert!(
        !polygon_includes_point(middle, &keyhole),
        "even-odd must read the hole as unpainted"
    );
    let corner = at(1.0, 1.0);
    assert!(
        polygon_includes_point(corner, &keyhole),
        "the ring outside the hole is still painted"
    );
}

#[test]
fn segments_that_do_not_reach_each_other_do_not_intersect() {
    let a = (at(0.0, 0.0), at(10.0, 0.0));
    let b = (at(5.0, 1.0), at(5.0, 10.0));
    assert!(
        segment_intersection_point(a, b, 1e-6).is_none(),
        "the lines cross, but the segments stop short of each other"
    );
    let c = (at(5.0, -1.0), at(5.0, 10.0));
    let hit = segment_intersection_point(a, c, 1e-6).expect("these do cross");
    assert_point_close(hit, at(5.0, 0.0));
}

#[test]
fn parallel_segments_never_report_an_intersection() {
    // Collinear overlap deliberately returns nothing here: it is the T-junction pass that
    // handles it, by finding the endpoint that necessarily lies on the other segment.
    let a = (at(0.0, 0.0), at(10.0, 0.0));
    let b = (at(4.0, 0.0), at(14.0, 0.0));
    assert!(segment_intersection_point(a, b, 1e-6).is_none());
}

#[test]
fn simplify_keeps_the_corners_and_drops_the_filler() {
    // A straight run of points down one edge carries no information; a corner does.
    let mut pts = vec![at(0.0, 0.0)];
    for i in 1..20 {
        pts.push(at(i as f64, 0.0));
    }
    pts.push(at(20.0, 20.0));
    let simplified = simplify_path(&pts, 0.75);
    assert_eq!(
        simplified.len(),
        3,
        "expected start, corner and end, got {simplified:?}"
    );
    assert_point_close(simplified[0], at(0.0, 0.0));
    assert_point_close(simplified[2], at(20.0, 20.0));
}

// -----------------------------------------------------------------------------
// what counts as paint
// -----------------------------------------------------------------------------

#[test]
fn only_a_solid_fully_opaque_background_covers_what_is_under_it() {
    let mut element = opaque_box(0.0, 0.0, 10.0, 10.0);
    assert!(renders_opaque_fill(&element));

    // A hatched fill is see-through by construction.
    element.fill_style = FillStyle::Hachure;
    assert!(!renders_opaque_fill(&element));
    element.fill_style = FillStyle::Solid;

    // So is anything below full opacity.
    element.opacity = 99.0;
    assert!(!renders_opaque_fill(&element));
    element.opacity = 100.0;

    // And so is a colour carrying its own alpha.
    element.background_color = "#ffec9900".into();
    assert!(!renders_opaque_fill(&element));
    element.background_color = "#ffec9980".into();
    assert!(!renders_opaque_fill(&element));
    element.background_color = "#ffec99ff".into();
    assert!(renders_opaque_fill(&element));
}

#[test]
fn an_open_stroke_never_paints_its_background() {
    // Whatever background colour it carries, the renderer draws nothing inside an open
    // path — so it cannot hide an outline beneath it either.
    let mut open = poly(&[(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)]);
    open.background_color = "#ffec99".into();
    open.fill_style = FillStyle::Solid;
    assert!(!renders_opaque_fill(&open));

    let mut closed = ring(&[(0.0, 0.0), (50.0, 0.0), (50.0, 50.0), (0.0, 50.0)]);
    closed.background_color = "#ffec99".into();
    closed.fill_style = FillStyle::Solid;
    assert!(renders_opaque_fill(&closed));
}

#[test]
fn generated_paint_is_recognised_by_its_shape_not_by_a_marker() {
    // A marker would go stale the moment someone restyles a fill, and a hand-drawn
    // strokeless background polygon is indistinguishable from a generated one anyway.
    let mut paint = ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    paint.background_color = "#ffec99".into();
    paint.stroke_color = "transparent".into();
    assert!(is_bucket_fill_compatible(&paint));

    // Give it a visible stroke and it has been repurposed into an outline: it now bounds
    // new fills rather than being restyled by them.
    paint.stroke_color = "#1e1e1e".into();
    assert!(!is_bucket_fill_compatible(&paint));
}

// -----------------------------------------------------------------------------
// the common case: a click inside a closed shape
// -----------------------------------------------------------------------------

#[test]
fn a_click_inside_a_rectangle_fills_that_rectangle() {
    let elements = vec![stroked_box(100.0, 100.0, 200.0, 120.0)];
    let filled = fill(&elements, at(200.0, 160.0)).expect("a rectangle encloses its middle");

    assert_eq!(filled.owner_id.as_deref(), Some(elements[0].id.as_str()));
    assert_bounds_near(&filled.scene_points, (100.0, 100.0, 300.0, 220.0), 1.0);
    // Closed exactly once: the ring repeats its first point as its last.
    let pts = &filled.scene_points;
    assert_point_close(pts[0], pts[pts.len() - 1]);
}

#[test]
fn a_click_outside_every_shape_fills_nothing() {
    let elements = vec![stroked_box(100.0, 100.0, 50.0, 50.0)];
    // Deliberately `NoOwner` rather than an error worth reporting: clicking empty canvas
    // should not nag.
    assert_eq!(
        fill(&elements, at(500.0, 500.0)),
        Err(BucketFillFailure::NoOwner)
    );
}

#[test]
fn a_click_fills_an_ellipse_to_its_curve_not_to_its_box() {
    let mut ellipse = ellipse_at(0.0, 0.0, 200.0, 200.0);
    ellipse.stroke_color = "#1e1e1e".into();
    let filled = fill(&[ellipse], at(100.0, 100.0)).expect("the centre is inside");

    // The ring is sampled around the curve, so its area is near a circle's rather than
    // the 40000 its bounding box would give.
    let area = polygon_area(&filled.scene_points);
    let circle = std::f64::consts::PI * 100.0 * 100.0;
    assert!(
        (area - circle).abs() / circle < 0.05,
        "expected roughly {circle}, got {area}"
    );
}

#[test]
fn a_shape_with_no_visible_stroke_still_owns_the_click() {
    // Its background is what makes it visible, so it renders a mark and can be an owner —
    // but with no stroke it is not an eligible *boundary*, which is the distinction the
    // owner path has to get right or a solid strokeless shape becomes unfillable.
    let mut shape = box_at(0.0, 0.0, 100.0, 100.0);
    shape.stroke_color = "transparent".into();
    shape.background_color = "#ffec99".into();
    let filled = fill(&[shape], at(50.0, 50.0)).expect("a visible shape encloses its middle");
    assert_bounds_near(&filled.scene_points, (0.0, 0.0, 100.0, 100.0), 1.0);
}

#[test]
fn a_wholly_invisible_shape_cannot_conjure_a_fill() {
    let mut ghost = box_at(0.0, 0.0, 100.0, 100.0);
    ghost.stroke_color = "transparent".into();
    ghost.background_color = "transparent".into();
    assert_eq!(
        fill(&[ghost], at(50.0, 50.0)),
        Err(BucketFillFailure::NoOwner),
        "a click on what looks like empty canvas must stay empty"
    );
}

// -----------------------------------------------------------------------------
// subdivision: the smallest region wins
// -----------------------------------------------------------------------------

#[test]
fn a_line_across_a_rectangle_splits_it_in_two() {
    // The half clicked is what fills — not the whole rectangle. This is the property that
    // separates a real arrangement from "fill the shape you hit".
    let rect = stroked_box(0.0, 0.0, 200.0, 100.0);
    let divider = poly(&[(100.0, -10.0), (100.0, 110.0)]);
    let elements = vec![rect, divider];

    let left = fill(&elements, at(50.0, 50.0)).expect("the left half is enclosed");
    assert_bounds_near(&left.scene_points, (0.0, 0.0, 100.0, 100.0), 1.0);

    let right = fill(&elements, at(150.0, 50.0)).expect("the right half is enclosed");
    assert_bounds_near(&right.scene_points, (100.0, 0.0, 200.0, 100.0), 1.0);
}

#[test]
fn the_region_clicked_is_the_smallest_one_containing_the_click() {
    // A rectangle split into three columns: clicking the middle must give the middle
    // column, not the whole rectangle, and not two columns merged.
    let rect = stroked_box(0.0, 0.0, 300.0, 100.0);
    let a = poly(&[(100.0, -10.0), (100.0, 110.0)]);
    let b = poly(&[(200.0, -10.0), (200.0, 110.0)]);
    let filled = fill(&[rect, a, b], at(150.0, 50.0)).expect("the middle column is enclosed");
    assert_bounds_near(&filled.scene_points, (100.0, 0.0, 200.0, 100.0), 1.0);
}

#[test]
fn two_overlapping_rectangles_fill_only_their_overlap() {
    let left = stroked_box(0.0, 0.0, 100.0, 100.0);
    let right = stroked_box(60.0, 0.0, 100.0, 100.0);
    let filled = fill(&[left, right], at(80.0, 50.0)).expect("the overlap is enclosed");
    assert_bounds_near(&filled.scene_points, (60.0, 0.0, 100.0, 100.0), 1.0);
}

// -----------------------------------------------------------------------------
// open strokes and bridging
// -----------------------------------------------------------------------------

#[test]
fn four_separate_lines_meeting_at_their_ends_enclose_a_region() {
    // No owner here at all — every element is an open line, so this goes through the
    // owner-less fallback and the region exists only in the arrangement.
    let elements = vec![
        poly(&[(0.0, 0.0), (100.0, 0.0)]),
        poly(&[(100.0, 0.0), (100.0, 100.0)]),
        poly(&[(100.0, 100.0), (0.0, 100.0)]),
        poly(&[(0.0, 100.0), (0.0, 0.0)]),
    ];
    let filled = fill(&elements, at(50.0, 50.0)).expect("four lines make a square");
    assert!(
        filled.owner_id.is_none(),
        "open lines own nothing; the region is the arrangement's"
    );
    assert_bounds_near(&filled.scene_points, (0.0, 0.0, 100.0, 100.0), 1.0);
    assert_eq!(
        filled.boundary_element_ids.len(),
        4,
        "all four lines bound it"
    );
}

#[test]
fn a_small_gap_between_strokes_is_bridged() {
    // A sketchy joint: the top edge stops 3px short of the corner, well inside the
    // 6px gap tolerance. It should still read as closed.
    let gap = 3.0;
    let elements = vec![
        poly(&[(0.0, 0.0), (100.0 - gap, 0.0)]),
        poly(&[(100.0, 0.0), (100.0, 100.0)]),
        poly(&[(100.0, 100.0), (0.0, 100.0)]),
        poly(&[(0.0, 100.0), (0.0, 0.0)]),
    ];
    let filled = fill(&elements, at(50.0, 50.0))
        .expect("a 3px gap is under the tolerance and should be bridged");
    assert_bounds_near(&filled.scene_points, (0.0, 0.0, 100.0, 100.0), 4.0);
}

#[test]
fn a_gap_wider_than_the_tolerance_leaves_the_region_open() {
    // The counterpart to the test above, and the more important one: bridging must not be
    // so eager that paint escapes through a gap the person can plainly see.
    let elements = vec![
        poly(&[(0.0, 0.0), (70.0, 0.0)]),
        poly(&[(100.0, 0.0), (100.0, 100.0)]),
        poly(&[(100.0, 100.0), (0.0, 100.0)]),
        poly(&[(0.0, 100.0), (0.0, 0.0)]),
    ];
    assert_eq!(
        fill(&elements, at(50.0, 50.0)),
        Err(BucketFillFailure::NoOwner),
        "a 30px hole in the outline is not a closed region"
    );
}

#[test]
fn bridging_does_not_move_the_strokes_it_bridges() {
    // The reason the bridge radius can afford to be generous: it adds a connector edge
    // rather than relocating a vertex, so the filled shape still hugs the real strokes.
    let elements = vec![
        poly(&[(0.0, 0.0), (100.0, 0.0)]),
        poly(&[(100.0, 0.0), (100.0, 100.0)]),
        poly(&[(100.0, 100.0), (0.0, 100.0)]),
        poly(&[(0.0, 95.0), (0.0, 0.0)]),
    ];
    let filled = fill(&elements, at(50.0, 50.0)).expect("the 5px gap is bridged");
    let (min_x, min_y, max_x, max_y) = ring_bounds(&filled.scene_points);
    assert!(
        min_x >= -0.6 && min_y >= -0.6 && max_x <= 100.6 && max_y <= 100.6,
        "the fill must not spill past the strokes: got {:?}",
        (min_x, min_y, max_x, max_y)
    );
}

#[test]
fn a_freehand_loop_that_renders_closed_fills() {
    // A freedraw whose ends are within the renderer's own closure threshold paints its
    // background, so a fill has to agree that it is closed — using that same rule rather
    // than the gap tolerance, or the two would disagree about the same shape.
    let mut stroke = poly(&[
        (0.0, 0.0),
        (100.0, 0.0),
        (100.0, 100.0),
        (0.0, 100.0),
        (0.0, 4.0),
    ]);
    stroke.kind = DrawElementType::Freedraw;
    let filled = fill(&[stroke], at(50.0, 50.0)).expect("a 4px closure gap renders closed");
    assert_bounds_near(&filled.scene_points, (0.0, 0.0, 100.0, 100.0), 2.0);
}

// -----------------------------------------------------------------------------
// islands
// -----------------------------------------------------------------------------

#[test]
fn a_shape_inside_the_region_becomes_a_hole() {
    // The net area is what matters: a keyhole ring's signed area already excludes its
    // holes, which is exactly what the fill paints.
    let outer = stroked_box(0.0, 0.0, 200.0, 200.0);
    let inner = stroked_box(75.0, 75.0, 50.0, 50.0);
    let filled = fill(&[outer, inner], at(20.0, 20.0)).expect("the ring between them is enclosed");

    let area = polygon_area(&filled.scene_points);
    let expected = 200.0 * 200.0 - 50.0 * 50.0;
    assert!(
        (area - expected).abs() < 400.0,
        "expected about {expected} of net paint, got {area}"
    );
    assert!(
        !polygon_includes_point(at(100.0, 100.0), &filled.scene_points),
        "the island's interior must stay unpainted"
    );
    assert!(
        polygon_includes_point(at(20.0, 20.0), &filled.scene_points),
        "the clicked corner must be painted"
    );
}

#[test]
fn only_the_outermost_island_becomes_a_hole() {
    // An island nested inside another already sits inside a hole; punching it again would
    // flip it back to painted under the even-odd rule.
    let outer = stroked_box(0.0, 0.0, 300.0, 300.0);
    let island = stroked_box(100.0, 100.0, 100.0, 100.0);
    let nested = stroked_box(130.0, 130.0, 40.0, 40.0);
    let filled = fill(&[outer, island, nested], at(20.0, 20.0)).expect("the surround is enclosed");

    assert!(
        !polygon_includes_point(at(110.0, 110.0), &filled.scene_points),
        "inside the outer island, outside the nested one: still a hole"
    );
    assert!(
        !polygon_includes_point(at(150.0, 150.0), &filled.scene_points),
        "the nested island sits inside the hole and must not be punched twice"
    );
}

#[test]
fn clicking_inside_the_island_fills_the_island() {
    let outer = stroked_box(0.0, 0.0, 200.0, 200.0);
    let inner = stroked_box(75.0, 75.0, 50.0, 50.0);
    let filled = fill(&[outer, inner], at(100.0, 100.0)).expect("the island encloses its middle");
    assert_bounds_near(&filled.scene_points, (75.0, 75.0, 125.0, 125.0), 1.0);
}

// -----------------------------------------------------------------------------
// visibility: only what can be seen stops a fill
// -----------------------------------------------------------------------------

#[test]
fn an_outline_buried_under_opaque_paint_does_not_stop_a_fill() {
    // The divider is completely hidden beneath the patch above it, so on screen there is
    // one region, and filling has to agree with what is on screen.
    let rect = stroked_box(0.0, 0.0, 200.0, 100.0);
    let divider = poly(&[(100.0, 10.0), (100.0, 90.0)]);
    let cover = opaque_box(20.0, 5.0, 160.0, 90.0);
    let filled = fill(&[rect, divider, cover], at(50.0, 50.0)).expect("one region, not two");

    assert!(
        polygon_includes_point(at(150.0, 50.0), &filled.scene_points),
        "the buried divider must not split the region: got {:?}",
        ring_bounds(&filled.scene_points)
    );
}

#[test]
fn a_visible_outline_above_the_paint_still_stops_a_fill() {
    // The mirror of the test above. Order decides it: a divider *above* the patch is
    // plainly visible and must still bound.
    let rect = stroked_box(0.0, 0.0, 200.0, 100.0);
    let cover = opaque_box(20.0, 5.0, 160.0, 90.0);
    let divider = poly(&[(100.0, -10.0), (100.0, 110.0)]);
    let filled = fill(&[rect, cover, divider], at(50.0, 50.0)).expect("the left half is enclosed");
    assert!(
        !polygon_includes_point(at(150.0, 50.0), &filled.scene_points),
        "a visible divider must keep the halves apart"
    );
}

// -----------------------------------------------------------------------------
// z-order
// -----------------------------------------------------------------------------

#[test]
fn the_fill_goes_below_the_outline_that_bounds_it() {
    // Otherwise the fill buries the very stroke that defined it, and the shape loses its
    // outline the moment it is filled.
    let rect = stroked_box(0.0, 0.0, 100.0, 100.0);
    let id = rect.id.clone();
    let filled = fill(&[rect], at(50.0, 50.0)).expect("enclosed");
    assert_eq!(filled.insertion.placement, Placement::Below);
    assert_eq!(filled.insertion.element_id, id);
}

#[test]
fn the_fill_goes_above_paint_it_would_otherwise_show_through() {
    // Older opaque paint inside the region would poke through a fill inserted beneath it,
    // and a fill you can see through reads as the tool having failed.
    let rect = stroked_box(0.0, 0.0, 200.0, 200.0);
    let paint = opaque_box(40.0, 40.0, 60.0, 60.0);
    let paint_id = paint.id.clone();
    let filled = fill(&[rect, paint], at(150.0, 150.0)).expect("enclosed");
    assert_eq!(filled.insertion.placement, Placement::Above);
    assert_eq!(filled.insertion.element_id, paint_id);
}

#[test]
fn the_fill_stays_below_a_mark_floating_inside_it() {
    // A label or an icon wholly inside the region is not a participant — nothing crosses
    // the boundary, so it never subdivides a face — and has to be caught by sampling
    // instead, or an opaque fill buries it.
    //
    // The label is placed *below* the rectangle on purpose. The answer is the lowest
    // element that must stay visible, and the rectangle is a participant either way, so
    // only a label beneath it can show whether the sampling found it: without it the
    // answer would be the rectangle.
    let rect = stroked_box(0.0, 0.0, 200.0, 200.0);
    let mut label = text_at(80.0, 90.0, 40.0, 20.0);
    label.stroke_color = "#1e1e1e".into();
    let label_id = label.id.clone();
    let filled = fill(&[label, rect], at(20.0, 20.0)).expect("enclosed");
    assert_eq!(filled.insertion.placement, Placement::Below);
    assert_eq!(
        filled.insertion.element_id, label_id,
        "a mark floating inside the region must keep the fill beneath it"
    );
}

#[test]
fn a_mark_outside_the_region_does_not_hold_the_fill_down() {
    // The counterpart: sampling must not count everything in the scene, or the fill sinks
    // below unrelated elements and the z-order becomes meaningless.
    let rect = stroked_box(0.0, 0.0, 200.0, 200.0);
    let rect_id = rect.id.clone();
    let mut elsewhere = text_at(800.0, 800.0, 40.0, 20.0);
    elsewhere.stroke_color = "#1e1e1e".into();
    let filled = fill(&[elsewhere, rect], at(20.0, 20.0)).expect("enclosed");
    assert_eq!(
        filled.insertion.element_id, rect_id,
        "a label far away from the region is irrelevant to where the fill goes"
    );
}

// -----------------------------------------------------------------------------
// limits and refusals
// -----------------------------------------------------------------------------

#[test]
fn a_region_below_the_minimum_area_is_refused() {
    let tiny = stroked_box(0.0, 0.0, 1.0, 1.0);
    let result = fill(&[tiny], at(0.5, 0.5));
    assert!(
        matches!(
            result,
            Err(BucketFillFailure::TooSmall)
                | Err(BucketFillFailure::OpenRegion)
                | Err(BucketFillFailure::InvalidPolygon)
        ),
        "a 1x1 region must not become an element, got {result:?}"
    );
}

#[test]
fn an_arrow_is_never_a_boundary() {
    // An arrow points at things; it does not enclose them. Excalidraw excludes it for the
    // same reason, and including it would make every annotated diagram unfillable.
    let rect = stroked_box(0.0, 0.0, 200.0, 100.0);
    let mut arrow = connector(100.0, -10.0, 100.0, 110.0, DrawElementType::Arrow);
    arrow.stroke_color = "#1e1e1e".into();
    let filled = fill(&[rect, arrow], at(50.0, 50.0)).expect("enclosed");
    assert_bounds_near(&filled.scene_points, (0.0, 0.0, 200.0, 100.0), 1.0);
}

#[test]
fn text_is_never_a_boundary() {
    let rect = stroked_box(0.0, 0.0, 200.0, 100.0);
    let mut label = text_at(90.0, 10.0, 20.0, 80.0);
    label.stroke_color = "#1e1e1e".into();
    let filled = fill(&[rect, label], at(50.0, 50.0)).expect("enclosed");
    assert_bounds_near(&filled.scene_points, (0.0, 0.0, 200.0, 100.0), 1.0);
}

#[test]
fn an_impossible_arrangement_is_refused_rather_than_hung() {
    // The cap exists so a click can never turn into a multi-second freeze. Asserting the
    // refusal is how we know the cap is actually consulted.
    let mut elements = vec![stroked_box(0.0, 0.0, 400.0, 400.0)];
    for i in 0..400 {
        let t = i as f64;
        elements.push(poly(&[(t, 0.0), (400.0 - t, 400.0)]));
        elements.push(poly(&[(0.0, t), (400.0, 400.0 - t)]));
    }
    let refs: Vec<&DrawElement> = elements.iter().collect();
    let options = BucketFillOptions {
        max_boundary_segments: 64,
        ..Default::default()
    };
    assert_eq!(
        compute_bucket_fill(at(200.0, 200.0), &refs, &options),
        Err(BucketFillFailure::TooComplex)
    );
}

// -----------------------------------------------------------------------------
// determinism
// -----------------------------------------------------------------------------

#[test]
fn the_same_scene_always_gives_the_same_fill() {
    // The arrangement is built out of hash maps, whose iteration order is arbitrary, and
    // bridging is order-dependent — an earlier bridge can close a later loose end. Without
    // deliberate ordering the same click would give different polygons on different runs.
    let elements = vec![
        poly(&[(0.0, 0.0), (100.0, 0.0)]),
        poly(&[(100.0, 0.0), (100.0, 100.0)]),
        poly(&[(100.0, 100.0), (0.0, 100.0)]),
        poly(&[(0.0, 98.0), (0.0, 0.0)]),
    ];
    let first = fill(&elements, at(50.0, 50.0)).expect("enclosed");
    for _ in 0..12 {
        let again = fill(&elements, at(50.0, 50.0)).expect("enclosed");
        assert_eq!(
            first.scene_points, again.scene_points,
            "the same scene must always give the same ring"
        );
        assert_eq!(first.boundary_element_ids, again.boundary_element_ids);
    }
}

#[test]
fn the_order_elements_sit_in_does_not_change_the_region() {
    // Hole detection used to be decided by comparing areas, which degenerates into a coin
    // flip on the outermost outline — and the flip was settled by enumeration order. This
    // is the regression guard for deciding it by the sign of the shoelace area instead.
    let outer = stroked_box(0.0, 0.0, 200.0, 200.0);
    let inner = stroked_box(75.0, 75.0, 50.0, 50.0);

    let forward = fill(&[outer.clone(), inner.clone()], at(20.0, 20.0)).expect("enclosed");
    let reversed = fill(&[inner, outer], at(20.0, 20.0)).expect("enclosed");

    assert!(
        (polygon_area(&forward.scene_points) - polygon_area(&reversed.scene_points)).abs() < 1.0,
        "the same two shapes must give the same net area whichever order they are in"
    );
    assert!(
        !polygon_includes_point(at(100.0, 100.0), &reversed.scene_points),
        "the island is still a hole with the elements the other way round"
    );
}

// -----------------------------------------------------------------------------
// restyling
// -----------------------------------------------------------------------------

#[test]
fn clicking_the_same_region_twice_restyles_rather_than_stacks() {
    let rect = stroked_box(0.0, 0.0, 100.0, 100.0);
    let filled = fill(&[rect], at(50.0, 50.0)).expect("enclosed");

    let mut paint = poly(&[(0.0, 0.0)]);
    paint.points = Some(
        filled
            .scene_points
            .iter()
            .map(|p| {
                [
                    p.x - filled.scene_points[0].x,
                    p.y - filled.scene_points[0].y,
                ]
            })
            .collect(),
    );
    paint.x = filled.scene_points[0].x;
    paint.y = filled.scene_points[0].y;
    paint.background_color = "#ffec99".into();
    paint.stroke_color = "transparent".into();

    assert!(
        is_restylable_fill(&paint, &filled.scene_points),
        "the same region should recolour the paint already there"
    );
}

#[test]
fn a_different_region_is_not_restylable() {
    // Without the area and bounds guard, a click inside a shape drawn *over* a filled
    // region would recolour the fill underneath instead of filling the shape.
    let mut paint = ring(&[(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)]);
    paint.background_color = "#ffec99".into();
    paint.stroke_color = "transparent".into();

    let small = [
        at(10.0, 10.0),
        at(40.0, 10.0),
        at(40.0, 40.0),
        at(10.0, 40.0),
        at(10.0, 10.0),
    ];
    assert!(!is_restylable_fill(&paint, &small));
}

// -----------------------------------------------------------------------------
// through the engine
// -----------------------------------------------------------------------------

#[test]
fn a_click_with_the_bucket_leaves_visible_paint() {
    // The style starts with a transparent background, and taking it literally produced an
    // element that was there, selected and undoable — but invisible. A bucket that paints
    // nothing reads as a tool that silently failed, which is the worst of the failures
    // available to it.
    let mut engine = engine_with_scene(divided_box(200.0, 200.0));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 100.0, false, false);
    engine.end_pointer();

    let painted: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        // By what makes an element paint rather than by its kind: the divider that
        // makes this region a region is a `Line` too.
        .filter(|e| !e.is_deleted && is_transparent(&e.stroke_color))
        .collect();
    assert_eq!(painted.len(), 1, "one fill, not none and not two");
    assert!(
        !is_transparent(&painted[0].background_color),
        "the fill must be visible, got {:?}",
        painted[0].background_color
    );
    assert!(
        is_transparent(&painted[0].stroke_color),
        "the fill must not draw its own outline over the strokes it came from"
    );
}

#[test]
fn the_fill_lands_beneath_the_outline_it_came_from() {
    let mut engine = engine_with_scene(divided_box(200.0, 200.0));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 100.0, false, false);
    engine.end_pointer();

    let order: Vec<DrawElementType> = engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .map(|e| e.kind)
        .collect();
    // Paint first, then the outline that bounds it and the line that halves it — a
    // stroke drawn over its own paint rather than buried by it.
    assert_eq!(
        order,
        vec![
            DrawElementType::Line,
            DrawElementType::Rectangle,
            DrawElementType::Line
        ],
        "the paint goes under the strokes, or filling a shape erases its outline"
    );
}

#[test]
fn a_fill_tells_the_host_about_itself() {
    // The bug that made the bucket useless in a browser: the paint appeared on the canvas
    // and never reached the server, so it was gone on the next load. `push_history` both
    // snapshots *and* publishes what changed, and it was being called before the element
    // was in the scene — so the host was handed a scene with no fill in it, believed it,
    // and saved that. Nothing in the geometry was wrong, which is why 41 green tests said
    // the feature worked.
    let mut engine = engine_with_scene(divided_box(200.0, 200.0));
    let _ = engine.drain_events();

    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 100.0, false, false);
    engine.end_pointer();

    let events = engine.drain_events();
    let published = events
        .scene_delta
        .map(|delta| {
            delta
                .updated
                .iter()
                .any(|e| e.kind == DrawElementType::Line)
        })
        .or_else(|| {
            events.scene_json.as_ref().map(|json| {
                json.contains("\"type\": \"line\"") || json.contains("\"type\":\"line\"")
            })
        });
    assert_eq!(
        published,
        Some(true),
        "the fill was created but never announced, so nothing downstream can save it"
    );
}

#[test]
fn undoing_a_fill_takes_away_the_paint_and_nothing_else() {
    // The same ordering bug seen from the other side. `push_history` snapshots the scene
    // it is called on, so calling it before the insert stored the pre-fill state as the
    // *current* one — and the undo that should have removed the paint went back a step
    // further than the user asked for.
    let mut engine = engine_with_scene(divided_box(200.0, 200.0));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 100.0, false, false);
    engine.end_pointer();
    assert_eq!(live_kinds(&engine).len(), 3);

    engine.undo();

    assert_eq!(
        live_kinds(&engine),
        vec![DrawElementType::Rectangle, DrawElementType::Line],
        "one undo should leave the scene that was there before the fill"
    );
}

/// The kinds still on the board, in z-order.
fn live_kinds(engine: &DrawEngine) -> Vec<DrawElementType> {
    engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .map(|e| e.kind)
        .collect()
}

#[test]
fn a_second_click_in_a_painted_region_restyles_it_through_the_engine() {
    // The engine end of "click twice". `clicking_the_same_region_twice_restyles_rather_
    // than_stacks` builds the paint by hand and asks `is_restylable_fill` about it, which
    // says nothing about what happens when the *engine* runs a second time over a scene
    // that now contains its own output — and that is the path a person takes. It panicked
    // in the browser on the second click, which is as bad as a bug gets: the WASM module
    // aborts and the whole board stops responding until the page is reloaded.
    let mut engine = engine_with_scene(divided_box(200.0, 200.0));
    engine.set_tool(DrawTool::BucketFill);

    engine.begin_pointer(50.0, 100.0, false, false);
    engine.end_pointer();
    assert_eq!(live_kinds(&engine).len(), 3);

    // Deliberately not the same pixel. Nobody clicks twice on the same pixel, and a
    // nearby point is what lands on the fill the first click left behind.
    engine.begin_pointer(60.0, 110.0, false, false);
    engine.end_pointer();

    assert_eq!(
        live_kinds(&engine).len(),
        3,
        "the second click should recolour the paint that is there, not add to it"
    );
}

#[test]
fn the_paint_a_fill_leaves_behind_can_actually_be_drawn() {
    // The gap the browser fell through. Every other test here stops at the scene: it
    // checks that an element exists, where it sits and what colour it is, and the engine
    // tests paint through a `NoopPainter` that never builds a shape. So nothing asked the
    // renderer whether the thing the bucket produces is drawable — and a second click, by
    // restyling the fill, made one that was not. The module aborted and the board froze.
    //
    // `element_drawable` is the renderer's own entry point and is pure, so the question
    // can be asked here rather than only in a browser.
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 200.0)]);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();
    engine.begin_pointer(90.0, 110.0, false, false);
    engine.end_pointer();

    for element in engine.get_scene().into_iter().filter(|e| !e.is_deleted) {
        // `None` is a fine answer — some elements have no drawable. A panic is not.
        let _ = draw_engine::render::shape::element_drawable(&element);
    }
}

// -----------------------------------------------------------------------------
// the element the fill becomes
// -----------------------------------------------------------------------------
//
// Everything above this line stops at `compute_bucket_fill`, which returns a polygon and
// is thoroughly right about it. The step that turns that polygon into a `DrawElement` had
// no test at all, and that is where the tool was broken: the region was computed
// correctly and then committed as an element that claimed to occupy nothing.

/// The fill the engine just made — the one element with no stroke.
fn painted(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|e| !e.is_deleted && is_transparent(&e.stroke_color))
        .expect("the bucket should have left paint behind")
}

#[test]
fn the_fill_declares_the_size_of_the_region_it_covers() {
    let mut engine = engine_with_scene(divided_box(200.0, 120.0));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();

    let paint = painted(&engine);
    let points = paint.points.as_deref().expect("a fill is its points");
    let width = points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max)
        - points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let height = points
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        - points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);

    // Not merely "non-zero": the whole point is that it agrees with the points, because
    // every consumer that cannot see the points — the API's bounds mirror, a thumbnail,
    // another frontend — has only these two numbers to go on.
    assert_close(paint.width, width);
    assert_close(paint.height, height);
    assert!(
        paint.width > 0.0 && paint.height > 0.0,
        "a region that covers a 200x120 box cannot be 0x0, and was: {}x{}",
        paint.width,
        paint.height
    );
}

#[test]
fn the_paint_can_be_picked_up_again() {
    // The failure a person actually meets. The fill appears, and then it is inert: it
    // cannot be clicked, so it cannot be selected, moved, recoloured or deleted. It was
    // hit-testable only along its own edge, which is exactly where the outline it was
    // traced from is already answering for the same click.
    let mut engine = engine_with_scene(divided_box(200.0, 120.0));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();
    let id = painted(&engine).id;

    let hit = engine.hit_test(50.0, 60.0, 10.0);
    assert_eq!(
        hit.map(|e| e.id),
        Some(id.clone()),
        "a click in the middle of the paint must find the paint"
    );

    // And the whole way through the tool that a person would use to grab it.
    engine.set_tool(DrawTool::Select);
    engine.clear_selection();
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();
    assert_eq!(
        engine.get_selection(),
        vec![id],
        "selecting it with the select tool must select it"
    );
}

#[test]
fn the_paint_moves_when_it_is_dragged() {
    let mut engine = engine_with_scene(divided_box(200.0, 120.0));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();
    let before = painted(&engine);

    engine.set_tool(DrawTool::Select);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.move_pointer(90.0, 90.0, false, false);
    engine.end_pointer();

    let after = painted(&engine);
    assert_close(after.x, before.x + 40.0);
    assert_close(after.y, before.y + 30.0);
}

// -----------------------------------------------------------------------------
// telling the person why nothing happened
// -----------------------------------------------------------------------------
//
// `BucketFillFailure` has had five variants and its own note that "an open region is
// worth a word to the person clicking" since the feature landed, and the word was never
// said: the pointer path discarded the `Result`, so a click that found no region did
// nothing at all — no element, no cursor change, no message. For someone who does not
// already know a region must be fully enclosed by *visible* strokes, that is
// indistinguishable from a tool that is broken.
//
// The engine emits a code, never a sentence. Which words to use, and in which language,
// is the host's half of the problem.

/// The notice a bucket click leaves behind, if any.
fn notice_of(engine: &mut DrawEngine) -> Option<Notice> {
    engine.drain_events().notice
}

#[test]
fn aiming_at_a_shape_and_getting_nothing_says_so() {
    // A shape *is* under the pointer — the person plainly aimed at something — and it
    // still refuses, here because the region is too small to become paint. That is the
    // case where silence reads as a broken tool, so it gets a word.
    //
    // A 1x1 box rather than a broken outline because the three refusals that reach this
    // branch are one message to the person clicking; which stage refused is a fact about
    // the algorithm, and `a_region_below_the_minimum_area_is_refused` is where that
    // distinction is pinned.
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 1.0, 1.0)]);
    engine.set_tool(DrawTool::BucketFill);
    let _ = notice_of(&mut engine);

    engine.begin_pointer(0.5, 0.5, false, false);
    engine.end_pointer();

    assert_eq!(live_kinds(&engine).len(), 1, "nothing was painted");
    assert_eq!(notice_of(&mut engine), Some(Notice::FillRegionNotClosed));
}

/// The owner-less half of the same refusal, and it stays quiet.
///
/// Three sides of a box enclose nothing and own nothing, so the fallback search comes
/// back empty — which is `NoOwner`, the same answer as a click on bare canvas.
/// Excalidraw draws the line in the same place (`App.bucketFill.ts@1118751f:197`): with no owner
/// there is no evidence the person was aiming at anything in particular.
#[test]
fn an_open_shape_that_owns_nothing_stays_quiet() {
    let mut engine = engine_with_scene(vec![
        poly(&[(0.0, 0.0), (200.0, 0.0)]),
        poly(&[(200.0, 0.0), (200.0, 200.0)]),
        poly(&[(200.0, 200.0), (0.0, 200.0)]),
    ]);
    engine.set_tool(DrawTool::BucketFill);
    let _ = notice_of(&mut engine);

    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();

    assert_eq!(live_kinds(&engine).len(), 3, "nothing was painted");
    assert_eq!(notice_of(&mut engine), None);
}

#[test]
fn a_click_on_bare_canvas_says_nothing() {
    // Excalidraw is deliberately silent here too (`App.bucketFill.ts@1118751f:197`). Clicking
    // empty space is not a mistake worth interrupting someone over, and a tool that
    // complains every time the pointer slips is one people stop reading.
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 100.0, 100.0)]);
    engine.set_tool(DrawTool::BucketFill);
    let _ = notice_of(&mut engine);

    engine.begin_pointer(900.0, 900.0, false, false);
    engine.end_pointer();

    assert_eq!(notice_of(&mut engine), None);
}

#[test]
fn a_fill_that_works_says_nothing() {
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 120.0)]);
    engine.set_tool(DrawTool::BucketFill);
    let _ = notice_of(&mut engine);

    engine.begin_pointer(100.0, 60.0, false, false);
    engine.end_pointer();

    assert_eq!(notice_of(&mut engine), None, "success is not news");
}

// -----------------------------------------------------------------------------
// what the paint belongs to
// -----------------------------------------------------------------------------
//
// A fill is a separate element and nothing links it back to the shape it was traced
// from — that is the design, here and upstream, because a region often is not a shape at
// all: the overlap of two rectangles, an area walled in by four loose lines, a ring with
// a hole punched through it. None of those can be written as some element's background.
//
// But there are two things a fill does inherit, and they are the only places the paint
// travels with what it was painted inside: the frame it sits in, and the group its owner
// belongs to. Group a shape with its paint's owner and the paint moves with the group;
// put the owner in a frame and dragging the frame takes the paint along. Excalidraw
// inherits exactly these two and nothing else (`App.bucketFill.ts@1118751f:227-243`).
//
// Without them a fill inside a frame is left behind the moment the frame moves, which
// looks like the paint coming unstuck from the drawing.

/// The owner of a region, grouped with something else so the group is real.
fn grouped(mut element: DrawElement, group: &str) -> DrawElement {
    element.group_ids = vec![group.to_string()];
    element
}

#[test]
fn the_paint_joins_the_group_its_owner_belongs_to() {
    let owner = grouped(stroked_box(0.0, 0.0, 200.0, 120.0), "g1");
    let mut engine = engine_with_scene(vec![owner, poly(&[(100.0, 0.0), (100.0, 120.0)])]);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();

    assert_eq!(
        painted(&engine).group_ids,
        vec!["g1".to_string()],
        "paint that belongs to a group moves when the group moves"
    );
}

#[test]
fn the_paint_joins_the_frame_its_owner_sits_in() {
    let mut owner = stroked_box(0.0, 0.0, 200.0, 120.0);
    owner.frame_id = Some("f1".into());
    let mut engine = engine_with_scene(vec![owner, poly(&[(100.0, 0.0), (100.0, 120.0)])]);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();

    assert_eq!(painted(&engine).frame_id.as_deref(), Some("f1"));
}

/// Filling the frame itself puts the paint *inside* it, not beside it.
#[test]
fn filling_a_frame_puts_the_paint_within_it() {
    let mut frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        },
    );
    frame.id = "frame-1".into();
    let mut engine = engine_with_scene(vec![frame]);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(150.0, 100.0, false, false);
    engine.end_pointer();

    assert_eq!(
        painted(&engine).frame_id.as_deref(),
        Some("frame-1"),
        "a frame is a container, so paint inside it is its child rather than its sibling"
    );
}

#[test]
fn an_ownerless_region_joins_the_group_all_its_walls_share() {
    // Four loose lines enclosing a square, every one of them in the same group. There is
    // no owner, so the group has to come from the walls — and only when they agree.
    let walls = vec![
        grouped(poly(&[(0.0, 0.0), (200.0, 0.0)]), "g2"),
        grouped(poly(&[(200.0, 0.0), (200.0, 200.0)]), "g2"),
        grouped(poly(&[(200.0, 200.0), (0.0, 200.0)]), "g2"),
        grouped(poly(&[(0.0, 200.0), (0.0, 0.0)]), "g2"),
    ];
    let mut engine = engine_with_scene(walls);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();

    assert_eq!(painted(&engine).group_ids, vec!["g2".to_string()]);
}

#[test]
fn walls_that_disagree_give_the_paint_no_group() {
    // One wall out of four belongs somewhere else, so there is no group the region as a
    // whole sits in. Guessing one would drag a stranger's shape along with the paint.
    let walls = vec![
        grouped(poly(&[(0.0, 0.0), (200.0, 0.0)]), "g2"),
        grouped(poly(&[(200.0, 0.0), (200.0, 200.0)]), "g2"),
        grouped(poly(&[(200.0, 200.0), (0.0, 200.0)]), "g2"),
        grouped(poly(&[(0.0, 200.0), (0.0, 0.0)]), "other"),
    ];
    let mut engine = engine_with_scene(walls);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();

    assert!(painted(&engine).group_ids.is_empty());
}

#[test]
fn a_plain_region_belongs_to_nothing() {
    let mut engine = engine_with_scene(divided_box(200.0, 120.0));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();

    let paint = painted(&engine);
    assert!(paint.group_ids.is_empty());
    assert_eq!(paint.frame_id, None);
}

// -----------------------------------------------------------------------------
// filling a shape colours the shape
// -----------------------------------------------------------------------------
//
// A deliberate divergence from Excalidraw, taken on the user's instruction after being
// reported three times. Upstream the bucket *always* inserts a polygon, even for a plain
// rectangle: `isBucketFillCompatible` rejects a rectangle on its first clause, so a
// rectangle can never be restyled by the tool. The paint is then an independent element
// with no back-reference, and moving the rectangle strands it — visibly, as a coloured
// block sitting where the shape used to be, with square corners where the shape's were
// round.
//
// That is defensible for a region that is not a shape, and indefensible for one that is.
// So: when the region under the click is exactly some shape's own inside — nothing else
// walls it, and that shape can carry a background — the bucket writes that shape's
// `backgroundColor`, which is what the person meant. It then moves, resizes, rotates and
// rounds with the shape because it *is* the shape.
//
// Everything a background cannot express still becomes a polygon, and that is most of
// what the tool is for: a sub-region cut by a line, the overlap of two shapes, a ring
// with a hole in it, a region walled in by loose strokes.

#[test]
fn filling_a_plain_shape_colours_the_shape_itself() {
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 120.0)]);
    engine.set_next_style(style_background("#ffc9c9"));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 60.0, false, false);
    engine.end_pointer();

    let live: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .collect();
    assert_eq!(live.len(), 1, "no second element: the shape *is* the paint");
    assert_eq!(live[0].kind, DrawElementType::Rectangle);
    assert_eq!(live[0].background_color, "#ffc9c9");
}

#[test]
fn a_coloured_shape_carries_its_colour_wherever_it_goes() {
    // The whole point of the divergence, and the thing the polygon could never do.
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 120.0)]);
    engine.set_next_style(style_background("#ffc9c9"));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 60.0, false, false);
    engine.end_pointer();

    engine.set_tool(DrawTool::Select);
    engine.begin_pointer(100.0, 0.0, false, false); // the top edge
    engine.move_pointer(180.0, 50.0, false, false);
    engine.end_pointer();

    let moved = engine
        .get_scene()
        .into_iter()
        .find(|e| !e.is_deleted)
        .unwrap();
    assert_close(moved.x, 80.0);
    assert_close(moved.y, 50.0);
    assert_eq!(moved.background_color, "#ffc9c9", "the colour came with it");
}

#[test]
fn clicking_a_coloured_shape_again_changes_its_colour() {
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 120.0)]);
    engine.set_tool(DrawTool::BucketFill);
    engine.set_next_style(style_background("#ffc9c9"));
    engine.begin_pointer(100.0, 60.0, false, false);
    engine.end_pointer();

    engine.set_next_style(style_background("#a5d8ff"));
    engine.begin_pointer(90.0, 70.0, false, false);
    engine.end_pointer();

    let live: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .collect();
    assert_eq!(live.len(), 1, "still one element, not a pile");
    assert_eq!(live[0].background_color, "#a5d8ff");
}

/// Everything a background cannot express still becomes paint.
#[test]
fn a_region_a_background_cannot_express_is_still_a_polygon() {
    // Half a rectangle, cut by a line. No shape's background is that region.
    let divided = vec![
        stroked_box(0.0, 0.0, 200.0, 120.0),
        poly(&[(100.0, 0.0), (100.0, 120.0)]),
    ];
    let mut engine = engine_with_scene(divided);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(50.0, 60.0, false, false);
    engine.end_pointer();

    assert_eq!(live_kinds(&engine).len(), 3, "a third element: the paint");
    let paint = painted(&engine);
    assert!(paint.width < 150.0, "and it is the half, not the whole");
}

#[test]
fn an_overlap_is_still_a_polygon() {
    let mut engine = engine_with_scene(vec![
        stroked_box(0.0, 0.0, 200.0, 120.0),
        stroked_box(100.0, 60.0, 200.0, 120.0),
    ]);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(150.0, 90.0, false, false);
    engine.end_pointer();
    assert_eq!(live_kinds(&engine).len(), 3);
}

#[test]
fn a_region_with_a_hole_in_it_is_still_a_polygon() {
    // An annulus. A background fills a shape solid; it has no way to leave a hole.
    let mut engine = engine_with_scene(vec![
        stroked_box(0.0, 0.0, 300.0, 300.0),
        stroked_box(100.0, 100.0, 100.0, 100.0),
    ]);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(30.0, 30.0, false, false);
    engine.end_pointer();
    assert_eq!(live_kinds(&engine).len(), 3);
}

#[test]
fn a_region_held_up_by_loose_strokes_is_still_a_polygon() {
    let walls = vec![
        poly(&[(0.0, 0.0), (200.0, 0.0)]),
        poly(&[(200.0, 0.0), (200.0, 200.0)]),
        poly(&[(200.0, 200.0), (0.0, 200.0)]),
        poly(&[(0.0, 200.0), (0.0, 0.0)]),
    ];
    let mut engine = engine_with_scene(walls);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 100.0, false, false);
    engine.end_pointer();
    assert_eq!(live_kinds(&engine).len(), 5, "four walls and the paint");
}

/// A frame is a container, not a shape with a background, so filling one paints.
#[test]
fn a_shape_that_cannot_carry_a_background_is_still_painted() {
    let mut frame = create_element_default(
        DrawElementType::Frame,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        },
    );
    frame.id = "frame-1".into();
    let mut engine = engine_with_scene(vec![frame]);
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(150.0, 100.0, false, false);
    engine.end_pointer();

    assert_eq!(live_kinds(&engine).len(), 2, "the frame keeps its own look");
}

#[test]
fn undoing_a_shape_fill_puts_the_colour_back() {
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 120.0)]);
    engine.set_next_style(style_background("#ffc9c9"));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 60.0, false, false);
    engine.end_pointer();
    engine.undo();

    let live: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|e| !e.is_deleted)
        .collect();
    assert_eq!(live.len(), 1);
    assert!(
        is_transparent(&live[0].background_color),
        "undo returns the shape to having no colour at all"
    );
}

#[test]
fn a_bucket_lays_down_solid_paint_unless_told_otherwise() {
    // The default style is hachure, which on a bucket click gives a shape crosshatched
    // in faint lines with white between them — the tool looking like it half worked. A
    // bucket means paint. Found by looking at the screen rather than at the scene: the
    // element had the colour on it and a pixel in the middle of it was white.
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 120.0)]);
    engine.set_next_style(style_background("#ffc9c9"));
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 60.0, false, false);
    engine.end_pointer();

    let shape = engine
        .get_scene()
        .into_iter()
        .find(|e| !e.is_deleted)
        .unwrap();
    assert_eq!(shape.fill_style, FillStyle::Solid);
}

#[test]
fn a_fill_style_that_was_actually_chosen_is_honoured() {
    // The other half: the Fill style row is offered while the bucket is active precisely
    // so it can be used, and a control that silently does nothing is worse than no
    // control. The raw patch is what tells the two cases apart — `get_next_style` merges
    // over the defaults and loses the difference.
    let mut engine = engine_with_scene(vec![stroked_box(0.0, 0.0, 200.0, 120.0)]);
    engine.set_next_style(DrawElementStylePatch {
        background_color: Some("#ffc9c9".into()),
        fill_style: Some(FillStyle::CrossHatch),
        ..Default::default()
    });
    engine.set_tool(DrawTool::BucketFill);
    engine.begin_pointer(100.0, 60.0, false, false);
    engine.end_pointer();

    let shape = engine
        .get_scene()
        .into_iter()
        .find(|e| !e.is_deleted)
        .unwrap();
    assert_eq!(shape.fill_style, FillStyle::CrossHatch);
}
