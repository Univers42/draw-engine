//! How an arrow attaches to the shapes at its ends, and where its ends are drawn.
//!
//! The model is Excalidraw's "simple" binding strategy at the pinned oracle
//! (`packages/element/src/binding.ts`). An end bound to a shape stores an [`Anchor`]: the
//! shape's id, a point on the shape as a ratio of its own **unrotated** width and height
//! (Excalidraw's `fixedPoint`), and a [`BindMode`]:
//!
//! - [`BindMode::Inside`] — the end sits exactly on the anchor. What you get by letting go
//!   anywhere inside a shape, or anywhere near it with Alt held.
//! - [`BindMode::Orbit`] — the end aims at the anchor but stops a gap clear of the outline,
//!   on the side facing where the arrow comes from. What you get by letting go near a
//!   shape's edge from outside it.
//!
//! Because the anchor lives in the shape's own frame, it turns with the shape: rotating a
//! shape carries every arrow end with it instead of leaving them at a world offset. And
//! because it is a point the person chose, the arrow arrives where they put it rather
//! than wherever a line between the two centres happens to cross.
//!
//! An arrow bound before anchors existed has no anchor fields; it reads as the centre in
//! orbit, which is exactly how every such arrow was always drawn.

use crate::camera::{Point, WorldBounds};
use crate::scene::element::{BindMode, DrawElement, DrawElementType};
use crate::scene::geometry::{
    element_rotated_bounds, is_transparent, normalize_rect, rotation_center, to_element_local,
    within_shape,
};
use crate::scene::outline_distance::signed_outline_distance;

/// Excalidraw's `BASE_BINDING_GAP` (`packages/element/src/binding.ts@1118751f:116`): the air
/// between an orbiting end and the outline, before half the target's stroke is added to it.
pub const BASE_BINDING_GAP: f64 = 5.0;
/// The gap in front of a shape with the default 2px stroke. See [`binding_gap`].
pub const BINDING_GAP: f64 = 6.0;
/// The air between a label and its shape's outline: Excalidraw's `BOUND_TEXT_PADDING`.
pub const LABEL_PADDING: f64 = crate::text::layout::BOUND_TEXT_PADDING;

/// How much of a shape's shorter side the gap may take.
///
/// Excalidraw's gap is absolute: 5 + half the stroke, in scene units, for every shape. A
/// shape drawn deep inside a zoomed-in presentation can be a single unit across, and a
/// six-unit gap left its arrows floating six shapes away from it. Capping the gap to a
/// quarter of the shape keeps a binding looking the same at every depth; for anything
/// larger than 24 units — every shape drawn at ordinary zoom — the cap never applies and
/// the gap is exactly Excalidraw's.
const GAP_SHARE_OF_SIDE: f64 = 0.25;

/// Excalidraw's `FIXED_POINT_BOUND` (`binding.ts@1118751f:2724`). An anchor can sit outside its
/// shape — an orbit anchor projected from a drop beside it — but not unboundedly far.
const FIXED_POINT_BOUND: f64 = 10.0;

/// Excalidraw insets a rectangle's diagonals by 15 at each end before projecting a drop
/// onto them (`packages/element/src/utils.ts@1118751f:553-562`), so an anchor never lands in a
/// corner. Capped to a share of the diagonal here for the same reason as the gap.
const DIAGONAL_INSET: f64 = 15.0;
const DIAGONAL_INSET_SHARE: f64 = 0.1;

/// How much of a shape's shorter side the midpoint snap may reach across. See
/// [`midpoint_snap_radius`].
const MIDPOINT_SNAP_SHARE_OF_SIDE: f64 = 0.25;

/// An arrow narrower and shorter than this, in screen pixels, has no direction yet — it is
/// the press that starts one — so there is no line to project its end along. Excalidraw's
/// is 3 scene units (`packages/element/src/utils.ts@1118751f:834-836`); in pixels here so it means
/// the same thing at every zoom. Excalidraw's check also turns the midpoint snap off for
/// the press, while its hover still shows the midpoint dot — a promise the press then
/// breaks. Here the snap does not need a direction, and the dot is kept.
const DEGENERATE_ARROW_PX: f64 = 3.0;

pub fn is_linear_element(element: &DrawElement) -> bool {
    matches!(element.kind, DrawElementType::Line | DrawElementType::Arrow)
}

/// Whether this element attaches itself to the shapes its ends meet.
///
/// **An arrow, and nothing else.** Excalidraw's `isBindingElementType` is the same single
/// comparison (`packages/element/src/typeChecks.ts@1118751f:178-182`), and the distinction is the
/// whole difference between the two linear kinds: an arrow *means* a relationship between
/// two things, so following them about is the point of it. A line is a line. Binding one
/// meant a line drawn across a diagram silently attached itself to whatever its ends
/// happened to pass over, and then moved on its own whenever those shapes did — which
/// reads as the drawing coming apart by itself.
pub fn is_binding_element(element: &DrawElement) -> bool {
    matches!(element.kind, DrawElementType::Arrow)
}

/// Whether this element can hold a label, and so be found by [`bindable_at`].
///
/// Narrower than what an arrow can attach to (`is_target_kind`): an arrow can attach to
/// a picture or a line of text, but neither takes a label.
pub fn is_bindable_element(element: &DrawElement) -> bool {
    matches!(
        element.kind,
        DrawElementType::Rectangle
            | DrawElementType::StickyNote
            | DrawElementType::Diamond
            | DrawElementType::Ellipse
    )
}

/// Whether an arrow end can attach to this kind of element.
///
/// Excalidraw's `isBindableElement(element, false)` (`typeChecks.ts@1118751f:184-202`): the three
/// shapes, pictures, embeds, frames and free-standing text — not a label, which belongs
/// to its container, and never a line or arrow. A locked one is not a target, but it
/// still stands in the way of what is behind it, so the lock is [`arrow_target_among`]'s
/// to judge.
fn is_target_kind(element: &DrawElement) -> bool {
    match element.kind {
        DrawElementType::Rectangle
        | DrawElementType::StickyNote
        | DrawElementType::Diamond
        | DrawElementType::Ellipse
        | DrawElementType::Image
        | DrawElementType::Embed
        | DrawElementType::Frame => true,
        DrawElementType::Text => element.container_id.is_none(),
        _ => false,
    }
}

/// Whether a target hides whatever is beneath it from an arrow end.
///
/// `isOpaqueForBinding` (`packages/element/src/collision.ts@1118751f:346-348`): a picture,
/// or a shape with a background. An arrow dropped on one binds to it or to something drawn
/// on top of it, never to a shape it cannot see.
fn occludes(element: &DrawElement) -> bool {
    match element.kind {
        // A note's paper is never transparent.
        DrawElementType::Image | DrawElementType::StickyNote => true,
        DrawElementType::Rectangle
        | DrawElementType::Diamond
        | DrawElementType::Ellipse
        | DrawElementType::Embed => !is_transparent(&element.background_color),
        _ => false,
    }
}

