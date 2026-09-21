use crate::camera::WorldBounds;
use crate::render::default_arrowhead;
use crate::scene::binding::linear_endpoints;
use crate::scene::element::{Arrowhead, DrawElement, DrawElementType, StrokeStyle};
use crate::scene::geometry::normalize_rect;

fn dash(style: StrokeStyle) -> &'static str {
    match style {
        StrokeStyle::Solid => "",
        StrokeStyle::Dashed => " stroke-dasharray=\"8 6\"",
        StrokeStyle::Dotted => " stroke-dasharray=\"2 4\"",
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn head_svg(
    kind: Arrowhead,
    tip_x: f64,
    tip_y: f64,
    angle: f64,
    element: &DrawElement,
    size: f64,
    opacity: f64,
) -> String {
    if kind == Arrowhead::None {
        return String::new();
    }
    let rotate = format!(
        " transform=\"rotate({} {tip_x} {tip_y}) translate({tip_x} {tip_y})\"",
        (angle * 180.0) / std::f64::consts::PI
    );
    let stroke = format!(
        "stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\" opacity=\"{opacity}\"",
        element.stroke_color, element.stroke_width
    );
    let solid = format!("fill=\"{}\" opacity=\"{opacity}\"", element.stroke_color);
    match kind {
        Arrowhead::Arrow => {
            let spread = std::f64::consts::PI / 7.0;
            let bx = -size * spread.cos();
            let by = size * spread.sin();
            format!(
                "<polyline points=\"{bx},{} 0,0 {bx},{by}\" fill=\"none\" {stroke}{rotate}/>",
                -by
            )
        }
        Arrowhead::Triangle => {
            format!(
                "<polygon points=\"0,0 {0},{1} {0},{2}\" {solid}{rotate}/>",
                -size,
                -size * 0.42,
                size * 0.42
            )
        }
        Arrowhead::Diamond => format!(
            "<polygon points=\"0,0 {0},{1} {2},0 {0},{3}\" {solid}{rotate}/>",
            -size * 0.5,
            -size * 0.42,
            -size,
            size * 0.42
        ),
        Arrowhead::Dot => format!(
            "<circle cx=\"{}\" cy=\"0\" r=\"{}\" {solid}{rotate}/>",
            -size * 0.3,
            size * 0.32
        ),
        Arrowhead::Bar => format!(
            "<line x1=\"0\" y1=\"{}\" x2=\"0\" y2=\"{}\" fill=\"none\" {stroke}{rotate}/>",
            -size * 0.5,
            size * 0.5
        ),
        Arrowhead::None => String::new(),
    }
}

fn linear_svg(element: &DrawElement) -> String {
    let (start, end) = linear_endpoints(element);
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy);
    if length < 0.5 {
        return String::new();
    }
    let angle = dy.atan2(dx);
    let size = 14.0_f64.max(element.stroke_width * 4.0);
    let opacity = element.opacity / 100.0;
    let shaft = format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" fill=\"none\" opacity=\"{opacity}\"{}/>",
        start.x, start.y, end.x, end.y, element.stroke_color, element.stroke_width, dash(element.stroke_style)
    );
    format!(
        "{shaft}{}{}",
        head_svg(
            default_arrowhead(element, "end"),
            end.x,
            end.y,
            angle,
            element,
            size,
            opacity
        ),
        head_svg(
            default_arrowhead(element, "start"),
            start.x,
            start.y,
            angle + std::f64::consts::PI,
            element,
            size,
            opacity
        )
    )
}

fn element_svg(element: &DrawElement) -> String {
    if matches!(element.kind, DrawElementType::Line | DrawElementType::Arrow) {
        return linear_svg(element);
    }
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    let fill = if element.background_color.is_empty() || element.background_color == "transparent" {
        "none"
    } else {
        element.background_color.as_str()
    };
    let common = format!(
        "stroke=\"{}\" stroke-width=\"{}\" fill=\"{fill}\" opacity=\"{}\"",
        element.stroke_color,
        element.stroke_width,
        element.opacity / 100.0
    );
    let dash = dash(element.stroke_style);
    let transform = if element.angle != 0.0 {
        format!(
            " transform=\"rotate({} {} {})\"",
            (element.angle * 180.0) / std::f64::consts::PI,
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0
        )
    } else {
        String::new()
    };
    match element.kind {
        DrawElementType::Ellipse => format!(
            "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" {common}{dash}{transform}/>",
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0,
            rect.width / 2.0,
            rect.height / 2.0
        ),
        DrawElementType::Diamond => format!(
            "<polygon points=\"{},{} {},{} {},{} {},{}\" {common}{dash}{transform}/>",
            rect.x + rect.width / 2.0,
            rect.y,
            rect.x + rect.width,
            rect.y + rect.height / 2.0,
            rect.x + rect.width / 2.0,
            rect.y + rect.height,
            rect.x,
            rect.y + rect.height / 2.0
        ),
        DrawElementType::Freedraw => {
            let pts = element
                .points
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|&[px, py]| format!("{},{}", rect.x + px, rect.y + py))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "<polyline points=\"{pts}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\" opacity=\"{}\"/>",
                element.stroke_color,
                element.stroke_width,
                element.opacity / 100.0
            )
        }
        DrawElementType::Text => text_svg(element, &rect, &transform),
        _ => {
            let radius = element.roundness.unwrap_or(0.0);
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{radius}\" {common}{dash}{transform}/>",
                rect.x, rect.y, rect.width, rect.height
            )
        }
    }
}

fn text_svg(element: &DrawElement, rect: &crate::scene::geometry::Rect, transform: &str) -> String {
    let font_size = element.font_size.unwrap_or(20.0);
    let lines = element.text.as_deref().unwrap_or("").split('\n');
    let spans = lines
        .enumerate()
        .map(|(i, line)| {
            let dy = if i == 0 {
                font_size * 0.85
            } else {
                font_size * 1.25
            };
            format!(
                "<tspan x=\"{}\" dy=\"{dy}\">{}</tspan>",
                rect.x,
                escape_xml(line)
            )
        })
        .collect::<String>();
    format!(
        // The family the canvas draws with, not a third opinion. Exporting in a different
        // face from the one the text was measured and laid out in makes every text box
        // the wrong size in the exported file.
        "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{font_size}\" fill=\"{}\" opacity=\"{}\"{transform}>{spans}</text>",
        rect.x,
        rect.y,
        crate::FONT_FAMILY,
        element.stroke_color,
        element.opacity / 100.0
    )
}

pub fn scene_to_svg(
    elements: &[DrawElement],
    bounds: WorldBounds,
    padding: f64,
    background: &str,
) -> String {
    let width = bounds.max_x - bounds.min_x + padding * 2.0;
    let height = bounds.max_y - bounds.min_y + padding * 2.0;
    let dx = padding - bounds.min_x;
    let dy = padding - bounds.min_y;
    let body = elements
        .iter()
        .filter(|el| !el.is_deleted)
        .map(element_svg)
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><rect width=\"{width}\" height=\"{height}\" fill=\"{background}\"/><g transform=\"translate({dx} {dy})\">{body}</g></svg>"
    )
}
