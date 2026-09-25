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

/// A flipped image's transform: the mirror the canvas applies, about the same centre.
///
/// A flip is stored as a negative width or height and the canvas draws it through a
/// scale of -1 (`with_element_transform`); an `<image>` with the normalized box and only
/// the rotation came out unflipped — and, rotated as well, turned the wrong way.
fn image_transform(
    element: &DrawElement,
    rect: &crate::scene::geometry::Rect,
    rotation_only: &str,
) -> String {
    let (sx, sy) = crate::scene::geometry::mirror_signs(element);
    if sx > 0.0 && sy > 0.0 {
        return rotation_only.to_string();
    }
    let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    format!(
        " transform=\"translate({cx} {cy}) rotate({}) scale({sx} {sy}) translate({} {})\"",
        (element.angle * 180.0) / std::f64::consts::PI,
        -cx,
        -cy
    )
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

fn linear_svg(element: &DrawElement, label: Option<&DrawElement>) -> String {
    let body = linear_body_svg(element);
    let Some(label) = label.filter(|_| !body.is_empty()) else {
        return body;
    };
    // The stroke cut away under the label, as the canvas cuts it: a mask white
    // everywhere around the arrow but for the label's padded box
    // (`staticSvgScene.ts@1118751f:404-470`). In user space, or an axis-aligned arrow's
    // zero-height bounding box would mask it away entirely (#11439 there).
    let hole = crate::render::label_hole(label);
    let crate::scene::geometry::Rect {
        x,
        y,
        width,
        height,
    } = crate::render::label_cut_reach(element);
    let id = escape_xml(&format!("mask-{}", element.id));
    format!(
        "<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\"><rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"#fff\"/><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#000\"/></mask><g mask=\"url(#{id})\">{body}</g>",
        hole.x, hole.y, hole.width, hole.height
    )
}

fn linear_body_svg(element: &DrawElement) -> String {
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

fn element_svg<'a>(
    element: &DrawElement,
    lookup: impl Fn(&str) -> Option<&'a DrawElement>,
) -> String {
    if matches!(element.kind, DrawElementType::Line | DrawElementType::Arrow) {
        return linear_svg(element, crate::render::linear_label(element, lookup));
    }
    if element.kind == DrawElementType::StickyNote {
        return sticky_svg(element, crate::scene::sticky::wall_clock_ms());
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
        DrawElementType::Text => text_svg(element),
        // The picture itself. This fell through to the arm below and was written as a
        // stroked `<rect>`, so a board with a photo on it exported an empty box. An image
        // with no picture yet — a board loaded without its file — exports as nothing
        // rather than as a reference to nowhere.
        DrawElementType::Image => match element.data_url.as_deref() {
            Some(url) => format!(
                "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{}\" preserveAspectRatio=\"none\" opacity=\"{}\"{}/>",
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                escape_xml(url),
                element.opacity / 100.0,
                image_transform(element, &rect, &transform)
            ),
            None => String::new(),
        },
        _ => {
            // The same radius the canvas draws, from the same function. This used to be
            // `roundness.unwrap_or(0.0)` — the `8` every rounded shape carries and the
            // canvas has always ignored — so an exported rectangle had an 8px corner
            // while the board showed 32, and an explicit corner radius did not export
            // at all.
            let radius = crate::render::shape::corner_radius(rect.width.min(rect.height), element);
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{radius}\" {common}{dash}{transform}/>",
                rect.x, rect.y, rect.width, rect.height
            )
        }
    }
}