/// How far outside a shape an arrow end still binds to it, in world units.
///
/// Excalidraw's `maxBindingDistance_simple` (`packages/element/src/binding.ts@1118751f:
/// 133-143`): 15 at zoom 1 and above, growing as the view zooms out — damped, so the reach
/// stays close to the binding gap — to at most 30.
pub fn max_binding_distance(zoom: f64) -> f64 {
    const BASE: f64 = 15.0;
    let zoom = if zoom > 0.0 && zoom < 1.0 { zoom } else { 1.0 };
    (BASE / (zoom * 1.5)).clamp(BASE, BASE * 2.0)
}

pub fn element_center(element: &DrawElement) -> Point {
    let rect = normalize_rect(element.x, element.y, element.width, element.height);
    Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }
}

/// Where a ray from `shape`'s centre toward `toward` leaves it, `gap` further on.
///
/// The pre-anchor placement, kept for callers that want exactly that. Binding itself no
/// longer uses it: it ignores rotation and can only aim through the centre.
pub fn attach_point(shape: &DrawElement, toward: Point, gap: f64) -> Point {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let rx = (rect.width / 2.0).max(0.5);
    let ry = (rect.height / 2.0).max(0.5);
    let mut dx = toward.x - cx;
    let mut dy = toward.y - cy;
    let length = dx.hypot(dy);
    if length < 1e-6 {
        return Point { x: cx, y: cy };
    }
    dx /= length;
    dy /= length;
    let t = match shape.kind {
        DrawElementType::Ellipse => 1.0 / (dx / rx).hypot(dy / ry),
        DrawElementType::Diamond => 1.0 / (dx.abs() / rx + dy.abs() / ry),
        _ => {
            let tx = if dx.abs() < 1e-6 {
                f64::INFINITY
            } else {
                rx / dx.abs()
            };
            let ty = if dy.abs() < 1e-6 {
                f64::INFINITY
            } else {
                ry / dy.abs()
            };
            tx.min(ty)
        }
    };
    let reach = t + gap;
    Point {
        x: cx + dx * reach,
        y: cy + dy * reach,
    }
}

// ------------------------------------------------------------------------------ anchors

/// Which end of an arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Start,
    End,
}

impl End {
    pub fn other(self) -> Self {
        match self {
            End::Start => End::End,
            End::End => End::Start,
        }
    }
}

/// What one end of an arrow is bound to, and where on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub element_id: String,
    /// A ratio of the shape's own unrotated width and height; `[0.5, 0.5]` is its centre.
    pub fixed_point: [f64; 2],
    pub mode: BindMode,
}

/// Where every binding made before anchors existed was aimed.
pub const LEGACY_FIXED_POINT: [f64; 2] = [0.5, 0.5];

/// The binding at one end of `arrow`, with an older binding's missing fields filled in.
pub fn anchor(arrow: &DrawElement, end: End) -> Option<Anchor> {
    let (id, fixed_point, mode) = match end {
        End::Start => (
            &arrow.start_binding,
            arrow.start_fixed_point,
            arrow.start_bind_mode,
        ),
        End::End => (
            &arrow.end_binding,
            arrow.end_fixed_point,
            arrow.end_bind_mode,
        ),
    };
    Some(Anchor {
        element_id: id.clone()?,
        fixed_point: clean_fixed_point(fixed_point.unwrap_or(LEGACY_FIXED_POINT)),
        mode: mode.unwrap_or(BindMode::Orbit),
    })
}

/// Binds or releases one end of `arrow`.
///
/// The only place the three binding fields are written together, so a released end can
/// never keep an anchor that belonged to a shape it no longer points at.
pub fn set_anchor(arrow: &mut DrawElement, end: End, anchor: Option<Anchor>) {
    let (id, fixed_point, mode) = match anchor {
        Some(a) => (Some(a.element_id), Some(a.fixed_point), Some(a.mode)),
        None => (None, None, None),
    };
    match end {
        End::Start => {
            arrow.start_binding = id;
            arrow.start_fixed_point = fixed_point;
            arrow.start_bind_mode = mode;
        }
        End::End => {
            arrow.end_binding = id;
            arrow.end_fixed_point = fixed_point;
            arrow.end_bind_mode = mode;
        }
    }
}

/// A stored anchor made safe to use: a document is data from anywhere, and a NaN here
/// would put an arrow end nowhere.
fn clean_fixed_point(fixed: [f64; 2]) -> [f64; 2] {
    fixed.map(|r| {
        if r.is_finite() {
            r.clamp(-FIXED_POINT_BOUND, FIXED_POINT_BOUND)
        } else {
            0.5
        }
    })
}

fn rotate_about(p: Point, centre: Point, angle: f64) -> Point {
    if angle == 0.0 {
        return p;
    }
    let (sin, cos) = angle.sin_cos();
    let (dx, dy) = (p.x - centre.x, p.y - centre.y);
    Point {
        x: centre.x + dx * cos - dy * sin,
        y: centre.y + dx * sin + dy * cos,
    }
}

/// The anchor for a world point on `shape`: the point un-turned into the shape's own frame
/// and expressed as a ratio of its size. `calculateFixedPointForNonElbowArrowBinding`
/// (`binding.ts@1118751f:2172-2223`).
pub fn fixed_point_at(shape: &DrawElement, world: Point) -> [f64; 2] {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    // A shape with no extent on an axis has nothing to take a ratio of. Excalidraw's
    // floor is one scene unit; here it is only "not zero", because a shape a fraction of
    // a unit across is an ordinary thing to draw deep inside a zoomed presentation.
    if rect.width <= f64::EPSILON || rect.height <= f64::EPSILON {
        return LEGACY_FIXED_POINT;
    }
    let (lx, ly) = to_element_local(shape, world.x, world.y);
    clean_fixed_point([(lx - rect.x) / rect.width, (ly - rect.y) / rect.height])
}

/// Where an anchor is on the board now: its point in the shape's frame, turned with the
/// shape. `getGlobalFixedPointForBindableElement` (`binding.ts@1118751f:2646-2661`).
pub fn focus_point(shape: &DrawElement, fixed_point: [f64; 2]) -> Point {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    let [fx, fy] = clean_fixed_point(fixed_point);
    rotate_about(
        Point {
            x: rect.x + rect.width * fx,
            y: rect.y + rect.height * fy,
        },
        rotation_center(shape),
        shape.angle,
    )
}

// ---------------------------------------------------------------------------- geometry

/// The gap an orbiting end keeps from `shape`'s outline.
///
/// Excalidraw's `getBindingGap` (`binding.ts@1118751f:125-131`) — 5 plus half the target's stroke —
/// capped by [`GAP_SHARE_OF_SIDE`] so it shrinks with the shape instead of dwarfing it.
pub fn binding_gap(shape: &DrawElement) -> f64 {
    let base = BASE_BINDING_GAP + shape.stroke_width.max(0.0) / 2.0;
    let side = shape.width.abs().min(shape.height.abs());
    base.min(side * GAP_SHARE_OF_SIDE)
}

