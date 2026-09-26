//! Arrowhead geometry, in element-local space.
//!
//! Transcribed from the oracle's `getArrowheadSize` / `getArrowheadAngle` /
//! `getArrowheadPoints` (`packages/element/src/bounds.ts@1118751f:714-909`) and the shape
//! each head is built from, `getArrowheadShapes` (`packages/element/src/shape.ts@1118751f:371-577`).
//!
//! # Where the points come from
//!
//! The oracle does not compute a head from the element's raw endpoint and the point
//! before it — it reads the **rough-sketched curve** the body was already drawn as: the
//! bezier control points of the last (or first) `bcurveTo` op, evaluated at `t = 0.3` to
//! find a point "just behind" the tip, and the direction from there to the tip. That is
//! why every function here takes `ops: &[Op]` — the same [`Drawable`](draw_rough::Drawable)
//! [`crate::render::shape::element_drawable`] built for the body — rather than
//! `element.points` directly; a head placed on the mathematical polyline instead would
//! sit at a visibly different angle once roughness bows the stroke away from it.
//!
//! # What is, and is not, byte-for-byte here
//!
//! [`get_arrowhead_points`] (and the size/angle it uses) is the part the oracle fixture
//! (`engine/tools/arrowhead-oracle/`, replayed by
//! `crates/draw-engine/tests/ci_arrowhead_oracle.rs`) asserts to `1e-9`: given the same
//! bezier ops, element points and stroke width, this returns the oracle's exact numbers.
//!
//! The rough *sketch* of the resulting line/polygon/circle is not: rough.js caches its
//! PRNG on the `options` object, so in the oracle every arrowhead shape drawn from the
//! same (spread-copied) options object continues the **one** random stream the body's
//! curve already advanced. Reproducing that would mean threading a continuing
//! `draw_rough::Ctx` out of [`crate::render::shape::element_drawable`] through every
//! arrowhead primitive — a change to the shared `draw-rough` generator API, not to
//! arrowheads, and out of scope for this change (`engine/crates/draw-rough` is relied on
//! by shapes far from arrows). Each primitive here is instead seeded from the element's
//! own seed, independently. `ponytail: seeded-per-primitive jitter, not a continued
//! stream; upgrade path is a Ctx-threading API on draw_rough::generator.` The
//! **structure** (op kinds, fill vs stroke, point positions) is exact regardless — only
//! the hand-drawn wobble's exact wiggle differs from a pixel-diff of the oracle.

use draw_rough::ops::{Op, OpSetKind};
use draw_rough::Drawable;

use crate::scene::element::{Arrowhead, DrawElement};

pub fn default_arrowhead(element: &DrawElement, end: &str) -> Arrowhead {
    let explicit = if end == "start" {
        element.start_arrowhead
    } else {
        element.end_arrowhead
    };
    if let Some(kind) = explicit {
        return kind;
    }
    if element.kind == crate::scene::DrawElementType::Arrow && end == "end" {
        Arrowhead::Arrow
    } else {
        Arrowhead::None
    }
}

/// Which end of the line the head sits on — which bezier op it reads, and which of the
/// element's own points it treats as the tip for the "last segment length" scale-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Start,
    End,
}

/// `getCurvePathOps(shape)` (`packages/utils/src/shape.ts@1118751f:199-211`): the ops of
/// the one `path`-kind set in a rough [`Drawable`], or its first set if none is a `path`
/// (a filled shape's fill set comes first). `shape[0]` in the oracle — the arrow body is
/// always exactly one `Drawable`, never the multi-drawable case a figure's extra stroke
/// produces.
pub fn curve_path_ops(drawable: &Drawable) -> &[Op] {
    for set in &drawable.sets {
        if set.kind == OpSetKind::Path {
            return &set.ops;
        }
    }
    drawable
        .sets
        .first()
        .map(|set| set.ops.as_slice())
        .unwrap_or(&[])
}

/// `CARDINALITY_MARKER_SIZE` / `CROWFOOT_ARROWHEAD_SIZE`
/// (`packages/element/src/bounds.ts@1118751f:710-711`).
const CARDINALITY_MARKER_SIZE: f64 = 20.0;
const CROWFOOT_ARROWHEAD_SIZE: f64 = 15.0;

