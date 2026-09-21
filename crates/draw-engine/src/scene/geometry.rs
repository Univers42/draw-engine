use crate::camera::WorldBounds;
use crate::scene::element::{DrawElement, DrawElementType};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn normalize_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x: if width < 0.0 { x + width } else { x },
        y: if height < 0.0 { y + height } else { y },
        width: width.abs(),
        height: height.abs(),
    }
}

pub fn element_bounds(element: &DrawElement) -> WorldBounds {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    WorldBounds {
        min_x: rect.x,
        min_y: rect.y,
        max_x: rect.x + rect.width,
        max_y: rect.y + rect.height,
    }
}

/// The union of every live element's bounds.
///
/// Generic over the iterable so it accepts both an owned slice and a list of borrowed
/// elements — the render path holds `&DrawElement` to avoid cloning the scene, and
/// should not have to clone it back just to measure it.
pub fn scene_bounds<'a>(
    elements: impl IntoIterator<Item = &'a DrawElement>,
) -> Option<WorldBounds> {
    let mut bounds: Option<WorldBounds> = None;
    for element in elements {
        if element.is_deleted {
            continue;
        }
        let next = element_bounds(element);
        bounds = Some(match bounds {
            None => next,
            Some(cur) => WorldBounds {
                min_x: cur.min_x.min(next.min_x),
                min_y: cur.min_y.min(next.min_y),
                max_x: cur.max_x.max(next.max_x),
                max_y: cur.max_y.max(next.max_y),
            },
        });
    }
    bounds
}

