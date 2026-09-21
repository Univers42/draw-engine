//! Transcription of roughjs 4.6.4 `bin/renderer.js`.
//!
//! # Rules of this file
//!
//! Every one of these has silently broken parity for somebody before:
//!
//! - **No algebraic simplification.** `(r * (max - min)) + min` stays exactly that.
//!   Floating-point addition is not associative, so rearranging changes the last bits.
//! - **No `mul_add`.** FMA is *more accurate* than a separate multiply and add, and
//!   therefore different. Clippy's `suboptimal_flops` actively suggests it, which is
//!   why the crate root denies that lint.
//! - **`Math.pow(x, 2)` is written `x * x`**, which is exactly equal and faster.
//! - **RNG call order is load-bearing.** JS evaluates arguments and array elements
//!   left to right; every draw here is bound to a `let` in that order so the sequence
//!   is visible rather than dependent on Rust's evaluation rules.
//! - **Loops accumulate their counter** (`angle = angle + increment`) rather than
//!   computing `start + i * increment`. The two differ once rounding creeps in, and
//!   they can even differ in iteration *count*.

use crate::ops::{Op, OpSet};
use crate::options::Ctx;

/// JS `x || fallback` for a number: `0` and `NaN` are falsy.
#[inline]
fn js_or(x: f64, fallback: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        fallback
    } else {
        x
    }
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// `line(x1, y1, x2, y2, o)`
pub fn line(x1: f64, y1: f64, x2: f64, y2: f64, c: &mut Ctx) -> OpSet {
    OpSet::path(double_line(x1, y1, x2, y2, c, false))
}

/// `linearPath(points, close, o)`
pub fn linear_path(points: &[[f64; 2]], close: bool, c: &mut Ctx) -> OpSet {
    let len = points.len();
    if len > 2 {
        let mut ops = Vec::new();
        for i in 0..(len - 1) {
            ops.extend(double_line(
                points[i][0],
                points[i][1],
                points[i + 1][0],
                points[i + 1][1],
                c,
                false,
            ));
        }
        if close {
            ops.extend(double_line(
                points[len - 1][0],
                points[len - 1][1],
                points[0][0],
                points[0][1],
                c,
                false,
            ));
        }
        OpSet::path(ops)
    } else if len == 2 {
        line(points[0][0], points[0][1], points[1][0], points[1][1], c)
    } else {
        OpSet::path(Vec::new())
    }
}

/// `polygon(points, o)`
pub fn polygon(points: &[[f64; 2]], c: &mut Ctx) -> OpSet {
    linear_path(points, true, c)
}

/// `rectangle(x, y, width, height, o)`
pub fn rectangle(x: f64, y: f64, width: f64, height: f64, c: &mut Ctx) -> OpSet {
    let points = [
        [x, y],
        [x + width, y],
        [x + width, y + height],
        [x, y + height],
    ];
    polygon(&points, c)
}

/// `curve(points, o)`
pub fn curve(points: &[[f64; 2]], c: &mut Ctx) -> OpSet {
    let mut o1 = curve_with_offset(points, 1.0 * (1.0 + c.o.roughness * 0.2), c);
    if !c.o.disable_multi_stroke {
        let mut c2 = c.clone_alter_seed();
        let o2 = curve_with_offset(points, 1.5 * (1.0 + c.o.roughness * 0.22), &mut c2);
        o1.extend(o2);
    }
    OpSet::path(o1)
}

/// The `{increment, rx, ry}` an ellipse is built from.
///
/// Split out exactly as rough does, because a pattern-filled ellipse generates the
/// params once and reuses them for both the outline and the fill — sharing that
/// struct is what keeps the two aligned.
#[derive(Clone, Copy, Debug)]
pub struct EllipseParams {
    pub increment: f64,
    pub rx: f64,
    pub ry: f64,
}

