//! Properties of the cardinality (crow's-foot) marks, over a swept corpus rather than one
//! picture.
//!
//! `ci_arrowhead_oracle.rs` holds every head's *numbers* to the oracle's own output, case by
//! case. This file asks what a per-case comparison cannot: that no two values in the set draw
//! the same picture, that a mark is anchored on and oriented by **its own end** of the line,
//! and that the compound marks really are their simple parts put at the oracle's offsets. The
//! type system cannot see any of that — a `case` arm returning the wrong shape is still a
//! valid `Arrowhead`.
//!
//! # Which set this is about, and why
//!
//! **The modern six, not the legacy four.** The oracle ships both (`types.ts@1118751f:342-367`:
//! `CardinalityArrowhead` and `ArrowheadLegacy`, unioned only by `AnyArrowhead`), and the
//! legacy four are read-only compatibility, not a second set of values:
//!
//! - the engine's enum has no legacy variant at all — the four are `#[serde(alias)]`s on their
//!   modern names (`scene/element.rs:77,84,86,88`);
//! - nothing on the writing side can produce one, so the legacy names are *inputs* only;
//! - the oracle does exactly this, in `normalizeArrowhead` (`arrowheads.ts@1118751f:3-21`)
//!   called on the way in from `restore.ts@1118751f:616-617,657-661`.
//!
//! So a board saved holding `crowfoot_one` loads as `cardinality_one`, renders the modern mark
//! and is written back under the modern name — pinned by
//! `a_saved_legacy_arrowhead_loads_under_its_modern_name`, through the same `load_scene` /
//! `export_json` door a file arrives at.
//!
//! `HEADS` is deliberately the *whole* rendered set, not the six alone: a distinctness claim is
//! only worth anything if it spans the values a mark could be confused with.
//!
//! # The one property deliberately **not** asserted
//!
//! "The start mark is the end mark turned half a circle" is **false**, and the reason is in the
//! oracle. `getArrowheadPoints` measures its "just behind the tip" point from `B(0.3)` — 0.3
//! *into* the curve it reads (`bounds.ts@1118751f:790-812`) — and the direction it derives
//! points *at* the tip (`:805-809`). It also returns a different first point per branch
//! (`:851-863`): the fork's convergence point for a crow's-foot, its own base otherwise. So the
//! two ends share a *measurement origin*, not a shape, and half-circle symmetry needs a body
//! symmetric about its own midpoint — which a real arrow is not. Asserting it would pin a
//! picture the oracle does not draw. What is asserted instead is the pair of facts that are
//! true and that a copied or mirrored mark fails: the mark's base is on the line's own axis, on
//! the inward side, at the oracle's distance.

mod common;

use draw_engine::render::arrowheads::{
    arrowhead_shapes, get_arrowhead_points, get_arrowhead_size, ArrowheadPrimitive, FillRole,
    Position, MAX_ARROWHEAD_REACH,
};
use draw_engine::{Arrowhead, DrawElementType, DrawEngine};
use draw_rough::ops::Op;

use std::collections::HashMap;

/// The fourteen heads that draw something, in the engine's own picker order
/// (`scene/element.rs:112-128`). `Arrowhead::None` is the fifteenth and draws nothing.
const HEADS: [Arrowhead; 14] = [
    Arrowhead::Arrow,
    Arrowhead::Bar,
    Arrowhead::Circle,
    Arrowhead::CircleOutline,
    Arrowhead::Triangle,
    Arrowhead::TriangleOutline,
    Arrowhead::Diamond,
    Arrowhead::DiamondOutline,
    Arrowhead::CardinalityOne,
    Arrowhead::CardinalityMany,
    Arrowhead::CardinalityOneOrMany,
    Arrowhead::CardinalityExactlyOne,
    Arrowhead::CardinalityZeroOrOne,
    Arrowhead::CardinalityZeroOrMany,
];

const STROKE_WIDTH: f64 = 2.0;
const TOLERANCE: f64 = 1e-6;
/// `getArrowheadAngle`'s default (`bounds.ts@1118751f:735-744`) — the half-angle every
/// cardinality mark is built from.
const HALF_ANGLE: f64 = 25.0;

/// The ops `getCurvePathOps` hands `getArrowheadPoints` for a two-point body: one `move` and one
/// `bcurveTo`. That is the whole contract — the oracle reads index `1` at the start and `len - 1`
/// at the end (`:768`), so those two ops are all it ever looks at.
fn body_ops(a: [f64; 2], b: [f64; 2], bow: bool) -> Vec<Op> {
    let c = if bow { a[1] } else { (a[1] + b[1]) / 2.0 };
    vec![
        Op::Move(a),
        Op::BCurveTo([a[0], a[1], (a[0] + b[0]) / 2.0, c, b[0], b[1]]),
    ]
}

