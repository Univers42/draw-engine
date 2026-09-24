//! From vtracer's filled paths to closed polygons a line element can carry.
//!
//! A traced shape is a set of subpaths filled together under SVG's non-zero rule: outer
//! boundaries one way round, holes the other. A line element is a single closed polyline,
//! filled on its own. So, per shape:
//!
//! 1. every subpath is **flattened** — cubics subdivided until they are within a
//!    tolerance of the curve — rounded to hundredths, de-duplicated and closed;
//! 2. rings are sorted into outers and holes by orientation, and each hole goes to the
//!    smallest outer that contains it;
//! 3. an outer and its holes are **simplified** only if together they would exceed the
//!    point budget — before splicing, each ring on its own, so the keyholes stay exact;
//! 4. each hole is **spliced** into its outer as a zero-width keyhole: across to the
//!    hole, round it the opposite way, and back along the same line. The doubled bridge
//!    cancels under either fill rule, so one polygon paints the region and leaves its
//!    holes empty — the same splice `bucket_fill.rs` makes for a fill with islands.

use vtracer::ir::{PathCmd, SubPath, VectorDoc};

/// A point in traced pixels.
pub type Pt = [f64; 2];

/// The editable output: every region's polygons, bottom to top, in traced pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapesPayload {
    /// The size of the traced image.
    pub width: u32,
    pub height: u32,
    pub shapes: Vec<TraceShape>,
}

/// One traced colour region: each ring is one element, `[x0, y0, x1, y1, …]`, closed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceShape {
    /// The region's colour, `0xRRGGBB`.
    pub rgb: u32,
    pub rings: Vec<Vec<f64>>,
}

impl TraceShape {
    /// The colour as CSS writes it, `#RRGGBB`.
    pub fn color(&self) -> String {
        format!("#{:06X}", self.rgb)
    }
}

/// [`ShapesPayload`] flat, as draw-engine takes it (`TraceRings` in its
/// `engine/vectorize.rs`): ring `i` is `lengths[i]` points in `colours[i]`, taken in
/// turn from `coords` as `x, y` pairs — each a **fraction of the picture**, so the
/// engine needs no idea of the size it was traced at.
///
/// Typed arrays rather than JSON: a detailed trace is hundreds of thousands of numbers,
/// which cross from the worker to the engine as three buffers moved, not megabytes of
/// text written out and parsed back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlatRings {
    pub colours: Vec<u32>,
    pub lengths: Vec<u32>,
    pub coords: Vec<f64>,
}

impl ShapesPayload {
    pub fn flat(&self) -> FlatRings {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let mut flat = FlatRings::default();
        for shape in &self.shapes {
            for ring in &shape.rings {
                flat.colours.push(shape.rgb);
                flat.lengths
                    .push(u32::try_from(ring.len() / 2).unwrap_or(u32::MAX));
                flat.coords.extend(
                    ring.as_chunks::<2>()
                        .0
                        .iter()
                        .flat_map(|[x, y]| [x / w, y / h]),
                );
            }
        }
        flat
    }
}

/// How deep a cubic is split before the flatness test is taken on trust. 2^16 pieces is
/// far past any curve a traced image of a few thousand pixels contains.
const MAX_SPLIT_DEPTH: u32 = 16;

/// Appends the flattened cubic `p0 p1 p2 p3` to `out`, excluding `p0` and ending on `p3`.
///
/// Adaptive: a piece is emitted as its chord once the chord is provably within
/// `tolerance` of it, and split in half otherwise. The test is Fischer's bound — with
/// `u = 3·p1 − 2·p0 − p3` and `v = 3·p2 − p0 − 2·p3`, the curve stays within
/// `√(max(ux², vx²) + max(uy², vy²)) / 4` of the chord, at every parameter.
pub fn flatten_cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, tolerance: f64, out: &mut Vec<Pt>) {
    split_cubic(p0, p1, p2, p3, 16.0 * tolerance * tolerance, 0, out);
}

fn split_cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, limit: f64, depth: u32, out: &mut Vec<Pt>) {
    let (ux, uy) = (
        3.0 * p1[0] - 2.0 * p0[0] - p3[0],
        3.0 * p1[1] - 2.0 * p0[1] - p3[1],
    );
    let (vx, vy) = (
        3.0 * p2[0] - p0[0] - 2.0 * p3[0],
        3.0 * p2[1] - p0[1] - 2.0 * p3[1],
    );
    if depth >= MAX_SPLIT_DEPTH || (ux * ux).max(vx * vx) + (uy * uy).max(vy * vy) <= limit {
        out.push(p3);
        return;
    }
    let mid = |a: Pt, b: Pt| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let (p01, p12, p23) = (mid(p0, p1), mid(p1, p2), mid(p2, p3));
    let (p012, p123) = (mid(p01, p12), mid(p12, p23));
    let at = mid(p012, p123);
    split_cubic(p0, p01, p012, at, limit, depth + 1, out);
    split_cubic(at, p123, p23, p3, limit, depth + 1, out);
}

