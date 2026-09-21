//! An arrow bound at both ends must never be drawn through the shapes it connects.
//!
//! Each end is placed on its shape's outline, a gap clear of it. That is right while the
//! shapes are apart. Once they close on each other the two points cross over: the tail
//! ends up on the far side of the head, so the arrow runs **backwards** and is drawn
//! almost entirely inside both shapes.
//!
//! Measured on excalidraw.com with one rectangle slid onto another, their arrow went 228
//! long, then 128, 48, 0, and stayed 0 — never once entering either shape. These check
//! the same property over a far wider sweep than a hand-drag can cover.

mod common;
use common::*;
use draw_engine::*;

fn shape(id: &str, x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    let mut el = filled(box_at(x, y, w, h));
    el.id = id.into();
    el
}

fn ellipse(id: &str, x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    let mut el = filled(ellipse_at(x, y, w, h));
    el.id = id.into();
    el
}

fn bound_arrow(start: Option<&str>, end: Option<&str>) -> DrawElement {
    let mut el = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 0.0,
        },
    );
    el.id = "arrow".into();
    el.points = Some(vec![[0.0, 0.0], [1.0, 0.0]]);
    el.start_binding = start.map(str::to_string);
    el.end_binding = end.map(str::to_string);
    el
}

/// The resolved arrow, as world-space endpoints.
fn settle(scene: Vec<DrawElement>) -> (Point, Point) {
    let out = refresh_bindings(&scene);
    let a = out.iter().find(|e| e.id == "arrow").unwrap();
    let points = a.points.as_ref().unwrap();
    (
        Point {
            x: a.x + points[0][0],
            y: a.y + points[0][1],
        },
        Point {
            x: a.x + points.last().unwrap()[0],
            y: a.y + points.last().unwrap()[1],
        },
    )
}

/// How much of the segment lies strictly inside the element, in world units.
fn depth_inside(p0: Point, p1: Point, el: &DrawElement) -> f64 {
    let b = element_bounds(el);
    const N: usize = 2000;
    let mut inside = 0usize;
    for i in 0..=N {
        let t = i as f64 / N as f64;
        let x = p0.x + (p1.x - p0.x) * t;
        let y = p0.y + (p1.y - p0.y) * t;
        // A margin, so merely touching the outline does not count as being inside.
        if x > b.min_x + 1.0 && x < b.max_x - 1.0 && y > b.min_y + 1.0 && y < b.max_y - 1.0 {
            inside += 1;
        }
    }
    (p1.x - p0.x).hypot(p1.y - p0.y) * inside as f64 / N as f64
}

fn assert_outside(label: &str, scene: Vec<DrawElement>) {
    let a = scene.iter().find(|e| e.id == "A").cloned();
    let b = scene.iter().find(|e| e.id == "B").cloned();
    let (p0, p1) = settle(scene);
    for el in [a, b].into_iter().flatten() {
        let depth = depth_inside(p0, p1, &el);
        assert!(
            depth < 2.0,
            "{label}: the arrow runs {depth:.0} units inside {}; \
             tail ({:.0}, {:.0}) tip ({:.0}, {:.0})",
            el.id,
            p0.x,
            p0.y,
            p1.x,
            p1.y
        );
    }
}

/// The whole point, swept: slide B across A from far right to far left, and at no
/// position may the arrow be drawn inside either shape.
#[test]
fn an_arrow_never_runs_inside_the_shapes_it_joins() {
    let mut x = 800.0;
    while x >= -200.0 {
        assert_outside(
            &format!("B at x={x}"),
            vec![
                shape("A", 300.0, 300.0, 160.0, 120.0),
                shape("B", x, 300.0, 160.0, 120.0),
                bound_arrow(Some("A"), Some("B")),
            ],
        );
        x -= 10.0;
    }
}

/// The same sweep vertically.
#[test]
fn nor_when_one_slides_over_the_other_vertically() {
    let mut y = 700.0;
    while y >= -100.0 {
        assert_outside(
            &format!("B at y={y}"),
            vec![
                shape("A", 300.0, 300.0, 160.0, 120.0),
                shape("B", 300.0, y, 160.0, 120.0),
                bound_arrow(Some("A"), Some("B")),
            ],
        );
        y -= 10.0;
    }
}

/// And diagonally, which is where a centre-to-centre aim is least like an axis.
#[test]
fn nor_on_a_diagonal_approach() {
    let mut t = 400.0;
    while t >= -200.0 {
        assert_outside(
            &format!("B at ({t}, {t})"),
            vec![
                shape("A", 300.0, 300.0, 160.0, 120.0),
                shape("B", 300.0 + t, 300.0 + t, 160.0, 120.0),
                bound_arrow(Some("A"), Some("B")),
            ],
        );
        t -= 10.0;
    }
}

/// Curved outlines take a different branch of `attach_point`, so they get their own
/// sweep.
#[test]
fn nor_between_ellipses() {
    let mut x = 800.0;
    while x >= -200.0 {
        assert_outside(
            &format!("B at x={x}"),
            vec![
                ellipse("A", 300.0, 300.0, 160.0, 120.0),
                ellipse("B", x, 300.0, 160.0, 120.0),
                bound_arrow(Some("A"), Some("B")),
            ],
        );
        x -= 10.0;
    }
}

