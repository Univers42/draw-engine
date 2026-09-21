//! Transcription of roughjs 4.6.4 `bin/fillers/*`.
//!
//! Every pattern fill starts from [`polygon_hachure_lines`] — a set of parallel spans
//! clipped to the polygon — and then decorates those spans: hachure strokes them,
//! zigzag splays each into a V, cross-hatch runs the whole thing twice at 90°, dashed
//! chops them up, and dots places ellipses along them.
//!
//! ## `dots` is not reproducible
//!
//! `DotFiller` calls `Math.random()` for every dot, with no seed involved. That makes
//! it non-deterministic **in rough.js itself** — two runs of rough disagree with each
//! other. We therefore cannot match it and neither can anyone else; the port draws
//! from the seeded stream instead so at least *our* output is stable, and the oracle
//! checks only the structure of a dots fill. Excalidraw does not expose this fill
//! style, so nothing user-visible depends on it.

use crate::geometry::{line_length, Line};
use crate::hachure::hachure_lines;
use crate::ops::{Op, OpSet};
use crate::options::{Ctx, FillStyle};
use crate::renderer;

/// `polygonHachureLines(polygonList, o)`
///
/// Note the `+ 90`: rough's `hachureAngle` is measured against the fill direction,
/// while `hachure-fill` wants the scanline direction.
fn polygon_hachure_lines(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> Vec<Line> {
    let angle = c.o.hachure_angle + 90.0;
    let mut gap = c.o.hachure_gap;
    if gap < 0.0 {
        gap = c.o.stroke_width * 4.0;
    }
    gap = gap.max(0.1);

    let mut skip_offset = 1.0;
    if c.o.roughness >= 1.0 {
        // `(o.randomizer?.next() || Math.random()) > 0.7`
        //
        // Two JS subtleties, both preserved: the draw only happens at roughness >= 1,
        // so the random stream advances differently for a smooth shape than a rough
        // one; and `||` means a `next()` of exactly 0 would fall through to
        // Math.random(), which only occurs on a zero seed.
        if c.random() > 0.7 {
            skip_offset = gap;
        }
    }

    hachure_lines(polygons.to_vec(), gap, angle, skip_offset)
}

/// Strokes each span with rough's double line — `HachureFiller.renderLines`.
fn render_lines(lines: &[Line], c: &mut Ctx) -> Vec<Op> {
    let mut ops = Vec::new();
    for line in lines {
        ops.extend(renderer::double_line_fill_ops(
            line[0][0], line[0][1], line[1][0], line[1][1], c,
        ));
    }
    ops
}

/// `HachureFiller._fillPolygons`
fn fill_hachure(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    let lines = polygon_hachure_lines(polygons, c);
    OpSet::fill_sketch(render_lines(&lines, c))
}

/// `HatchFiller.fillPolygons` — hachure twice, the second pass turned 90°.
fn fill_hatch(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    let mut set = fill_hachure(polygons, c);

    // `Object.assign({}, o, {hachureAngle: o.hachureAngle + 90})` produces a new
    // options object but **keeps the same randomizer reference**, so the second pass
    // continues the same random stream rather than restarting it.
    let saved_angle = c.o.hachure_angle;
    c.o.hachure_angle = saved_angle + 90.0;
    let set2 = fill_hachure(polygons, c);
    c.o.hachure_angle = saved_angle;

    set.ops.extend(set2.ops);
    set
}

/// `ZigZagFiller.fillPolygons`
fn fill_zigzag(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    let mut gap = c.o.hachure_gap;
    if gap < 0.0 {
        gap = c.o.stroke_width * 4.0;
    }
    gap = gap.max(0.1);

    let saved_gap = c.o.hachure_gap;
    c.o.hachure_gap = gap;
    let lines = polygon_hachure_lines(polygons, c);
    c.o.hachure_gap = saved_gap;

    let zigzag_angle = (std::f64::consts::PI / 180.0) * c.o.hachure_angle;
    let dgx = gap * 0.5 * zigzag_angle.cos();
    let dgy = gap * 0.5 * zigzag_angle.sin();

    let mut zigzag_lines: Vec<Line> = Vec::new();
    for [p1, p2] in lines.iter().copied() {
        // `if (lineLength([p1, p2]))` — zero-length spans are dropped.
        if line_length([p1, p2]) != 0.0 {
            zigzag_lines.push([[p1[0] - dgx, p1[1] + dgy], p2]);
            zigzag_lines.push([[p1[0] + dgx, p1[1] - dgy], p2]);
        }
    }

    OpSet::fill_sketch(render_lines(&zigzag_lines, c))
}

/// `DashedFiller.dashedLine`
fn fill_dashed(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    let lines = polygon_hachure_lines(polygons, c);

    let fallback = if c.o.hachure_gap < 0.0 {
        c.o.stroke_width * 4.0
    } else {
        c.o.hachure_gap
    };
    let offset = if c.o.dash_offset < 0.0 {
        fallback
    } else {
        c.o.dash_offset
    };
    let gap = if c.o.dash_gap < 0.0 {
        fallback
    } else {
        c.o.dash_gap
    };

    let mut ops = Vec::new();
    for line in &lines {
        let length = line_length(*line);
        let count = (length / (offset + gap)).floor();
        let start_offset = (length + gap - (count * (offset + gap))) / 2.0;

        let (p1, p2) = if line[0][0] > line[1][0] {
            (line[1], line[0])
        } else {
            (line[0], line[1])
        };
        let alpha = ((p2[1] - p1[1]) / (p2[0] - p1[0])).atan();

        let mut i = 0.0;
        while i < count {
            let lstart = i * (offset + gap);
            let lend = lstart + offset;
            let start = [
                p1[0] + (lstart * alpha.cos()) + (start_offset * alpha.cos()),
                p1[1] + lstart * alpha.sin() + (start_offset * alpha.sin()),
            ];
            let end = [
                p1[0] + (lend * alpha.cos()) + (start_offset * alpha.cos()),
                p1[1] + (lend * alpha.sin()) + (start_offset * alpha.sin()),
            ];
            ops.extend(renderer::double_line_fill_ops(
                start[0], start[1], end[0], end[1], c,
            ));
            i += 1.0;
        }
    }

    OpSet::fill_sketch(ops)
}

/// `ZigZagLineFiller.zigzagLines`
fn fill_zigzag_line(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    let gap = if c.o.hachure_gap < 0.0 {
        c.o.stroke_width * 4.0
    } else {
        c.o.hachure_gap
    };
    let zo = if c.o.zigzag_offset < 0.0 {
        gap
    } else {
        c.o.zigzag_offset
    };

    let saved_gap = c.o.hachure_gap;
    c.o.hachure_gap = gap + zo;
    let lines = polygon_hachure_lines(polygons, c);

    let mut ops = Vec::new();
    for line in &lines {
        let length = line_length(*line);
        // `Math.round` here, not floor — and JS's rounding, hence js_round.
        let count = crate::jsnum::js_round(length / (2.0 * zo));

        let (p1, p2) = if line[0][0] > line[1][0] {
            (line[1], line[0])
        } else {
            (line[0], line[1])
        };
        let alpha = ((p2[1] - p1[1]) / (p2[0] - p1[0])).atan();

        let mut i = 0.0;
        while i < count {
            let lstart = i * 2.0 * zo;
            let lend = (i + 1.0) * 2.0 * zo;
            let dz = (2.0 * (zo * zo)).sqrt();
            let start = [p1[0] + (lstart * alpha.cos()), p1[1] + lstart * alpha.sin()];
            let end = [p1[0] + (lend * alpha.cos()), p1[1] + (lend * alpha.sin())];
            let middle = [
                start[0] + dz * (alpha + std::f64::consts::PI / 4.0).cos(),
                start[1] + dz * (alpha + std::f64::consts::PI / 4.0).sin(),
            ];
            ops.extend(renderer::double_line_fill_ops(
                start[0], start[1], middle[0], middle[1], c,
            ));
            ops.extend(renderer::double_line_fill_ops(
                middle[0], middle[1], end[0], end[1], c,
            ));
            i += 1.0;
        }
    }

    c.o.hachure_gap = saved_gap;
    OpSet::fill_sketch(ops)
}

/// `DotFiller.dotsOnLines`
///
/// See the module note: the jitter is `Math.random()` upstream, so this cannot match
/// rough.js. We draw from the seeded stream instead, which makes our output stable
/// where rough's is not.
fn fill_dots(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    let saved_angle = c.o.hachure_angle;
    c.o.hachure_angle = 0.0;
    let lines = polygon_hachure_lines(polygons, c);

    let mut gap = c.o.hachure_gap;
    if gap < 0.0 {
        gap = c.o.stroke_width * 4.0;
    }
    gap = gap.max(0.1);

    let mut fweight = c.o.fill_weight;
    if fweight < 0.0 {
        fweight = c.o.stroke_width / 2.0;
    }

    let ro = gap / 4.0;
    let mut ops = Vec::new();
    for line in &lines {
        let length = line_length(*line);
        let dl = length / gap;
        let count = dl.ceil() - 1.0;
        let offset = length - (count * gap);
        let x = ((line[0][0] + line[1][0]) / 2.0) - (gap / 4.0);
        let min_y = line[0][1].min(line[1][1]);

        let mut i = 0.0;
        while i < count {
            let y = min_y + offset + (i * gap);
            let jx = c.random();
            let jy = c.random();
            let cx = (x - ro) + jx * 2.0 * ro;
            let cy = (y - ro) + jy * 2.0 * ro;
            let el = renderer::ellipse(cx, cy, fweight, fweight, c);
            ops.extend(el.ops);
            i += 1.0;
        }
    }

    c.o.hachure_angle = saved_angle;
    OpSet::fill_sketch(ops)
}

/// `patternFillPolygons(polygonList, o)` — dispatches on `fillStyle`.
pub fn pattern_fill_polygons(polygons: &[Vec<[f64; 2]>], c: &mut Ctx) -> OpSet {
    match c.o.fill_style {
        FillStyle::Hachure => fill_hachure(polygons, c),
        FillStyle::CrossHatch => fill_hatch(polygons, c),
        FillStyle::ZigZag => fill_zigzag(polygons, c),
        FillStyle::Dashed => fill_dashed(polygons, c),
        FillStyle::ZigZagLine => fill_zigzag_line(polygons, c),
        FillStyle::Dots => fill_dots(polygons, c),
        // `solid` never reaches a filler — the generator branches before this.
        FillStyle::Solid => fill_hachure(polygons, c),
    }
}
