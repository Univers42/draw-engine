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