/// How far from one of `shape`'s side midpoints a drop still snaps onto it.
///
/// `radius` is the host's reach in world units; it is capped to a share of the shape so
/// a small shape does not snap from everywhere, which would take the choice of where an
/// arrow lands away from the person drawing it.
pub fn midpoint_snap_radius(shape: &DrawElement, radius: f64) -> f64 {
    let side = shape.width.abs().min(shape.height.abs());
    radius.min(side * MIDPOINT_SNAP_SHARE_OF_SIDE).max(0.0)
}

/// The four points an orbiting end snaps to: the middle of each side, turned with the
/// shape — right, bottom, left, top. For a diamond these are its vertices.
/// `getSnapOutlineMidPoint` (`packages/element/src/utils.ts@1118751f:788-808`).
pub fn side_midpoints(shape: &DrawElement) -> [Point; 4] {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    let c = rotation_center(shape);
    let at = |x: f64, y: f64| rotate_about(Point { x, y }, c, shape.angle);
    [
        at(rect.x + rect.width, rect.y + rect.height / 2.0),
        at(rect.x + rect.width / 2.0, rect.y + rect.height),
        at(rect.x, rect.y + rect.height / 2.0),
        at(rect.x + rect.width / 2.0, rect.y),
    ]
}

/// Whether `p` is inside `shape` itself, filled or not — the test that decides between an
/// end bound [`BindMode::Inside`] and one in orbit. `isPointInElement`
/// (`packages/element/src/collision.ts@1118751f:823-878`): the painted outline, so a
/// rounded corner's cut-away is outside, and so is a point exactly on the outline.
pub fn is_inside(shape: &DrawElement, p: Point) -> bool {
    signed_outline_distance(shape, p) > 0.0
}

fn contains(shape: &DrawElement, p: Point, grow: f64) -> bool {
    let (lx, ly) = to_element_local(shape, p.x, p.y);
    within_shape(shape, lx, ly, grow)
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

/// The parameter interval over which the line `a + t·d` lies inside an axis-aligned box
/// of half-extents `(hx, hy)` centred on the origin — the slab method.
fn box_interval(a: (f64, f64), d: (f64, f64), hx: f64, hy: f64) -> Option<(f64, f64)> {
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    for (p, v, h) in [(a.0, d.0, hx), (a.1, d.1, hy)] {
        if v.abs() < 1e-300 {
            if p.abs() > h {
                return None;
            }
            continue;
        }
        let (t1, t2) = ((-h - p) / v, (h - p) / v);
        lo = lo.max(t1.min(t2));
        hi = hi.min(t1.max(t2));
    }
    (lo <= hi).then_some((lo, hi))
}

/// The parameter interval over which the line `a + t·d` lies inside the ellipse with
/// semi-axes `(rx, ry)` centred on `c`.
fn ellipse_interval(
    a: (f64, f64),
    d: (f64, f64),
    c: (f64, f64),
    rx: f64,
    ry: f64,
) -> Option<(f64, f64)> {
    let (rx, ry) = (rx.max(1e-300), ry.max(1e-300));
    let (px, py) = ((a.0 - c.0) / rx, (a.1 - c.1) / ry);
    let (vx, vy) = (d.0 / rx, d.1 / ry);
    let qa = vx * vx + vy * vy;
    if qa < 1e-300 {
        return (px * px + py * py <= 1.0).then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let qb = 2.0 * (px * vx + py * vy);
    let qc = px * px + py * py - 1.0;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    Some(((-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)))
}

/// The parameter interval over which the line through `a` and `b` lies inside `shape`'s
/// outline pushed out by `gap`, worked in the shape's own unrotated frame.
///
/// Every target is convex, so a line meets its outline at most twice and the inside is one
/// interval. The outline pushed out by the gap is Excalidraw's too
/// (`intersectElementWithLineSegment` with an offset, `collision.ts@1118751f:675-799`): a box grows
/// into a rounded box, so an arrow arriving at a corner keeps the same gap as one arriving
/// square on.
fn outline_interval(shape: &DrawElement, a: Point, b: Point, gap: f64) -> Option<(f64, f64)> {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (ax, ay) = to_element_local(shape, a.x, a.y);
    let (bx, by) = to_element_local(shape, b.x, b.y);
    let p = (ax - cx, ay - cy);
    let d = (bx - ax, by - ay);
    let (hx, hy) = (rect.width / 2.0, rect.height / 2.0);
    let gap = gap.max(0.0);

    match shape.kind {
        DrawElementType::Ellipse => ellipse_interval(p, d, (0.0, 0.0), hx + gap, hy + gap),
        DrawElementType::Diamond => {
            // |x|/hx + |y|/hy <= 1 pushed out by `gap` is the same diamond with its four
            // sides moved out along their normals: |x|/hx + |y|/hy <= 1 + gap·|n|.
            let (hx, hy) = (hx.max(1e-300), hy.max(1e-300));
            let k = 1.0 + gap * (1.0 / (hx * hx) + 1.0 / (hy * hy)).sqrt();
            let mut lo = f64::NEG_INFINITY;
            let mut hi = f64::INFINITY;
            for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                let (nx, ny) = (sx / hx, sy / hy);
                let at = nx * p.0 + ny * p.1;
                let along = nx * d.0 + ny * d.1;
                if along.abs() < 1e-300 {
                    if at > k {
                        return None;
                    }
                    continue;
                }
                let t = (k - at) / along;
                if along > 0.0 {
                    hi = hi.min(t);
                } else {
                    lo = lo.max(t);
                }
            }
            (lo <= hi).then_some((lo, hi))
        }
        _ => {
            // A (possibly rounded) box pushed out by the gap is a box with corners of
            // radius `r + gap` round an inner box: the union of two crossed boxes and four
            // corner discs. The union is convex, so the line's interval through it runs
            // from the earliest entry into any piece to the latest exit from any.
            let r = crate::render::shape::corner_radius(rect.width.min(rect.height), shape)
                .clamp(0.0, hx.min(hy));
            let (ix, iy) = (hx - r, hy - r);
            let reach = r + gap;
            let mut pieces = vec![
                box_interval(p, d, ix + reach, iy),
                box_interval(p, d, ix, iy + reach),
            ];
            if reach > 0.0 {
                for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                    pieces.push(ellipse_interval(p, d, (sx * ix, sy * iy), reach, reach));
                }
            }
            pieces
                .into_iter()
                .flatten()
                .fold(None, |acc: Option<(f64, f64)>, (lo, hi)| match acc {
                    None => Some((lo, hi)),
                    Some((a, b)) => Some((a.min(lo), b.max(hi))),
                })
        }
    }
}

/// Where the segment `a → b` crosses `shape`'s outline pushed out by `gap`.
fn outline_crossings(shape: &DrawElement, a: Point, b: Point, gap: f64) -> Vec<Point> {
    if distance(a, b) <= f64::EPSILON * (a.x.abs() + a.y.abs() + 1.0) {
        return Vec::new();
    }
    let Some((lo, hi)) = outline_interval(shape, a, b, gap) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(2);
    for t in [lo, hi] {
        if (0.0..=1.0).contains(&t) {
            out.push(lerp(a, b, t));
        }
    }
    out
}

