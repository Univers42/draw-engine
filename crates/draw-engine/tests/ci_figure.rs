//! The `figure` element: a parametric shape (polygon, star, parallelogram, trapezoid,
//! cylinder, document) treated like a rectangle or diamond everywhere the engine touches
//! an element — hit test, binding, style, duplication, flowchart, export. The shape
//! itself is `scene::figure::outline`'s own business and is unit-tested there
//! (`crates/draw-engine/src/scene/figure.rs`); this file is the integration surface.

mod common;
use common::*;
use draw_engine::scene::BindMode;
use draw_engine::selection::linear::world_points;
use draw_engine::*;

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("element exists")
}

fn triangle(x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    filled(figure_at(x, y, w, h, FigureKind::Polygon, Some(3), None))
}

// ------------------------------------------------------------------------------ creation

#[test]
fn the_shapes_picker_is_what_a_palette_insert_takes() {
    let mut engine = engine_with_scene(Vec::new());
    engine.set_next_figure(FigureParams {
        kind: FigureKind::Star,
        sides: Some(6),
        ratio: Some(0.4),
    });
    let id = engine
        .insert_default_shape(DrawElementType::Figure, 400.0, 300.0)
        .expect("figure is a supported kind");

    let figure = element(&engine, &id);
    assert_eq!(figure.kind, DrawElementType::Figure);
    assert_eq!(
        figure.figure,
        Some(FigureParams {
            kind: FigureKind::Star,
            sides: Some(6),
            ratio: Some(0.4),
        })
    );
}

#[test]
fn dragging_the_figure_tool_takes_the_same_pending_params() {
    let mut engine = engine_with_scene(Vec::new());
    engine.set_next_figure(FigureParams {
        kind: FigureKind::Cylinder,
        sides: None,
        ratio: Some(0.3),
    });
    engine.set_tool(DrawTool::Figure);
    engine.begin_pointer(0.0, 0.0, false, false);
    engine.move_pointer(120.0, 80.0, false, false);
    engine.end_pointer();

    let scene = engine.get_scene();
    let figure = scene.last().expect("a figure was drawn");
    assert_eq!(figure.kind, DrawElementType::Figure);
    assert_eq!(
        figure.figure.as_ref().map(|p| p.kind),
        Some(FigureKind::Cylinder)
    );
    assert_eq!(figure.figure.as_ref().and_then(|p| p.ratio), Some(0.3));
}

#[test]
fn absent_params_are_the_kinds_own_defaults() {
    let mut engine = engine_with_scene(Vec::new());
    engine.set_next_figure(FigureParams {
        kind: FigureKind::Trapezoid,
        sides: None,
        ratio: None,
    });
    let id = engine
        .insert_default_shape(DrawElementType::Figure, 0.0, 0.0)
        .unwrap();
    let params = element(&engine, &id).figure.expect("a figure has params");
    assert_eq!(params.kind, FigureKind::Trapezoid);
    assert!(
        params.ratio.is_none(),
        "the default is the kind's own, not stored"
    );
}

// ------------------------------------------------------------------------------ hit test

#[test]
fn hit_testing_follows_the_real_silhouette_not_the_bounding_box() {
    // A vertex-at-top triangle in a 100x100 box: the apex is at (50, 0), the base runs
    // the full bottom edge. The box's top-left corner is nowhere near the triangle.
    let tri = triangle(0.0, 0.0, 100.0, 100.0);
    assert!(
        !hit_test_element(&tri, 5.0, 5.0, 0.0),
        "inside the box, outside the triangle"
    );
    assert!(
        hit_test_element(&tri, 50.0, 50.0, 0.0),
        "inside both the box and the triangle"
    );
    // The same point, but on a plain box, is a hit — the contrast is the point.
    let box_el = filled(box_at(0.0, 0.0, 100.0, 100.0));
    assert!(hit_test_element(&box_el, 5.0, 5.0, 0.0));
}

#[test]
fn a_transparent_figure_is_hit_only_on_its_outline() {
    let tri = figure_at(0.0, 0.0, 100.0, 100.0, FigureKind::Polygon, Some(3), None);
    assert!(
        !hit_test_element(&tri, 50.0, 50.0, 0.0),
        "the middle of a hollow figure is not itself"
    );
}