/// `generateEllipseParams(width, height, o)`
pub fn generate_ellipse_params(width: f64, height: f64, c: &mut Ctx) -> EllipseParams {
    let psq = (std::f64::consts::PI
        * 2.0
        * (((width / 2.0) * (width / 2.0) + (height / 2.0) * (height / 2.0)) / 2.0).sqrt())
    .sqrt();
    let step_count =
        c.o.curve_step_count
            .max((c.o.curve_step_count / (200.0f64).sqrt()) * psq)
            .ceil();
    let increment = (std::f64::consts::PI * 2.0) / step_count;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    let curve_fit_randomness = 1.0 - c.o.curve_fitting;
    rx += c.offset_opt(rx * curve_fit_randomness, 1.0);
    ry += c.offset_opt(ry * curve_fit_randomness, 1.0);
    EllipseParams { increment, rx, ry }
}

/// `ellipseWithParams(x, y, o, ellipseParams)` — returns `(opset, estimatedPoints)`.
///
/// The estimated points are the polygon a pattern fill is computed against, which is
/// why they are returned rather than discarded.
pub fn ellipse_with_params(
    x: f64,
    y: f64,
    c: &mut Ctx,
    p: EllipseParams,
) -> (OpSet, Vec<[f64; 2]>) {
    // `_offset(0.1, _offset(0.4, 1, o), o)`: JS evaluates the inner call first, so the
    // inner draw precedes the outer one. Binding it makes that order explicit.
    let inner = c.offset(0.4, 1.0, 1.0);
    let overlap = p.increment * c.offset(0.1, inner, 1.0);

    let (ap1, cp1) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.0, overlap, c);
    let mut o1 = curve_points(&ap1, None, c);

    if !c.o.disable_multi_stroke && c.o.roughness != 0.0 {
        let (ap2, _) = compute_ellipse_points(p.increment, x, y, p.rx, p.ry, 1.5, 0.0, c);
        let o2 = curve_points(&ap2, None, c);
        o1.extend(o2);
    }

    (OpSet::path(o1), cp1)
}

/// `ellipse(x, y, width, height, o)`
pub fn ellipse(x: f64, y: f64, width: f64, height: f64, c: &mut Ctx) -> OpSet {
    let params = generate_ellipse_params(width, height, c);
    ellipse_with_params(x, y, c, params).0
}

/// `solidFillPolygon(polygonList, o)`
pub fn solid_fill_polygon(polygon_list: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    let mut ops = Vec::new();
    for points in polygon_list {
        if !points.is_empty() {
            let offset = js_or(c.o.max_randomness_offset, 0.0);
            let len = points.len();
            if len > 2 {
                let dx = c.offset_opt(offset, 1.0);
                let dy = c.offset_opt(offset, 1.0);
                ops.push(Op::Move([points[0][0] + dx, points[0][1] + dy]));
                for p in points.iter().take(len).skip(1) {
                    let dx = c.offset_opt(offset, 1.0);
                    let dy = c.offset_opt(offset, 1.0);
                    ops.push(Op::LineTo([p[0] + dx, p[1] + dy]));
                }
            }
        }
    }
    OpSet::fill_path(ops)
}