pub fn distance_to_segment(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let length_sq = dx * dx + dy * dy;
    if length_sq < 1e-9 {
        return (px - ax).hypot(py - ay);
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / length_sq;
    let t = t.clamp(0.0, 1.0);
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// The pointer expressed in the element's own frame.
///
/// Every shape is defined unrotated and then turned about its centre by `angle`, so a hit
/// test only has to turn the *query* back by the same amount and can then work in the
/// simple axis-aligned space the shape is described in. Without this a rotated element is
/// tested against the box it would occupy if it had never been turned: a square rotated
/// 45 degrees is selectable from the empty space beyond its flat corners and dead along
/// its actual points.
pub fn to_element_local(element: &DrawElement, wx: f64, wy: f64) -> (f64, f64) {
    if element.angle == 0.0 {
        return (wx, wy);
    }
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let (sin, cos) = (-element.angle).sin_cos();
    let dx = wx - cx;
    let dy = wy - cy;
    (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
}

/// The axis-aligned box an element occupies once its rotation is taken into account.
///
/// [`element_bounds`] deliberately returns the *unrotated* box, which is what resize and
/// the stored geometry are expressed in. Anything asking "where is this on the board" —
/// a marquee, a fit-to-content — wants this one instead.
pub fn element_rotated_bounds(element: &DrawElement) -> WorldBounds {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    if element.angle == 0.0 {
        return WorldBounds {
            min_x: rect.x,
            min_y: rect.y,
            max_x: rect.x + rect.width,
            max_y: rect.y + rect.height,
        };
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let (sin, cos) = element.angle.sin_cos();
    let hw = rect.width / 2.0;
    let hh = rect.height / 2.0;
    // For an axis-aligned box turned by `angle`, the half-extent of the result has this
    // closed form — no need to walk the four corners.
    let ex = hw * cos.abs() + hh * sin.abs();
    let ey = hw * sin.abs() + hh * cos.abs();
    WorldBounds {
        min_x: cx - ex,
        min_y: cy - ey,
        max_x: cx + ex,
        max_y: cy + ey,
    }
}

/// Whether a colour paints nothing.
///
/// Excalidraw's `isTransparent`: the literal keyword, or any hex that carries a fully
/// zero alpha channel. A shape painted in one of these is an outline and nothing more.
pub fn is_transparent(color: &str) -> bool {
    let c = color.trim();
    if c.eq_ignore_ascii_case("transparent") {
        return true;
    }
    // #RRGGBBAA and #RGBA, the two hex forms that can carry alpha.
    match c.len() {
        9 => c.starts_with('#') && c[7..].eq_ignore_ascii_case("00"),
        5 => c.starts_with('#') && c[4..].eq_ignore_ascii_case("0"),
        _ => false,
    }
}

/// Whether the element's inside is part of it, or only its outline is.
///
/// Excalidraw's `shouldTestInside`. A shape with a background is a solid object and can
/// be picked up anywhere; a transparent one is a drawn outline, and its middle is the
/// canvas showing through.
///
/// The hollow/solid distinction only means anything for the three shapes that have a
/// fill to leave out. Text, freehand strokes, images and frames are their own content:
/// their "background" being transparent says nothing about whether their middle is
/// clickable, and treating a line of text as an outline would make it unselectable
/// except at its edges.
///
/// **Known divergence.** Excalidraw also treats a shape as solid when it carries bound
/// text, via `hasBoundTextElement`. That needs the container's `boundElements`
/// back-reference, which this schema does not have yet — the link only runs the other
/// way, from the label's `container_id`. So a *transparent* shape with a label is hollow
/// here and solid there. Its label is still clickable, so the shape is still reachable.
fn has_solid_interior(element: &DrawElement) -> bool {
    if !matches!(
        element.kind,
        DrawElementType::Rectangle | DrawElementType::Diamond | DrawElementType::Ellipse
    ) {
        return true;
    }
    !is_transparent(&element.background_color)
}

/// Whether the point is inside the element's shape, grown by `grow`.
///
/// `grow` is signed: positive inflates the shape, negative shrinks it. Testing both
/// gives the band around the outline without needing a separate distance function per
/// shape.
fn within_shape(element: &DrawElement, wx: f64, wy: f64, grow: f64) -> bool {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let rx = rect.width / 2.0 + grow;
    let ry = rect.height / 2.0 + grow;
    if rx <= 0.0 || ry <= 0.0 {
        // Shrunk past nothing: the shape has no interior left at this inset.
        return false;
    }
    let nx = (wx - cx) / rx;
    let ny = (wy - cy) / ry;
    match element.kind {
        DrawElementType::Ellipse => nx * nx + ny * ny <= 1.0,
        DrawElementType::Diamond => nx.abs() + ny.abs() <= 1.0,
        _ => nx.abs() <= 1.0 && ny.abs() <= 1.0,
    }
}

/// Whether a click at `(wx, wy)` lands on the element.
///
/// A filled shape is solid: anywhere within it, plus `tolerance` beyond its edge. A
/// transparent one is only its outline, so the test is the band of width `tolerance`
/// either side of that outline and its middle belongs to whatever is behind it.
///
/// This is Excalidraw's rule, checked against the running app rather than inferred:
/// on a clean board holding one transparent rectangle, clicking its dead centre selects
/// nothing and clicking its border selects it. Treating the hollow middle as a hit meant
/// a large transparent shape swallowed every click meant for the things drawn inside it.
pub fn hit_test_element(element: &DrawElement, wx: f64, wy: f64, tolerance: f64) -> bool {
    let (wx, wy) = to_element_local(element, wx, wy);
    if matches!(element.kind, DrawElementType::Line | DrawElementType::Arrow) {
        return hit_linear(element, wx, wy, tolerance);
    }

    if !within_shape(element, wx, wy, tolerance) {
        return false;
    }
    if has_solid_interior(element) {
        return true;
    }
    // Outline only: inside the grown shape but not inside the shrunken one.
    !within_shape(element, wx, wy, -tolerance)
}

fn hit_linear(element: &DrawElement, wx: f64, wy: f64, tolerance: f64) -> bool {
    let points = match &element.points {
        Some(points) if points.len() >= 2 => points,
        _ => return false,
    };
    // Half the stroke sits either side of the path, so that much is genuinely part of
    // the line; the tolerance is the aiming margin on top. This used to be
    // `max(tolerance, stroke) + 4`, which conflated the two and grew faster than either.
    let reach = tolerance + element.stroke_width / 2.0;
    for window in points.windows(2) {
        let ax = element.x + window[0][0];
        let ay = element.y + window[0][1];
        let bx = element.x + window[1][0];
        let by = element.y + window[1][1];
        if distance_to_segment(wx, wy, ax, ay, bx, by) <= reach {
            return true;
        }
    }
    false
}

pub fn hit_test(
    elements: &[DrawElement],
    wx: f64,
    wy: f64,
    tolerance: f64,
) -> Option<&DrawElement> {
    for element in elements.iter().rev() {
        if element.is_deleted {
            continue;
        }
        if hit_test_element(element, wx, wy, tolerance) {
            return Some(element);
        }
    }
    None
}