#[test]
fn figures_are_bindable_like_any_other_shape() {
    assert!(is_bindable_element(&triangle(0.0, 0.0, 50.0, 50.0)));
}

// ------------------------------------------------------------------------------ binding

#[test]
fn an_arrow_binds_to_a_figure_it_is_dropped_inside() {
    let tri = triangle(0.0, 0.0, 100.0, 100.0);
    let tri_id = tri.id.clone();
    let mut engine = engine_with_scene(vec![tri]);
    engine.set_tool(DrawTool::Arrow);
    // The apex, well inside the triangle's own silhouette.
    engine.begin_pointer(-100.0, 50.0, false, false);
    engine.move_pointer(50.0, 5.0, false, false);
    engine.end_pointer();

    let arrow = engine.get_scene().into_iter().last().unwrap();
    assert_eq!(arrow.end_binding.as_deref(), Some(tri_id.as_str()));
    assert_eq!(arrow.end_bind_mode, Some(BindMode::Inside));
}

#[test]
fn a_point_in_the_box_corner_but_outside_the_silhouette_orbits_rather_than_binds_inside() {
    let tri = triangle(0.0, 0.0, 100.0, 100.0);
    let tri_id = tri.id.clone();
    let mut engine = engine_with_scene(vec![tri]);
    engine.set_tool(DrawTool::Arrow);
    // (42, 10): inside the triangle's bounding box, a few units outside its real slanted
    // left edge (which sits at x = 45 at this height) — close enough to still bind, far
    // enough that a box-shaped test would have called it a hit and bound it inside.
    engine.begin_pointer(-100.0, 10.0, false, false);
    engine.move_pointer(42.0, 10.0, false, false);
    engine.end_pointer();

    let arrow = engine.get_scene().into_iter().last().unwrap();
    assert_eq!(arrow.end_binding.as_deref(), Some(tri_id.as_str()));
    assert_eq!(
        arrow.end_bind_mode,
        Some(BindMode::Orbit),
        "the real outline decided, not the box"
    );
}

// ------------------------------------------------------------------------------ style

#[test]
fn the_sides_stepper_is_one_undo_step() {
    let mut engine = engine_with_scene(vec![figure_at(
        0.0,
        0.0,
        100.0,
        100.0,
        FigureKind::Polygon,
        Some(6),
        None,
    )]);
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id.clone()]);

    engine.apply_style(DrawElementStylePatch {
        figure_sides: Some(8),
        ..Default::default()
    });
    assert_eq!(element(&engine, &id).figure.and_then(|p| p.sides), Some(8));
    engine.undo();
    assert_eq!(element(&engine, &id).figure.and_then(|p| p.sides), Some(6));
}

#[test]
fn the_ratio_slider_previews_live_and_commits_once() {
    let mut engine = engine_with_scene(vec![figure_at(
        0.0,
        0.0,
        100.0,
        100.0,
        FigureKind::Star,
        None,
        Some(0.3),
    )]);
    let id = engine.get_scene()[0].id.clone();
    let before_version = element(&engine, &id).version;
    engine.select(vec![id.clone()]);

    engine.preview_style(DrawElementStylePatch {
        figure_ratio: Some(0.5),
        ..Default::default()
    });
    engine.preview_style(DrawElementStylePatch {
        figure_ratio: Some(0.6),
        ..Default::default()
    });
    assert_eq!(
        element(&engine, &id).figure.as_ref().and_then(|p| p.ratio),
        Some(0.6)
    );
    assert_eq!(
        element(&engine, &id).version,
        before_version,
        "a preview is not stamped"
    );

    engine.apply_style(DrawElementStylePatch {
        figure_ratio: Some(0.6),
        ..Default::default()
    });
    engine.undo();
    assert_eq!(
        element(&engine, &id).figure.and_then(|p| p.ratio),
        Some(0.3),
        "the whole drag was one step"
    );
}