/// `getArrowheadSize(arrowhead)` (`bounds.ts@1118751f:714-732`).
pub fn get_arrowhead_size(arrowhead: Arrowhead) -> f64 {
    match arrowhead {
        Arrowhead::Arrow => 25.0,
        Arrowhead::Diamond | Arrowhead::DiamondOutline => 12.0,
        Arrowhead::CardinalityMany
        | Arrowhead::CardinalityOneOrMany
        | Arrowhead::CardinalityZeroOrMany => CROWFOOT_ARROWHEAD_SIZE,
        Arrowhead::CardinalityOne
        | Arrowhead::CardinalityExactlyOne
        | Arrowhead::CardinalityZeroOrOne => CARDINALITY_MARKER_SIZE,
        _ => 15.0,
    }
}

/// `getArrowheadAngle(arrowhead)` (`bounds.ts@1118751f:735-744`).
pub fn get_arrowhead_angle(arrowhead: Arrowhead) -> f64 {
    match arrowhead {
        Arrowhead::Bar => 90.0,
        Arrowhead::Arrow => 20.0,
        _ => 25.0,
    }
}

/// How far *any* head can reach past its point, for [`crate::render::bounds::render_bounds`]'s
/// screen-culling pad — not an oracle constant, just twice the biggest [`get_arrowhead_size`]
/// (`arrow`, 25) to cover the diamond's opposite point, which doubles its own `min_size`
/// past the tip (`get_arrowhead_points`'s diamond branch). Unlike the padding this
/// replaced, it does not grow with stroke width: the oracle's sizes do not either.
pub const MAX_ARROWHEAD_REACH: f64 = 50.0;

fn rotate_about(point: [f64; 2], center: [f64; 2], radians: f64) -> [f64; 2] {
    if radians == 0.0 {
        return point;
    }
    let (x, y) = (point[0], point[1]);
    let (cx, cy) = (center[0], center[1]);
    let (s, c) = radians.sin_cos();
    [
        (x - cx) * c - (y - cy) * s + cx,
        (x - cx) * s + (y - cy) * c + cy,
    ]
}

fn degrees_to_radians(degrees: f64) -> f64 {
    (degrees * std::f64::consts::PI) / 180.0
}

