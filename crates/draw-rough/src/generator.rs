//! Transcription of roughjs 4.6.4 `bin/generator.js` — the `RoughGenerator` shape
//! methods that turn a shape plus options into a [`Drawable`].
//!
//! # The ordering trap
//!
//! Every filled shape computes its **outline first** and pushes it **last**:
//!
//! ```js
//! const outline = rectangle(x, y, width, height, o);   // 1. consumes randomness
//! if (o.fill) { paths.push(patternFillPolygons(...)); } // 2. consumes randomness
//! if (o.stroke !== NOS) { paths.push(outline); }        // 3. but paints on top
//! ```
//!
//! So the random stream is consumed outline-then-fill while the op sets come back
//! fill-then-outline. Swapping either order produces a shape that is structurally
//! identical and numerically wrong — the kind of bug that passes an op-count check and
//! fails only on coordinates.
//!
//! It also means a fill is **not reproducible on its own**: rough decides the hachure
//! `skipOffset` from `o.randomizer?.next() || Math.random()`, so a fill computed
//! without an outline first falls back to `Math.random()` and is non-deterministic even
//! in rough itself. Always go through this module, never straight to the fillers.

use crate::fillers::pattern_fill_polygons;
use crate::ops::{Drawable, Op, OpSet, OpSetKind};
use crate::options::{Ctx, FillStyle, Options};
use crate::renderer;
use crate::renderer::Segment;

/// `_mergedShape(input)` — drops every `move` after the first, so a multi-pass stroke
/// becomes one continuous fillable path instead of several disjoint ones.
fn merged_shape(ops: Vec<Op>) -> Vec<Op> {
    ops.into_iter()
        .enumerate()
        .filter(|(i, op)| *i == 0 || !matches!(op, Op::Move(_)))
        .map(|(_, op)| op)
        .collect()
}

/// `generator.line(x1, y1, x2, y2, options)`
pub fn line(x1: f64, y1: f64, x2: f64, y2: f64, o: Options) -> Drawable {
    let mut c = Ctx::new(o);
    Drawable {
        shape: "line",
        sets: vec![renderer::line(x1, y1, x2, y2, &mut c)],
    }
}

/// `generator.rectangle(x, y, width, height, options)`
pub fn rectangle(x: f64, y: f64, width: f64, height: f64, o: Options) -> Drawable {
    let mut c = Ctx::new(o);
    let mut paths = Vec::new();

    let outline = renderer::rectangle(x, y, width, height, &mut c);

    if c.o.filled {
        let points = vec![
            [x, y],
            [x + width, y],
            [x + width, y + height],
            [x, y + height],
        ];
        if c.o.fill_style == FillStyle::Solid {
            paths.push(renderer::solid_fill_polygon(&[points], &mut c));
        } else {
            paths.push(pattern_fill_polygons(&[points], &mut c));
        }
    }

    paths.push(outline);
    Drawable {
        shape: "rectangle",
        sets: paths,
    }
}

/// `generator.polygon(points, options)`
pub fn polygon(points: &[[f64; 2]], o: Options) -> Drawable {
    let mut c = Ctx::new(o);
    let mut paths = Vec::new();

    let outline = renderer::linear_path(points, true, &mut c);

    if c.o.filled {
        let pts = points.to_vec();
        if c.o.fill_style == FillStyle::Solid {
            paths.push(renderer::solid_fill_polygon(&[pts], &mut c));
        } else {
            paths.push(pattern_fill_polygons(&[pts], &mut c));
        }
    }

    paths.push(outline);
    Drawable {
        shape: "polygon",
        sets: paths,
    }
}

/// `generator.linearPath(points, options)` — never filled.
pub fn linear_path(points: &[[f64; 2]], o: Options) -> Drawable {
    let mut c = Ctx::new(o);
    Drawable {
        shape: "linearPath",
        sets: vec![renderer::linear_path(points, false, &mut c)],
    }
}