#[test]
fn a_ratio_edit_on_a_polygon_is_a_no_op() {
    // Polygon has no ratio at all (`FigureKind::Polygon`'s doc); the control simply is
    // not offered, and a patch reaching it anyway changes nothing.
    let mut engine = engine_with_scene(vec![figure_at(
        0.0,
        0.0,
        100.0,
        100.0,
        FigureKind::Polygon,
        Some(5),
        None,
    )]);
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id.clone()]);
    engine.apply_style(DrawElementStylePatch {
        figure_ratio: Some(0.5),
        ..Default::default()
    });
    assert_eq!(element(&engine, &id).figure.and_then(|p| p.ratio), None);
}

#[test]
fn picking_a_kind_restyles_the_selection_and_the_next_figure_as_one_undo_step() {
    let mut engine = engine_with_scene(vec![figure_at(
        0.0,
        0.0,
        100.0,
        100.0,
        FigureKind::Polygon,
        Some(7),
        None,
    )]);
    let id = engine.get_scene()[0].id.clone();
    engine.select(vec![id.clone()]);

    engine.apply_style(DrawElementStylePatch {
        figure_kind: Some(FigureKind::Star),
        ..Default::default()
    });

    let after = element(&engine, &id).figure.expect("still a figure");
    assert_eq!(after.kind, FigureKind::Star);
    assert_eq!(after.sides, Some(7), "polygon <-> star keeps its sides");
    assert_eq!(
        after.ratio, None,
        "ratio resets to the new kind's own default"
    );
    assert_eq!(
        engine.next_figure().kind,
        FigureKind::Star,
        "the tool's next figure follows the pick too, like set_arrow_type"
    );

    engine.undo();
    let before = element(&engine, &id).figure.expect("still a figure");
    assert_eq!(
        before.kind,
        FigureKind::Polygon,
        "one undo step reverts the kind"
    );
    assert_eq!(before.sides, Some(7));
}

#[test]
fn a_ratio_change_relays_out_the_bound_label() {
    let mut fig = figure_at(
        0.0,
        0.0,
        200.0,
        100.0,
        FigureKind::Trapezoid,
        None,
        Some(0.4),
    );
    fig.id = "fig".into();
    let mut label = text_at(0.0, 0.0, 24.0, 25.0);
    label.id = "label".into();
    label.text = Some("hi".into());
    label.container_id = Some(fig.id.clone());
    // Left, not the label default of Center once bound: `figure_text_inset_fraction`'s
    // inset is symmetric, so a centred label's *position* would not move even though its
    // available width does — this pins the axis that actually moves.
    label.text_align = Some(TextAlign::Left);
    // Already laid out correctly for the figure's *current* ratio (0.4) — what
    // `container_coords` puts here: padding(5) + ratio(0.4) * width(200).
    label.x = 85.0;
    fig.bound_text_id = Some(label.id.clone());

    let mut engine = engine_with_measure(vec![fig, label]);
    engine.select(vec!["fig".into()]);

    engine.apply_style(DrawElementStylePatch {
        figure_ratio: Some(0.1),
        ..Default::default()
    });

    let after = element(&engine, "label");
    assert_close(after.x, 25.0); // padding(5) + ratio(0.1) * width(200), relaid out
}

#[test]
fn a_bound_arrows_orbiting_end_follows_a_sides_change_onto_the_new_outline() {
    let tri = triangle(0.0, 0.0, 100.0, 100.0);
    let tri_id = tri.id.clone();
    let mut engine = engine_with_scene(vec![tri]);
    engine.set_tool(DrawTool::Arrow);
    // Same drop as `a_point_in_the_box_corner_but_outside_the_silhouette_orbits...`:
    // just clear of the triangle's slanted left edge, so the end orbits it.
    engine.begin_pointer(-100.0, 10.0, false, false);
    engine.move_pointer(42.0, 10.0, false, false);
    engine.end_pointer();

    let arrow_id = engine
        .get_scene()
        .into_iter()
        .last()
        .expect("an arrow was drawn")
        .id;
    let before = {
        let arrow = element(&engine, &arrow_id);
        assert_eq!(arrow.end_binding.as_deref(), Some(tri_id.as_str()));
        assert_eq!(arrow.end_bind_mode, Some(BindMode::Orbit));
        *world_points(&arrow).last().expect("an arrow has an end")
    };

    engine.select(vec![tri_id]);
    engine.apply_style(DrawElementStylePatch {
        figure_sides: Some(12),
        ..Default::default()
    });

    let after = *world_points(&element(&engine, &arrow_id))
        .last()
        .expect("an arrow has an end");
    let moved = (after.x - before.x).hypot(after.y - before.y);
    assert!(
        moved > 1.0,
        "the orbiting end should re-resolve onto the 12-gon's outline, moved {moved} from {before:?} to {after:?}"
    );
}