fn nearest_to(points: Vec<Point>, to: Point) -> Option<Point> {
    points
        .into_iter()
        .min_by(|p, q| distance(*p, to).total_cmp(&distance(*q, to)))
}

/// The two lines an orbit anchor is projected onto: a rectangle's diagonals, inset from
/// the corners, or the centre lines of anything else. `getDiagonalsForBindableElement`
/// (`packages/element/src/utils.ts@1118751f:550-633`).
fn projection_lines(shape: &DrawElement) -> [(Point, Point); 2] {
    let rect = normalize_rect(shape.x, shape.y, shape.width, shape.height);
    let c = rotation_center(shape);
    let at = |x: f64, y: f64| rotate_about(Point { x, y }, c, shape.angle);
    let (l, t, r, b) = (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height);
    let (mx, my) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let rectangular = !matches!(
        shape.kind,
        DrawElementType::Ellipse | DrawElementType::Diamond
    );
    let lines = if rectangular {
        [(at(l, t), at(r, b)), (at(r, t), at(l, b))]
    } else {
        [(at(mx, t), at(mx, b)), (at(l, my), at(r, my))]
    };
    if !shape.kind.is_rect_like() {
        return lines;
    }
    lines.map(|(p, q)| {
        let len = distance(p, q);
        if len <= f64::EPSILON {
            return (p, q);
        }
        let inset = DIAGONAL_INSET.min(len * DIAGONAL_INSET_SHARE) / len;
        (lerp(p, q, inset), lerp(p, q, 1.0 - inset))
    })
}

/// Where the ray from `from` through `toward` first crosses the segment `p → q`, as the
/// distance along the ray and the point.
fn ray_hits_segment(from: Point, toward: Point, p: Point, q: Point) -> Option<(f64, Point)> {
    let (dx, dy) = (toward.x - from.x, toward.y - from.y);
    let (ex, ey) = (q.x - p.x, q.y - p.y);
    let denom = dx * ey - dy * ex;
    if denom.abs() < 1e-300 {
        return None;
    }
    let (wx, wy) = (p.x - from.x, p.y - from.y);
    let t = (wx * ey - wy * ex) / denom;
    let u = (wx * dy - wy * dx) / denom;
    (t >= 0.0 && (0.0..=1.0).contains(&u)).then_some((
        t,
        Point {
            x: from.x + dx * t,
            y: from.y + dy * t,
        },
    ))
}

// ------------------------------------------------------------------------- choosing

/// The shape an arrow end at `(x, y)` would attach to: the one whose **outline** is
/// nearest.
///
/// A transcription of Excalidraw's `getBindingCandidates` and
/// `getHoveredElementForBinding` (`packages/element/src/collision.ts@1118751f:350-486`,
/// #10753 4850bf33 and dc2c16d9):
///
/// - candidates are walked top of the z-order first; each is measured by its signed
///   distance to its real outline ([`signed_outline_distance`]: positive inside, negative
///   outside), and one the point is outside of counts only within `tolerance`;
/// - a frame is bound only from outside, near its border, so a point inside a frame is
///   aimed at what the frame holds; a shape inside a frame is skipped where the frame
///   clips it from view (`isPointClippedByEnclosingFrame`, `collision.ts@1118751f:283-298`);
/// - the walk stops at the first opaque shape — filled, or a picture — the point is
///   inside, so nothing hidden under it can be bound through it. A locked shape is never
///   a candidate, but an opaque one still hides what is behind it;
/// - the nearest outline wins, ties going to the one on top. When the point is inside the
///   winner, a smaller shape the point is also inside — overlapping the winner by more
///   than a quarter of its own area and under three quarters of the winner's size — takes
///   over, so a shape nested in another is reached from anywhere inside it.
///
/// So in a pack of overlapping squares a point just outside one square's edge binds that
/// square in orbit, however many other squares it is inside.
///
/// `candidates` must run top of the z-order first.
pub fn arrow_target_among<'a>(
    candidates: impl Iterator<Item = &'a DrawElement>,
    lookup: &dyn Fn(&str) -> Option<&'a DrawElement>,
    x: f64,
    y: f64,
    tolerance: f64,
    exclude_id: Option<&str>,
) -> Option<&'a DrawElement> {
    let p = Point { x, y };
    let clipped = |element: &DrawElement| {
        element
            .frame_id
            .as_deref()
            .and_then(lookup)
            .filter(|frame| frame.kind == DrawElementType::Frame && !frame.is_deleted)
            .is_some_and(|frame| {
                let b = crate::scene::geometry::element_bounds(frame);
                !(b.min_x..=b.max_x).contains(&x) || !(b.min_y..=b.max_y).contains(&y)
            })
    };
    // The cheap reject first, as the oracle does: the bounds grown by the reach.
    let reach = tolerance.max(1.0);
    // (element, signed distance to its outline), top first.
    let mut found: Vec<(&'a DrawElement, f64)> = Vec::new();
    for element in candidates {
        if element.is_deleted || Some(element.id.as_str()) == exclude_id || !is_target_kind(element)
        {
            continue;
        }
        let b = element_rotated_bounds(element);
        if x < b.min_x - reach || x > b.max_x + reach || y < b.min_y - reach || y > b.max_y + reach
        {
            continue;
        }
        if clipped(element) {
            continue;
        }
        let d = signed_outline_distance(element, p);
        let inside = d > 0.0;
        if (inside && element.kind == DrawElementType::Frame) || (!inside && -d >= tolerance) {
            continue;
        }
        if element.locked != Some(true) {
            found.push((element, d));
        }
        // `d >= 0.0` holds on the outline too (`-0.0`), exactly as the oracle's does.
        if d >= 0.0 && occludes(element) {
            break;
        }
    }
    // Stable: equal distances keep the top of the stack first.
    found.sort_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
    let &(nearest, distance) = found.first()?;
    if distance >= 0.0 {
        let area = |b: &WorldBounds| ((b.max_x - b.min_x) * (b.max_y - b.min_y)).max(1e-5);
        let outer = own_bounds(nearest);
        let outer_area = area(&outer);
        let nested = found[1..].iter().find(|(element, d)| {
            if *d < 0.0 {
                return false;
            }
            let b = own_bounds(element);
            let w = (b.max_x.min(outer.max_x) - b.min_x.max(outer.min_x)).max(0.0);
            let h = (b.max_y.min(outer.max_y) - b.min_y.max(outer.min_y)).max(0.0);
            let own = area(&b);
            w * h / own > 0.25 && own / outer_area < 0.75
        });
        if let Some(&(element, _)) = nested {
            return Some(element);
        }
    }
    Some(nearest)
}

