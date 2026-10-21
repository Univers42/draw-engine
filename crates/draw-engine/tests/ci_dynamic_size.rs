//! Dynamic size: what is drawn zoomed out looks as it would at 100%.
//!
//! The invariant (`docs/reference/dynamic-size.md`): with the setting on, an element drawn
//! at zoom `z` is, on screen, the element the same gesture draws at 100% with the setting
//! off — same thickness, arrowheads, sloppiness, corners, dashes and text size. In drawing
//! units it is that element `1/z` times over, so its exported SVG is the 100% one with
//! every length multiplied by `1/z`.
//!
//! Every element here is made the way a person makes one: a tool, a camera, a gesture.

mod common;
use common::*;
use draw_engine::render::bounds::render_bounds;
use draw_engine::render::shape::element_drawable;
use draw_engine::*;

const SCALE: f64 = 10.0;

fn at_zoom(engine: &mut DrawEngine, zoom: f64) {
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale: zoom,
    });
}

/// A browser's measure: glyph widths in proportion to the font size. The shared
/// `measure_text` adds a fixed 4 to every line, which no font does, and which would make
/// a 200px text a different shape from a 20px one.
fn in_proportion(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let widest = lines.iter().map(|line| line.len()).max().unwrap_or(0);
    (
        widest as f64 * font_size * 0.5,
        lines.len() as f64 * font_size * TEXT_LINE_HEIGHT,
    )
}

/// The scene after `gesture` runs with `tool` at `zoom`, the setting `dynamic`.
fn drawn(
    tool: DrawTool,
    zoom: f64,
    dynamic: bool,
    gesture: impl Fn(&mut DrawEngine),
) -> Vec<DrawElement> {
    let mut engine = engine_with_scene(vec![]);
    engine.set_measure_text(in_proportion);
    engine.set_dynamic_size(dynamic);
    at_zoom(&mut engine, zoom);
    engine.set_tool(tool);
    gesture(&mut engine);
    engine.get_scene()
}

fn drag(engine: &mut DrawEngine) {
    engine.begin_pointer(100.0, 100.0, false, false);
    for step in 1..=8 {
        let t = f64::from(step) / 8.0;
        engine.move_pointer(100.0 + 200.0 * t, 100.0 + 120.0 * t * t, false, false);
    }
    engine.end_pointer();
}

fn type_at(engine: &mut DrawEngine, text: &str) {
    engine.begin_pointer(225.0, 225.0, false, false);
    engine.end_pointer();
    engine.update_text_edit(text);
    engine.commit_text_edit(text, true);
}

fn union(elements: &[DrawElement]) -> WorldBounds {
    elements
        .iter()
        .map(render_bounds)
        .reduce(|a, b| WorldBounds {
            min_x: a.min_x.min(b.min_x),
            min_y: a.min_y.min(b.min_y),
            max_x: a.max_x.max(b.max_x),
            max_y: a.max_y.max(b.max_y),
        })
        .expect("something was drawn")
}

/// Every number in the attributes that carry a length.
fn lengths(svg: &str) -> Vec<f64> {
    const NAMES: [&str; 14] = [
        "d",
        "stroke-width",
        "stroke-dasharray",
        "font-size",
        "x",
        "y",
        "x1",
        "y1",
        "x2",
        "y2",
        "points",
        "transform",
        "width",
        "height",
    ];
    let mut out = Vec::new();
    for name in NAMES {
        let needle = format!(" {name}=\"");
        let mut rest = svg;
        while let Some(at) = rest.find(&needle) {
            rest = &rest[at + needle.len()..];
            let end = rest.find('"').expect("a closed attribute");
            let value = &rest[..end];
            let mut number = String::new();
            for c in value.chars().chain(std::iter::once(' ')) {
                let has_digit = number.chars().any(|d| d.is_ascii_digit());
                let sign = c == '-' && (number.is_empty() || number.ends_with('e'));
                if c.is_ascii_digit() || c == '.' || (c == 'e' && has_digit) || sign {
                    number.push(c);
                } else {
                    if has_digit {
                        out.push(number.parse::<f64>().expect("a number"));
                    }
                    number.clear();
                    if c == '-' {
                        number.push(c);
                    }
                }
            }
        }
    }
    out
}