fn round(p: Pt) -> Pt {
    [
        (p[0] * 100.0).round() / 100.0,
        (p[1] * 100.0).round() / 100.0,
    ]
}

/// One subpath as a closed ring — last point equal to the first — with its curves
/// flattened, every point rounded to hundredths of a pixel, and no point repeated.
pub fn flatten_subpath(sub: &SubPath, tolerance: f64) -> Vec<Pt> {
    let mut raw: Vec<Pt> = Vec::with_capacity(sub.commands.len() * 4);
    let mut at: Pt = [0.0, 0.0];
    for command in &sub.commands {
        match *command {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => {
                at = [p.x, p.y];
                raw.push(at);
            }
            PathCmd::CubicTo(c1, c2, end) => {
                let end = [end.x, end.y];
                flatten_cubic(at, [c1.x, c1.y], [c2.x, c2.y], end, tolerance, &mut raw);
                at = end;
            }
            PathCmd::Close => {}
        }
    }
    let mut ring: Vec<Pt> = Vec::with_capacity(raw.len() + 1);
    for p in raw.into_iter().map(round) {
        if ring.last() != Some(&p) {
            ring.push(p);
        }
    }
    if let (Some(&first), Some(&last)) = (ring.first(), ring.last()) {
        if first != last {
            ring.push(first);
        }
    }
    ring
}