/// A sticky note as the oracle exports one (`staticSvgScene.ts@1118751f:158-266`): its
/// shadow, its paper, an edge shadow clipped to the paper, and the date — in a group
/// placed at the note and turned about its centre. The date is absolute, so an export
/// never goes stale; `now` only decides whether it shows the year.
pub(crate) fn sticky_svg(element: &DrawElement, now: f64) -> String {
    use crate::scene::sticky::{
        sticky_footer, sticky_path_commands, sticky_path_data, STICKY_NOTE_EDGE_SHADOW_OPACITY,
        STICKY_NOTE_EDGE_SHADOW_WIDTH, STICKY_NOTE_FOOTER_FONT_FAMILY,
        STICKY_NOTE_FOOTER_FONT_SIZE, STICKY_NOTE_FOOTER_OPACITY, STICKY_NOTE_SHADOW_OPACITY,
    };
    let opacity = element.opacity / 100.0;
    let opacity = if opacity == 1.0 {
        String::new()
    } else {
        format!(" opacity=\"{opacity}\"")
    };
    let shadow = sticky_path_data(&sticky_path_commands(element, true));
    let paper = sticky_path_data(&sticky_path_commands(element, false));
    let clip = format!("sticky-note-clipPath-{}", escape_xml(&element.id));
    let footer = sticky_footer(element, now).map_or(String::new(), |footer| {
        format!(
            "<text x=\"{}\" y=\"{}\" font-family=\"{STICKY_NOTE_FOOTER_FONT_FAMILY}\" font-size=\"{STICKY_NOTE_FOOTER_FONT_SIZE}px\" text-anchor=\"end\" direction=\"ltr\" fill=\"{}\" fill-opacity=\"{STICKY_NOTE_FOOTER_OPACITY}\">{}</text>",
            footer.x,
            footer.y,
            escape_xml(&element.stroke_color),
            escape_xml(&footer.text)
        )
    });
    format!(
        "<clipPath id=\"{clip}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{paper}\" fill=\"#000\" stroke=\"none\"/></clipPath>\
<g transform=\"translate({} {}) rotate({} {} {})\"{opacity}>\
<path d=\"{shadow}\" fill=\"#000\" fill-opacity=\"{STICKY_NOTE_SHADOW_OPACITY}\" stroke=\"none\"/>\
<path d=\"{paper}\" fill=\"{}\" stroke=\"none\"/>\
<path d=\"{paper}\" fill=\"none\" stroke=\"#000\" stroke-opacity=\"{STICKY_NOTE_EDGE_SHADOW_OPACITY}\" stroke-width=\"{}\" clip-path=\"url(#{clip})\"/>\
{footer}</g>",
        element.x,
        element.y,
        (element.angle * 180.0) / std::f64::consts::PI,
        element.width / 2.0,
        element.height / 2.0,
        escape_xml(&element.background_color),
        STICKY_NOTE_EDGE_SHADOW_WIDTH * 2.0,
    )
}

/// A text, one `<text>` per line, placed as the canvas places it
/// (`staticSvgScene.ts@1118751f:776-832`): in the family it was measured in, each line at
/// its alignment's anchor and its baseline ([`crate::render::text_line_placement`]),
/// whitespace kept as typed. A text with no family is drawn on the canvas from the top of
/// each line; here its first baseline stays 0.85 of the size below the top, where this
/// export has always put it.
///
/// ponytail: no `direction` — the canvas does not lay out right-to-left text either.
fn text_svg(element: &DrawElement) -> String {
    let text = element.text.as_deref().unwrap_or("");
    let font_size = crate::text::layout::font_size_of(element);
    let family = crate::scene::resolved_font_family(element);
    let (first, line_height) = crate::render::text_line_placement(element);
    let first = if family.is_some() {
        first
    } else {
        font_size * 0.85
    };
    let align = crate::scene::resolved_text_align(element);
    let anchor_x = crate::render::text_anchor_x(align, element.width);
    let text_anchor = match align {
        crate::scene::TextAlign::Left => "start",
        crate::scene::TextAlign::Center => "middle",
        crate::scene::TextAlign::Right => "end",
    };
    let css = crate::text::font::css_stack(family);
    let fill = escape_xml(&element.stroke_color);
    let lines = text
        .split('\n')
        .enumerate()
        .map(|(i, line)| {
            format!(
                "<text x=\"{anchor_x}\" y=\"{}\" font-family=\"{css}\" font-size=\"{font_size}px\" fill=\"{fill}\" text-anchor=\"{text_anchor}\" style=\"white-space: pre;\" dominant-baseline=\"alphabetic\">{}</text>",
                i as f64 * line_height + first,
                escape_xml(line)
            )
        })
        .collect::<String>();
    format!(
        "<g transform=\"translate({} {}) rotate({} {} {})\" opacity=\"{}\">{lines}</g>",
        element.x,
        element.y,
        (element.angle * 180.0) / std::f64::consts::PI,
        element.width / 2.0,
        element.height / 2.0,
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
    // Labels by id, for the arrows whose stroke is cut away under theirs.
    let labels: std::collections::HashMap<&str, &DrawElement> = elements
        .iter()
        .filter(|el| el.kind == DrawElementType::Text && el.container_id.is_some())
        .map(|el| (el.id.as_str(), el))
        .collect();
    let body = elements
        .iter()
        .filter(|el| !el.is_deleted)
        .map(|el| element_svg(el, |id| labels.get(id).copied()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><rect width=\"{width}\" height=\"{height}\" fill=\"{background}\"/><g transform=\"translate({dx} {dy})\">{body}</g></svg>"
    )
}