/// The box `getElementBounds` gives a shape (`bounds.ts@1118751f:176-209`), which sizes
/// the nested-shape takeover: tight to a turned diamond's corners and a turned ellipse's
/// curve, where [`element_rotated_bounds`] boxes the turned box — up to twice the area.
fn own_bounds(element: &DrawElement) -> WorldBounds {
    let turned_curve = matches!(
        element.kind,
        DrawElementType::Diamond | DrawElementType::Ellipse
    );
    if !turned_curve || element.angle == 0.0 {
        return element_rotated_bounds(element);
    }
    let plain = crate::scene::geometry::element_bounds(element);
    let c = rotation_center(element);
    let (hw, hh) = (
        (plain.max_x - plain.min_x) / 2.0,
        (plain.max_y - plain.min_y) / 2.0,
    );
    let (sin, cos) = element.angle.sin_cos();
    let (ex, ey) = if element.kind == DrawElementType::Diamond {
        // The four corners are the side midpoints of the box, turned.
        (
            (hw * cos).abs().max((hh * sin).abs()),
            (hw * sin).abs().max((hh * cos).abs()),
        )
    } else {
        ((hw * cos).hypot(hh * sin), (hw * sin).hypot(hh * cos))
    };
    WorldBounds {
        min_x: c.x - ex,
        min_y: c.y - ey,
        max_x: c.x + ex,
        max_y: c.y + ey,
    }
}

/// The shape a label placed at `(x, y)` would go into.
///
/// Every match is collected and the smallest bounding-box area wins, z-order only breaking
/// a tie — so a small shape nested inside a larger one is reachable wherever each was
/// drawn. Not fill-aware: an empty rectangle still takes a label placed in its middle.
///
/// Generic over the iterator so the hot paths can walk the scene by reference.
/// `candidates` must already run **top of the z-order first**.
pub fn bindable_among<'a>(
    candidates: impl Iterator<Item = &'a DrawElement>,
    x: f64,
    y: f64,
    tolerance: f64,
    exclude_id: Option<&str>,
) -> Option<&'a DrawElement> {
    let mut best: Option<(&'a DrawElement, f64)> = None;
    for element in candidates {
        if element.is_deleted
            || Some(element.id.as_str()) == exclude_id
            || !is_bindable_element(element)
        {
            continue;
        }
        if !contains(element, Point { x, y }, tolerance) {
            continue;
        }
        let area = element.width.abs() * element.height.abs();
        if best.is_none_or(|(_, best_area)| area < best_area) {
            best = Some((element, area));
        }
    }
    best.map(|(element, _)| element)
}

pub fn bindable_at<'a>(
    elements: &'a [DrawElement],
    x: f64,
    y: f64,
    tolerance: f64,
    exclude_id: Option<&str>,
) -> Option<&'a DrawElement> {
    bindable_among(elements.iter().rev(), x, y, tolerance, exclude_id)
}

/// What the person is doing with the end being placed.
#[derive(Clone, Copy, Debug)]
pub struct EndDrop {
    /// Where the end is being put, in world units.
    pub pointer: Point,
    /// How near a shape counts as on it, in world units.
    pub tolerance: f64,
    /// How near a side midpoint snaps onto it, in world units, before
    /// [`midpoint_snap_radius`] caps it to the shape.
    pub snap: f64,
    /// Alt: bind exactly where the end is, even from outside the shape.
    pub exact: bool,
    /// Shift: the segment is held to an angle, so a midpoint snap would break it.
    pub angle_locked: bool,
    /// The world size of one screen pixel.
    pub pixel: f64,
    /// Grid snapping is on: the end lands on the grid, and a midpoint snap would pull it
    /// off it — Excalidraw's grid mode turns the snap off (`binding.ts@1118751f:885-887`).
    pub grid: bool,
}

/// The side midpoint of `shape` a drop at `pointer` snaps to, if any.
pub fn snapped_midpoint(shape: &DrawElement, pointer: Point, reach: f64) -> Option<Point> {
    let radius = midpoint_snap_radius(shape, reach);
    if radius <= 0.0 {
        return None;
    }
    side_midpoints(shape)
        .into_iter()
        .map(|m| (m, distance(m, pointer)))
        .filter(|(_, d)| *d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(m, _)| m)
}

/// The side midpoint to mark for a pointer at `pointer` near `shape`, and whether a drop
/// there would snap onto it (`true`) or it is only close (`false`, within twice the snap).
/// `renderBindingHighlightForBindableElement_simple`
/// (`packages/excalidraw/renderer/interactiveScene.ts@1118751f:284-322`). Nothing inside the shape:
/// a drop there binds exactly where it is.
pub fn midpoint_mark(shape: &DrawElement, pointer: Point, reach: f64) -> Option<(Point, bool)> {
    if is_inside(shape, pointer) {
        return None;
    }
    let radius = midpoint_snap_radius(shape, reach);
    let (m, d) = side_midpoints(shape)
        .into_iter()
        .map(|m| (m, distance(m, pointer)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    if d <= radius {
        Some((m, true))
    } else if d <= radius * 2.0 {
        Some((m, false))
    } else {
        None
    }
}

/// Where an orbit anchor goes for a drop beside `shape`: where the arrow's own line,
/// continued through the drop, first crosses the shape's projection lines.
///
/// `projectFixedPointOntoDiagonal` (`packages/element/src/utils.ts@1118751f:810-902`). The arrow's
/// line is aimed from the far end's anchor for a straight arrow, or from the neighbouring
/// point of a bent one. The result is the anchor that keeps the end arriving where the
/// person aimed it — along the side it came in on, at the height they chose — instead of
/// at the one point a line from the other shape's centre would pick.
fn projected_anchor<'a>(
    arrow: &DrawElement,
    end: End,
    shape: &DrawElement,
    pointer: Point,
    lookup: &dyn Fn(&str) -> Option<&'a DrawElement>,
) -> Option<Point> {
    let points = crate::selection::linear::world_points(arrow);
    if points.len() < 2 {
        return None;
    }
    let mut from = match end {
        End::Start => points[1],
        End::End => points[points.len() - 2],
    };
    if points.len() == 2 {
        if let Some(other) = anchor(arrow, end.other()) {
            if let Some(other_shape) = lookup(&other.element_id).filter(|s| !s.is_deleted) {
                from = focus_point(other_shape, other.fixed_point);
            }
        }
    }
    if distance(from, pointer) <= f64::EPSILON {
        return None;
    }
    let hit = projection_lines(shape)
        .into_iter()
        .filter_map(|(p, q)| ray_hits_segment(from, pointer, p, q))
        .min_by(|a, b| a.0.total_cmp(&b.0))?
        .1;
    contains(shape, hit, 0.0).then_some(hit)
}

/// What dropping one end of `arrow` at `drop.pointer` binds it to.
///
/// Returns the new binding for that end, and — only when it has to change — the other
/// end's. Excalidraw's `getBindingStrategyForDraggingBindingElementEndpoints_simple`
/// (`binding.ts@1118751f:654-962`):
///
/// - nothing near: unbound;
/// - both ends on the same shape: both sit exactly where they are ([`BindMode::Inside`]),
///   since orbiting a shape from inside it has no side to arrive on;
/// - inside the shape, or anywhere near it with Alt: exactly at the drop;
/// - beside it: in orbit, anchored at the side midpoint it is near, or else where the
///   arrow's line through the drop meets the shape's projection lines (once the arrow has
///   a direction), or else the drop;
/// - with Shift held, the far end's orbit anchor also moves onto the held angle.
pub fn anchor_for_drop<'a>(
    candidates: impl Iterator<Item = &'a DrawElement>,
    lookup: &dyn Fn(&str) -> Option<&'a DrawElement>,
    arrow: &DrawElement,
    end: End,
    drop: &EndDrop,
) -> (Option<Anchor>, Option<Anchor>) {
    let pointer = drop.pointer;
    let Some(hit) = arrow_target_among(
        candidates,
        lookup,
        pointer.x,
        pointer.y,
        drop.tolerance,
        Some(arrow.id.as_str()),
    ) else {
        return (None, None);
    };
    let at = |mode: BindMode, p: Point| Anchor {
        element_id: hit.id.clone(),
        fixed_point: fixed_point_at(hit, p),
        mode,
    };

    if let Some(other) = anchor(arrow, end.other()).filter(|o| o.element_id == hit.id) {
        return (
            Some(at(BindMode::Inside, pointer)),
            Some(Anchor {
                mode: BindMode::Inside,
                ..other
            }),
        );
    }
    if drop.exact || is_inside(hit, pointer) {
        return (Some(at(BindMode::Inside, pointer)), None);
    }
    let reach = DEGENERATE_ARROW_PX * drop.pixel;
    let directed = arrow.width.abs() >= reach || arrow.height.abs() >= reach;
    let focus = (!drop.angle_locked && !drop.grid)
        .then(|| snapped_midpoint(hit, pointer, drop.snap))
        .flatten()
        .or_else(|| {
            directed
                .then(|| projected_anchor(arrow, end, hit, pointer, lookup))
                .flatten()
        })
        .unwrap_or(pointer);
    let other = drop
        .angle_locked
        .then(|| reprojected_other(arrow, end, lookup))
        .flatten();
    (Some(at(BindMode::Orbit, focus)), other)
}

