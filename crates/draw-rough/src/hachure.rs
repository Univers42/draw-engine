//! Transcription of the `hachure-fill` package (`bin/hachure.js`), which rough.js
//! depends on for every pattern fill.
//!
//! It is an even-odd scanline fill: rotate the polygon so the hatch direction becomes
//! horizontal, sweep a scanline down it maintaining an active-edge table, emit a span
//! between each pair of crossings, then rotate the spans back.

use crate::geometry::Line;
use crate::jsnum::js_round;

/// Rotates points **in place** about `center` by `degrees`, as the JS does.
///
/// The in-place mutation matters: `hachure_lines` rotates the polygon forward, scans
/// it, then rotates it back by `-degrees`. That round trip is not exact in floating
/// point, so the caller's polygon comes back very slightly changed — faithful to the
/// original, and harmless because rough never reuses a polygon after filling it.
pub fn rotate_points(points: &mut [[f64; 2]], center: [f64; 2], degrees: f64) {
    if points.is_empty() {
        return;
    }
    let [cx, cy] = center;
    let angle = (std::f64::consts::PI / 180.0) * degrees;
    let cos = angle.cos();
    let sin = angle.sin();
    for p in points.iter_mut() {
        let [x, y] = *p;
        p[0] = ((x - cx) * cos) - ((y - cy) * sin) + cx;
        p[1] = ((x - cx) * sin) + ((y - cy) * cos) + cy;
    }
}

/// Rotates every endpoint of every line, in place.
fn rotate_lines(lines: &mut [Line], center: [f64; 2], degrees: f64) {
    let mut points: Vec<[f64; 2]> = Vec::with_capacity(lines.len() * 2);
    for line in lines.iter() {
        points.push(line[0]);
        points.push(line[1]);
    }
    rotate_points(&mut points, center, degrees);
    for (i, line) in lines.iter_mut().enumerate() {
        line[0] = points[i * 2];
        line[1] = points[i * 2 + 1];
    }
}

fn are_same_points(p1: [f64; 2], p2: [f64; 2]) -> bool {
    p1[0] == p2[0] && p1[1] == p2[1]
}

/// `hachureLines(polygons, hachureGap, hachureAngle, hachureStepOffset)`
///
/// Takes the polygons by value because the JS mutates them and the mutation is not
/// observable to the caller in any path rough actually takes.
pub fn hachure_lines(
    mut polygons: Vec<Vec<[f64; 2]>>,
    hachure_gap: f64,
    hachure_angle: f64,
    hachure_step_offset: f64,
) -> Vec<Line> {
    let angle = hachure_angle;
    let gap = hachure_gap.max(0.1);
    let rotation_center = [0.0, 0.0];

    // `if (angle)` — a zero angle skips both rotations entirely, which is not the same
    // as rotating by zero: the round trip introduces rounding that this path avoids.
    if angle != 0.0 {
        for polygon in polygons.iter_mut() {
            rotate_points(polygon, rotation_center, angle);
        }
    }

    let mut lines = straight_hachure_lines(&polygons, gap, hachure_step_offset);

    if angle != 0.0 {
        for polygon in polygons.iter_mut() {
            rotate_points(polygon, rotation_center, -angle);
        }
        rotate_lines(&mut lines, rotation_center, -angle);
    }

    lines
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    ymin: f64,
    ymax: f64,
    x: f64,
    islope: f64,
}

/// Orders two floats the way a JS comparator returning `(a - b) / |a - b|` does,
/// treating an incomparable pair as equal (JS coerces the resulting NaN to "no swap").
fn js_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// `straightHachureLines(polygons, gap, hachureStepOffset)`
fn straight_hachure_lines(
    polygons: &[Vec<[f64; 2]>],
    gap: f64,
    hachure_step_offset: f64,
) -> Vec<Line> {
    let mut vertex_array: Vec<Vec<[f64; 2]>> = Vec::new();
    for polygon in polygons {
        let mut vertices = polygon.clone();
        if vertices.is_empty() {
            continue;
        }
        if !are_same_points(vertices[0], vertices[vertices.len() - 1]) {
            vertices.push([vertices[0][0], vertices[0][1]]);
        }
        if vertices.len() > 2 {
            vertex_array.push(vertices);
        }
    }

    let mut lines: Vec<Line> = Vec::new();
    let gap = gap.max(0.1);

    // Sorted edge table. Horizontal edges are skipped: they never cross a scanline.
    let mut edges: Vec<Edge> = Vec::new();
    for vertices in &vertex_array {
        for i in 0..vertices.len() - 1 {
            let p1 = vertices[i];
            let p2 = vertices[i + 1];
            if p1[1] != p2[1] {
                let ymin = p1[1].min(p2[1]);
                edges.push(Edge {
                    ymin,
                    ymax: p1[1].max(p2[1]),
                    x: if ymin == p1[1] { p1[0] } else { p2[0] },
                    islope: (p2[0] - p1[0]) / (p2[1] - p1[1]),
                });
            }
        }
    }

    // JS `Array.prototype.sort` is stable (required since ES2019), as is `sort_by`.
    edges.sort_by(|e1, e2| {
        js_cmp(e1.ymin, e2.ymin)
            .then_with(|| js_cmp(e1.x, e2.x))
            .then_with(|| js_cmp(e1.ymax, e2.ymax))
    });

    if edges.is_empty() {
        return lines;
    }

    let mut active_edges: Vec<Edge> = Vec::new();
    let mut y = edges[0].ymin;
    let mut iteration: u64 = 0;

    // A bound the JS does not have. Every real polygon terminates long before this;
    // it exists so a degenerate scene cannot wedge the browser tab in an unbreakable
    // loop inside WASM. If this ever trips, the input is pathological, not the port.
    const MAX_SCANLINES: u64 = 10_000_000;

    while (!active_edges.is_empty() || !edges.is_empty()) && iteration < MAX_SCANLINES {
        if !edges.is_empty() {
            let mut ix: isize = -1;
            for (i, edge) in edges.iter().enumerate() {
                if edge.ymin > y {
                    break;
                }
                ix = i as isize;
            }
            let take = (ix + 1) as usize;
            for edge in edges.drain(0..take) {
                active_edges.push(edge);
            }
        }

        active_edges.retain(|e| e.ymax > y);
        active_edges.sort_by(|a, b| js_cmp(a.x, b.x));

        // The JS nests these; collapsed here because clippy is right that they are
        // one condition, and `&&` is exactly equivalent.
        if (hachure_step_offset != 1.0 || (iteration as f64) % gap == 0.0) && active_edges.len() > 1
        {
            {
                let mut i = 0;
                while i < active_edges.len() {
                    let nexti = i + 1;
                    if nexti >= active_edges.len() {
                        break;
                    }
                    let ce = active_edges[i];
                    let ne = active_edges[nexti];
                    lines.push([[js_round(ce.x), y], [js_round(ne.x), y]]);
                    i += 2;
                }
            }
        }

        y += hachure_step_offset;
        for e in active_edges.iter_mut() {
            e.x += hachure_step_offset * e.islope;
        }
        iteration += 1;
    }

    lines
}