/// `getArrowheadPoints(element, shape, position, arrowhead, offsetMultiplier)`
/// (`bounds.ts@1118751f:746-909`).
///
/// `element_points` is the element's own `points` (element-local, unrotated — Excalidraw's
/// `element.points`), used only for the "how long is the last segment" scale-down, never
/// for the head's own placement. `ops` is [`curve_path_ops`] of the body's own rough
/// shape — see the module doc for why.
///
/// Returns the oracle's flat coordinate array: 3 numbers (`[cx, cy, diameter]`) for a
/// circle, 6 for everything else but a diamond, 8 for a diamond. `None` where the oracle
/// returns `null` — no head, no shape yet, or too few ops to read a curve from.
pub fn get_arrowhead_points(
    element_points: &[[f64; 2]],
    stroke_width: f64,
    ops: &[Op],
    position: Position,
    arrowhead: Arrowhead,
    offset_multiplier: f64,
) -> Option<Vec<f64>> {
    if arrowhead == Arrowhead::None {
        return None;
    }
    if ops.is_empty() {
        return None;
    }

    let index = match position {
        Position::Start => 1,
        Position::End => ops.len() - 1,
    };
    let data = match ops.get(index) {
        Some(Op::BCurveTo(data)) => *data,
        _ => return None,
    };
    let p3 = [data[4], data[5]];
    let p2 = [data[2], data[3]];
    let p1 = [data[0], data[1]];

    // The previous op's end point — usually the `move` that opened this stroke pass,
    // occasionally the curve segment before it. Anything else (a bare `lineTo`, or no
    // previous op at all) reads as the origin, exactly as the oracle leaves its default.
    let p0 = match index.checked_sub(1).and_then(|i| ops.get(i)) {
        Some(Op::Move([x, y])) => [*x, *y],
        Some(Op::BCurveTo(prev)) => [prev[4], prev[5]],
        _ => [0.0, 0.0],
    };

    // B(t) = p0(1-t)^3 + 3p1 t(1-t)^2 + 3p2 t^2(1-t) + p3 t^3
    let equation = |t: f64, idx: usize| -> f64 {
        let one_minus_t = 1.0 - t;
        (one_minus_t * one_minus_t * one_minus_t) * p3[idx]
            + 3.0 * t * (one_minus_t * one_minus_t) * p2[idx]
            + 3.0 * (t * t) * one_minus_t * p1[idx]
            + p0[idx] * (t * t * t)
    };

    let (x2, y2) = match position {
        Position::Start => (p0[0], p0[1]),
        Position::End => (p3[0], p3[1]),
    };

    let (x1, y1) = (equation(0.3, 0), equation(0.3, 1));

    let distance = (x2 - x1).hypot(y2 - y1);
    let nx = (x2 - x1) / distance;
    let ny = (y2 - y1) / distance;

    let size = get_arrowhead_size(arrowhead);

    let length = {
        let point_at = |i: usize| element_points.get(i).copied().unwrap_or([0.0, 0.0]);
        let [cx, cy] = match position {
            Position::End => point_at(element_points.len().saturating_sub(1)),
            Position::Start => point_at(0),
        };
        let [px, py] = if element_points.len() > 1 {
            match position {
                Position::End => point_at(element_points.len() - 2),
                Position::Start => point_at(1),
            }
        } else {
            [0.0, 0.0]
        };
        (cx - px).hypot(cy - py)
    };

    let length_multiplier = match arrowhead {
        Arrowhead::Diamond | Arrowhead::DiamondOutline => 0.25,
        _ => 0.5,
    };
    let min_size = size.min(length * length_multiplier);
    let tx = x2 - nx * min_size * offset_multiplier;
    let ty = y2 - ny * min_size * offset_multiplier;
    let xs = tx - nx * min_size;
    let ys = ty - ny * min_size;

    if matches!(arrowhead, Arrowhead::Circle | Arrowhead::CircleOutline) {
        let diameter = (ys - ty).hypot(xs - tx) + stroke_width - 2.0;
        return Some(vec![tx, ty, diameter]);
    }

    let angle = get_arrowhead_angle(arrowhead);

    if matches!(
        arrowhead,
        Arrowhead::CardinalityMany | Arrowhead::CardinalityOneOrMany
    ) {
        // Swap (xs, ys) with (x2, y2): rotate (tx, ty) about (xs, ys), not the other way
        // round — the crow's-foot fork opens from the offset base, not from the tip.
        let [x3, y3] = rotate_about([tx, ty], [xs, ys], degrees_to_radians(-angle));
        let [x4, y4] = rotate_about([tx, ty], [xs, ys], degrees_to_radians(angle));
        return Some(vec![xs, ys, x3, y3, x4, y4]);
    }

    let [x3, y3] = rotate_about([xs, ys], [tx, ty], -degrees_to_radians(angle));
    let [x4, y4] = rotate_about([xs, ys], [tx, ty], degrees_to_radians(angle));

    if matches!(arrowhead, Arrowhead::Diamond | Arrowhead::DiamondOutline) {
        let point_at = |i: usize| element_points.get(i).copied().unwrap_or([0.0, 0.0]);
        let (ox, oy) = if position == Position::Start {
            let (px, py) = if element_points.len() > 1 {
                let p = point_at(1);
                (p[0], p[1])
            } else {
                (0.0, 0.0)
            };
            let [ox, oy] = rotate_about(
                [tx + min_size * 2.0, ty],
                [tx, ty],
                (py - ty).atan2(px - tx),
            );
            (ox, oy)
        } else {
            let (px, py) = if element_points.len() > 1 {
                let p = point_at(element_points.len() - 2);
                (p[0], p[1])
            } else {
                (0.0, 0.0)
            };
            let [ox, oy] = rotate_about(
                [tx - min_size * 2.0, ty],
                [tx, ty],
                (ty - py).atan2(tx - px),
            );
            (ox, oy)
        };
        return Some(vec![tx, ty, x3, y3, ox, oy, x4, y4]);
    }

    Some(vec![tx, ty, x3, y3, x4, y4])
}

/// Whether a filled shape (triangle, diamond, circle) is filled with the stroke colour
/// (solid) or the page's own colour (outline — it reads as a hole punched through the
/// arrow's stroke). `packages/element/src/shape.ts@1118751f:401,421,458` each choose
/// between `strokeColor` and `backgroundFillColor` this way; a compound head's own small
/// circle (zero-or-one, zero-or-many) is always the outline kind
/// (`shape.ts@1118751f:537,561`), independent of the outer head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRole {
    Solid,
    Outline,
}