/// The far end's orbit anchor moved onto the arrow's held angle.
///
/// With Shift held the dragged end is placed on an angle from the far end — but an
/// orbiting far end is drawn toward its anchor, not toward where it happens to be, so an
/// anchor off that line bends the arrow off the angle it was held to. Re-projecting it
/// from the far end's current point puts it back on the line. Excalidraw's `angleLocked`
/// branch (`packages/element/src/binding.ts@1118751f:942-959`). An end sitting inside its shape is
/// exactly where it was put, and stays.
fn reprojected_other<'a>(
    arrow: &DrawElement,
    end: End,
    lookup: &dyn Fn(&str) -> Option<&'a DrawElement>,
) -> Option<Anchor> {
    let far = end.other();
    let other = anchor(arrow, far).filter(|a| a.mode == BindMode::Orbit)?;
    let shape = lookup(&other.element_id).filter(|s| !s.is_deleted)?;
    let points = crate::selection::linear::world_points(arrow);
    let endpoint = *match far {
        End::Start => points.first(),
        End::End => points.last(),
    }?;
    let focus = projected_anchor(arrow, far, shape, endpoint, lookup).unwrap_or(endpoint);
    Some(Anchor {
        fixed_point: fixed_point_at(shape, focus),
        ..other
    })
}

// ------------------------------------------------------------------------- resolving

/// One bound end: its anchor and the live shape it is anchored to.
struct Bound<'a> {
    anchor: Anchor,
    shape: &'a DrawElement,
}

fn bound<'a>(
    arrow: &DrawElement,
    end: End,
    lookup: &dyn Fn(&str) -> Option<&'a DrawElement>,
) -> Option<Bound<'a>> {
    let anchor = anchor(arrow, end)?;
    let shape = lookup(&anchor.element_id).filter(|s| !s.is_deleted)?;
    Some(Bound { anchor, shape })
}

/// Where the line an orbiting end is drawn along comes from: the far anchor for a
/// straight arrow, the neighbouring point of a bent one.
fn aim(points: &[Point], end: End, other: Option<&Bound<'_>>) -> Point {
    let last = points.len() - 1;
    let neighbour = match end {
        End::Start => points[1.min(last)],
        End::End => points[last.saturating_sub(1)],
    };
    match other {
        Some(o) if points.len() == 2 => focus_point(o.shape, o.anchor.fixed_point),
        _ => neighbour,
    }
}

/// Where one bound end is drawn. `updateBoundPoint` (`binding.ts@1118751f:1948-2104`).
///
/// An inside end is its anchor. An orbiting end runs from its anchor toward [`aim`] and
/// stops where that line leaves the outline, a gap clear of it — or stays on its anchor
/// when the line never leaves, which is an anchor the person put outside the shape.
///
/// Excalidraw also sends an orbiting end *onto* its anchor when the arrow gets short or
/// its outline point falls inside the far shape (`binding.ts@1118751f:2036-2093`). Its anchors
/// mostly sit on the outline, so there that is a small step; an anchor at a shape's
/// centre — every arrow bound before anchors existed — made it a jump deep into the
/// shape. Here an orbiting end never goes inside its shape, and the one thing those rules
/// guard against, an arrow turning inside out, is handled by [`resolve_endpoints`].
///
/// Also says whether an orbiting end is **trapped**: its anchor inside the shape and no
/// way out toward its aim. An anchor put outside the shape that the line never leaves
/// from is not trapped — the end simply stays on it, as Excalidraw keeps one on its focus
/// (`utils.ts@1118751f:897-901`, `binding.ts@1118751f:889`).
fn resolve_end(
    points: &[Point],
    end: End,
    this: &Bound<'_>,
    other: Option<&Bound<'_>>,
) -> (Point, bool) {
    let focus = focus_point(this.shape, this.anchor.fixed_point);
    if this.anchor.mode == BindMode::Inside {
        return (focus, false);
    }
    let aim = aim(points, end, other);
    match nearest_to(
        outline_crossings(this.shape, focus, aim, binding_gap(this.shape)),
        aim,
    ) {
        Some(outline) => (outline, false),
        // The box, edge included, not [`is_inside`]: an anchor snapped onto a side
        // midpoint sits exactly on the outline, and is as held by its shape as one a
        // hair inside it.
        None => (focus, contains(this.shape, focus, 0.0)),
    }
}

/// Both ends of `element` as its bindings put them, or `None` when neither end is bound.
///
/// # Never inside out
///
/// Each orbiting end sits on its shape's outline, facing the other. That is right while
/// the shapes are apart. Once they close on each other the two outline points cross over:
/// the tail sits beyond the head and the arrow runs **backwards**, through both shapes.
/// Measured on excalidraw.com with one rectangle slid onto another, the arrow went 228
/// long, 128, 48, then 0 and stayed 0 — never entering either shape. So a straight arrow
/// orbiting at both ends collapses onto its tail when it would run against the line
/// between its anchors, or when both ends are trapped — each anchor inside the other
/// shape, where no arrow fits at all. One end leaving is a shape nested inside a larger
/// one, and draws; an anchor put outside its shape is not trapped, and draws where it is.
fn resolve_endpoints<'a>(
    element: &DrawElement,
    lookup: &dyn Fn(&str) -> Option<&'a DrawElement>,
) -> Option<(Point, Point)> {
    resolve_endpoints_from(element, lookup).map(|(next, _)| next)
}

