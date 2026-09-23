use crate::camera::{Point, WorldBounds};
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

/// Whether the element's geometry lives in its point list rather than in a width and
/// height.
///
/// For these, `x`/`y` is the position of the **first point**, not a corner of a box, and
/// `width`/`height` are the size of the point cloud rather than an offset from `x`. So
/// `x + width` is not the right edge and never was: an arrow whose tail is dragged past
/// its head has points running negative, and reading its box as `[x, x + width]` puts it
/// entirely to the right of where it is drawn.
pub fn is_point_based(element: &DrawElement) -> bool {
    matches!(
        element.kind,
        DrawElementType::Line | DrawElementType::Arrow | DrawElementType::Freedraw
    )
}

/// The element's own coordinate box, relative to `element.x` / `element.y`.
///
/// This is the region the geometry is generated into, and it is the single place that
/// knows how each kind of element is anchored. Everything that needs an element's extent
/// or its centre of rotation derives it from here, so the painter, the hit test and the
/// selection frame cannot disagree about where a shape is.
pub fn local_box(element: &DrawElement) -> Rect {
    if is_point_based(element) {
        if let Some(points) = element.points.as_deref() {
            if !points.is_empty() {
                let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
                let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
                for p in points {
                    min_x = min_x.min(p[0]);
                    min_y = min_y.min(p[1]);
                    max_x = max_x.max(p[0]);
                    max_y = max_y.max(p[1]);
                }
                return Rect {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                };
            }
        }
    }
    // A shape is generated at the origin spanning its absolute size; the sign of the
    // extent is a mirror, applied by the painter, and does not move the local box.
    Rect {
        x: 0.0,
        y: 0.0,
        width: element.width.abs(),
        height: element.height.abs(),
    }
}

/// Where the element turns about, in its own coordinates.
///
/// Rotation happens about the centre of [`local_box`]. For a shape that is the middle of
/// its box, as before. For a line or arrow it is the middle of the points — which for a
/// leftward arrow is **behind** `x`, where `x + width / 2` would have put the pivot a
/// full width beyond its own tip.
pub fn local_center(element: &DrawElement) -> (f64, f64) {
    let b = local_box(element);
    (b.x + b.width / 2.0, b.y + b.height / 2.0)
}

/// Which axes the element is mirrored on, as a scale of +1 or -1.
///
/// A negative extent means "mirrored" **only for shapes**, where the extent is the whole
/// description of the geometry. A line or arrow carries its shape in its points, so
/// mirroring one reverses those points; the sign its width happens to have is a leftover
/// of how it was last written and means nothing. Reading it as a mirror would flip the
/// element twice, and would move its pivot outside itself.
pub fn mirror_signs(element: &DrawElement) -> (f64, f64) {
    if is_point_based(element) {
        return (1.0, 1.0);
    }
    (
        if element.width < 0.0 { -1.0 } else { 1.0 },
        if element.height < 0.0 { -1.0 } else { 1.0 },
    )
}

/// The element's world-space centre of rotation.
pub fn rotation_center(element: &DrawElement) -> Point {
    let (lcx, lcy) = local_center(element);
    let (sx, sy) = mirror_signs(element);
    Point {
        x: element.x + sx * lcx,
        y: element.y + sy * lcy,
    }
}

/// The unrotated box the element occupies in the world.
pub fn element_bounds(element: &DrawElement) -> WorldBounds {
    if is_point_based(element) {
        let b = local_box(element);
        return WorldBounds {
            min_x: element.x + b.x,
            min_y: element.y + b.y,
            max_x: element.x + b.x + b.width,
            max_y: element.y + b.y + b.height,
        };
    }
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
    // The same pivot the painter uses. Computing it here as `x + width / 2` is what put
    // the hit test a full width away from a leftward arrow.
    let c = rotation_center(element);
    let (sin, cos) = (-element.angle).sin_cos();
    let dx = wx - c.x;
    let dy = wy - c.y;
    (c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos)
}