/// Every coordinate of the rough sketch the canvas paints, in the element's own space.
fn sketch(element: &DrawElement) -> Vec<f64> {
    element_drawable(element)
        .map(|drawable| {
            drawable
                .sets
                .iter()
                .flat_map(|set| set.ops.iter().flat_map(|op| op.data().to_vec()))
                .collect()
        })
        .unwrap_or_default()
}

/// The invariant: drawn at 10% with the setting on, the scene exports as the 100% scene,
/// setting off, with every length ten times over. Seeds are the 100% scene's, so the two
/// are the same hand-drawn stroke.
fn assert_invariant(tool: DrawTool, gesture: impl Fn(&mut DrawEngine)) {
    let normal = drawn(tool, 1.0, false, &gesture);
    let mut zoomed = drawn(tool, 1.0 / SCALE, true, &gesture);
    assert_eq!(normal.len(), zoomed.len(), "{tool:?}: the same elements");
    for (z, n) in zoomed.iter_mut().zip(&normal) {
        assert_eq!(z.kind, n.kind, "{tool:?}: in the same order");
        assert_eq!(
            n.size_scale, None,
            "{tool:?}: the setting off scales nothing"
        );
        assert_eq!(
            z.size_scale,
            Some(SCALE),
            "{tool:?}: drawn at 10%, scale 10"
        );
        z.seed = n.seed;
    }
    let bounds = union(&normal);
    let scaled = WorldBounds {
        min_x: bounds.min_x * SCALE,
        min_y: bounds.min_y * SCALE,
        max_x: bounds.max_x * SCALE,
        max_y: bounds.max_y * SCALE,
    };
    // The canvas paints the sketch; the export draws clean geometry. Both are checked.
    for (z, n) in zoomed.iter().zip(&normal) {
        let (z_ops, n_ops) = (sketch(z), sketch(n));
        assert_eq!(z_ops.len(), n_ops.len(), "{tool:?}: the same sketch");
        for (i, (z, n)) in z_ops.iter().zip(&n_ops).enumerate() {
            assert!(
                (z - n * SCALE).abs() <= 1e-6 * (1.0 + z.abs()),
                "{tool:?}: sketch value #{i} is {z} at 10%, {n} at 100% — not {SCALE} times over"
            );
        }
    }
    let at_100 = lengths(&svg_at(&normal, bounds, 0.0));
    let at_10 = lengths(&svg_at(&zoomed, scaled, 0.0));
    assert_eq!(at_100.len(), at_10.len(), "{tool:?}: the same markup");
    for (i, (z, n)) in at_10.iter().zip(&at_100).enumerate() {
        assert!(
            (z - n * SCALE).abs() <= 0.06 + 1e-9 * z.abs(),
            "{tool:?}: length #{i} is {z} at 10%, {n} at 100% — not {SCALE} times over"
        );
    }
}

#[test]
fn a_rectangle_drawn_at_ten_percent_is_the_hundred_percent_one_ten_times_over() {
    assert_invariant(DrawTool::Rectangle, drag);
}

#[test]
fn a_diamond_drawn_at_ten_percent_is_the_hundred_percent_one_ten_times_over() {
    assert_invariant(DrawTool::Diamond, drag);
}

#[test]
fn an_ellipse_drawn_at_ten_percent_is_the_hundred_percent_one_ten_times_over() {
    assert_invariant(DrawTool::Ellipse, drag);
}

#[test]
fn a_line_drawn_at_ten_percent_is_the_hundred_percent_one_ten_times_over() {
    assert_invariant(DrawTool::Line, drag);
}