/// `generator.ellipse(x, y, width, height, options)`
///
/// Note the solid-fill branch calls `ellipseWithParams` a **second** time with the same
/// params — reusing the geometry but advancing the random stream again, so the fill is
/// a differently-jittered copy of the outline rather than the same path reused.
pub fn ellipse(x: f64, y: f64, width: f64, height: f64, o: Options) -> Drawable {
    let mut c = Ctx::new(o);
    let mut paths = Vec::new();

    let params = renderer::generate_ellipse_params(width, height, &mut c);
    let (outline, estimated_points) = renderer::ellipse_with_params(x, y, &mut c, params);

    if c.o.filled {
        if c.o.fill_style == FillStyle::Solid {
            let (mut shape, _) = renderer::ellipse_with_params(x, y, &mut c, params);
            shape.kind = OpSetKind::FillPath;
            paths.push(shape);
        } else {
            paths.push(pattern_fill_polygons(&[estimated_points], &mut c));
        }
    }

    paths.push(outline);
    Drawable {
        shape: "ellipse",
        sets: paths,
    }
}

/// `generator.curve(points, options)`
///
/// # Known gap: pattern fill
///
/// Solid fill and the unfilled stroke are exact. A **pattern** fill is approximated:
/// rough runs `curveToBezier` then `pointsOnBezierCurves` to flatten the curve before
/// hatching it, and neither is ported, so the control polygon is hatched instead. The
/// hachure lines are therefore clipped slightly inside the curve where it bows outward.
/// Same trade, and the same reason, as the flattening in [`path`].
///
/// This used to abort instead, on the stated grounds that only a filled freedraw could
/// reach it and this engine produced none. That was wrong: a line with three or more
/// points is drawn as a curve, so giving any hand-drawn polyline a background in the
/// inspector reached it — and a `todo!()` in WASM is not an error, it is the module
/// aborting and the board freezing until the page is reloaded.
pub fn curve(points: &[[f64; 2]], o: Options) -> Drawable {
    let mut c = Ctx::new(o);
    let mut paths = Vec::new();

    let outline = renderer::curve(points, &mut c);

    if c.o.filled && points.len() >= 3 {
        if c.o.fill_style == FillStyle::Solid {
            // A softer, single-pass copy: `roughness + fillShapeRoughnessGain`, or 0
            // when roughness is 0 — `o.roughness ? … : 0` in the JS.
            let saved = c.o;
            c.o.disable_multi_stroke = true;
            c.o.roughness = if saved.roughness != 0.0 {
                saved.roughness + saved.fill_shape_roughness_gain
            } else {
                0.0
            };
            let fill_shape = renderer::curve(points, &mut c);
            // No restore: `c` is not consulted again, and the fill deliberately shares
            // (and advances) the same random stream the outline used.
            let _ = saved;

            paths.push(OpSet {
                kind: OpSetKind::FillPath,
                ops: merged_shape(fill_shape.ops),
            });
        } else {
            // The control polygon, not the flattened curve — see the note above.
            paths.push(pattern_fill_polygons(&[points.to_vec()], &mut c));
        }
    }

    paths.push(outline);
    Drawable {
        shape: "curve",
        sets: paths,
    }
}

/// `generator.path(d, options)`, taking pre-normalized segments instead of a string.
///
/// Excalidraw builds its rounded-rectangle and rounded-diamond `d` programmatically and
/// hands it to rough, which parses it straight back. We skip the round trip: the caller
/// supplies the segments `normalize(absolutize(parsePath(d)))` would have produced,
/// which is faster and removes a parser from the trust chain.
///
/// # Known gap: pattern fill
///
/// A **solid** fill matches rough exactly. A **pattern** fill (hachure and friends) does
/// not: rough flattens the path to a polygon with `pointsOnPath`, whose adaptive Bézier
/// subdivision is not ported yet, so the polygon this uses is a uniform flattening
/// instead. The fill therefore sits very slightly differently inside a rounded shape
/// with a non-solid background. The outline — which is what the eye reads — is exact,
/// and the oracle covers it; this gap is tracked rather than hidden.
pub fn path(segments: &[Segment], o: Options) -> Drawable {
    let mut c = Ctx::new(o);
    let mut paths = Vec::new();

    if segments.is_empty() {
        return Drawable {
            shape: "path",
            sets: paths,
        };
    }

    let outline = renderer::svg_path(segments, &mut c);

    if c.o.filled {
        if c.o.fill_style == FillStyle::Solid {
            let saved = c.o;
            c.o.disable_multi_stroke = true;
            c.o.roughness = if saved.roughness != 0.0 {
                saved.roughness + saved.fill_shape_roughness_gain
            } else {
                0.0
            };
            let fill_shape = renderer::svg_path(segments, &mut c);
            // No restore: `c` is not consulted again, and the fill deliberately shares
            // (and advances) the same random stream the outline used.
            let _ = saved;

            paths.push(OpSet {
                kind: OpSetKind::FillPath,
                ops: merged_shape(fill_shape.ops),
            });
        } else {
            let polygon = flatten_segments(segments, 16);
            paths.push(pattern_fill_polygons(&[polygon], &mut c));
        }
    }

    paths.push(outline);
    Drawable {
        shape: "path",
        sets: paths,
    }
}

