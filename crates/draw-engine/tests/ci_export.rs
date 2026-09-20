#![allow(clippy::cloned_ref_to_slice_refs)]
mod common;
use common::*;
use draw_engine::*;

#[test]
fn scene_to_json_valid_osidraw_format() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let json = scene_to_json(&[rect]);
    assert!(json.contains("\"type\": \"osidraw\""));
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"elements\""));
}

#[test]
fn scene_to_json_skips_deleted_elements() {
    let mut deleted = box_at(0.0, 0.0, 100.0, 60.0);
    deleted.is_deleted = true;
    let live = box_at(10.0, 10.0, 50.0, 50.0);

    let json = scene_to_json(&[deleted, live.clone()]);
    let parsed = elements_from_json(&json).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].id, live.id);
}

#[test]
fn elements_from_json_valid_roundtrip() {
    let rect = box_at(10.0, 20.0, 80.0, 40.0);
    let json = scene_to_json(std::slice::from_ref(&rect));
    let parsed = elements_from_json(&json).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].id, rect.id);
    assert_close(parsed[0].width, 80.0);
}

#[test]
fn elements_from_json_invalid_type_returns_none() {
    let json = r#"{"type": "not_osidraw", "version": 1, "elements": []}"#;
    assert_eq!(elements_from_json(json), None);
}

#[test]
fn elements_from_json_malformed_json_returns_none() {
    assert_eq!(elements_from_json("invalid json string"), None);
    assert_eq!(elements_from_json("{"), None);
}

#[test]
fn elements_from_json_missing_elements_field_returns_none() {
    let json = r#"{"type": "osidraw", "version": 1}"#;
    assert_eq!(elements_from_json(json), None);
}

#[test]
fn scene_to_svg_contains_svg_tags() {
    let rect = box_at(0.0, 0.0, 100.0, 60.0);
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 60.0,
    };
    let svg = scene_to_svg(&[rect], bounds, 10.0, "#ffffff");
    assert!(svg.starts_with("<svg "));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("<rect "));
}

#[test]
fn scene_to_svg_skips_deleted_elements() {
    let mut deleted = box_at(0.0, 0.0, 100.0, 60.0);
    deleted.is_deleted = true;
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 60.0,
    };
    let svg = scene_to_svg(&[deleted], bounds, 10.0, "#ffffff");
    // Should only have the background rect:
    assert_eq!(svg.matches("<rect ").count(), 1);
}

#[test]
fn scene_to_svg_ellipse_render() {
    let el = ellipse_at(10.0, 20.0, 60.0, 80.0);
    let bounds = WorldBounds {
        min_x: 10.0,
        min_y: 20.0,
        max_x: 70.0,
        max_y: 100.0,
    };
    let svg = scene_to_svg(&[el], bounds, 5.0, "#ffffff");
    assert!(svg.contains("<ellipse "));
}

#[test]
fn scene_to_svg_diamond_polygon_render() {
    let dia = diamond_at(0.0, 0.0, 80.0, 80.0);
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 80.0,
        max_y: 80.0,
    };
    let svg = scene_to_svg(&[dia], bounds, 5.0, "#ffffff");
    assert!(svg.contains("<polygon points="));
}

#[test]
fn scene_to_svg_text_escapes_xml_entities() {
    let mut text = text_at(0.0, 0.0, 100.0, 30.0);
    text.text = Some("<tag> & \"quotes\"".into());
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 30.0,
    };
    let svg = scene_to_svg(&[text], bounds, 5.0, "#ffffff");
    assert!(svg.contains("&lt;tag&gt;"));
    assert!(svg.contains("&amp;"));
    assert!(svg.contains("&quot;quotes&quot;"));
}

#[test]
fn scene_to_svg_linear_connector_render() {
    let arrow = connector(0.0, 0.0, 100.0, 100.0, DrawElementType::Arrow);
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 100.0,
    };
    let svg = scene_to_svg(&[arrow], bounds, 5.0, "#ffffff");
    assert!(svg.contains("<line "));
}

#[test]
fn scene_to_svg_freedraw_polyline_render() {
    let mut freedraw = create_element_default(
        DrawElementType::Freedraw,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        },
    );
    freedraw.points = Some(vec![[0.0, 0.0], [25.0, 30.0], [50.0, 50.0]]);
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 50.0,
        max_y: 50.0,
    };
    let svg = scene_to_svg(&[freedraw], bounds, 5.0, "#ffffff");
    assert!(svg.contains("<polyline points="));
}

#[test]
fn scene_to_svg_stroke_style_dashed() {
    let mut rect = box_at(0.0, 0.0, 100.0, 60.0);
    rect.stroke_style = StrokeStyle::Dashed;
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 60.0,
    };
    let svg = scene_to_svg(&[rect], bounds, 5.0, "#ffffff");
    assert!(svg.contains("stroke-dasharray=\"8 6\""));
}

#[test]
fn scene_to_svg_stroke_style_dotted() {
    let mut rect = box_at(0.0, 0.0, 100.0, 60.0);
    rect.stroke_style = StrokeStyle::Dotted;
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 60.0,
    };
    let svg = scene_to_svg(&[rect], bounds, 5.0, "#ffffff");
    assert!(svg.contains("stroke-dasharray=\"2 4\""));
}

#[test]
fn scene_to_svg_arrowhead_marker_rendered() {
    let mut arrow = connector(0.0, 0.0, 150.0, 0.0, DrawElementType::Arrow);
    arrow.end_arrowhead = Some(Arrowhead::Diamond);
    let bounds = WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 150.0,
        max_y: 0.0,
    };
    let svg = scene_to_svg(&[arrow], bounds, 10.0, "#ffffff");
    assert!(svg.contains("<polygon "));
}
