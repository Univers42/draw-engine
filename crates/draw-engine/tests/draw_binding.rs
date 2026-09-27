use draw_engine::*;

fn box_at(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Rectangle,
        Geometry {
            x,
            y,
            width: w,
            height: h,
        },
    )
}

fn connector(x1: f64, y1: f64, x2: f64, y2: f64, kind: DrawElementType) -> DrawElement {
    let mut el = create_element_default(
        kind,
        Geometry {
            x: x1,
            y: y1,
            width: x2 - x1,
            height: y2 - y1,
        },
    );
    el.points = Some(vec![[0.0, 0.0], [x2 - x1, y2 - y1]]);
    el
}

#[test]
fn linear_tools_and_bindable_shapes() {
    assert!(is_linear_tool(DrawTool::Arrow));
    assert!(is_linear_tool(DrawTool::Line));
    assert!(!is_linear_tool(DrawTool::Rectangle));
    assert!(is_linear_element(&connector(
        0.0,
        0.0,
        10.0,
        10.0,
        DrawElementType::Arrow
    )));
    assert!(is_bindable_element(&box_at(0.0, 0.0, 100.0, 60.0)));
    assert!(!is_bindable_element(&connector(
        0.0,
        0.0,
        10.0,
        10.0,
        DrawElementType::Arrow
    )));
}

#[test]
fn linear_from_drag_origin() {
    let drag = linear_from_drag(20.0, 30.0, 60.0, 90.0, false);
    assert_eq!((drag.x, drag.y), (20.0, 30.0));
    assert_eq!(drag.points, vec![[0.0, 0.0], [40.0, 60.0]]);
    assert!(!is_degenerate_linear(drag.width, drag.height, 4.0));
    let click = linear_from_drag(20.0, 30.0, 21.0, 30.0, false);
    assert!(is_degenerate_linear(click.width, click.height, 4.0));
}

#[test]
fn shift_snaps_to_45() {
    let (dx, dy) = constrain_to_angle(100.0, 17.0);
    assert_eq!(dy.round(), 0.0);
    assert_eq!(dx.round(), (100.0_f64.hypot(17.0)).round());
    let (dx, dy) = constrain_to_angle(100.0, 119.0);
    assert_eq!(dx.round(), dy.round());
    let snapped = linear_from_drag(0.0, 0.0, 100.0, 17.0, true);
    assert_eq!(snapped.height.round(), 0.0);
}

/// `design.md:828`, "Vertical" — the third of the three angle-snapping rows
/// (`design.md:825-832` lists Horizontal, Vertical, 45°, Configurable increments), and the
/// one `shift_snaps_to_45` above never fed: it covers the 0° and the 45° case and stops
/// there.
///
/// The claim is that a drag that is *nearly* vertical comes out vertical, with the pointer
/// still as far from the start as it was — a snap re-aims the line, it does not shorten it.
/// Asserted on the offset rather than on a rounded pair, because a rounded pair would also
/// pass for a drag that was already vertical.
///
/// The tolerance is not sloppiness: the snap lands on `cos(90°)`, and IEEE-754's `cos(π/2)`
/// is 6.1e-17 rather than 0, so the x offset comes out around 6e-15 on a drag this long.
/// Exactly vertical is not a representable answer; "vertical to a rounding error" is.
#[test]
fn shift_snaps_a_near_vertical_drag_to_exactly_vertical() {
    // 10 across and 100 down: 84.3°, which is inside the 22.5° either side of 90° that
    // rounds to vertical, and nowhere near horizontal.
    let dragged = 10.0_f64.hypot(100.0);
    let (dx, dy) = constrain_to_angle(10.0, 100.0);
    assert!(dx.abs() < 1e-9, "the x offset is {dx}, not vertical");
    assert!(
        (dy - dragged).abs() < 1e-9,
        "at the drag's own length {dragged}, not {dy}"
    );

    // And through the drag itself, so this is the path a person takes rather than the
    // helper alone: the line's second point sits straight below the first.
    let snapped = linear_from_drag(200.0, 300.0, 210.0, 400.0, true);
    assert_eq!(snapped.x, 200.0, "the start does not move");
    assert_eq!(snapped.y, 300.0);
    assert_eq!(snapped.points[0], [0.0, 0.0]);
    assert!(snapped.width.abs() < 1e-9, "no run at all to the side");
    assert!((snapped.height - dragged).abs() < 1e-9);
}