// ------------------------------------------------------------------------------ duplicate / copy-paste

#[test]
fn duplicating_a_figure_keeps_its_params() {
    let fig = figure_at(0.0, 0.0, 80.0, 80.0, FigureKind::Document, None, Some(0.2));
    let id = fig.id.clone();
    let mut engine = engine_with_scene(vec![fig]);
    engine.select(vec![id.clone()]);
    engine.duplicate_selection(10.0, 10.0);

    let copies: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|el| el.id != id && el.kind == DrawElementType::Figure)
        .collect();
    assert_eq!(copies.len(), 1);
    assert_eq!(
        copies[0].figure,
        Some(FigureParams {
            kind: FigureKind::Document,
            sides: None,
            ratio: Some(0.2),
        })
    );
}

#[test]
fn copy_and_paste_round_trips_the_params_through_json() {
    let fig = figure_at(
        0.0,
        0.0,
        60.0,
        60.0,
        FigureKind::Parallelogram,
        None,
        Some(0.3),
    );
    let id = fig.id.clone();
    let mut engine = engine_with_scene(vec![fig]);
    engine.select(vec![id.clone()]);
    engine.copy_selection();
    assert!(engine.paste_json(None, Some((200.0, 200.0))));

    let pasted: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|el| el.id != id)
        .collect();
    assert_eq!(pasted.len(), 1);
    assert_eq!(
        pasted[0].figure.as_ref().map(|p| p.kind),
        Some(FigureKind::Parallelogram)
    );
    assert_eq!(pasted[0].figure.as_ref().and_then(|p| p.ratio), Some(0.3));
}

// ------------------------------------------------------------------------------ flowchart

#[test]
fn growing_a_flowchart_chain_from_a_figure_clones_its_params() {
    let template = figure_at(
        0.0,
        0.0,
        100.0,
        60.0,
        FigureKind::Trapezoid,
        None,
        Some(0.3),
    );
    let mut engine = engine_with_scene(vec![template.clone()]);
    engine.select(vec![template.id.clone()]);
    engine.flowchart_create(LinkDirection::Right);
    engine.flowchart_commit();

    let grown: Vec<DrawElement> = engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == DrawElementType::Figure && el.id != template.id)
        .collect();
    assert_eq!(grown.len(), 1);
    assert_eq!(
        grown[0].figure,
        Some(FigureParams {
            kind: FigureKind::Trapezoid,
            sides: None,
            ratio: Some(0.3),
        })
    );
}

// ------------------------------------------------------------------------------ export

#[test]
fn svg_export_traces_the_real_outline_not_a_rectangle() {
    let fig = figure_at(10.0, 10.0, 50.0, 50.0, FigureKind::Star, Some(5), Some(0.4));
    let engine = engine_with_scene(vec![fig]);
    let svg = engine.export_svg(0.0).unwrap();
    assert!(
        svg.contains("<polygon"),
        "a figure exports as its own outline: {svg}"
    );
}

#[test]
fn a_cylinders_open_rim_exports_as_its_own_stroke() {
    let fig = figure_at(0.0, 0.0, 50.0, 50.0, FigureKind::Cylinder, None, Some(0.2));
    let engine = engine_with_scene(vec![fig]);
    let svg = engine.export_svg(0.0).unwrap();
    assert!(svg.contains("<polygon"));
    assert!(
        svg.contains("<polyline"),
        "the cap's front rim is a second, open stroke: {svg}"
    );
}