/// One shape entirely swallowing the other is the extreme case.
#[test]
fn nor_when_one_shape_contains_the_other() {
    for inset in [10.0, 30.0, 55.0] {
        assert_outside(
            &format!("inset {inset}"),
            vec![
                shape("A", 300.0, 300.0, 300.0, 240.0),
                shape(
                    "B",
                    300.0 + inset,
                    300.0 + inset,
                    300.0 - inset * 2.0,
                    240.0 - inset * 2.0,
                ),
                bound_arrow(Some("A"), Some("B")),
            ],
        );
    }
}

/// Exactly coincident shapes: degenerate, and must still not draw anything through them.
#[test]
fn nor_when_the_shapes_are_exactly_coincident() {
    assert_outside(
        "coincident",
        vec![
            shape("A", 300.0, 300.0, 160.0, 120.0),
            shape("B", 300.0, 300.0, 160.0, 120.0),
            bound_arrow(Some("A"), Some("B")),
        ],
    );
}

/// While the shapes are apart, the arrow really does span the gap — the clamp must not
/// fire early and swallow a perfectly good arrow.
#[test]
fn a_clear_gap_still_gets_a_full_arrow() {
    let (p0, p1) = settle(vec![
        shape("A", 300.0, 300.0, 160.0, 120.0),
        shape("B", 700.0, 300.0, 160.0, 120.0),
        bound_arrow(Some("A"), Some("B")),
    ]);
    // A's right edge is 460, B's left edge is 700, each a gap clear.
    assert_close(p0.x, 460.0 + BINDING_GAP);
    assert_close(p1.x, 700.0 - BINDING_GAP);
    assert_close(p0.y, 360.0);
    assert_close(p1.y, 360.0);
}

/// The attachment orbits the outline as the shapes move around each other. This is the
/// part that already worked, and it must keep working.
#[test]
fn the_attachment_moves_round_the_outline() {
    let right = settle(vec![
        shape("A", 300.0, 300.0, 160.0, 120.0),
        shape("B", 700.0, 300.0, 160.0, 120.0),
        bound_arrow(Some("A"), Some("B")),
    ]);
    assert_close(right.0.x, 466.0);
    assert_close(right.0.y, 360.0);

    let below = settle(vec![
        shape("A", 300.0, 300.0, 160.0, 120.0),
        shape("B", 300.0, 700.0, 160.0, 120.0),
        bound_arrow(Some("A"), Some("B")),
    ]);
    assert_close(below.0.x, 380.0);
    assert_close(below.0.y, 420.0 + BINDING_GAP);

    let left = settle(vec![
        shape("A", 300.0, 300.0, 160.0, 120.0),
        shape("B", -100.0, 300.0, 160.0, 120.0),
        bound_arrow(Some("A"), Some("B")),
    ]);
    assert_close(left.0.x, 300.0 - BINDING_GAP);
    assert_close(left.0.y, 360.0);

    let above = settle(vec![
        shape("A", 300.0, 300.0, 160.0, 120.0),
        shape("B", 300.0, -100.0, 160.0, 120.0),
        bound_arrow(Some("A"), Some("B")),
    ]);
    assert_close(above.0.x, 380.0);
    assert_close(above.0.y, 300.0 - BINDING_GAP);
}

/// A free end is wherever the user left it, including inside a shape. Only a pair of
/// bound ends can cross, so the clamp must not touch a half-bound arrow.
#[test]
fn a_free_end_is_left_alone() {
    let mut arrow = bound_arrow(Some("A"), None);
    // The free end sits well to the right.
    arrow.x = 600.0;
    arrow.y = 360.0;
    arrow.points = Some(vec![[0.0, 0.0], [200.0, 0.0]]);

    let out = refresh_bindings(&[shape("A", 300.0, 300.0, 160.0, 120.0), arrow]);
    let a = out.iter().find(|e| e.id == "arrow").unwrap();
    let points = a.points.as_ref().unwrap();
    let tip = Point {
        x: a.x + points[1][0],
        y: a.y + points[1][1],
    };
    assert_close(tip.x, 800.0);
    assert_close(tip.y, 360.0);
}

/// Shrinking the gap between two shapes must shorten the arrow monotonically, with no
/// jump as it closes. A reversal here is the visible "rebound".
#[test]
fn the_arrow_shortens_smoothly_as_the_shapes_close() {
    let mut previous = f64::INFINITY;
    let mut x = 800.0;
    while x >= 460.0 {
        let (p0, p1) = settle(vec![
            shape("A", 300.0, 300.0, 160.0, 120.0),
            shape("B", x, 300.0, 160.0, 120.0),
            bound_arrow(Some("A"), Some("B")),
        ]);
        let length = (p1.x - p0.x).hypot(p1.y - p0.y);
        assert!(
            length <= previous + 1e-6,
            "the arrow grew as the shapes closed: {length} after {previous} at x={x}"
        );
        previous = length;
        x -= 10.0;
    }
    assert!(previous < 2.0, "and ends collapsed, not reversed");
}