/// The mirror of the case above, for the same reason: `design.md:826` (Horizontal) is
/// covered by the existing test's `dy.round() == 0.0`, which is a rounded comparison and
/// would pass for a drag that was already horizontal.
#[test]
fn shift_snaps_a_near_horizontal_drag_to_exactly_horizontal() {
    let dragged = 100.0_f64.hypot(10.0);
    let (dx, dy) = constrain_to_angle(100.0, 10.0);
    assert!(dy.abs() < 1e-9, "the y offset is {dy}, not level");
    assert!((dx - dragged).abs() < 1e-9);
}

/// The step is 45°, so the two axes and the two diagonals are the only outcomes, and a
/// drag between two of them goes to the nearer one. The boundaries are halfway: 22.5°
/// between level and 45°, 67.5° between 45° and vertical. Each case below is a degree or
/// two either side of one, so this fails if the snapping ever becomes 90°-only (a different
/// feature) or 15° (the oracle's — see below).
///
/// **Deliberate divergence from the oracle, and an open question — not endorsed here.**
/// The oracle's Shift-constrain for a linear drag is `getPerfectElementSize`
/// (`packages/element/src/sizeHelpers.ts@1118751f:155-185`), and its step is
/// `SHIFT_LOCKING_ANGLE = Math.PI / 12` — **15°**, not 45°
/// (`packages/common/src/constants.ts@1118751f:31`). It also keeps the drag's *width* and
/// recomputes the height (`sizeHelpers.ts@1118751f:179`), where `constrain_to_angle` keeps
/// the drag's *length* and re-aims it. So a 30° drag is exactly 45° here and 30° in the
/// oracle, and the two rules disagree about which extent survives a snap. Nothing in
/// `docs/reference/` or the registry records the 45° step as a chosen divergence, so this
/// test pins what the engine does today and says nothing about it being right: see the
/// Phase 3.4a report.
#[test]
fn the_snap_step_is_45_degrees_so_a_drag_lands_on_an_axis_or_a_diagonal() {
    // A drag of `deg`, always 100 across so the numbers stay readable.
    let at = |deg: f64| {
        let dy = 100.0 * deg.to_radians().tan();
        constrain_to_angle(100.0, dy)
    };

    let (dx, dy) = at(20.0);
    assert!(
        dy.abs() < 1e-9,
        "20° is 20° from level and 25° from 45°: it rounds down"
    );
    assert!((dx - 100.0_f64.hypot(100.0 * 20.0_f64.to_radians().tan())).abs() < 1e-9);

    for deg in [30.0, 67.0] {
        let (dx, dy) = at(deg);
        assert!(
            (dx - dy).abs() < 1e-9,
            "{deg}° is past the halfway mark, so it rounds to the 45° diagonal"
        );
    }

    let (dx, dy) = at(68.0);
    assert!(
        dx.abs() < 1e-9,
        "68° is past 67.5°: it rounds up to vertical"
    );
    assert!(dy > 0.0, "and upwards, not downwards");

    // A zero-length drag has no direction to snap, and must not invent one.
    assert_eq!(constrain_to_angle(0.0, 0.0), (0.0, 0.0));
}

#[test]
fn attach_point_on_outline() {
    let shape = box_at(0.0, 0.0, 100.0, 60.0);
    let right = attach_point(&shape, Point { x: 500.0, y: 30.0 }, BINDING_GAP);
    assert_eq!(right.x.round(), 100.0 + BINDING_GAP);
    assert_eq!(right.y.round(), 30.0);
    let up = attach_point(&shape, Point { x: 50.0, y: -500.0 }, BINDING_GAP);
    assert_eq!(up.y.round(), -BINDING_GAP);
    let mut ellipse = shape.clone();
    ellipse.kind = DrawElementType::Ellipse;
    let mut diamond = shape;
    diamond.kind = DrawElementType::Diamond;
    let dir = Point { x: 500.0, y: 500.0 };
    assert!(
        attach_point(&diamond, dir, BINDING_GAP).x < attach_point(&ellipse, dir, BINDING_GAP).x
    );
}