/// `doubleLineFillOps(x1, y1, x2, y2, o)` — the entry point the fillers use.
pub fn double_line_fill_ops(x1: f64, y1: f64, x2: f64, y2: f64, c: &mut Ctx) -> Vec<Op> {
    double_line(x1, y1, x2, y2, c, true)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// `_doubleLine(x1, y1, x2, y2, o, filling)`
fn double_line(x1: f64, y1: f64, x2: f64, y2: f64, c: &mut Ctx, filling: bool) -> Vec<Op> {
    let single_stroke = if filling {
        c.o.disable_multi_stroke_fill
    } else {
        c.o.disable_multi_stroke
    };
    let o1 = line_ops(x1, y1, x2, y2, c, true, false);
    if single_stroke {
        return o1;
    }
    let o2 = line_ops(x1, y1, x2, y2, c, true, true);
    let mut out = o1;
    out.extend(o2);
    out
}

/// `_line(x1, y1, x2, y2, o, move, overlay)`
///
/// The RNG draw order, which must not change: `divergePoint`, `midDispX`, `midDispY`,
/// then two for the `move` (only when `move` is set), then four for the control
/// points, then two more for the endpoint — the last two only when `preserveVertices`
/// is false.
fn line_ops(x1: f64, y1: f64, x2: f64, y2: f64, c: &mut Ctx, mv: bool, overlay: bool) -> Vec<Op> {
    let length_sq = (x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2);
    let length = length_sq.sqrt();

    let roughness_gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        (-0.0016668) * length + 1.233334
    };

    let mut offset = js_or(c.o.max_randomness_offset, 0.0);
    if (offset * offset * 100.0) > length_sq {
        offset = length / 10.0;
    }
    let half_offset = offset / 2.0;

    let diverge_point = 0.2 + c.random() * 0.2;

    let mid_disp_x0 = c.o.bowing * c.o.max_randomness_offset * (y2 - y1) / 200.0;
    let mid_disp_y0 = c.o.bowing * c.o.max_randomness_offset * (x1 - x2) / 200.0;
    let mid_disp_x = c.offset_opt(mid_disp_x0, roughness_gain);
    let mid_disp_y = c.offset_opt(mid_disp_y0, roughness_gain);

    let preserve = c.o.preserve_vertices;
    let mut ops = Vec::new();

    if mv {
        if overlay {
            let dx = if preserve {
                0.0
            } else {
                c.offset_opt(half_offset, roughness_gain)
            };
            let dy = if preserve {
                0.0
            } else {
                c.offset_opt(half_offset, roughness_gain)
            };
            ops.push(Op::Move([x1 + dx, y1 + dy]));
        } else {
            let dx = if preserve {
                0.0
            } else {
                c.offset_opt(offset, roughness_gain)
            };
            let dy = if preserve {
                0.0
            } else {
                c.offset_opt(offset, roughness_gain)
            };
            ops.push(Op::Move([x1 + dx, y1 + dy]));
        }
    }

    // The overlay pass perturbs by half the offset; the base pass by the full offset.
    let amp = if overlay { half_offset } else { offset };

    let r1 = c.offset_opt(amp, roughness_gain);
    let r2 = c.offset_opt(amp, roughness_gain);
    let r3 = c.offset_opt(amp, roughness_gain);
    let r4 = c.offset_opt(amp, roughness_gain);
    let r5 = if preserve {
        0.0
    } else {
        c.offset_opt(amp, roughness_gain)
    };
    let r6 = if preserve {
        0.0
    } else {
        c.offset_opt(amp, roughness_gain)
    };

    ops.push(Op::BCurveTo([
        mid_disp_x + x1 + (x2 - x1) * diverge_point + r1,
        mid_disp_y + y1 + (y2 - y1) * diverge_point + r2,
        mid_disp_x + x1 + 2.0 * (x2 - x1) * diverge_point + r3,
        mid_disp_y + y1 + 2.0 * (y2 - y1) * diverge_point + r4,
        x2 + r5,
        y2 + r6,
    ]));

    ops
}

/// `_curveWithOffset(points, offset, o)`
fn curve_with_offset(points: &[[f64; 2]], offset: f64, c: &mut Ctx) -> Vec<Op> {
    let mut ps: Vec<[f64; 2]> = Vec::new();

    // The first point is pushed twice, each with its own pair of draws — not one pair
    // reused. Four draws, not two.
    let a = c.offset_opt(offset, 1.0);
    let b = c.offset_opt(offset, 1.0);
    ps.push([points[0][0] + a, points[0][1] + b]);
    let a = c.offset_opt(offset, 1.0);
    let b = c.offset_opt(offset, 1.0);
    ps.push([points[0][0] + a, points[0][1] + b]);

    for i in 1..points.len() {
        let a = c.offset_opt(offset, 1.0);
        let b = c.offset_opt(offset, 1.0);
        ps.push([points[i][0] + a, points[i][1] + b]);
        if i == points.len() - 1 {
            let a = c.offset_opt(offset, 1.0);
            let b = c.offset_opt(offset, 1.0);
            ps.push([points[i][0] + a, points[i][1] + b]);
        }
    }

    curve_points(&ps, None, c)
}