/// The axis-aligned box an element occupies once its rotation is taken into account.
///
/// [`element_bounds`] deliberately returns the *unrotated* box, which is what resize and
/// the stored geometry are expressed in. Anything asking "where is this on the board" —
/// a marquee, a fit-to-content — wants this one instead.
pub fn element_rotated_bounds(element: &DrawElement) -> WorldBounds {
    let plain = element_bounds(element);
    if element.angle == 0.0 {
        return plain;
    }
    let c = rotation_center(element);
    let (cx, cy) = (c.x, c.y);
    let (sin, cos) = element.angle.sin_cos();
    let hw = (plain.max_x - plain.min_x) / 2.0;
    let hh = (plain.max_y - plain.min_y) / 2.0;
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

/// How many points a curve is sampled at when it has to be described as a polygon.
///
/// Four box corners would let a loop drawn snugly around a circle miss it, and would let
/// a frame's edge appear to cross an ellipse it never touches.
const OUTLINE_SAMPLES: usize = 24;

/// The element's own outline, as a closed ring of world-space points.
///
/// The single answer to "what shape is this element, really" — used by the lasso to
/// decide what a loop encloses and by frames to decide what they contain. Two
/// definitions of an element's outline would eventually disagree, and the disagreement
/// would look like a selection bug rather than a geometry one.
///
/// A line, arrow or freehand stroke is its own path and comes back open; everything else
/// is its box, turned if it is turned.
pub fn element_outline(element: &DrawElement) -> Vec<Point> {
    if is_point_based(element) {
        let points = crate::selection::linear::world_points(element);
        if !points.is_empty() {
            return points;
        }
    }

    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    let centre = rotation_center(element);
    let (sin, cos) = element.angle.sin_cos();
    let turn = |x: f64, y: f64| {
        let (dx, dy) = (x - centre.x, y - centre.y);
        Point {
            x: centre.x + dx * cos - dy * sin,
            y: centre.y + dx * sin + dy * cos,
        }
    };

    match element.kind {
        DrawElementType::Ellipse => {
            let (rx, ry) = (rect.width / 2.0, rect.height / 2.0);
            let (cx, cy) = (rect.x + rx, rect.y + ry);
            (0..OUTLINE_SAMPLES)
                .map(|i| {
                    let t = i as f64 / OUTLINE_SAMPLES as f64 * std::f64::consts::TAU;
                    turn(cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect()
        }
        DrawElementType::Diamond => {
            let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            vec![
                turn(cx, rect.y),
                turn(rect.x + rect.width, cy),
                turn(cx, rect.y + rect.height),
                turn(rect.x, cy),
            ]
        }
        _ => vec![
            turn(rect.x, rect.y),
            turn(rect.x + rect.width, rect.y),
            turn(rect.x + rect.width, rect.y + rect.height),
            turn(rect.x, rect.y + rect.height),
        ],
    }
}

/// Whether the outline of this element closes back on itself.
///
/// Open paths must not have their last point joined to their first, or a line drawn
/// across a frame would appear to enclose the triangle between its ends.
pub fn outline_is_closed(element: &DrawElement) -> bool {
    !is_point_based(element)
}

/// `> 0` when `p` is left of the directed line `a -> b`.
pub fn cross(a: Point, b: Point, p: Point) -> f64 {
    (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)
}

/// Whether two segments cross, touching included.
///
/// Collinear touching counts, so a loop drawn exactly along an edge still catches it and
/// an element laid exactly on a frame's border still counts as meeting it.
pub fn segments_intersect(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    let d1 = cross(p3, p4, p1);
    let d2 = cross(p3, p4, p2);
    let d3 = cross(p1, p2, p3);
    let d4 = cross(p1, p2, p4);

    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    let on = |a: Point, b: Point, p: Point, d: f64| {
        d == 0.0
            && p.x >= a.x.min(b.x)
            && p.x <= a.x.max(b.x)
            && p.y >= a.y.min(b.y)
            && p.y <= a.y.max(b.y)
    };
    on(p3, p4, p1, d1) || on(p3, p4, p2, d2) || on(p1, p2, p3, d3) || on(p1, p2, p4, d4)
}

/// The edges of an outline, respecting whether it closes.
pub fn outline_edges(outline: &[Point], closed: bool) -> Vec<(Point, Point)> {
    if outline.len() < 2 {
        return Vec::new();
    }
    let count = if closed {
        outline.len()
    } else {
        outline.len() - 1
    };
    (0..count)
        .map(|i| (outline[i], outline[(i + 1) % outline.len()]))
        .collect()
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
/// fill to leave out. Text, freehand strokes and images are their own content: their
/// "background" being transparent says nothing about whether their middle is clickable,
/// and treating a line of text as an outline would make it unselectable except at its
/// edges.
///
/// A **frame** is the exception in the other direction, and has to be stated rather than
/// left to the default. It is a boundary you reach through: its middle belongs to
/// whatever is inside it, so it is grabbed by its border. Counted as solid — which is
/// what the fall-through did, since a frame is not one of the three fillable shapes — a
/// frame swallows every click that lands in it, and the moment you framed a diagram its
/// contents became unselectable.
///
/// **Known divergence.** Excalidraw also treats a shape as solid when it carries bound
/// text, via `hasBoundTextElement`. That needs the container's `boundElements`
/// back-reference, which this schema does not have yet — the link only runs the other
/// way, from the label's `container_id`. So a *transparent* shape with a label is hollow
/// here and solid there. Its label is still clickable, so the shape is still reachable.
fn has_solid_interior(element: &DrawElement) -> bool {
    if element.kind == DrawElementType::Frame {
        return false;
    }
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

/// How close a path's ends must be for it to read as closed.
///
/// Excalidraw's `LINE_CONFIRM_THRESHOLD` (`packages/common/src/constants.ts:23`). It lives
/// here, in the geometry, because three separate questions turn on it and they have to
/// give the same answer: whether the renderer paints a path's background, whether a fill
/// treats a stroke as a wall, and whether a click in the middle of a path belongs to it.
/// They used to be three constants in three files, which is how paint came to exist that
/// could not be clicked.
pub const LINE_CONFIRM_THRESHOLD: f64 = 8.0;

/// Whether a path's ends are close enough that it reads — and paints — as closed.
///
/// `isPathALoop`, `packages/element/src/utils.ts:511-525`. Asks about a path that already
/// exists, where the threshold is a plain world distance.
pub fn is_path_a_loop(points: &[[f64; 2]]) -> bool {
    is_path_a_loop_within(points, LINE_CONFIRM_THRESHOLD)
}

/// [`is_path_a_loop`] against a caller-supplied tolerance.
///
/// The tolerance is a parameter for exactly one caller: a path still being placed, where
/// Excalidraw divides the threshold by the zoom so that closing a loop by hand is equally
/// easy at any magnification — the question there is how accurately a hand can aim, which
/// is a fact about the screen. Every other caller is asking about a finished path and
/// wants the static form above.
///
/// The **three-point minimum** is shared and load-bearing. Without it the second point of
/// every path closes a loop, because the second point is necessarily placed near the first
/// while the segment between them is still being aimed — so no path could get past two
/// points.
pub fn is_path_a_loop_within(points: &[[f64; 2]], tolerance: f64) -> bool {
    if points.len() < 3 {
        return false;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    (first[0] - last[0]).hypot(first[1] - last[1]) <= tolerance
}

/// Whether a point-based element encloses a region that belongs to it.
///
/// `shouldTestInside`, `packages/element/src/collision.ts:82-102`: a line is grabbable
/// from the inside when it paints a background *and* its path is a loop, and an arrow
/// never is — it points at something, so its middle is not a region however closed and
/// however filled it happens to be.
///
/// This is what a bucket fill is made of. The paint it leaves behind is a closed line with
/// a background and no stroke, so without this it is hit only along its own edge — which
/// is exactly where the outline it was traced from is already answering for the same
/// click. The paint was visible and inert: it could not be selected, moved, recoloured or
/// deleted.
fn encloses_its_interior(element: &DrawElement) -> bool {
    if matches!(element.kind, DrawElementType::Arrow) {
        return false;
    }
    if is_transparent(&element.background_color) {
        return false;
    }
    element.points.as_deref().is_some_and(is_path_a_loop)
}

/// Whether the point is inside the ring the element's points trace.
///
/// Non-zero winding, because that is the rule `ctx.fill()` uses and the hit test has to
/// agree with what was actually painted. For the keyholed polygons a bucket fill produces
/// the choice does not matter — holes are spliced with the opposite winding precisely so
/// that both rules agree — but for a path that crosses itself the two disagree, and then
/// the painter is the authority.
fn interior_contains(element: &DrawElement, wx: f64, wy: f64) -> bool {
    let Some(points) = element.points.as_deref() else {
        return false;
    };
    let ring: Vec<Point> = points
        .iter()
        .map(|p| Point {
            x: element.x + p[0],
            y: element.y + p[1],
        })
        .collect();
    polygon_includes_point_non_zero(Point { x: wx, y: wy }, &ring)
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
        // The path itself first: it is the cheaper test, and it answers for the open
        // paths that are most of what these two kinds are.
        if hit_linear(element, wx, wy, tolerance) {
            return true;
        }
        return encloses_its_interior(element) && interior_contains(element, wx, wy);
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

// -----------------------------------------------------------------------------
// polygon primitives
//
// Excalidraw's `packages/math/src/polygon.ts`, transcribed. These are the pieces the
// bucket fill decides on, and two of them are decided by *sign*, so the winding
// convention has to survive the port exactly — see `polygon_signed_area`.
// -----------------------------------------------------------------------------

/// Excalidraw's `PRECISION`: the tolerance at which two points count as the same.
pub const PRECISION: f64 = 10e-5;

/// Whether the ring already repeats its first vertex as its last.
pub fn polygon_is_closed(polygon: &[Point], tolerance: f64) -> bool {
    match (polygon.first(), polygon.last()) {
        (Some(first), Some(last)) => {
            (first.x - last.x).abs() <= tolerance && (first.y - last.y).abs() <= tolerance
        }
        _ => false,
    }
}

/// The signed area of a polygon by the shoelace formula.
///
/// **Positive when the vertices wind counter-clockwise in a y-down system.** The bucket
/// fill separates bounded faces from the outside contour by the sign of this and nothing
/// else, so flipping the convention here does not make fills slightly wrong — it makes
/// the tool select the outside of every region instead of the inside.
///
/// Accepts the ring open or closed; a repeated closing vertex is dropped first so it
/// cannot contribute a zero-width term.
pub fn polygon_signed_area(polygon: &[Point], tolerance: f64) -> f64 {
    let pts = if polygon_is_closed(polygon, tolerance) {
        &polygon[..polygon.len() - 1]
    } else {
        polygon
    };
    if pts.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        sum += pts[j].x * pts[i].y - pts[i].x * pts[j].y;
        j = i;
    }
    sum / 2.0
}

/// The unsigned area of a polygon.
///
/// Wraps modulo rather than stripping a closing vertex, because a repeated vertex
/// contributes a zero term either way — so this accepts a ring open or closed and answers
/// the same for both.
pub fn polygon_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    (sum / 2.0).abs()
}

/// Even-odd containment, which is the rule the renderer fills with.
///
/// This is the one to ask when the question is "does the paint cover this point", because
/// it answers *no* inside a keyhole's hole — matching what is actually painted.
pub fn polygon_includes_point(point: Point, polygon: &[Point]) -> bool {
    if polygon.is_empty() {
        return false;
    }
    let (x, y) = (point.x, point.y);
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (xi, yi) = (polygon[i].x, polygon[i].y);
        let (xj, yj) = (polygon[j].x, polygon[j].y);
        if ((yi > y && yj <= y) || (yi <= y && yj > y)) && x < (xj - xi) * (y - yi) / (yj - yi) + xi
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Non-zero winding containment.
///
/// Used for face selection, where the ring is a simple cell of the arrangement and the
/// two rules agree — but a winding test is insensitive to which way the face was walked,
/// and the walk hands back bounded and unbounded faces in opposite orientations.
pub fn polygon_includes_point_non_zero(point: Point, polygon: &[Point]) -> bool {
    let (x, y) = (point.x, point.y);
    let mut winding = 0i32;
    for i in 0..polygon.len() {
        let j = (i + 1) % polygon.len();
        let (xi, yi) = (polygon[i].x, polygon[i].y);
        let (xj, yj) = (polygon[j].x, polygon[j].y);
        if yi <= y {
            if yj > y && (xj - xi) * (y - yi) - (x - xi) * (yj - yi) > 0.0 {
                winding += 1;
            }
        } else if yj <= y && (xj - xi) * (y - yi) - (x - xi) * (yj - yi) < 0.0 {
            winding -= 1;
        }
    }
    winding != 0
}

/// Where two segments cross, if they do.
///
/// Excalidraw's `lineSegmentIntersectionPoints`: intersect the infinite lines, then keep
/// the point only if it lies on *both* segments within `threshold`. Parallel lines give
/// `None`, including collinear overlapping ones — those are handled instead by the
/// T-junction pass, which finds the endpoint that necessarily lies on the other segment.
pub fn segment_intersection_point(
    a: (Point, Point),
    b: (Point, Point),
    threshold: f64,
) -> Option<Point> {
    let a1 = a.1.y - a.0.y;
    let b1 = a.0.x - a.1.x;
    let a2 = b.1.y - b.0.y;
    let b2 = b.0.x - b.1.x;
    let d = a1 * b2 - a2 * b1;
    if d == 0.0 {
        return None;
    }
    let c1 = a1 * a.0.x + b1 * a.0.y;
    let c2 = a2 * b.0.x + b2 * b.0.y;
    let candidate = Point {
        x: (c1 * b2 - c2 * b1) / d,
        y: (a1 * c2 - a2 * c1) / d,
    };
    let on = |seg: (Point, Point)| {
        let distance =
            distance_to_segment(candidate.x, candidate.y, seg.0.x, seg.0.y, seg.1.x, seg.1.y);
        distance == 0.0 || distance < threshold
    };
    if on(a) && on(b) {
        Some(candidate)
    } else {
        None
    }
}

/// Whether the swept segment `a -> b` touches `element`.
///
/// The eraser's question, and the reason it is a *segment* rather than a point: pointer
/// moves are coalesced to one per animation frame, so a quick drag across the board
/// arrives as a handful of samples tens of pixels apart. Asking about the samples steps
/// straight over everything between them, which is what made the eraser feel like it had
/// to be swept again and again over the same place.
///
/// Exact rather than sampled. Sampling the segment would reintroduce the same gap at a
/// smaller scale, and the step needed to close it properly would be a hit test every few
/// pixels of a drag — the cost of which is paid on every frame of every sweep.
///
/// The fill rule is the same one the rest of hit-testing uses, because it comes from the
/// same place: `hit_test_element` on the endpoints covers a click and a sweep that begins
/// or ends inside a filled shape, and the edge crossings cover passing through. A hollow
/// rectangle is therefore erased by its outline and not by its empty middle, exactly as
/// it is selected by its outline and not by its middle.
pub fn segment_hits_element(element: &DrawElement, a: Point, b: Point, tolerance: f64) -> bool {
    // Boxes first. The exact tests below build an outline — a fresh allocation per
    // element — and a sweep asks this of every element on the board on every frame, so
    // almost all of that work is for elements the segment comes nowhere near.
    if !segment_box_overlaps(a, b, element, tolerance) {
        return false;
    }
    if hit_test_element(element, a.x, a.y, tolerance)
        || hit_test_element(element, b.x, b.y, tolerance)
    {
        return true;
    }
    // A zero-length sweep is a click, and the endpoints above have already answered it.
    if (b.x - a.x).abs() < f64::EPSILON && (b.y - a.y).abs() < f64::EPSILON {
        return false;
    }
    let outline = element_outline(element);
    outline_edges(&outline, outline_is_closed(element))
        .into_iter()
        .any(|(from, to)| segments_intersect(a, b, from, to))
}

/// Whether the segment's bounding box overlaps the element's, allowing `tolerance`.
///
/// A cheap reject, and only a reject: two boxes overlapping says nothing about whether
/// the segment touches the shape. It is worth having because it is arithmetic on six
/// numbers where the alternative allocates an outline.
fn segment_box_overlaps(a: Point, b: Point, element: &DrawElement, tolerance: f64) -> bool {
    let bounds = element_rotated_bounds(element);
    a.x.min(b.x) <= bounds.max_x + tolerance
        && a.x.max(b.x) >= bounds.min_x - tolerance
        && a.y.min(b.y) <= bounds.max_y + tolerance
        && a.y.max(b.y) >= bounds.min_y - tolerance
}