/// One rough-generator call `getArrowheadShapes` (`shape.ts@1118751f:371-577`) makes for a
/// head — a single primitive per `generator.line` / `generator.polygon` /
/// `generator.circle` call, in the oracle's call order, so painting them in order and
/// seeding each independently (see the module doc) reproduces the same picture.
#[derive(Clone, Debug, PartialEq)]
pub enum ArrowheadPrimitive {
    /// `generator.line` — a single stroked segment: a chevron barb, the bar, a
    /// crow's-foot fork line, the "one" marker's cross-tick.
    Line([[f64; 2]; 2]),
    /// `generator.polygon`, always closed and filled — the triangle or the diamond.
    Polygon(Vec<[f64; 2]>, FillRole),
    /// `generator.circle`, always filled.
    Circle {
        center: [f64; 2],
        diameter: f64,
        role: FillRole,
    },
}

/// `cardinalityOneOrManyOffset` / `cardinalityZeroCircleScale`
/// (`shape.ts@1118751f:390-391`).
const CARDINALITY_ONE_OR_MANY_OFFSET: f64 = -0.25;
const CARDINALITY_ZERO_CIRCLE_SCALE: f64 = 0.8;

/// An "outline" head is filled with the page's own colour, so it reads as a hole punched
/// through the arrow's stroke; a "solid" head is filled with the stroke colour itself
/// (`shape.ts@1118751f:386-389`'s `canvasBackgroundColor` vs `element.strokeColor`, dark-
/// mode filtered there — this engine keeps element colours literal and only the canvas
/// changes, so the caller's own background already carries that). Shared by the canvas
/// painter (the live `DrawTheme::background`) and the SVG exporter (the background rect
/// the export itself paints) — one colour choice, not two that can drift apart.
pub fn arrowhead_fill_color<'a>(
    role: FillRole,
    stroke_color: &'a str,
    background: &'a str,
) -> &'a str {
    match role {
        FillRole::Solid => stroke_color,
        FillRole::Outline => background,
    }
}

fn lines_to_tip(points: Option<Vec<f64>>) -> Vec<ArrowheadPrimitive> {
    // `generateArrowheadLinesToTip` (`shape.ts@1118751f:309-324`): [x2,y2,x3,y3,x4,y4] ->
    // two strokes, each from a fork point back to the shared tip (x2, y2).
    let Some(p) = points.filter(|p| p.len() == 6) else {
        return Vec::new();
    };
    vec![
        ArrowheadPrimitive::Line([[p[2], p[3]], [p[0], p[1]]]),
        ArrowheadPrimitive::Line([[p[4], p[5]], [p[0], p[1]]]),
    ]
}

fn cardinality_one_line(points: Option<Vec<f64>>) -> Vec<ArrowheadPrimitive> {
    // `generateArrowheadCardinalityOne` (`shape.ts@1118751f:295-307`): [,,x3,y3,x4,y4] ->
    // one cross-tick from (x3,y3) to (x4,y4).
    let Some(p) = points.filter(|p| p.len() == 6) else {
        return Vec::new();
    };
    vec![ArrowheadPrimitive::Line([[p[2], p[3]], [p[4], p[5]]])]
}

fn outline_circle(points: Option<Vec<f64>>, diameter_scale: f64) -> Vec<ArrowheadPrimitive> {
    let Some(p) = points.filter(|p| p.len() == 3) else {
        return Vec::new();
    };
    vec![ArrowheadPrimitive::Circle {
        center: [p[0], p[1]],
        diameter: p[2] * diameter_scale,
        role: FillRole::Outline,
    }]
}