#[test]
fn an_arrow_and_its_head_drawn_at_ten_percent_are_the_hundred_percent_ones_ten_times_over() {
    assert_invariant(DrawTool::Arrow, drag);
}

#[test]
fn a_freehand_stroke_drawn_at_ten_percent_is_the_hundred_percent_one_ten_times_over() {
    assert_invariant(DrawTool::Freedraw, drag);
}

#[test]
fn a_text_typed_at_ten_percent_is_the_hundred_percent_one_ten_times_over() {
    assert_invariant(DrawTool::Text, |engine| type_at(engine, "Hydrawn"));
}

#[test]
fn a_note_and_its_label_made_at_ten_percent_are_the_hundred_percent_ones_ten_times_over() {
    assert_invariant(DrawTool::StickyNote, |engine| type_at(engine, "Hi"));
}

#[test]
fn a_dashed_rectangle_keeps_its_dashes_on_screen() {
    let dashed = |engine: &mut DrawEngine| {
        engine.set_next_style(DrawElementStylePatch {
            stroke_style: Some(StrokeStyle::Dashed),
            ..Default::default()
        });
        drag(engine);
    };
    assert_invariant(DrawTool::Rectangle, dashed);
}

#[test]
fn at_one_hundred_percent_the_setting_changes_nothing() {
    let on = drawn(DrawTool::Rectangle, 1.0, true, drag);
    assert_eq!(
        on[0].size_scale, None,
        "no field at 100%: the oracle's element"
    );
}

#[test]
fn the_scale_is_clamped_so_widths_stay_inside_the_contract() {
    let far = drawn(DrawTool::Rectangle, 0.01, true, drag);
    assert_eq!(far[0].size_scale, Some(10.0));
}

#[test]
fn a_label_typed_into_a_shape_takes_the_shapes_scale() {
    let mut engine = engine_with_measure(vec![]);
    engine.set_dynamic_size(true);
    at_zoom(&mut engine, 0.1);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine);
    at_zoom(&mut engine, 1.0);
    assert!(engine.edit_selected_text(), "Enter opens the shape's label");
    engine.update_text_edit("label");
    engine.commit_text_edit("label", true);
    let scene = engine.get_scene();
    let label = scene
        .iter()
        .find(|e| e.kind == DrawElementType::Text)
        .expect("the label");
    assert_eq!(
        label.size_scale,
        Some(SCALE),
        "the shape's scale, not the camera's"
    );
}

#[test]
fn a_paste_keeps_the_scale_it_came_with() {
    let mut engine = engine_with_measure(vec![]);
    engine.set_dynamic_size(true);
    at_zoom(&mut engine, 0.1);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine);
    let copied = engine.copy_selection().expect("the rectangle is selected");
    at_zoom(&mut engine, 0.5);
    assert!(engine.paste_json(Some(&copied), None));
    let scene = engine.get_scene();
    assert_eq!(scene.len(), 2);
    assert!(scene.iter().all(|e| e.size_scale == Some(SCALE)));
    assert_eq!(scene[0].stroke_width, scene[1].stroke_width);
}

#[test]
fn the_style_panel_speaks_in_keys_relative_to_the_scale() {
    let mut engine = engine_with_measure(vec![]);
    engine.set_dynamic_size(true);
    at_zoom(&mut engine, 0.1);
    engine.set_tool(DrawTool::Rectangle);
    drag(&mut engine);
    let width = engine.get_scene()[0].stroke_width;
    assert_eq!(width, 20.0, "the default 2, ten times over");
    assert_eq!(
        engine.selection_style().stroke_width,
        Some(2.0),
        "shown as bold"
    );
    engine.apply_style(DrawElementStylePatch {
        stroke_width: Some(4.0),
        ..Default::default()
    });
    assert_eq!(
        engine.get_scene()[0].stroke_width,
        40.0,
        "extra bold, at its scale"
    );
}