/// [`resolve_endpoints`], with where the ends are now.
fn resolve_endpoints_from<'a>(
    element: &DrawElement,
    lookup: &dyn Fn(&str) -> Option<&'a DrawElement>,
) -> Option<((Point, Point), (Point, Point))> {
    let start = bound(element, End::Start, lookup);
    let end = bound(element, End::End, lookup);
    if start.is_none() && end.is_none() {
        return None;
    }
    let points = crate::selection::linear::world_points(element);
    if points.len() < 2 {
        return None;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    let (next_start, start_trapped) = start
        .as_ref()
        .map(|b| resolve_end(&points, End::Start, b, end.as_ref()))
        .unwrap_or((first, false));
    let (next_end, end_trapped) = end
        .as_ref()
        .map(|b| resolve_end(&points, End::End, b, start.as_ref()))
        .unwrap_or((last, false));
    if let (Some(s), Some(e)) = (&start, &end) {
        let orbit = |b: &Bound<'_>| b.anchor.mode == BindMode::Orbit;
        if points.len() == 2 && orbit(s) && orbit(e) {
            let (fs, fe) = (
                focus_point(s.shape, s.anchor.fixed_point),
                focus_point(e.shape, e.anchor.fixed_point),
            );
            let run = (next_end.x - next_start.x) * (fe.x - fs.x)
                + (next_end.y - next_start.y) * (fe.y - fs.y);
            if run < 0.0 || (start_trapped && end_trapped) {
                return Some(((next_start, next_start), (first, last)));
            }
        }
    }
    Some(((next_start, next_end), (first, last)))
}

// -------------------------------------------------------------------------- linears

pub fn linear_endpoints(element: &DrawElement) -> (Point, Point) {
    let points = element
        .points
        .as_deref()
        .unwrap_or(&[[0.0, 0.0], [0.0, 0.0]]);
    let first = points.first().copied().unwrap_or([0.0, 0.0]);
    let last = points.last().copied().unwrap_or([0.0, 0.0]);
    (
        Point {
            x: element.x + first[0],
            y: element.y + first[1],
        },
        Point {
            x: element.x + last[0],
            y: element.y + last[1],
        },
    )
}

/// Replaces a linear element's geometry with a straight run from `start` to `end`.
///
/// Destroys any intermediate points, which is correct only while the line *is* two
/// points — drawing one, or rebuilding one from its ends. Anything re-anchoring an
/// existing line wants [`linear_retarget`]. `start` and `end` are world positions, so the
/// result carries no rotation.
pub fn linear_from_endpoints(mut element: DrawElement, start: Point, end: Point) -> DrawElement {
    element.x = start.x;
    element.y = start.y;
    element.width = end.x - start.x;
    element.height = end.y - start.y;
    element.angle = 0.0;
    element.points = Some(vec![[0.0, 0.0], [end.x - start.x, end.y - start.y]]);
    element
}

/// Moves a linear element's two ends, leaving everything between them where it is.
///
/// Binding used to go through [`linear_from_endpoints`], which rewrites the point list as
/// a straight pair. So the moment a bound shape was nudged, an arrow that had been given
/// midpoints collapsed to a straight line and the user's edits were gone — silently, and
/// unrecoverably once the history entry was folded.
///
/// Interior points keep their **world** positions — including any turn the line carried,
/// which is folded into its points, so it has none left over. The origin is re-pinned to
/// the first point, which is the invariant the rest of the engine relies on, and the
/// extent is recomputed from the points, which is what
/// [`crate::scene::geometry::element_bounds`] measures.
pub fn linear_retarget(element: DrawElement, start: Point, end: Point) -> DrawElement {
    let mut world = crate::selection::linear::world_points(&element);
    if world.len() < 3 {
        // Two points are entirely defined by their ends; nothing to preserve.
        return linear_from_endpoints(element, start, end);
    }
    let last = world.len() - 1;
    world[0] = start;
    world[last] = end;
    crate::selection::linear::from_world_points(&element, &world)
}

/// Puts a label where it goes in its container at the size it already has
/// (`computeBoundTextPosition`, [`crate::text::layout::bound_text_position`]), turned with
/// it — an arrow's label never turns.
///
/// Position only: its lines and its size are [`crate::text::layout_text`]'s, laid out when
/// its text, its font, its alignment or its wrap changes, and on every move of a resize
/// of its container ([`crate::text::layout::bound_text_resize`]) — not when its container
/// is moved. This runs on every move of a container and on a peer's patch, where the
/// label arrives laid out by whoever wrote it.
///
/// Divergence: a label taller than its room starts at the padded top, whatever its
/// vertical alignment — text that overflows downward is still read from its first line,
/// text pushed up off the shape is not. The oracle never meets such a label, because
/// laying one out grows its shape; here one saved before shapes grew is placed on every
/// move of its shape.
pub fn layout_label(mut label: DrawElement, container: &DrawElement) -> DrawElement {
    use crate::text::layout::{bound_text_max_height, bound_text_position};
    let overflows = !is_linear_element(container)
        && label.height > bound_text_max_height(container, label.height);
    let at = if overflows {
        let mut from_top = label.clone();
        from_top.vertical_align = Some(crate::scene::VerticalAlign::Top);
        bound_text_position(container, &from_top)
    } else {
        bound_text_position(container, &label)
    };
    label.x = at.x;
    label.y = at.y;
    label.angle = if is_linear_element(container) {
        0.0
    } else {
        container.angle
    };
    label
}

/// Recomputes bound geometry **in place**, touching only the elements that change.
///
/// [`refresh_bindings`] rebuilds the whole scene: it clones every element into a map,
/// clones them all again into a result vector, and the caller then writes every one
/// back. That runs on every pointer move during a drag, so its cost was proportional to
/// the size of the document rather than to the one shape being moved — the reason
/// dragging a single rectangle got slower as a board filled up.
///
/// This walks the same logic but writes only what actually moved. A scene with no
/// bindings costs one pass and no allocation at all.
///
/// Only for what `touched` names — the elements the commit in progress changed. An arrow
/// is re-resolved when it or either shape it is bound to was touched; a label when it or
/// its container was, or its container was re-routed here. So an edit re-resolves the
/// arrows of what it moved, as the oracle's `updateBoundElements(changedElement)` does
/// (`packages/element/src/align.ts@1118751f:45-48`), and nothing else: re-resolving the whole
/// board rewrote an arrow saved before its ends were anchored on the first unrelated
/// edit — stamped as part of that edit, or, on a peer's patch, not stamped at all, so
/// this engine and the server held two geometries under one version.
///
/// Returns the ids of labels bound to a **linear** container among those touched: this
/// module has no font measurer, so it only repositions a shape's label here
/// ([`layout_label`], position only — correct for a shape, which is resized through its
/// own handles, laid out again by [`crate::text::layout::bound_text_resize`]). An arrow
/// has no such handle: dragging a point *is* its resize, so its wrap width
/// (`ARROW_LABEL_WIDTH_FRACTION * width`) changes on every move, direct or through a
/// bound shape (`handleBindTextResize`, called from both
/// `linearElementEditor.ts@1118751f:629-636` and `binding.ts@1118751f:1416-1418`). The
/// caller — [`crate::engine::DrawEngine::rewrap_linear_labels`], which has a measurer —
/// rewraps and repositions the ids returned here in one `layout_text`, so this leaves
/// them exactly as it found them.
pub fn refresh_bindings_in_place(
    scene: &mut crate::scene::store::Scene,
    touched: &std::collections::HashSet<String>,
) -> Vec<String> {
    let touches = |id: Option<&str>| id.is_some_and(|id| touched.contains(id));
    // Pass 1: arrows with a bound end.
    //
    // Collected before writing because the reads borrow the scene immutably; only the
    // elements that genuinely changed are cloned.
    let mut moved: Vec<DrawElement> = Vec::new();
    {
        let lookup = |id: &str| scene.get(id);
        for element in scene.iter_ordered() {
            // Arrows only. A line carries no binding to refresh, and asking anyway would
            // resurrect one saved by an older build that did bind them.
            if !is_binding_element(element)
                || (element.start_binding.is_none() && element.end_binding.is_none())
            {
                continue;
            }
            if !touches(Some(&element.id))
                && !touches(element.start_binding.as_deref())
                && !touches(element.end_binding.as_deref())
            {
                continue;
            }
            let Some(((next_start, next_end), now)) = resolve_endpoints_from(element, &lookup)
            else {
                continue;
            };
            // Ends where they already are: nothing to rewrite, and nothing cloned to find
            // that out — this runs for every bound arrow on every move of anything. Not
            // for a turned two-point arrow, which the rewrite straightens.
            if (element.angle == 0.0 || element.points.as_ref().is_some_and(|p| p.len() > 2))
                && same_point(next_start, now.0)
                && same_point(next_end, now.1)
            {
                continue;
            }
            // `linear_retarget`, not `linear_from_endpoints`: the latter rewrites the
            // point list as a straight pair, so every bend a user had put in an arrow
            // vanished the moment the shape it pointed at was nudged.
            let next = linear_retarget(element.clone(), next_start, next_end);
            if &next != element {
                moved.push(next);
            }
        }
    }
    let rerouted: std::collections::HashSet<String> =
        moved.iter().map(|element| element.id.clone()).collect();
    for element in moved {
        scene.put(element);
    }

    // Pass 2: bound labels follow their container. Runs after the linear pass because a
    // label on an arrow has to follow the arrow's new endpoints.
    let mut relaid: Vec<DrawElement> = Vec::new();
    let mut linear_labels: Vec<String> = Vec::new();
    for element in scene.iter_ordered() {
        if element.kind != DrawElementType::Text {
            continue;
        }
        let Some(container_id) = element.container_id.as_deref() else {
            continue;
        };
        if !touches(Some(&element.id))
            && !touches(Some(container_id))
            && !rerouted.contains(container_id)
        {
            continue;
        }
        let Some(container) = scene.get(container_id).filter(|c| !c.is_deleted) else {
            continue;
        };
        if is_linear_element(container) {
            // Rewrapped and repositioned together by the caller — see the doc comment
            // above.
            linear_labels.push(element.id.clone());
            continue;
        }
        let laid = layout_label(element.clone(), container);
        if &laid != element {
            relaid.push(laid);
        }
    }
    for element in relaid {
        scene.put(element);
    }
    linear_labels
}

/// Equal but for the rounding of a world ↔ local round trip.
fn same_point(a: Point, b: Point) -> bool {
    let scale = 1.0 + a.x.abs().max(a.y.abs());
    (a.x - b.x).abs() <= 1e-12 * scale && (a.y - b.y).abs() <= 1e-12 * scale
}

/// [`refresh_bindings_in_place`] for one arrow and its label.
///
/// For a gesture that changes that arrow and nothing it is bound to — drawing it, or
/// dragging one of its points — so nothing else on the board has anything to re-resolve.
/// Refreshing the whole board there made each move cost every bound arrow on it.
///
/// `id` is always the linear element itself, so its label — if it has one — is always a
/// linear container's: returned for the caller to rewrap and reposition together
/// ([`crate::engine::DrawEngine::rewrap_linear_labels`]), for the reason given on
/// [`refresh_bindings_in_place`].
pub fn refresh_binding_of(scene: &mut crate::scene::store::Scene, id: &str) -> Option<String> {
    let next = {
        let lookup = |id: &str| scene.get(id);
        scene
            .get(id)
            .filter(|element| is_binding_element(element))
            .and_then(|element| {
                let (start, end) = resolve_endpoints(element, &lookup)?;
                let next = linear_retarget(element.clone(), start, end);
                (&next != element).then_some(next)
            })
    };
    if let Some(next) = next {
        scene.put(next);
    }
    let label_id = scene.get(id)?.bound_text_id.clone()?;
    scene
        .get(&label_id)
        .is_some_and(|label| !label.is_deleted)
        .then_some(label_id)
}

/// Recomputes bound geometry over a detached element list.
///
/// Retained for callers that hold a plain slice — chiefly the tests, which assert on the
/// returned list. Per-frame and per-event code should use [`refresh_bindings_in_place`],
/// which does not clone the scene.
pub fn refresh_bindings(elements: &[DrawElement]) -> Vec<DrawElement> {
    // Arrows are never targets, so every arrow resolves against the list as given.
    let by_id: std::collections::HashMap<&str, &DrawElement> =
        elements.iter().map(|el| (el.id.as_str(), el)).collect();
    let lookup = |id: &str| by_id.get(id).copied();
    let linears_done: Vec<DrawElement> = elements
        .iter()
        .map(|element| {
            if element.is_deleted || !is_binding_element(element) {
                return element.clone();
            }
            match resolve_endpoints(element, &lookup) {
                Some((next_start, next_end)) => {
                    linear_retarget(element.clone(), next_start, next_end)
                }
                None => element.clone(),
            }
        })
        .collect();
    // Labels follow their container as it is *now*: an arrow's label its new ends.
    let containers: std::collections::HashMap<&str, &DrawElement> =
        linears_done.iter().map(|el| (el.id.as_str(), el)).collect();
    linears_done
        .iter()
        .map(|element| {
            if element.is_deleted || element.kind != DrawElementType::Text {
                return element.clone();
            }
            let Some(container_id) = element.container_id.as_deref() else {
                return element.clone();
            };
            match containers.get(container_id).filter(|c| !c.is_deleted) {
                Some(container) => layout_label(element.clone(), container),
                None => element.clone(),
            }
        })
        .collect()
}