/// `getArrowheadShapes(element, shape, position, arrowhead, ...)`
/// (`shape.ts@1118751f:371-577`), stopping short of the rough options and colours — those
/// belong to whoever paints or exports the primitives (`wasm/paint.rs`, `export/svg.rs`),
/// which disagree on how faithfully they sketch a shape but must not disagree on what
/// shape it is.
pub fn arrowhead_shapes(
    element_points: &[[f64; 2]],
    stroke_width: f64,
    ops: &[Op],
    position: Position,
    arrowhead: Arrowhead,
) -> Vec<ArrowheadPrimitive> {
    let points = |kind: Arrowhead, offset: f64| {
        get_arrowhead_points(element_points, stroke_width, ops, position, kind, offset)
    };

    match arrowhead {
        Arrowhead::None => Vec::new(),

        Arrowhead::Circle | Arrowhead::CircleOutline => {
            let Some(p) = points(arrowhead, 0.0).filter(|p| p.len() == 3) else {
                return Vec::new();
            };
            let role = if arrowhead == Arrowhead::CircleOutline {
                FillRole::Outline
            } else {
                FillRole::Solid
            };
            vec![ArrowheadPrimitive::Circle {
                center: [p[0], p[1]],
                diameter: p[2],
                role,
            }]
        }

        Arrowhead::Triangle | Arrowhead::TriangleOutline => {
            let Some(p) = points(arrowhead, 0.0).filter(|p| p.len() == 6) else {
                return Vec::new();
            };
            let role = if arrowhead == Arrowhead::TriangleOutline {
                FillRole::Outline
            } else {
                FillRole::Solid
            };
            vec![ArrowheadPrimitive::Polygon(
                vec![[p[0], p[1]], [p[2], p[3]], [p[4], p[5]]],
                role,
            )]
        }

        Arrowhead::Diamond | Arrowhead::DiamondOutline => {
            let Some(p) = points(arrowhead, 0.0).filter(|p| p.len() == 8) else {
                return Vec::new();
            };
            let role = if arrowhead == Arrowhead::DiamondOutline {
                FillRole::Outline
            } else {
                FillRole::Solid
            };
            vec![ArrowheadPrimitive::Polygon(
                vec![[p[0], p[1]], [p[2], p[3]], [p[4], p[5]], [p[6], p[7]]],
                role,
            )]
        }

        Arrowhead::CardinalityOne => cardinality_one_line(points(Arrowhead::CardinalityOne, 0.0)),

        Arrowhead::CardinalityMany => lines_to_tip(points(Arrowhead::CardinalityMany, 0.0)),

        Arrowhead::CardinalityOneOrMany => {
            let mut shapes = lines_to_tip(points(Arrowhead::CardinalityMany, 0.0));
            shapes.extend(cardinality_one_line(points(
                Arrowhead::CardinalityOne,
                CARDINALITY_ONE_OR_MANY_OFFSET,
            )));
            shapes
        }

        Arrowhead::CardinalityExactlyOne => {
            let mut shapes = cardinality_one_line(points(Arrowhead::CardinalityOne, -0.5));
            shapes.extend(cardinality_one_line(points(Arrowhead::CardinalityOne, 0.0)));
            shapes
        }

        Arrowhead::CardinalityZeroOrOne => {
            let mut shapes = outline_circle(
                points(Arrowhead::CircleOutline, 1.5),
                CARDINALITY_ZERO_CIRCLE_SCALE,
            );
            shapes.extend(cardinality_one_line(points(
                Arrowhead::CardinalityOne,
                -0.5,
            )));
            shapes
        }

        Arrowhead::CardinalityZeroOrMany => {
            let mut shapes = lines_to_tip(points(Arrowhead::CardinalityMany, 0.0));
            shapes.extend(outline_circle(
                points(Arrowhead::CircleOutline, 1.5),
                CARDINALITY_ZERO_CIRCLE_SCALE,
            ));
            shapes
        }

        // `case "bar": case "arrow": default:` (`shape.ts@1118751f:567-576`).
        Arrowhead::Bar | Arrowhead::Arrow => lines_to_tip(points(arrowhead, 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_rough::ops::OpSet;

    /// A two-point straight body, sketched exactly as `draw_rough::generator::linear_path`
    /// would for a two-point arrow: one `move` and one `bcurveTo` per stroke pass.
    fn straight_ops(from: [f64; 2], to: [f64; 2]) -> Vec<Op> {
        vec![
            Op::Move(from),
            Op::BCurveTo([from[0], from[1], to[0], to[1], to[0], to[1]]),
        ]
    }

    #[test]
    fn no_head_for_none() {
        let ops = straight_ops([0.0, 0.0], [100.0, 0.0]);
        assert!(get_arrowhead_points(
            &[[0.0, 0.0], [100.0, 0.0]],
            2.0,
            &ops,
            Position::End,
            Arrowhead::None,
            0.0,
        )
        .is_none());
        assert!(arrowhead_shapes(
            &[[0.0, 0.0], [100.0, 0.0]],
            2.0,
            &ops,
            Position::End,
            Arrowhead::None,
        )
        .is_empty());
    }

    #[test]
    fn no_head_without_a_curve_to_read() {
        assert!(get_arrowhead_points(
            &[[0.0, 0.0], [100.0, 0.0]],
            2.0,
            &[],
            Position::End,
            Arrowhead::Arrow,
            0.0,
        )
        .is_none());
    }

    /// The tip of an arrow head sits exactly on the body's own endpoint.
    #[test]
    fn the_tip_lands_on_the_endpoint() {
        let points = [[0.0, 0.0], [100.0, 0.0]];
        let ops = straight_ops(points[0], points[1]);
        let p =
            get_arrowhead_points(&points, 2.0, &ops, Position::End, Arrowhead::Arrow, 0.0).unwrap();
        assert!((p[0] - 100.0).abs() < 1e-9);
        assert!((p[1] - 0.0).abs() < 1e-9);
    }

    #[test]
    fn curve_path_ops_prefers_the_path_set() {
        let fill = OpSet::fill_path(vec![Op::Move([0.0, 0.0])]);
        let path = OpSet::path(vec![Op::Move([1.0, 1.0])]);
        let drawable = Drawable {
            shape: "polygon",
            sets: vec![fill, path.clone()],
        };
        assert_eq!(curve_path_ops(&drawable), path.ops.as_slice());
    }

    #[test]
    fn curve_path_ops_falls_back_to_the_first_set() {
        let only = OpSet::fill_sketch(vec![Op::Move([2.0, 2.0])]);
        let drawable = Drawable {
            shape: "ellipse",
            sets: vec![only.clone()],
        };
        assert_eq!(curve_path_ops(&drawable), only.ops.as_slice());
    }

    /// An outline head (`circle_outline`, `triangle_outline`, `diamond_outline`) is a hole
    /// punched in the stroke, so it must follow whatever the caller is actually painting
    /// on — a dark theme's background here, not a fixed white.
    #[test]
    fn outline_head_follows_the_background() {
        assert_eq!(
            arrowhead_fill_color(FillRole::Outline, "#1e1e1e", "#121212"),
            "#121212",
        );
        assert_eq!(
            arrowhead_fill_color(FillRole::Outline, "#1e1e1e", "#ffffff"),
            "#ffffff",
        );
    }

    /// A solid head (`triangle`, `diamond`, `circle`, the cardinality markers) is punched
    /// out of nothing — it is filled with the stroke, regardless of the background.
    #[test]
    fn solid_head_uses_the_stroke_color() {
        assert_eq!(
            arrowhead_fill_color(FillRole::Solid, "#1e1e1e", "#121212"),
            "#1e1e1e",
        );
    }

    #[test]
    fn sizes_match_the_oracle_constants() {
        assert_eq!(get_arrowhead_size(Arrowhead::Arrow), 25.0);
        assert_eq!(get_arrowhead_size(Arrowhead::Diamond), 12.0);
        assert_eq!(get_arrowhead_size(Arrowhead::DiamondOutline), 12.0);
        assert_eq!(get_arrowhead_size(Arrowhead::CardinalityMany), 15.0);
        assert_eq!(get_arrowhead_size(Arrowhead::CardinalityZeroOrMany), 15.0);
        assert_eq!(get_arrowhead_size(Arrowhead::CardinalityOne), 20.0);
        assert_eq!(get_arrowhead_size(Arrowhead::CardinalityExactlyOne), 20.0);
        assert_eq!(get_arrowhead_size(Arrowhead::Bar), 15.0);
        assert_eq!(get_arrowhead_size(Arrowhead::Triangle), 15.0);
    }

    #[test]
    fn angles_match_the_oracle_constants() {
        assert_eq!(get_arrowhead_angle(Arrowhead::Bar), 90.0);
        assert_eq!(get_arrowhead_angle(Arrowhead::Arrow), 20.0);
        assert_eq!(get_arrowhead_angle(Arrowhead::Triangle), 25.0);
        assert_eq!(get_arrowhead_angle(Arrowhead::CardinalityOne), 25.0);
    }

    #[test]
    fn a_circle_head_returns_a_center_and_diameter() {
        let points = [[0.0, 0.0], [100.0, 0.0]];
        let ops = straight_ops(points[0], points[1]);
        let shapes = arrowhead_shapes(&points, 2.0, &ops, Position::End, Arrowhead::CircleOutline);
        assert_eq!(shapes.len(), 1);
        assert!(matches!(
            shapes[0],
            ArrowheadPrimitive::Circle {
                role: FillRole::Outline,
                ..
            }
        ));
    }

    #[test]
    fn a_triangle_head_is_a_solid_three_point_polygon() {
        let points = [[0.0, 0.0], [100.0, 0.0]];
        let ops = straight_ops(points[0], points[1]);
        let shapes = arrowhead_shapes(&points, 2.0, &ops, Position::End, Arrowhead::Triangle);
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            ArrowheadPrimitive::Polygon(pts, FillRole::Solid) => assert_eq!(pts.len(), 3),
            other => panic!("expected a solid triangle, got {other:?}"),
        }
    }

    #[test]
    fn a_diamond_head_is_a_four_point_polygon() {
        let points = [[0.0, 0.0], [100.0, 0.0]];
        let ops = straight_ops(points[0], points[1]);
        let shapes = arrowhead_shapes(&points, 2.0, &ops, Position::End, Arrowhead::DiamondOutline);
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            ArrowheadPrimitive::Polygon(pts, FillRole::Outline) => assert_eq!(pts.len(), 4),
            other => panic!("expected an outline diamond, got {other:?}"),
        }
    }

    #[test]
    fn a_crowfoot_many_head_is_two_lines_to_the_same_tip() {
        let points = [[0.0, 0.0], [100.0, 0.0]];
        let ops = straight_ops(points[0], points[1]);
        let shapes = arrowhead_shapes(
            &points,
            2.0,
            &ops,
            Position::End,
            Arrowhead::CardinalityMany,
        );
        assert_eq!(shapes.len(), 2);
        let ArrowheadPrimitive::Line([_, a]) = shapes[0] else {
            panic!("expected a line");
        };
        let ArrowheadPrimitive::Line([_, b]) = shapes[1] else {
            panic!("expected a line");
        };
        assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    }

    /// Every compound cardinality head draws at least the same "one" tick or crow's-foot
    /// fork its name promises, plus — for the zero variants — the small outline circle.
    #[test]
    fn compound_cardinality_heads_combine_the_expected_primitives() {
        let points = [[0.0, 0.0], [100.0, 0.0]];
        let ops = straight_ops(points[0], points[1]);

        let one_or_many = arrowhead_shapes(
            &points,
            2.0,
            &ops,
            Position::End,
            Arrowhead::CardinalityOneOrMany,
        );
        assert_eq!(one_or_many.len(), 3, "two fork lines plus the one tick");

        let exactly_one = arrowhead_shapes(
            &points,
            2.0,
            &ops,
            Position::End,
            Arrowhead::CardinalityExactlyOne,
        );
        assert_eq!(exactly_one.len(), 2, "two parallel ticks");

        let zero_or_one = arrowhead_shapes(
            &points,
            2.0,
            &ops,
            Position::End,
            Arrowhead::CardinalityZeroOrOne,
        );
        assert!(zero_or_one
            .iter()
            .any(|s| matches!(s, ArrowheadPrimitive::Circle { .. })));
        assert!(zero_or_one
            .iter()
            .any(|s| matches!(s, ArrowheadPrimitive::Line(_))));

        let zero_or_many = arrowhead_shapes(
            &points,
            2.0,
            &ops,
            Position::End,
            Arrowhead::CardinalityZeroOrMany,
        );
        assert_eq!(zero_or_many.len(), 3, "two fork lines plus the circle");
    }

    #[test]
    fn legacy_wire_names_deserialize_to_their_modern_value() {
        let dot: Arrowhead = serde_json::from_str("\"dot\"").unwrap();
        assert_eq!(dot, Arrowhead::Circle);
        let one: Arrowhead = serde_json::from_str("\"crowfoot_one\"").unwrap();
        assert_eq!(one, Arrowhead::CardinalityOne);
        let many: Arrowhead = serde_json::from_str("\"crowfoot_many\"").unwrap();
        assert_eq!(many, Arrowhead::CardinalityMany);
        let one_or_many: Arrowhead = serde_json::from_str("\"crowfoot_one_or_many\"").unwrap();
        assert_eq!(one_or_many, Arrowhead::CardinalityOneOrMany);
    }

    #[test]
    fn modern_names_round_trip_and_never_emit_the_legacy_spelling() {
        for kind in [
            Arrowhead::Circle,
            Arrowhead::TriangleOutline,
            Arrowhead::CardinalityZeroOrMany,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert!(!json.contains("dot") && !json.contains("crowfoot"));
            let back: Arrowhead = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
    }
}