/// Flattens path segments into a polygon by sampling each cubic uniformly.
///
/// See the note on [`path`]: rough uses `pointsOnPath`'s adaptive subdivision here, so
/// this is an approximation used only to decide where a pattern fill's hachure lines are
/// clipped. `steps` is fixed rather than tolerance-driven precisely so the result is
/// deterministic and cheap; the visible effect is confined to the corners of a
/// hachure-filled rounded shape.
fn flatten_segments(segments: &[Segment], steps: usize) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    let mut cur = [0.0, 0.0];
    let mut first = [0.0, 0.0];

    for seg in segments {
        match *seg {
            Segment::MoveTo(p) => {
                cur = p;
                first = p;
                out.push(p);
            }
            Segment::LineTo(p) => {
                out.push(p);
                cur = p;
            }
            Segment::CurveTo([x1, y1, x2, y2, x, y]) => {
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let mt = 1.0 - t;
                    // de Casteljau, written out: B(t) = (1-t)^3 P0 + 3(1-t)^2 t P1 +
                    // 3(1-t) t^2 P2 + t^3 P3
                    let a = mt * mt * mt;
                    let b = 3.0 * mt * mt * t;
                    let c2 = 3.0 * mt * t * t;
                    let d = t * t * t;
                    out.push([
                        a * cur[0] + b * x1 + c2 * x2 + d * x,
                        a * cur[1] + b * y1 + c2 * y2 + d * y,
                    ]);
                }
                cur = [x, y];
            }
            Segment::Close => {
                out.push(first);
                cur = first;
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(fill: bool) -> Options {
        Options {
            seed: 12345,
            filled: fill,
            ..Default::default()
        }
    }

    /// The fill is painted under the outline, so it must come first in `sets` — even
    /// though it is computed second.
    #[test]
    fn filled_shapes_put_the_fill_before_the_outline() {
        let d = rectangle(0.0, 0.0, 100.0, 60.0, opts(true));
        assert_eq!(d.sets.len(), 2);
        assert_eq!(d.sets[0].kind, OpSetKind::FillSketch);
        assert_eq!(d.sets[1].kind, OpSetKind::Path);
    }

    #[test]
    fn unfilled_shapes_emit_only_an_outline() {
        let d = rectangle(0.0, 0.0, 100.0, 60.0, opts(false));
        assert_eq!(d.sets.len(), 1);
        assert_eq!(d.sets[0].kind, OpSetKind::Path);
    }

    /// Computing the outline first is what seeds the randomizer, so the outline of a
    /// filled shape must be identical to the outline of the same unfilled shape.
    #[test]
    fn filling_does_not_disturb_the_outline() {
        let filled = rectangle(0.0, 0.0, 100.0, 60.0, opts(true));
        let plain = rectangle(0.0, 0.0, 100.0, 60.0, opts(false));
        assert_eq!(filled.sets.last().unwrap().ops, plain.sets[0].ops);
    }

    #[test]
    fn merged_shape_keeps_only_the_leading_move() {
        let ops = vec![
            Op::Move([0.0, 0.0]),
            Op::LineTo([1.0, 1.0]),
            Op::Move([2.0, 2.0]),
            Op::LineTo([3.0, 3.0]),
        ];
        let merged = merged_shape(ops);
        assert_eq!(merged.len(), 3);
        assert!(matches!(merged[0], Op::Move(_)));
        assert!(!merged[1..].iter().any(|op| matches!(op, Op::Move(_))));
    }
}