/// `_curve(points, closePoint, o)`
fn curve_points(points: &[[f64; 2]], close_point: Option<[f64; 2]>, c: &mut Ctx) -> Vec<Op> {
    let len = points.len();
    let mut ops = Vec::new();

    if len > 3 {
        let s = 1.0 - c.o.curve_tightness;
        ops.push(Op::Move([points[1][0], points[1][1]]));
        let mut i = 1;
        while i + 2 < len {
            let cached = points[i];
            let b1 = [
                cached[0] + (s * points[i + 1][0] - s * points[i - 1][0]) / 6.0,
                cached[1] + (s * points[i + 1][1] - s * points[i - 1][1]) / 6.0,
            ];
            let b2 = [
                points[i + 1][0] + (s * points[i][0] - s * points[i + 2][0]) / 6.0,
                points[i + 1][1] + (s * points[i][1] - s * points[i + 2][1]) / 6.0,
            ];
            let b3 = [points[i + 1][0], points[i + 1][1]];
            ops.push(Op::BCurveTo([b1[0], b1[1], b2[0], b2[1], b3[0], b3[1]]));
            i += 1;
        }
        if let Some(cp) = close_point {
            let ro = c.o.max_randomness_offset;
            let dx = c.offset_opt(ro, 1.0);
            let dy = c.offset_opt(ro, 1.0);
            ops.push(Op::LineTo([cp[0] + dx, cp[1] + dy]));
        }
    } else if len == 3 {
        ops.push(Op::Move([points[1][0], points[1][1]]));
        ops.push(Op::BCurveTo([
            points[1][0],
            points[1][1],
            points[2][0],
            points[2][1],
            points[2][0],
            points[2][1],
        ]));
    } else if len == 2 {
        ops.extend(double_line(
            points[0][0],
            points[0][1],
            points[1][0],
            points[1][1],
            c,
            false,
        ));
    }

    ops
}

/// `_computeEllipsePoints(increment, cx, cy, rx, ry, offset, overlap, o)`
///
/// Returns `(allPoints, corePoints)`.
#[allow(clippy::too_many_arguments)]
fn compute_ellipse_points(
    increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    offset: f64,
    overlap: f64,
    c: &mut Ctx,
) -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
    let core_only = c.o.roughness == 0.0;
    let mut core_points: Vec<[f64; 2]> = Vec::new();
    let mut all_points: Vec<[f64; 2]> = Vec::new();

    if core_only {
        let increment = increment / 4.0;
        all_points.push([cx + rx * (-increment).cos(), cy + ry * (-increment).sin()]);
        let mut angle = 0.0f64;
        while angle <= std::f64::consts::PI * 2.0 {
            let p = [cx + rx * angle.cos(), cy + ry * angle.sin()];
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }
        all_points.push([cx + rx * (0.0f64).cos(), cy + ry * (0.0f64).sin()]);
        all_points.push([cx + rx * increment.cos(), cy + ry * increment.sin()]);
    } else {
        let rad_offset = c.offset_opt(0.5, 1.0) - (std::f64::consts::PI / 2.0);

        let a = c.offset_opt(offset, 1.0);
        let b = c.offset_opt(offset, 1.0);
        all_points.push([
            a + cx + 0.9 * rx * (rad_offset - increment).cos(),
            b + cy + 0.9 * ry * (rad_offset - increment).sin(),
        ]);

        let end_angle = std::f64::consts::PI * 2.0 + rad_offset - 0.01;
        let mut angle = rad_offset;
        while angle < end_angle {
            let a = c.offset_opt(offset, 1.0);
            let b = c.offset_opt(offset, 1.0);
            let p = [a + cx + rx * angle.cos(), b + cy + ry * angle.sin()];
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }

        let a = c.offset_opt(offset, 1.0);
        let b = c.offset_opt(offset, 1.0);
        all_points.push([
            a + cx + rx * (rad_offset + std::f64::consts::PI * 2.0 + overlap * 0.5).cos(),
            b + cy + ry * (rad_offset + std::f64::consts::PI * 2.0 + overlap * 0.5).sin(),
        ]);

        let a = c.offset_opt(offset, 1.0);
        let b = c.offset_opt(offset, 1.0);
        all_points.push([
            a + cx + 0.98 * rx * (rad_offset + overlap).cos(),
            b + cy + 0.98 * ry * (rad_offset + overlap).sin(),
        ]);

        let a = c.offset_opt(offset, 1.0);
        let b = c.offset_opt(offset, 1.0);
        all_points.push([
            a + cx + 0.9 * rx * (rad_offset + overlap * 0.5).cos(),
            b + cy + 0.9 * ry * (rad_offset + overlap * 0.5).sin(),
        ]);
    }

    (all_points, core_points)
}
