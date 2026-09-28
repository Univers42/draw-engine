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

/// The step is the oracle's `SHIFT_LOCKING_ANGLE` — **15 degrees**
/// (`packages/common/src/constants.ts@1118751f:31`) — so a drag lands on one of 24
/// directions, not on one of eight. Read with `ci_end_snap`, which holds the same lock to
/// the oracle's ray-intersection; this one is about the two **axis** outcomes, which the
/// oracle writes out as branches of their own (`sizeHelpers.ts@1118751f:229-234`) and
/// which are *not* the general case's projection — a flat lock keeps the raw width rather
/// than the projected one, so a drag of 100 across and 10 down comes out 100 long, not
/// 99.5.
#[test]
fn shift_snaps_to_the_oracles_15_degree_step() {
    // 9.6° is nearer 15 than level, so it goes to 15 — where a 45° step sent it to level.
    let (dx, dy) = constrain_to_angle(100.0, 17.0);
    let locked = 15.0_f64.to_radians();
    assert!(
        (dy.atan2(dx).to_degrees() - 15.0).abs() < 1e-9,
        "a 9.6° drag locked to {}°",
        dy.atan2(dx).to_degrees()
    );
    let along = 100.0 * locked.cos() + 17.0 * locked.sin();
    assert!(
        (dx - along * locked.cos()).abs() < 1e-9 && (dy - along * locked.sin()).abs() < 1e-9,
        "the endpoint is not the projection onto the 15° ray"
    );

    // 50.0° is nearer 45 than either neighbour, and 45 is still one of the oracle's steps.
    let (dx, dy) = constrain_to_angle(100.0, 119.0);
    assert!(
        (dx - dy).abs() < 1e-9,
        "a 50° drag should land on the 45° diagonal"
    );

    let snapped = linear_from_drag(0.0, 0.0, 100.0, 17.0, true);
    assert!(snapped.height > 0.0, "a 9.6° drag is not level any more");
}

/// `design.md:828`, "Vertical" — the third of the three angle-snapping rows
/// (`design.md:825-832` lists Horizontal, Vertical, 45°, Configurable increments).
///
/// The claim is that a drag that is *nearly* vertical comes out vertical, and that it
/// keeps the extent it was already vertical in: the oracle's square branch **zeroes the
/// width** and leaves the height alone (`sizeHelpers.ts@1118751f:231-232`), so a drag 10
/// across and 100 down comes out 100 long, not the 100.5 it was dragged. A snap re-aims
/// the line, it does not preserve its length — which is the second half of the oracle's
/// rule and the half a rotation of the delta gets wrong.
#[test]
fn shift_snaps_a_near_vertical_drag_to_exactly_vertical() {
    // 10 across and 100 down is 84.3°, inside the 7.5° either side of 90° that rounds to
    // vertical, and nowhere near any other step.
    let (dx, dy) = constrain_to_angle(10.0, 100.0);
    assert!(dx.abs() < 1e-9, "the x offset is {dx}, not vertical");
    assert_eq!(dy, 100.0, "the height should survive the snap untouched");

    // And through the drag itself, so this is the path a person takes rather than the
    // helper alone: the line's second point sits straight below the first.
    let snapped = linear_from_drag(200.0, 300.0, 210.0, 400.0, true);
    assert_eq!(snapped.x, 200.0, "the start does not move");
    assert_eq!(snapped.y, 300.0);
    assert_eq!(snapped.points[0], [0.0, 0.0]);
    assert!(snapped.width.abs() < 1e-9, "no run at all to the side");
    assert_eq!(snapped.height, 100.0);
}

/// The mirror of the case above, for the same reason: `design.md:826` (Horizontal).
///
/// The oracle's flat branch zeroes the **height** and leaves the width alone
/// (`sizeHelpers.ts@1118751f:229-230`), so a drag 100 across and 10 down comes out 100
/// long. It is not the projection, which would have shortened it to 99.5: the branch keeps
/// the raw component and discards the other one outright.
#[test]
fn shift_snaps_a_near_horizontal_drag_to_exactly_horizontal() {
    let (dx, dy) = constrain_to_angle(100.0, 10.0);
    assert_eq!(dy, 0.0, "not level");
    assert_eq!(dx, 100.0, "the width should survive the snap untouched");
}

/// The step is 15°, so a drag lands on one of 24 directions, and a drag between two of
/// them goes to the nearer. The boundaries are halfway: 7.5° either side of each. Each
/// case below is a degree or two off one, so this fails if the snapping ever becomes
/// 90°-only, or steps by 45 again.
///
/// **The rounding is the oracle's, tie included.** `Math.round` breaks a tie **toward
/// +infinity** (`Math.round(-0.5) === -0`) and `f64::round` breaks it away from zero, so a
/// port using `.round()` is a whole step out on every negative half-step — the same trap
/// `getGridPoint` has, and the reason the rounding is written as `floor(x + 0.5)`.
#[test]
fn the_snap_step_is_15_degrees_so_a_drag_lands_on_one_of_24_directions() {
    // A drag of `deg`, always 100 across so the numbers stay readable.
    let at = |deg: f64| {
        let dy = 100.0 * deg.to_radians().tan();
        constrain_to_angle(100.0, dy)
    };
    let bearing = |dx: f64, dy: f64| dy.atan2(dx).to_degrees().rem_euclid(360.0);

    // Either side of the 7.5° boundary between level and 15°.
    for (deg, want) in [(6.0, 0.0), (9.0, 15.0)] {
        let (dx, dy) = at(deg);
        assert!(
            (bearing(dx, dy) - want).abs() < 1e-9,
            "a {deg}° drag locked to {}°, not {want}°",
            bearing(dx, dy)
        );
    }

    // A 30° drag is 30° in the oracle. Under the old 45° step this was the diagonal, which
    // is the whole of what the step being wrong looked like from the outside.
    let (dx, dy) = at(30.0);
    assert!((bearing(dx, dy) - 30.0).abs() < 1e-9);

    // And a few degrees either side of the 82.5° boundary before vertical.
    for (deg, want) in [(81.0, 75.0), (84.0, 90.0)] {
        let (dx, dy) = at(deg);
        assert!(
            (bearing(dx, dy) - want).abs() < 1e-9,
            "a {deg}° drag locked to {}°, not {want}°",
            bearing(dx, dy)
        );
    }

    let (dx, dy) = at(84.0);
    assert!(dx.abs() < 1e-9, "84° rounds up to vertical");
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