/// **Straight**: both control points on the chord, so the curve's own direction and the chord's
/// are the same and [`inward`] is the oracle's `n`.
///
/// A test that wants to say "on the line's axis" has to use this body. On a bowed one the axis
/// `n` is derived from the curve (`:805-809`), so it genuinely differs from the chord, and a test
/// measuring against the chord would be measuring the oracle's *choice* rather than its
/// arithmetic — the bowed corpus below is the guard on that choice instead.
fn straight_corpus(count: usize) -> Vec<(Vec<[f64; 2]>, Vec<Op>)> {
    corpus(count, false)
}

/// **Bowed**: a body where the sketched curve's direction at each end differs from the chord's, so
/// the axis a mark is placed on is the curve's and not the line of endpoints. Used by every
/// assertion that does not need to know which way the axis points, and by the two
/// rigid-motion tests, which hold on any body.
fn bowed_corpus(count: usize) -> Vec<(Vec<[f64; 2]>, Vec<Op>)> {
    corpus(count, true)
}

/// A body long enough that no head is clamped by `min(size, length × 0.5)`, skewed so the axis is
/// not a screen axis and a mark drawn in the wrong frame is visible.
fn long_body() -> (Vec<[f64; 2]>, Vec<Op>) {
    let a = [10.0, 90.0];
    let b = [210.0, 40.0];
    (vec![a, b], body_ops(a, b, true))
}

/// A fixed-seed LCG. A fuzz corpus needs no dependency and no wall clock — the same corpus on
/// every machine, on every run, forever.
struct Corpus(u64);

impl Corpus {
    fn next(&mut self, low: f64, high: f64) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        let unit = ((self.0 >> 11) as f64) / ((1u64 << 53) as f64);
        low + unit * (high - low)
    }
}

/// `count` two-point bodies at random angles and lengths, each paired with its own endpoints.
fn corpus(count: usize, bow: bool) -> Vec<(Vec<[f64; 2]>, Vec<Op>)> {
    let mut rng = Corpus(0x5EED);
    (0..count)
        .map(|_| {
            let a = [rng.next(-200.0, 200.0), rng.next(-200.0, 200.0)];
            let b = [a[0] + rng.next(20.0, 400.0), a[1] + rng.next(-200.0, 200.0)];
            (vec![a, b], body_ops(a, b, bow))
        })
        .collect()
}

/// The endpoint a mark at `position` is anchored on.
fn endpoint(points: &[[f64; 2]], position: Position) -> [f64; 2] {
    match position {
        Position::Start => points[0],
        Position::End => points[points.len() - 1],
    }
}

/// The unit vector pointing from `position`'s endpoint into the line's own interior — the oracle's
/// `−n`, since `n` points *at* the tip, so a mark sits at `tip − n · minSize · offsetMultiplier`
/// and a positive offset walks it inward.
///
/// Only exact on a [`straight_corpus`] body; see [`body_ops`].
fn inward(points: &[[f64; 2]], position: Position) -> [f64; 2] {
    let (a, b) = (points[0], points[points.len() - 1]);
    let from = endpoint(points, position);
    let to = if position == Position::Start { b } else { a };
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let d = dx.hypot(dy);
    [dx / d, dy / d]
}