#[test]
fn bindable_at_topmost() {
    let under = box_at(0.0, 0.0, 100.0, 60.0);
    let over = box_at(20.0, 10.0, 100.0, 60.0);
    let arrow = connector(0.0, 0.0, 200.0, 200.0, DrawElementType::Arrow);
    let scene = vec![under.clone(), over.clone(), arrow];
    assert_eq!(
        bindable_at(&scene, 40.0, 30.0, 0.0, None).map(|e| e.id.clone()),
        Some(over.id.clone())
    );
    assert_eq!(
        bindable_at(&scene, 40.0, 30.0, 0.0, Some(&over.id)).map(|e| e.id.clone()),
        Some(under.id)
    );
    assert!(bindable_at(&scene, 900.0, 900.0, 0.0, None).is_none());
}

#[test]
fn refresh_bindings_follows_shape() {
    let left = box_at(0.0, 0.0, 100.0, 60.0);
    let right = box_at(300.0, 0.0, 100.0, 60.0);
    let mut arrow = connector(50.0, 30.0, 350.0, 30.0, DrawElementType::Arrow);
    arrow.start_binding = Some(left.id.clone());
    arrow.end_binding = Some(right.id.clone());
    let bound = refresh_bindings(&[left.clone(), right.clone(), arrow.clone()]);
    let first = linear_endpoints(&bound[2]);
    assert_eq!(first.0.x.round(), 100.0 + BINDING_GAP);
    assert_eq!(first.1.x.round(), 300.0 - BINDING_GAP);
    let mut moved = right;
    moved.y = 240.0;
    let after = refresh_bindings(&[left, moved.clone(), arrow]);
    let second = linear_endpoints(&after[2]);
    assert!(second.1.y > first.1.y + 100.0);
    assert!(second.0.y > first.0.y);
    assert!(!hit_test_element(&moved, second.1.x, second.1.y, 0.0));
}

#[test]
fn deleted_binding_ignored() {
    let mut shape = box_at(0.0, 0.0, 100.0, 60.0);
    shape.is_deleted = true;
    let mut arrow = connector(50.0, 30.0, 350.0, 30.0, DrawElementType::Arrow);
    arrow.start_binding = Some(shape.id.clone());
    let next = refresh_bindings(&[shape, arrow.clone()]);
    assert_eq!(linear_endpoints(&next[1]), linear_endpoints(&arrow));
}

#[test]
fn layout_label_centres() {
    let container = box_at(100.0, 100.0, 200.0, 80.0);
    let mut label = create_element_default(
        DrawElementType::Text,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 24.0,
        },
    );
    label.container_id = Some(container.id.clone());
    let placed_scene = refresh_bindings(&[container.clone(), label.clone()]);
    let placed = &placed_scene[1];
    assert_eq!(placed.y + placed.height / 2.0, element_center(&container).y);
    assert_eq!(placed.x + placed.width / 2.0, element_center(&container).x);
    assert_eq!(layout_label(label, &container), *placed);
}

#[test]
fn connector_hit_on_stroke() {
    let arrow = connector(0.0, 0.0, 200.0, 200.0, DrawElementType::Arrow);
    assert!(hit_test_element(&arrow, 100.0, 100.0, 4.0));
    assert!(!hit_test_element(&arrow, 180.0, 20.0, 4.0));
}

#[test]
fn default_arrowheads() {
    let arrow = connector(0.0, 0.0, 10.0, 0.0, DrawElementType::Arrow);
    assert_eq!(default_arrowhead(&arrow, "end"), Arrowhead::Arrow);
    assert_eq!(default_arrowhead(&arrow, "start"), Arrowhead::None);
    assert_eq!(
        default_arrowhead(
            &connector(0.0, 0.0, 10.0, 0.0, DrawElementType::Line),
            "end"
        ),
        Arrowhead::None
    );
    let mut explicit = arrow.clone();
    explicit.end_arrowhead = Some(Arrowhead::Diamond);
    assert_eq!(default_arrowhead(&explicit, "end"), Arrowhead::Diamond);
    explicit.end_arrowhead = Some(Arrowhead::None);
    assert_eq!(default_arrowhead(&explicit, "end"), Arrowhead::None);
}