/// The signed area of a ring (the shoelace formula), closed or not. The sign is the
/// orientation, which is all the fill rules look at.
pub fn signed_area(ring: &[Pt]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Whether `p` is inside the ring, by the even-odd rule.
fn contains(ring: &[Pt], p: Pt) -> bool {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

#[derive(Clone, Copy)]
struct Bounds {
    min: Pt,
    max: Pt,
}

impl Bounds {
    fn of(ring: &[Pt]) -> Self {
        ring.iter().fold(
            Self {
                min: [f64::INFINITY; 2],
                max: [f64::NEG_INFINITY; 2],
            },
            |b, p| Self {
                min: [b.min[0].min(p[0]), b.min[1].min(p[1])],
                max: [b.max[0].max(p[0]), b.max[1].max(p[1])],
            },
        )
    }

    fn holds(&self, other: &Self) -> bool {
        self.min[0] <= other.min[0]
            && self.min[1] <= other.min[1]
            && self.max[0] >= other.max[0]
            && self.max[1] >= other.max[1]
    }
}

/// Whether most of a few of `hole`'s vertices are inside `outer`. More than one, because
/// a traced hole can touch its outer at a pixel corner, and that vertex is on the edge.
fn holds_hole(outer: &[Pt], hole: &[Pt]) -> bool {
    let samples = hole.len().min(5);
    let step = hole.len() / samples.max(1);
    let inside = (0..samples)
        .filter(|&k| contains(outer, hole[k * step]))
        .count();
    inside * 2 > samples
}

fn perpendicular(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / length
}

/// Ramer–Douglas–Peucker over an open ring, read as closed. Keeps its first point, so
/// the ring still starts where it did.
///
/// ponytail: a second RDP beside draw-engine's `simplify_path` — this crate cannot link
/// the engine without shipping it in the trace WASM. One shared geometry crate if a third
/// copy is ever needed.
fn simplify(ring: &[Pt], tolerance: f64) -> Vec<Pt> {
    let n = ring.len();
    if n <= 3 {
        return ring.to_vec();
    }
    // Closed: the last index stands for the first point again.
    let at = |i: usize| ring[i % n];
    let mut keep = vec![false; n + 1];
    keep[0] = true;
    keep[n] = true;
    let mut stack = vec![(0, n)];
    while let Some((start, end)) = stack.pop() {
        let (mut far, mut far_distance) = (0, 0.0);
        for i in start + 1..end {
            let distance = perpendicular(at(i), at(start), at(end));
            if distance > far_distance {
                far = i;
                far_distance = distance;
            }
        }
        if far_distance > tolerance {
            keep[far] = true;
            stack.push((start, far));
            stack.push((far, end));
        }
    }
    (0..n).filter(|&i| keep[i]).map(|i| ring[i]).collect()
}

/// An outer ring and the holes it keeps.
struct Part {
    outer: Vec<Pt>,
    holes: Vec<Vec<Pt>>,
}

impl Part {
    /// Points once spliced and closed: each hole costs its own points, a return to its
    /// start and the repeated attachment vertex.
    fn spliced_len(outer: &[Pt], holes: &[Vec<Pt>]) -> usize {
        outer.len() + 1 + holes.iter().map(|h| h.len() + 2).sum::<usize>()
    }

    /// Simplified until it fits `budget`, each ring on its own. Holes simplified below
    /// a triangle had no area left to leave unpainted and are let go.
    fn fit(self, budget: usize) -> Self {
        if Self::spliced_len(&self.outer, &self.holes) <= budget {
            return self;
        }
        let mut tolerance = 0.5;
        loop {
            let outer = simplify(&self.outer, tolerance);
            let holes: Vec<Vec<Pt>> = self
                .holes
                .iter()
                .map(|h| simplify(h, tolerance))
                .filter(|h| h.len() >= 3)
                .collect();
            // Converges: at a large enough tolerance every ring is down to its first
            // point, and a hole of fewer than three is gone.
            if Self::spliced_len(&outer, &holes) <= budget {
                return Self { outer, holes };
            }
            tolerance *= 2.0;
        }
    }

    /// The outer with every hole spliced in, closed.
    fn splice(self) -> Vec<Pt> {
        let mut ring = self.outer;
        for hole in self.holes {
            ring = splice_hole(&ring, &hole);
        }
        ring.push(ring[0]);
        ring
    }
}

/// Splices `hole` into `ring` (both open) as a zero-width keyhole.
///
/// Bridged from the hole's leftmost vertex to the ring vertex nearest it — linear in the
/// ring, where the shortest bridge overall (`bucket_fill.rs`) is quadratic, and a traced
/// shape can carry hundreds of holes. Any bridge is correct for the fill, since its two
/// passes cancel; the short one only keeps it out of sight if a stroke is added later.
fn splice_hole(ring: &[Pt], hole: &[Pt]) -> Vec<Pt> {
    let oriented: Vec<Pt> = if signed_area(hole).signum() == signed_area(ring).signum() {
        hole.iter().rev().copied().collect()
    } else {
        hole.to_vec()
    };
    let (j, &start) = oriented
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])))
        .expect("a hole has vertices");
    let (i, _) = ring
        .iter()
        .enumerate()
        .map(|(i, p)| (i, (p[0] - start[0]).powi(2) + (p[1] - start[1]).powi(2)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a ring has vertices");

    let mut out = Vec::with_capacity(ring.len() + oriented.len() + 2);
    out.extend_from_slice(&ring[..=i]);
    for k in 0..=oriented.len() {
        out.push(oriented[(j + k) % oriented.len()]);
    }
    out.push(ring[i]);
    out.extend_from_slice(&ring[i + 1..]);
    out
}

/// Outer rings with their holes spliced in, each closed and within `budget` points.
///
/// `rings` are closed rings in any order, filled together by the non-zero rule. Holes
/// are the rings turning against the largest one. A hole no outer contains paints under
/// non-zero like any other ring, so it becomes an outer of its own rather than vanishing.
pub fn compose(rings: Vec<Vec<Pt>>, budget: usize) -> Vec<Vec<Pt>> {
    // Open rings from here on; closed again by `splice`.
    let rings: Vec<(Vec<Pt>, f64)> = rings
        .into_iter()
        .map(|mut ring| {
            if ring.len() > 1 && ring.first() == ring.last() {
                ring.pop();
            }
            let area = signed_area(&ring);
            (ring, area)
        })
        .filter(|(ring, area)| ring.len() >= 3 && *area != 0.0)
        .collect();
    let Some(outer_sign) = rings
        .iter()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(_, area)| area.signum())
    else {
        return Vec::new();
    };

    let (outer_rings, hole_rings): (Vec<_>, Vec<_>) = rings
        .into_iter()
        .partition(|(_, area)| area.signum() == outer_sign);
    let mut parts: Vec<(Part, Bounds, f64)> = outer_rings
        .into_iter()
        .map(|(outer, area)| {
            let bounds = Bounds::of(&outer);
            (
                Part {
                    outer,
                    holes: Vec::new(),
                },
                bounds,
                area.abs(),
            )
        })
        .collect();

    for (hole, area) in hole_rings {
        let bounds = Bounds::of(&hole);
        let owner = parts
            .iter()
            .enumerate()
            .filter(|(_, (part, outer_bounds, _))| {
                outer_bounds.holds(&bounds) && holds_hole(&part.outer, &hole)
            })
            .min_by(|a, b| a.1 .2.total_cmp(&b.1 .2))
            .map(|(k, _)| k);
        match owner {
            Some(k) => parts[k].0.holes.push(hole),
            None => parts.push((
                Part {
                    outer: hole,
                    holes: Vec::new(),
                },
                bounds,
                area.abs(),
            )),
        }
    }

    parts
        .into_iter()
        .map(|(part, _, _)| part.fit(budget).splice())
        .filter(|ring| ring.len() >= 4)
        .collect()
}

/// The editable payload of a trace: every shape, bottom to top, as spliced rings.
pub fn shapes_payload(doc: &VectorDoc, tolerance: f64, budget: usize) -> ShapesPayload {
    ShapesPayload {
        width: doc.width,
        height: doc.height,
        shapes: doc
            .shapes
            .iter()
            .map(|shape| {
                let rings = shape
                    .path
                    .subpaths
                    .iter()
                    .map(|sub| flatten_subpath(sub, tolerance))
                    .collect();
                let color = shape.paint.color();
                TraceShape {
                    rgb: u32::from(color.r) << 16 | u32::from(color.g) << 8 | u32::from(color.b),
                    rings: compose(rings, budget)
                        .into_iter()
                        .map(|ring| ring.into_iter().flatten().collect())
                        .collect(),
                }
            })
            .collect(),
    }
}