/// `getArrowheadPoints`'s own first pair, the mark's base on the axis: the endpoint itself at
/// `offset` 0, moved along the axis by `minSize × offsetMultiplier` (`:818-820`). For a
/// crow's-foot this is the fork's *convergence* point, one further size back (`:852-863`) — an
/// oracle quirk, pinned by `a_fork_converges_one_size_behind_its_base` rather than smoothed over
/// here.
fn base_of(
    points: &[[f64; 2]],
    ops: &[Op],
    position: Position,
    head: Arrowhead,
    offset: f64,
) -> [f64; 2] {
    let p = get_arrowhead_points(points, STROKE_WIDTH, ops, position, head, offset)
        .unwrap_or_else(|| panic!("{head:?} at {position:?} produced no points"));
    [p[0], p[1]]
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < TOLERANCE
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// A vector's coordinates resolved onto the line's axis and onto its normal: `(along, across)`.
fn resolve(vector: [f64; 2], axis: [f64; 2]) -> (f64, f64) {
    (
        vector[0] * axis[0] + vector[1] * axis[1],
        vector[0] * -axis[1] + vector[1] * axis[0],
    )
}

/// The coordinates a primitive is *made of* — a line's two ends, a polygon's corners, a circle's
/// centre.
///
/// A circle contributes its centre and not its rim, and that is deliberate: the rim has no
/// distinguished points, so sampling it would bake a world-fixed set of angles into the geometry
/// and a test comparing two of them would be testing the sampler. [`sampled`] is for the one
/// assertion that needs a bound rather than a set of points.
fn outline(shape: &ArrowheadPrimitive) -> Vec<[f64; 2]> {
    match shape {
        ArrowheadPrimitive::Line([a, b]) => vec![*a, *b],
        ArrowheadPrimitive::Polygon(p, _) => p.clone(),
        ArrowheadPrimitive::Circle { center, .. } => vec![*center],
    }
}

/// [`outline`] with a circle's rim sampled at eight points of its real radius, for a bound.
fn sampled(shape: &ArrowheadPrimitive) -> Vec<[f64; 2]> {
    match shape {
        ArrowheadPrimitive::Circle {
            center, diameter, ..
        } => {
            let radius = diameter / 2.0;
            (0..8)
                .map(|k| {
                    let angle = f64::from(k) * std::f64::consts::TAU / 8.0;
                    [
                        center[0] + radius * angle.cos(),
                        center[1] + radius * angle.sin(),
                    ]
                })
                .collect()
        }
        other => outline(other),
    }
}

/// Every value in the set draws something, and no two of the twenty-eight drawings (fourteen
/// heads × two ends) is the same picture as another.
///
/// The body is skewed, not symmetric: a head drawn by mirroring its twin would be invisible on
/// a body symmetric about its own midpoint. Two values drawing identical geometry is a bug the
/// type system cannot see — both are valid `Arrowhead`s, both reach the renderer, and only the
/// picture is wrong.
#[test]
fn no_two_heads_draw_the_same_picture() {
    let (points, ops) = long_body();
    let mut by_picture: HashMap<String, String> = HashMap::new();

    for head in HEADS {
        for position in [Position::Start, Position::End] {
            let shapes = arrowhead_shapes(&points, STROKE_WIDTH, &ops, position, head);
            assert!(!shapes.is_empty(), "{head:?} at {position:?} drew nothing");
            let key = shape_key(&shapes);
            let who = format!("{head:?} at {position:?}");
            if let Some(first) = by_picture.insert(key, who.clone()) {
                panic!("{who} draws exactly what {first} draws");
            }
        }
    }
}

/// One head's drawing as a single comparable string.
///
/// The **fill role is part of the key**, deliberately: `circle` and `circle_outline` share every
/// coordinate and differ only in what fills them, and two marks differing in ink are two
/// different pictures. Keying on coordinates alone would call that pair a collision and force a
/// real difference out of the geometry to satisfy the test.
fn shape_key(shapes: &[ArrowheadPrimitive]) -> String {
    shapes
        .iter()
        .map(|s| match s {
            ArrowheadPrimitive::Line([a, b]) => format!("L{a:?}{b:?}"),
            ArrowheadPrimitive::Polygon(p, role) => format!("P{role:?}{p:?}"),
            ArrowheadPrimitive::Circle {
                center,
                diameter,
                role,
            } => format!("C{role:?}{center:?}{diameter:?}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every head's base sits **on the line's own axis**: nothing across it, at either end.
///
/// `n` is derived from the body's sketched curve rather than from `points` (the module header
/// on `render::arrowheads` says why), so a mark placed from the raw polyline rather than from
/// the curve is off this axis by a visible amount on a bowed body — and invisible on a
/// straight one. The corpus is bowed for that reason.
#[test]
fn every_marks_base_sits_on_the_lines_own_axis() {
    for (index, (points, ops)) in straight_corpus(24).into_iter().enumerate() {
        for head in HEADS {
            for position in [Position::Start, Position::End] {
                let tip = endpoint(&points, position);
                let axis = inward(&points, position);
                let base = base_of(&points, &ops, position, head, 0.0);
                let (along, across) = resolve([base[0] - tip[0], base[1] - tip[1]], axis);
                assert!(
                    near(across, 0.0),
                    "case {index}, {head:?} at {position:?} is {across} off the line's axis",
                );
                assert!(
                    along >= -TOLERANCE,
                    "case {index}, {head:?} at {position:?} is {along} outward of its endpoint",
                );
            }
        }
    }
}

/// A mark's offset walks it along that axis by `minSize × offsetMultiplier` and nothing else —
/// inward for a positive offset, out past the endpoint for a negative one, which is how
/// `exactly_one` and `zero_or_one` put their second tick *outside* the box the line ends on.
///
/// This is `tx = x2 − nx·minSize·offsetMultiplier, ty = …` (`:818-820`) as a property, and it
/// is what an anchor mutation by a few pixels breaks.
#[test]
fn an_offset_walks_a_mark_along_the_axis_by_exactly_its_own_size() {
    for (index, (points, ops)) in straight_corpus(24).into_iter().enumerate() {
        for head in HEADS {
            for position in [Position::Start, Position::End] {
                let axis = inward(&points, position);
                for offset in [-0.5, 0.25, 1.5] {
                    let base = base_of(&points, &ops, position, head, offset);
                    let plain = base_of(&points, &ops, position, head, 0.0);
                    let (along, _) = resolve([base[0] - plain[0], base[1] - plain[1]], axis);
                    let want = get_arrowhead_size(head) * offset;
                    assert!(
                        near(along, want),
                        "case {index}, {head:?} at {position:?} offset {offset} moved {along} \
                         along the axis, not {want}",
                    );
                }
            }
        }
    }
}

/// A crow's-foot's two legs share one convergence point, sit a mark's width from it, and open
/// **toward** the tip at ±25° — the shape that makes a crow's-foot a crow's-foot.
///
/// `generateArrowheadLinesToTip` draws both strokes to the shared `[x2, y2]` (`shape.ts@1118751f:309-324`)
/// and the rotation that puts the two legs `±angle` about the tip is a *different* rotation
/// from the one every other head uses (`:852-861` against `:866-874`); getting the centre
/// backwards is the bug that draws a fork opening into the box.
#[test]
fn a_crowfoot_opens_toward_the_tip_at_twenty_five_degrees() {
    let angle = HALF_ANGLE.to_radians();
    for (index, (points, ops)) in bowed_corpus(24).into_iter().enumerate() {
        for position in [Position::Start, Position::End] {
            let tip = endpoint(&points, position);
            let base = base_of(&points, &ops, position, Arrowhead::CardinalityMany, 0.0);
            let [leg, other] = legs_of(&arrowhead_shapes(
                &points,
                STROKE_WIDTH,
                &ops,
                position,
                Arrowhead::CardinalityMany,
            ));
            let size = get_arrowhead_size(Arrowhead::CardinalityMany);
            assert!(
                near(distance(base, tip), size),
                "case {index}: the base is not one size in"
            );
            for end in [leg, other] {
                assert!(
                    near(distance(end, base), size),
                    "case {index}: a leg of the wrong length"
                );
                let axis = [(tip[0] - base[0]) / size, (tip[1] - base[1]) / size];
                let (along, across) = resolve([end[0] - base[0], end[1] - base[1]], axis);
                assert!(
                    near(along, size * angle.cos()) && near(across.abs(), size * angle.sin()),
                    "case {index}: a leg at {along} along and {across} across, not {} and ±{}",
                    size * angle.cos(),
                    size * angle.sin(),
                );
            }
        }
    }
}

/// The fork's convergence point is one mark-size *further back* than the base every other head
/// reports — the oracle returns `[xs, ys, …]` for a crow's-foot and `[tx, ty, …]` for the rest
/// (`:852-863` against `:866`). Pinned rather than smoothed over, because the difference is a
/// visible size on the picture and a future reader will otherwise assume it is a mistake.
#[test]
fn a_fork_converges_one_size_inside_its_tip() {
    for (index, (points, ops)) in straight_corpus(12).into_iter().enumerate() {
        for position in [Position::Start, Position::End] {
            let tip = endpoint(&points, position);
            let axis = inward(&points, position);
            let fork = base_of(&points, &ops, position, Arrowhead::CardinalityMany, 0.0);
            let size = get_arrowhead_size(Arrowhead::CardinalityMany);
            let (along, _) = resolve([fork[0] - tip[0], fork[1] - tip[1]], axis);
            assert!(
                near(along, size),
                "case {index}: a {position:?} fork converges {along} inside its tip, not {size}",
            );
        }
    }
}

/// The "one" mark is a bar **across** the axis, symmetric about it, `size × cos 25°` from the
/// base — the same 25° the fork opens by, so the two read as one family.
///
/// Written against the construction (`rotate_about(xs, base, ±angle)`, `bounds.ts@1118751f:866-874`)
/// so it pins the angle and the placement together: a bar at a plausible-looking wrong angle
/// fails it, and one a few pixels out fails it too.
#[test]
fn a_one_mark_is_a_perpendicular_bar_centred_on_the_axis() {
    for (index, (points, ops)) in straight_corpus(24).into_iter().enumerate() {
        for position in [Position::Start, Position::End] {
            let shapes = arrowhead_shapes(
                &points,
                STROKE_WIDTH,
                &ops,
                position,
                Arrowhead::CardinalityOne,
            );
            check_one_bar(index, (&points, &ops), position, &single_line(&shapes));
        }
    }
}

/// A body's points and its ops, as the pair [`get_arrowhead_points`] wants them.
type Body<'a> = (&'a [[f64; 2]], &'a [Op]);

/// Every claim about the "one" bar, on one body and one end.
///
/// Four assertions, and the last is the one worth reading: the first three are all
/// *differences* — the width, and the bar's centre and one end measured from the mark's own base —
/// so a mark slid sideways off the line's axis while keeping its own shape passes all three. Only
/// the fourth measures against the line, and it is what sees a sideways anchor.
fn check_one_bar(index: usize, (points, ops): Body<'_>, position: Position, bar: &[[f64; 2]; 2]) {
    let angle = HALF_ANGLE.to_radians();
    let size = get_arrowhead_size(Arrowhead::CardinalityOne);
    let axis = inward(points, position);
    let [a, b] = *bar;
    let base = base_of(points, ops, position, Arrowhead::CardinalityOne, 0.0);
    let tip = endpoint(points, position);
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let from = |p: [f64; 2]| [p[0] - base[0], p[1] - base[1]];

    assert!(
        near(distance(a, b), 2.0 * size * angle.sin()),
        "case {index}: a bar of {} should be {} wide",
        distance(a, b),
        2.0 * size * angle.sin(),
    );
    let (mid_on, mid_off) = resolve(from(mid), axis);
    assert!(
        near(mid_off, 0.0) && near(mid_on, size * angle.cos()),
        "case {index}: the bar's centre is {mid_on} along and {mid_off} off the axis, not {} and 0",
        size * angle.cos(),
    );
    let (along, across) = resolve(from(a), axis);
    assert!(
        near(along, size * angle.cos()) && near(across.abs(), size * angle.sin()),
        "case {index}: a bar end at {along} along and {across} across",
    );
    let (from_tip, across_tip) = resolve([mid[0] - tip[0], mid[1] - tip[1]], axis);
    assert!(
        near(from_tip, size * angle.cos()) && near(across_tip, 0.0),
        "case {index}: the bar's centre is {from_tip} inside the line's end and {across_tip} off \
         it, not {} and 0",
        size * angle.cos(),
    );
}

/// A mark's ink stays on the **inward** side of the line's end, so a cardinality mark overlaps
/// the box the line ends on rather than hanging off it — the question the geometry has to
/// answer, and the one that decides whether a mark reads as attached to the shape.
///
/// A circle is the one exception, and the exception is the oracle's: a circle head is centred
/// *on* the endpoint (`:851-855`), so half of it necessarily reaches outward. Stated here
/// rather than left as a failing tolerance.
#[test]
fn a_marks_ink_stays_inside_the_line_it_ends_on() {
    for (index, (points, ops)) in straight_corpus(24).into_iter().enumerate() {
        for head in HEADS {
            if matches!(head, Arrowhead::Circle | Arrowhead::CircleOutline) {
                continue;
            }
            for position in [Position::Start, Position::End] {
                let tip = endpoint(&points, position);
                let axis = inward(&points, position);
                for shape in arrowhead_shapes(&points, STROKE_WIDTH, &ops, position, head) {
                    for corner in outline(&shape) {
                        let (inside, _) = resolve([corner[0] - tip[0], corner[1] - tip[1]], axis);
                        assert!(
                            inside >= -STROKE_WIDTH / 2.0,
                            "case {index}: {head:?} at {position:?} puts ink {} past the end of \
                             the line, on the far side of the box",
                            -inside,
                        );
                    }
                }
            }
        }
    }
}

/// Each compound mark is its simple parts, in the oracle's order, each slid along the axis by
/// its own offset — `cardinalityOneOrManyOffset` and the `1.5` / `-0.5` literals at
/// `shape.ts@1118751f:390,519,539,543,554,563`.
///
/// The parts are re-derived here from `getArrowheadPoints` and the oracle's own three shape
/// builders rather than read back from the compound, so a compound that drew a private fourth
/// copy of the fork — at the right coordinates, so still passing every per-case comparison in
/// `ci_arrowhead_oracle.rs` — fails here on the count and the order.
#[test]
fn a_compound_mark_is_its_simple_parts_in_the_oracles_order() {
    for (index, (points, ops)) in bowed_corpus(12).into_iter().enumerate() {
        for (compound, parts) in compound_offsets() {
            let drawn = arrowhead_shapes(&points, STROKE_WIDTH, &ops, Position::End, compound);
            let want: Vec<ArrowheadPrimitive> = parts
                .iter()
                .flat_map(|(part, offset)| part_at(&points, &ops, Position::End, *part, *offset))
                .collect();
            assert_eq!(
                shape_key(&drawn),
                shape_key(&want),
                "case {index}: {compound:?} is not its parts in order",
            );
        }
    }
}

/// Each compound mark, with the simple mark and offset of every part it is made of, in the
/// oracle's own order.
fn compound_offsets() -> Vec<(Arrowhead, Vec<(Arrowhead, f64)>)> {
    vec![
        (
            Arrowhead::CardinalityOneOrMany,
            vec![
                (Arrowhead::CardinalityMany, 0.0),
                (Arrowhead::CardinalityOne, -0.25),
            ],
        ),
        (
            Arrowhead::CardinalityExactlyOne,
            vec![
                (Arrowhead::CardinalityOne, -0.5),
                (Arrowhead::CardinalityOne, 0.0),
            ],
        ),
        (
            Arrowhead::CardinalityZeroOrOne,
            vec![
                (Arrowhead::CircleOutline, 1.5),
                (Arrowhead::CardinalityOne, -0.5),
            ],
        ),
        (
            Arrowhead::CardinalityZeroOrMany,
            vec![
                (Arrowhead::CardinalityMany, 0.0),
                (Arrowhead::CircleOutline, 1.5),
            ],
        ),
    ]
}

/// One simple part, drawn the way `getArrowheadShapes` draws it when it is a compound's
/// ingredient: a fork or a tick for the two line builders, a hole punched with the page's colour
/// for the circle, at `cardinalityZeroCircleScale`.
fn part_at(
    points: &[[f64; 2]],
    ops: &[Op],
    position: Position,
    part: Arrowhead,
    offset: f64,
) -> Vec<ArrowheadPrimitive> {
    let Some(p) = get_arrowhead_points(points, STROKE_WIDTH, ops, position, part, offset) else {
        return Vec::new();
    };
    match part {
        Arrowhead::CardinalityMany => vec![
            ArrowheadPrimitive::Line([[p[2], p[3]], [p[0], p[1]]]),
            ArrowheadPrimitive::Line([[p[4], p[5]], [p[0], p[1]]]),
        ],
        Arrowhead::CardinalityOne => vec![ArrowheadPrimitive::Line([[p[2], p[3]], [p[4], p[5]]])],
        Arrowhead::CircleOutline => vec![ArrowheadPrimitive::Circle {
            center: [p[0], p[1]],
            diameter: p[2] * 0.8,
            role: FillRole::Outline,
        }],
        other => panic!("{other:?} is not a compound mark's ingredient"),
    }
}

/// The "zero" circle is a hole punched with the page's colour, and the two compound marks draw
/// theirs at `cardinalityZeroCircleScale` = 0.8 of its own diameter — the oracle passes that
/// last argument to the same call the plain `circle_outline` head makes without it
/// (`shape.ts@1118751f:396-402` against `:533-540,557-564`).
#[test]
fn a_zero_circle_is_an_outline_hole_at_eight_tenths_scale() {
    for (index, (points, ops)) in bowed_corpus(12).into_iter().enumerate() {
        let size = get_arrowhead_size(Arrowhead::CircleOutline);
        for (head, scale) in [
            (Arrowhead::CircleOutline, 1.0),
            (Arrowhead::CardinalityZeroOrOne, 0.8),
            (Arrowhead::CardinalityZeroOrMany, 0.8),
        ] {
            let drawn = circle_of(&arrowhead_shapes(
                &points,
                STROKE_WIDTH,
                &ops,
                Position::End,
                head,
            ))
            .expect("a zero mark carries a circle")
            .clone();
            let ArrowheadPrimitive::Circle { diameter, role, .. } = drawn else {
                unreachable!()
            };
            assert_eq!(
                role,
                FillRole::Outline,
                "case {index}: {head:?} filled its circle"
            );
            // The unscaled diameter is `minSize + strokeWidth - 2` (`:851-854`).
            let want = (size + STROKE_WIDTH - 2.0) * scale;
            assert!(
                near(diameter, want),
                "case {index}: {head:?} drew a circle of {diameter}, not {want}",
            );
        }
    }
}

/// A mark never reaches past the pad screen-culling pads by (`render::bounds::MAX_ARROWHEAD_REACH`),
/// so no head can be dropped off the screen.
///
/// The compound marks are why this is worth a test: they are the heads whose reach comes from an
/// *offset* — `zero_or_*`'s circle sits 1.5 sizes back along the line and `exactly_one`'s second
/// tick 0.5 sizes past the endpoint — not from their own `getArrowheadSize`, which is what that
/// pad is sized from.
#[test]
fn no_mark_reaches_past_the_culling_pad() {
    for (points, ops) in bowed_corpus(24) {
        for head in HEADS {
            for position in [Position::Start, Position::End] {
                let tip = endpoint(&points, position);
                for shape in arrowhead_shapes(&points, STROKE_WIDTH, &ops, position, head) {
                    for vertex in sampled(&shape) {
                        let reach = distance(vertex, tip);
                        assert!(
                            reach <= MAX_ARROWHEAD_REACH,
                            "{head:?} at {position:?} reaches {reach}, past the \
                             {MAX_ARROWHEAD_REACH} pad culling pads by",
                        );
                    }
                }
            }
        }
    }
}

/// A short line clamps the mark rather than letting it overhang: `minSize = min(size, length ×
/// 0.5)` (`:815-816`) is the whole of the oracle's scale-down, and `getArrowheadSize` is read
/// nowhere else (`:714-732` has exactly one caller, `:812`) — so a mark never counts against the
/// line's length, and the line is drawn to its own endpoint with the mark over it.
#[test]
fn a_mark_is_clamped_to_half_its_line() {
    for (index, (points, ops)) in bowed_corpus(24).into_iter().enumerate() {
        let half = distance(points[0], points[1]) / 2.0;
        for position in [Position::Start, Position::End] {
            let tip = endpoint(&points, position);
            for head in HEADS {
                let reach = distance(base_of(&points, &ops, position, head, 0.0), tip);
                assert!(
                    reach <= half + TOLERANCE,
                    "case {index}: a {head:?} at {position:?} reaches {reach} from the tip of a \
                     half-{half} line",
                );
            }
        }
    }
}

/// Moving the whole line moves the mark with it, by exactly as much.
///
/// This is the anchoring property, stated so it does not need to know which way the line points:
/// every coordinate a head uses is either the tip, a point on the body's curve, or a distance
/// from one of those, so the whole drawing is a function of the line alone. A mark anchored to a
/// screen position, to `points` while the rest of the geometry uses the curve, or to a stale
/// cached ops list fails it — and so does a mark translated by a few pixels.
#[test]
fn moving_the_line_moves_every_mark_with_it() {
    let shift = [37.0, -64.0];
    for (points, ops) in bowed_corpus(12) {
        let moved: Vec<[f64; 2]> = points
            .iter()
            .map(|p| [p[0] + shift[0], p[1] + shift[1]])
            .collect();
        let moved_ops = move_ops(&ops, shift);
        for head in HEADS {
            for position in [Position::Start, Position::End] {
                let shapes = arrowhead_shapes(&points, STROKE_WIDTH, &ops, position, head);
                let after = arrowhead_shapes(&moved, STROKE_WIDTH, &moved_ops, position, head);
                assert_eq!(
                    shapes.len(),
                    after.len(),
                    "{head:?} at {position:?} drew a different number of parts once moved",
                );
                for (was, now) in shapes
                    .iter()
                    .flat_map(outline)
                    .zip(after.iter().flat_map(outline))
                {
                    assert!(
                        near(now[0] - was[0], shift[0]) && near(now[1] - was[1], shift[1]),
                        "{head:?} at {position:?}: {was:?} moved to {now:?}, not by {shift:?}",
                    );
                }
            }
        }
    }
}

/// Turning the whole line about its own endpoint turns every mark by the same angle about that
/// same point — the orientation property, and the one a mark placed in a fixed frame, or mirrored
/// rather than rotated, cannot pass.
///
/// Held on **bowed** bodies, where the oracle's axis is the curve's direction and not the chord's,
/// so this is a statement about the whole placement rather than a coincidence of the two.
#[test]
fn turning_the_line_turns_every_mark_about_its_own_endpoint() {
    let turn: f64 = 0.7;
    let (sin, cos) = turn.sin_cos();
    for (points, ops) in bowed_corpus(12) {
        for head in HEADS {
            for position in [Position::Start, Position::End] {
                let about = endpoint(&points, position);
                let spin = |p: [f64; 2]| {
                    let (x, y) = (p[0] - about[0], p[1] - about[1]);
                    [x * cos - y * sin + about[0], x * sin + y * cos + about[1]]
                };
                let (turned_points, turned_ops) = turn_body(&points, &ops, &spin);
                let shapes = arrowhead_shapes(&points, STROKE_WIDTH, &ops, position, head);
                let after =
                    arrowhead_shapes(&turned_points, STROKE_WIDTH, &turned_ops, position, head);
                for (was, now) in shapes
                    .iter()
                    .flat_map(outline)
                    .zip(after.iter().flat_map(outline))
                {
                    let want = spin(was);
                    assert!(
                        near(now[0], want[0]) && near(now[1], want[1]),
                        "{head:?} at {position:?}: {was:?} turned to {now:?}, not to {want:?}",
                    );
                }
            }
        }
    }
}

/// A body's own points and ops, each put through `f`.
fn turn_body(
    points: &[[f64; 2]],
    ops: &[Op],
    f: &impl Fn([f64; 2]) -> [f64; 2],
) -> (Vec<[f64; 2]>, Vec<Op>) {
    let moved: Vec<[f64; 2]> = points.iter().copied().map(f).collect();
    (moved, ops.iter().map(|op| move_op(op, f)).collect())
}

/// One op put through `f`: a `Move` moves, and a curve's six numbers are three points.
fn move_op(op: &Op, f: &impl Fn([f64; 2]) -> [f64; 2]) -> Op {
    match op {
        Op::Move(p) => Op::Move(f(*p)),
        Op::BCurveTo(d) => {
            let mut out = [0.0; 6];
            for (i, p) in [0, 2, 4].into_iter().enumerate() {
                let moved = f([d[p], d[p + 1]]);
                out[i * 2] = moved[0];
                out[i * 2 + 1] = moved[1];
            }
            Op::BCurveTo(out)
        }
        other => *other,
    }
}

fn move_ops(ops: &[Op], shift: [f64; 2]) -> Vec<Op> {
    let f = |p: [f64; 2]| [p[0] + shift[0], p[1] + shift[1]];
    ops.iter().map(|op| move_op(op, &f)).collect()
}

/// The crow's-foot's two leg ends. `generateArrowheadLinesToTip` draws both strokes to the shared
/// `[x2, y2]`, so the ink is each pair's first point.
fn legs_of(fork: &[ArrowheadPrimitive]) -> [[f64; 2]; 2] {
    fork.iter()
        .map(|s| match s {
            ArrowheadPrimitive::Line([leg, _]) => *leg,
            other => panic!("a crow's foot is two lines, got {other:?}"),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or_else(|_| panic!("a crow's foot is two lines, got {fork:?}"))
}

/// A mark that is one line — `bar`, `cardinality_one`, or either of them inside a compound.
fn single_line(shapes: &[ArrowheadPrimitive]) -> [[f64; 2]; 2] {
    match shapes {
        [ArrowheadPrimitive::Line(pair)] => *pair,
        other => panic!("expected one line, got {other:?}"),
    }
}

/// The circle a mark carries, if it has one — the "zero" half of a zero-or mark.
fn circle_of(shapes: &[ArrowheadPrimitive]) -> Option<ArrowheadPrimitive> {
    shapes
        .iter()
        .find(|s| matches!(s, ArrowheadPrimitive::Circle { .. }))
        .cloned()
}

/// What a board saved with a legacy arrowhead **loads as**, pinned through the door a file arrives
/// at.
///
/// The oracle normalizes on the way in (`arrowheads.ts@1118751f:3-21`, from
/// `restore.ts@1118751f:616-617,657-661`) and this engine does the same with serde aliases
/// (`scene/element.rs:77,84,86,88`), so `crowfoot_one` becomes `cardinality_one` in memory — the
/// modern mark, drawn with the modern geometry — and the next write spells it the modern way. The
/// legacy names are inputs only, which is why `Arrowhead` in `engine/src/types.ts:47-63` and
/// `ARROWHEADS` in `packages/contract/src/element.ts:48` carry the six and the four appears only in
/// `LEGACY_ARROWHEADS` (`:72-77`), never in what the front may write.
#[test]
fn a_saved_legacy_arrowhead_loads_under_its_modern_name() {
    let legacy = board_json(&[
        ("startArrowhead", "crowfoot_one"),
        ("endArrowhead", "crowfoot_many"),
    ]);
    let saved = load_and_save(&legacy);

    for (wire, modern) in [
        ("crowfoot_one", "cardinality_one"),
        ("crowfoot_many", "cardinality_many"),
    ] {
        assert!(!saved.contains(wire), "{wire} survived the load: {saved}");
        assert!(saved.contains(modern), "{modern} is missing: {saved}");
    }
}

/// The `dot` of the legacy four is a plain circle's old name, not a cardinality mark, and it has
/// to keep rendering too. Pinned beside the crow's-foot three because a fix that only handled
/// `crowfoot_*` would pass the test above and lose a head.
#[test]
fn the_legacy_dot_loads_as_a_circle() {
    let saved = load_and_save(&board_json(&[("endArrowhead", "dot")]));
    assert!(
        !saved.contains("\"dot\""),
        "the legacy spelling survived: {saved}"
    );
    assert!(saved.contains("\"circle\""), "a head went missing: {saved}");
}

/// A board holding one arrow, with `field` set to each legacy name in turn.
fn board_json(fields: &[(&str, &str)]) -> String {
    let arrow = common::connector(0.0, 0.0, 100.0, 0.0, DrawElementType::Arrow);
    let mut doc: serde_json::Value =
        serde_json::from_str(&draw_engine::scene_to_json(&[arrow])).expect("the board parses");
    let element = &mut doc["elements"][0];
    for (field, value) in fields {
        element[*field] = serde_json::Value::String((*value).to_string());
    }
    doc.to_string()
}

/// Load a board the way a file does and hand back what the next save would write, so a test
/// reads as the round trip it is rather than as two loose assertions.
fn load_and_save(board: &str) -> String {
    let mut engine = DrawEngine::new();
    assert!(
        engine.load_scene(board),
        "a board with a legacy name did not load"
    );
    engine.export_json()
}
