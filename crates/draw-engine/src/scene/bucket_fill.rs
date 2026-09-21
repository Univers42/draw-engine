//! Bucket fill: the region under a click, as a polygon.
//!
//! A raster bucket floods pixels. That is not available here — there are no pixels to
//! flood, only elements — so the region is derived geometrically instead, and the result
//! is a real polygon element that moves, scales and exports like anything else drawn.
//!
//! The pipeline, in order:
//!
//! 1. **Owner** — the closed element under the click, if there is one. The common case,
//!    and it bounds the search to that element's box.
//! 2. **Boundary segments** — every visible outline near the click, clipped to the parts
//!    that are actually on screen. Anything buried under opaque paint is not a boundary,
//!    because the person clicking cannot see it.
//! 3. **Planar arrangement** — those segments made into a graph: near-identical vertices
//!    merged, crossings split, T-junctions split, visible gaps bridged.
//! 4. **Face extraction** — a half-edge walk that traces every cell of the arrangement.
//! 5. **Face selection** — the smallest *bounded* face containing the click, plus the
//!    islands inside it, which become holes.
//! 6. **Simplification** — the ring reduced to a sane number of points, holes spliced in
//!    as zero-width keyholes so the result is still one closed polygon.
//! 7. **Z-order** — where the new element goes in the stack so it neither buries what is
//!    inside it nor lets older paint show through it.
//!
//! Transcribed from Excalidraw's `packages/element/src/bucketFill.ts`. The two places
//! this deliberately differs from theirs are marked at their definitions:
//! `line_element_ideal_segments` and `freedraw_ideal_segments`, both because our renderer
//! does not smooth paths the way theirs does, so the logical path *is* the drawn path.

use std::collections::{HashMap, HashSet};

use crate::camera::{Point, WorldBounds};
use crate::scene::element::{DrawElement, DrawElementType, FillStyle};
use crate::scene::geometry::{
    distance_to_segment, element_bounds, element_outline, is_transparent, outline_edges,
    outline_is_closed, polygon_area, polygon_includes_point, polygon_includes_point_non_zero,
    polygon_signed_area, segment_intersection_point,
};
// The same Ramer–Douglas–Peucker the lasso simplifies its trail with, and the same one
// Excalidraw's freedraw renderer uses. One implementation, so a fill's outline and a
// lasso's loop cannot disagree about what a redundant point is.
use crate::selection::lasso::simplify_path;

/// How big a visual gap between strokes still counts as closed, in world px.
///
/// This is a *bridging* radius, not a snapping one: gaps are closed by adding short
/// connector edges between loose stroke ends, never by relocating vertices — so a
/// generous value here does not distort the filled shape.
pub const BUCKET_FILL_GAP_TOLERANCE: f64 = 6.0;

/// How far apart two fills' bounds may lie and still count as the same region.
///
/// Covers the pipeline's own jitter — polygon simplification (~0.75px) and node merging
/// (`snap_epsilon`) — while staying under a typical stroke width, so "the same region"
/// means what it looks like it means.
pub const BUCKET_FILL_REGION_MATCH_TOLERANCE: f64 = 2.0;

/// How deep inside a coverer a stroke must lie before it counts as hidden.
///
/// A fill's polygon runs along the *centreline* of the strokes bounding it, so without a
/// margin the fill — itself a coverer — would clip its own boundary strokes on the next
/// click and adjacent regions would silently merge. A stroke sitting on a coverer's edge
/// still paints its outer half; only strokes buried deeper than this stay clipped.
pub const BUCKET_FILL_COVER_MARGIN: f64 = 2.0;

/// How close a path's ends must be for the renderer to treat it as closed.
///
/// Excalidraw's `LINE_CONFIRM_THRESHOLD`. Whether a fill treats a stroke as closed has to
/// agree with whether the renderer paints its background, or a shape that looks filled
/// would not stop a fill — so this is deliberately the renderer's rule and not
/// `gap_tolerance`.
pub const LINE_CONFIRM_THRESHOLD: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BucketFillOptions {
    /// Geometric fidelity: vertices closer than this collapse to one graph node, and a
    /// node this close to a stroke splits it.
    ///
    /// Keep it **small**. Merging relocates a vertex to the first-seen position, so this
    /// is the upper bound on how far the filled shape may drift from the actual strokes.
    pub snap_epsilon: f64,
    /// Connectivity: loose stroke ends within this distance of other geometry are bridged
    /// with a connector edge so the region reads as closed.
    ///
    /// Unlike `snap_epsilon` this costs no fidelity — bridges add edges, they never move
    /// vertices — so it can afford to be generous.
    pub gap_tolerance: f64,
    /// Faces and polygons smaller than this are discarded.
    pub min_area: f64,
    /// Above this many input segments, give up rather than freeze on the click.
    pub max_boundary_segments: usize,
    /// Cap on the number of points in the generated polygon.
    pub max_generated_points: usize,
    /// Initial half-extent of the owner-less search box, doubled up to three times while
    /// the face found still touches the box frontier.
    pub fallback_search_radius: f64,
}

impl Default for BucketFillOptions {
    fn default() -> Self {
        Self {
            snap_epsilon: 0.5,
            gap_tolerance: BUCKET_FILL_GAP_TOLERANCE,
            min_area: 4.0,
            max_boundary_segments: 2560,
            max_generated_points: 1536,
            fallback_search_radius: 512.0,
        }
    }
}

/// Why no polygon came back.
///
/// Separate variants because the host says different things about them: an open region is
/// worth a word to the person clicking, whereas a click on empty canvas is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketFillFailure {
    /// Nothing closed under the pointer, and the fallback search found no region either.
    NoOwner,
    /// There is a shape here, but its outline does not enclose the click.
    OpenRegion,
    /// The arrangement was too big to resolve in the time a click may take.
    TooComplex,
    /// A region, but smaller than `min_area`.
    TooSmall,
    /// The ring collapsed under simplification.
    InvalidPolygon,
}

/// Where the fill goes in the scene order, relative to an existing element.
///
/// Relative rather than absolute so the caller can resolve it against whatever array it
/// inserts into, including one that still holds deleted elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Above,
    Below,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BucketFillInsertion {
    pub placement: Placement,
    pub element_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BucketFill {
    /// The closed element under the click, or `None` for a region resolved by the
    /// owner-less fallback — a region formed by open lines belongs to no one element.
    pub owner_id: Option<String>,
    /// The elements, other than the owner, whose outlines actually bound the fill.
    pub boundary_element_ids: Vec<String>,
    /// The closed ring, in world coordinates.
    ///
    /// When the region contains islands this is a keyhole path: hole contours spliced in
    /// through zero-width bridges, so it still renders as one polygon with its holes
    /// unpainted.
    pub scene_points: Vec<Point>,
    pub insertion: BucketFillInsertion,
}

// -----------------------------------------------------------------------------
// small geometry helpers
// -----------------------------------------------------------------------------

fn expand_bounds(b: WorldBounds, pad: f64) -> WorldBounds {
    WorldBounds {
        min_x: b.min_x - pad,
        min_y: b.min_y - pad,
        max_x: b.max_x + pad,
        max_y: b.max_y + pad,
    }
}

fn bounds_intersect(a: WorldBounds, b: WorldBounds) -> bool {
    a.min_x <= b.max_x && a.max_x >= b.min_x && a.min_y <= b.max_y && a.max_y >= b.min_y
}

fn point_distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Where `q` falls along the line through `a`-`b`, as a fraction of its length.
fn project_param(a: Point, b: Point, q: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return 0.0;
    }
    ((q.x - a.x) * dx + (q.y - a.y) * dy) / len2
}

fn point_at_param(a: Point, b: Point, t: f64) -> Point {
    Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

fn perpendicular_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = dx.hypot(dy);
    if len == 0.0 {
        return point_distance(p, a);
    }
    ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len
}

/// Visit every grid cell the `inflate`-expanded segment passes through.
///
/// Column by column rather than over the whole bounding box, so a long diagonal visits the
/// cells it actually passes near instead of the whole rectangle it spans — which for a
/// scene-crossing stroke is the difference between a handful of cells and thousands.
fn for_each_cell_along_segment(
    a: Point,
    b: Point,
    inflate: f64,
    cell_size: f64,
    mut visit: impl FnMut(i64, i64),
) {
    let min_x = a.x.min(b.x);
    let max_x = a.x.max(b.x);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let from_cx = ((min_x - inflate) / cell_size).floor() as i64;
    let to_cx = ((max_x + inflate) / cell_size).floor() as i64;
    for cx in from_cx..=to_cx {
        // The segment's y-extent over this column's (inflated) x-interval.
        let (mut y0, mut y1) = (a.y, b.y);
        if dx != 0.0 {
            let t0 = (min_x.max(cx as f64 * cell_size - inflate) - a.x) / dx;
            let t1 = (max_x.min((cx + 1) as f64 * cell_size + inflate) - a.x) / dx;
            y0 = a.y + t0 * dy;
            y1 = a.y + t1 * dy;
        }
        let from_cy = ((y0.min(y1) - inflate) / cell_size).floor() as i64;
        let to_cy = ((y0.max(y1) + inflate) / cell_size).floor() as i64;
        for cy in from_cy..=to_cy {
            visit(cx, cy);
        }
    }
}

/// Remove the `cuts` intervals from the `base` intervals.
fn subtract_intervals(base: Vec<(f64, f64)>, cuts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut result = base;
    for &(c0, c1) in cuts {
        let mut next = Vec::new();
        for &(b0, b1) in &result {
            if c1 <= b0 || c0 >= b1 {
                next.push((b0, b1));
                continue;
            }
            if c0 > b0 {
                next.push((b0, c0));
            }
            if c1 < b1 {
                next.push((c1, b1));
            }
        }
        result = next;
    }
    result
}

/// Node store that merges points within `eps` into one node.
///
/// Cell-hashed rather than scanned, and that is not only for speed: "the first node
/// already within `eps` wins" is the rule, so the order candidates are examined in is part
/// of the answer whenever two existing nodes both sit within `eps` of a new point.
struct NodeStore {
    nodes: Vec<Point>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    eps: f64,
    cell_size: f64,
}

impl NodeStore {
    fn new(eps: f64) -> Self {
        Self {
            nodes: Vec::new(),
            cells: HashMap::new(),
            eps,
            // Never below 1: an epsilon of 0.5 with a matching cell size would put the
            // 3×3 neighbourhood at 1.5px, which is still wider than eps, but a much
            // smaller epsilon would make the grid enormous for no gain.
            cell_size: eps.max(1.0),
        }
    }

    fn get_or_create(&mut self, p: Point) -> usize {
        let cx = (p.x / self.cell_size).floor() as i64;
        let cy = (p.y / self.cell_size).floor() as i64;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(bucket) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &idx in bucket {
                        if point_distance(self.nodes[idx], p) <= self.eps {
                            return idx;
                        }
                    }
                }
            }
        }
        let idx = self.nodes.len();
        self.nodes.push(p);
        self.cells.entry((cx, cy)).or_default().push(idx);
        idx
    }

    /// Visit every node within `radius` of `p`.
    ///
    /// A superset, at cell granularity — nodes slightly beyond `radius` come back too, so
    /// callers check the distance themselves.
    fn for_each_in_radius(&self, p: Point, radius: f64, mut visit: impl FnMut(usize)) {
        let from_cx = ((p.x - radius) / self.cell_size).floor() as i64;
        let to_cx = ((p.x + radius) / self.cell_size).floor() as i64;
        let from_cy = ((p.y - radius) / self.cell_size).floor() as i64;
        let to_cy = ((p.y + radius) / self.cell_size).floor() as i64;
        for cx in from_cx..=to_cx {
            for cy in from_cy..=to_cy {
                if let Some(bucket) = self.cells.get(&(cx, cy)) {
                    for &index in bucket {
                        visit(index);
                    }
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// what an element is, for the purposes of a fill
// -----------------------------------------------------------------------------

/// Whether this element type paints its `background_color` at all.
fn has_background(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle
            | DrawElementType::Ellipse
            | DrawElementType::Diamond
            | DrawElementType::Line
            | DrawElementType::Freedraw
            | DrawElementType::Embed
    )
}

/// Element types whose outlines can take part in a fill, as owner or as boundary.
///
/// Text, image, embed and arrows are out: an arrow is a pointer rather than an enclosure,
/// and the rest have no outline a person would expect to hold paint.
fn is_fill_boundary_type(kind: DrawElementType) -> bool {
    matches!(
        kind,
        DrawElementType::Rectangle
            | DrawElementType::Diamond
            | DrawElementType::Ellipse
            | DrawElementType::Frame
            | DrawElementType::Line
            | DrawElementType::Freedraw
    )
}

/// Whether a colour is fully opaque — not transparent, and carrying no alpha below 1.
///
/// Anything under a colour that is not opaque shows through, so such a colour cannot hide
/// an outline and must not count as a coverer.
fn is_opaque_color(color: &str) -> bool {
    let c = color.trim();
    if is_transparent(c) {
        return false;
    }
    if let Some(hex) = c.strip_prefix('#') {
        // #RGBA and #RRGGBBAA carry an alpha channel; #RGB and #RRGGBB do not.
        return match hex.len() {
            4 => u8::from_str_radix(&hex[3..4], 16)
                .map(|a| a == 0xf)
                .unwrap_or(false),
            8 => u8::from_str_radix(&hex[6..8], 16)
                .map(|a| a == 0xff)
                .unwrap_or(false),
            _ => true,
        };
    }
    if let Some(rest) = c.strip_prefix("rgba(").and_then(|r| r.strip_suffix(')')) {
        if let Some(alpha) = rest.split(',').nth(3) {
            return alpha
                .trim()
                .parse::<f64>()
                .map(|a| a >= 1.0)
                .unwrap_or(false);
        }
    }
    true
}

/// A path whose ends are close enough that the renderer draws it closed.
fn is_path_a_loop(points: &[[f64; 2]]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    (first[0] - last[0]).hypot(first[1] - last[1]) <= LINE_CONFIRM_THRESHOLD
}

/// A line element's points, closed and with more than three of them.
fn is_valid_polygon(points: &[[f64; 2]]) -> bool {
    points.len() > 3
        && (points[0][0] - points[points.len() - 1][0]).abs() <= crate::scene::geometry::PRECISION
        && (points[0][1] - points[points.len() - 1][1]).abs() <= crate::scene::geometry::PRECISION
}

fn element_points(element: &DrawElement) -> &[[f64; 2]] {
    element.points.as_deref().unwrap_or(&[])
}

fn is_invisible(element: &DrawElement) -> bool {
    element.opacity <= 0.0
}

/// Whether the element paints an opaque background — i.e. can actually hide what is under
/// it.
///
/// Types that never render a background, see-through fill styles, partial opacity and
/// colours carrying alpha all fail this, because anything beneath them still shows.
pub fn renders_opaque_fill(element: &DrawElement) -> bool {
    if !has_background(element.kind)
        || element.fill_style != FillStyle::Solid
        || element.opacity < 100.0
        || !is_opaque_color(&element.background_color)
    {
        return false;
    }
    // An open stroke never paints its background, whatever colour it is set to.
    if matches!(
        element.kind,
        DrawElementType::Line | DrawElementType::Freedraw
    ) && !is_path_a_loop(element_points(element))
    {
        return false;
    }
    true
}

/// Whether the element puts any pixels on the canvas at all.
fn renders_any_mark(element: &DrawElement) -> bool {
    !is_invisible(element)
        && (element.kind == DrawElementType::Image
            || !is_transparent(&element.stroke_color)
            || (has_background(element.kind) && !is_transparent(&element.background_color)))
}

/// Whether the element reads as pure paint the bucket could have produced.
///
/// Recognised by **shape, not by a marker**: a marker would go stale the moment someone
/// restyles a fill, and a hand-drawn strokeless background polygon is indistinguishable
/// from a generated one anyway. Such paint is never an owner, and is what the host
/// restyles in place when the same region is clicked again.
pub fn is_bucket_fill_compatible(element: &DrawElement) -> bool {
    element.kind == DrawElementType::Line
        && is_valid_polygon(element_points(element))
        && !is_invisible(element)
        && !is_transparent(&element.background_color)
        && is_transparent(&element.stroke_color)
}

fn is_closed_owner_candidate(element: &DrawElement) -> bool {
    if !is_fill_boundary_type(element.kind) {
        return false;
    }
    match element.kind {
        DrawElementType::Line => is_valid_polygon(element_points(element)),
        DrawElementType::Freedraw => is_path_a_loop(element_points(element)),
        _ => true,
    }
}

/// Boundary and coverer roles are decided by **visibility, not provenance**: a generated
/// fill takes part like anything else. In practice its transparent stroke keeps it out of
/// the boundary set — but once someone gives a fill a visible stroke, that outline
/// genuinely bounds regions on screen and has to bound new fills too.
fn is_eligible_boundary(element: &DrawElement) -> bool {
    !is_invisible(element)
        && is_fill_boundary_type(element.kind)
        && !is_transparent(&element.stroke_color)
}

// -----------------------------------------------------------------------------
// element geometry
// -----------------------------------------------------------------------------

/// Boundary segments for a line element, from its logical path.
///
/// Not from its rough rendering: rough draws a line as two jittery passes, and filling to
/// those makes the fill hug the inner pass and leaves an unfilled sliver between them. The
/// logical path runs down the stroke's centreline, so the fill straddles the sketchy
/// stroke — which is how shape backgrounds are painted already. It also makes the
/// endpoints genuine degree-1 nodes, which is exactly what the bridging pass keys on.
///
/// **Differs from Excalidraw here:** theirs re-samples a curved line along the same bezier
/// fit the renderer uses. Ours has no curve fitting for lines, so the points *are* the
/// path and there is nothing to re-sample.
fn line_element_ideal_segments(element: &DrawElement) -> Vec<(Point, Point)> {
    let points = crate::selection::linear::world_points(element);
    points.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Boundary segments for a freedraw element.
///
/// **Differs from Excalidraw here:** theirs uses perfect-freehand's smoothed centreline,
/// because their renderer smooths between raw points that can sit 20px apart. Ours draws
/// the raw points, so the raw points are what is on screen.
fn freedraw_ideal_segments(element: &DrawElement) -> Vec<(Point, Point)> {
    line_element_ideal_segments(element)
}

/// Every segment of the element's outline.
fn element_segments(element: &DrawElement) -> Vec<(Point, Point)> {
    match element.kind {
        DrawElementType::Line | DrawElementType::Arrow => line_element_ideal_segments(element),
        DrawElementType::Freedraw => freedraw_ideal_segments(element),
        _ => {
            let outline = element_outline(element);
            outline_edges(&outline, outline_is_closed(element))
        }
    }
}

/// The element's outline as a closed ring, for containment tests.
fn element_ring(element: &DrawElement) -> Vec<Point> {
    element_outline(element)
}

fn is_point_in_element(p: Point, element: &DrawElement) -> bool {
    let ring = element_ring(element);
    ring.len() >= 3 && polygon_includes_point(p, &ring)
}

fn distance_to_element(element: &DrawElement, p: Point) -> f64 {
    element_segments(element)
        .iter()
        .map(|(a, b)| distance_to_segment(p.x, p.y, a.x, a.y, b.x, b.y))
        .fold(f64::INFINITY, f64::min)
}

fn intersect_element_with_segment(
    element: &DrawElement,
    a: Point,
    b: Point,
    threshold: f64,
) -> Vec<Point> {
    element_segments(element)
        .iter()
        .filter_map(|&(c, d)| segment_intersection_point((a, b), (c, d), threshold))
        .collect()
}

/// Clip a segment down to the parts that are actually visible.
///
/// A portion only counts as covered when it lies inside the coverer by at least `margin`.
/// A stroke whose centreline sits *on* a coverer's edge — which is the shape a fill
/// bounded by that stroke takes — still paints its outer half, so it has to keep bounding;
/// without the margin a fill would clip away its own boundary on the next click and
/// neighbouring regions would quietly merge into one.
fn clip_segment_to_visible(
    a: Point,
    b: Point,
    coverers: &[&DrawElement],
    eps: f64,
    margin: f64,
) -> Vec<(Point, Point)> {
    let mut intervals = vec![(0.0, 1.0)];
    for coverer in coverers {
        if intervals.is_empty() {
            break;
        }
        let hits = intersect_element_with_segment(coverer, a, b, eps);
        let mut breaks: Vec<f64> = hits
            .iter()
            .map(|&p| project_param(a, b, p))
            .filter(|&t| t > 0.0 && t < 1.0)
            .collect();
        breaks.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        breaks.insert(0, 0.0);
        breaks.push(1.0);

        let mut covered = Vec::new();
        for k in 0..breaks.len() - 1 {
            let (t0, t1) = (breaks[k], breaks[k + 1]);
            if t1 - t0 < 1e-9 {
                continue;
            }
            let mid = point_at_param(a, b, (t0 + t1) / 2.0);
            if is_point_in_element(mid, coverer) && distance_to_element(coverer, mid) > margin {
                covered.push((t0, t1));
            }
        }
        if !covered.is_empty() {
            intervals = subtract_intervals(intervals, &covered);
        }
    }
    intervals
        .into_iter()
        .filter(|&(t0, t1)| {
            point_distance(point_at_param(a, b, t0), point_at_param(a, b, t1)) >= eps
        })
        .map(|(t0, t1)| (point_at_param(a, b, t0), point_at_param(a, b, t1)))
        .collect()
}

// -----------------------------------------------------------------------------
// planar arrangement and face extraction
// -----------------------------------------------------------------------------

struct WorkingSegment {
    a: usize,
    b: usize,
    pa: Point,
    pb: Point,
    box_: WorldBounds,
    element_id: String,
    splits: Vec<(usize, f64)>,
}

/// A segment and the element it came from.
#[derive(Clone)]
struct SourceSegment {
    a: Point,
    b: Point,
    element_id: String,
}

/// A face ring plus the elements whose outlines bound it.
#[derive(Clone)]
struct Face {
    ring: Vec<Point>,
    contributors: HashSet<String>,
    /// Which connected component of the arrangement this face belongs to.
    ///
    /// Faces of *other* components lying inside a selected face are islands, and become
    /// holes. Faces of the same component are just its own subdivisions, and do not.
    component_id: usize,
}

fn edge_key(u: usize, v: usize) -> (usize, usize) {
    if u < v {
        (u, v)
    } else {
        (v, u)
    }
}

/// Build the planar straight-line graph and extract its face rings.
///
/// Returns an empty list when there are no usable edges — not an error, just nothing
/// enclosed — and `None` when the arrangement is too big to resolve.
///
/// Collinear overlapping segments need no pass of their own: a 1D overlap always puts at
/// least one segment's endpoint on the other segment, so the T-junction pass splits them
/// and the node-pair dedupe in `add_edge` collapses the coincident pieces into one.
fn build_faces(raw_segments: &[SourceSegment], options: &BucketFillOptions) -> Option<Vec<Face>> {
    let eps = options.snap_epsilon;
    // Two radii, deliberately decoupled. `eps` governs node merging and T-junctions, and
    // must stay small because merging *relocates* vertices and T-junctions route edges
    // *through* nodes — so it bounds how far the fill can drift from the real strokes.
    // `gap_tolerance` governs bridging, which only ever adds edges, so it can be generous.
    let mut store = NodeStore::new(eps);
    let mut segments: Vec<WorkingSegment> = Vec::new();
    // The first element that produced each node, used to attribute bridge edges.
    let mut node_element: HashMap<usize, String> = HashMap::new();

    for source in raw_segments {
        // Sub-epsilon segments are deliberately *not* dropped: their endpoints merge into
        // the same node, which collapses them while keeping the outline chain connected.
        // Dropping them would disconnect densely subdivided curves — the tiny corner arcs
        // a diamond has even with no roundness — and leave the region open.
        let a = store.get_or_create(source.a);
        let b = store.get_or_create(source.b);
        node_element
            .entry(a)
            .or_insert_with(|| source.element_id.clone());
        node_element
            .entry(b)
            .or_insert_with(|| source.element_id.clone());
        if a == b {
            continue;
        }
        let pa = store.nodes[a];
        let pb = store.nodes[b];
        segments.push(WorkingSegment {
            a,
            b,
            pa,
            pb,
            element_id: source.element_id.clone(),
            box_: WorldBounds {
                min_x: pa.x.min(pb.x),
                min_y: pa.y.min(pb.y),
                max_x: pa.x.max(pb.x),
                max_y: pa.y.max(pb.y),
            },
            splits: vec![(a, 0.0), (b, 1.0)],
        });
    }

    // Transversal intersections. Broad phase: sort by bbox min-x and sweep, so each
    // candidate pair is tested once. A grid is a poor fit here — dense overlapping
    // segments share many consecutive cells, and deduping the geometry test still
    // re-enumerates every pair in every cell.
    let mut by_min_x: Vec<usize> = (0..segments.len()).collect();
    by_min_x.sort_by(|&a, &b| {
        segments[a]
            .box_
            .min_x
            .partial_cmp(&segments[b].box_.min_x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for oi in 0..by_min_x.len() {
        let i = by_min_x[oi];
        let sweep_max_x = segments[i].box_.max_x + eps;
        for &j in by_min_x.iter().skip(oi + 1) {
            if segments[j].box_.min_x > sweep_max_x {
                break;
            }
            if !bounds_intersect(expand_bounds(segments[i].box_, eps), segments[j].box_) {
                continue;
            }
            let Some(intersection) = segment_intersection_point(
                (segments[i].pa, segments[i].pb),
                (segments[j].pa, segments[j].pb),
                eps,
            ) else {
                continue;
            };
            let node = store.get_or_create(intersection);
            // Intersection nodes need a source too: one may end up a loose end after
            // visibility clipping and get bridged, and bridge attribution reads this.
            node_element
                .entry(node)
                .or_insert_with(|| segments[i].element_id.clone());
            let p = store.nodes[node];
            let ti = project_param(segments[i].pa, segments[i].pb, p);
            let tj = project_param(segments[j].pa, segments[j].pb, p);
            segments[i].splits.push((node, ti));
            segments[j].splits.push((node, tj));
        }
    }

    // Intersection splitting can inflate the node count quadratically in pathological
    // scenes. Bail before the later passes turn a click into a multi-second freeze.
    if store.nodes.len() > options.max_boundary_segments * 4 {
        return None;
    }

    // Broad phase for T-junctions: put each segment in the cells along its path, inflated
    // by `eps`. The cell size scales with the scene so even a scene-spanning segment walks
    // a bounded number of cells.
    let mut scene_min_x = f64::INFINITY;
    let mut scene_min_y = f64::INFINITY;
    let mut scene_max_x = f64::NEG_INFINITY;
    let mut scene_max_y = f64::NEG_INFINITY;
    for s in &segments {
        scene_min_x = scene_min_x.min(s.box_.min_x);
        scene_min_y = scene_min_y.min(s.box_.min_y);
        scene_max_x = scene_max_x.max(s.box_.max_x);
        scene_max_y = scene_max_y.max(s.box_.max_y);
    }
    let scene_span = (scene_max_x - scene_min_x)
        .max(scene_max_y - scene_min_y)
        .max(0.0);
    let seg_cell_size = (eps * 2.0).max(scene_span / 128.0);
    let mut seg_grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (index, s) in segments.iter().enumerate() {
        for_each_cell_along_segment(s.pa, s.pb, eps, seg_cell_size, |cx, cy| {
            seg_grid.entry((cx, cy)).or_default().push(index);
        });
    }

    // T-junctions: split any segment passing within `eps` of an existing node.
    // Deliberately tight — routing an edge through a node further off the stroke would
    // visibly bend the filled shape. Wider gaps are the bridging pass's job instead.
    //
    // Each node tests only the segments in its own cell: the eps-inflated insertion above
    // guarantees any segment within `eps` of the node is there. No nodes are created in
    // this pass, so the grid stays complete throughout it.
    for n in 0..store.nodes.len() {
        let q = store.nodes[n];
        let key = (
            (q.x / seg_cell_size).floor() as i64,
            (q.y / seg_cell_size).floor() as i64,
        );
        let Some(bucket) = seg_grid.get(&key) else {
            continue;
        };
        for &index in bucket {
            let segment = &segments[index];
            if n == segment.a || n == segment.b {
                continue;
            }
            let t = project_param(segment.pa, segment.pb, q);
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            if distance_to_segment(
                q.x,
                q.y,
                segment.pa.x,
                segment.pa.y,
                segment.pb.x,
                segment.pb.y,
            ) <= eps
            {
                segments[index].splits.push((n, t));
            }
        }
    }

    // Emit atomic edges.
    let mut edge_set: HashSet<(usize, usize)> = HashSet::new();
    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut edge_to_elements: HashMap<(usize, usize), HashSet<String>> = HashMap::new();

    macro_rules! add_edge {
        ($store:expr, $u:expr, $v:expr, $element:expr) => {{
            let (u, v) = ($u, $v);
            if u != v && point_distance($store.nodes[u], $store.nodes[v]) >= eps {
                let key = edge_key(u, v);
                edge_to_elements
                    .entry(key)
                    .or_default()
                    .insert(String::from($element));
                if edge_set.insert(key) {
                    adjacency.entry(u).or_default().push(v);
                    adjacency.entry(v).or_default().push(u);
                }
            }
        }};
    }

    for segment in &segments {
        let mut by_node: Vec<(usize, f64)> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();
        for &(node, t) in &segment.splits {
            if seen.insert(node) {
                by_node.push((node, t));
            }
        }
        by_node.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        for k in 0..by_node.len().saturating_sub(1) {
            add_edge!(store, by_node[k].0, by_node[k + 1].0, &segment.element_id);
        }
    }

    if edge_set.is_empty() {
        // Nothing usable. Not an error — just no enclosed region.
        return Some(Vec::new());
    }

    // -------------------------------------------------------------------------
    // Bridging: close visual gaps up to `gap_tolerance`, so a sketchy joint — a stroke
    // stopping a few px short of another — still encloses a region.
    //
    // Additive by design. Each dangling end gets a short connector *edge* to its nearest
    // reachable geometry: an existing node, or a point on a nearby edge, splitting that
    // edge at the projection, which lies exactly on the stroke. Existing vertices are
    // never relocated, so unlike snapping a generous radius here cannot distort the
    // filled shape — the worst artefact is a tiny connector spanning the visual gap,
    // usually hidden under the stroke width.
    //
    // A dangling end is just a degree-1 node. That relies on boundaries being single-pass
    // paths, which is why the ideal-segment functions above exist: the rough renderer's
    // double-pass geometry would make every end degree-2 and nothing would ever bridge.
    // -------------------------------------------------------------------------
    let bridge_radius = eps.max(options.gap_tolerance);
    let loose_ends: Vec<usize> = {
        let mut ends: Vec<usize> = adjacency
            .iter()
            .filter(|(_, n)| n.iter().collect::<HashSet<_>>().len() == 1)
            .map(|(&node, _)| node)
            .collect();
        // A HashMap hands its keys back in an arbitrary order, and bridging is
        // order-dependent — an earlier bridge can close a later end. Sorting makes the
        // same scene give the same fill every time.
        ends.sort_unstable();
        ends
    };

    if !loose_ends.is_empty() {
        // Spatial hash of live edges, kept up to date as bridging splits them. Cells are
        // at least `bridge_radius` wide, so any edge within that radius of a query point
        // lies in the point's 3×3 neighbourhood. Entries are never removed — edges killed
        // by `unlink` are filtered out on lookup against `edge_set`.
        let edge_cell_size = bridge_radius.max(scene_span / 128.0).max(1.0);
        let mut edge_grid: HashMap<(i64, i64), Vec<(usize, usize)>> = HashMap::new();

        macro_rules! insert_live_edge {
            ($store:expr, $u:expr, $v:expr) => {{
                let (u, v) = ($u, $v);
                for_each_cell_along_segment(
                    $store.nodes[u],
                    $store.nodes[v],
                    0.0,
                    edge_cell_size,
                    |cx, cy| {
                        edge_grid.entry((cx, cy)).or_default().push((u, v));
                    },
                );
            }};
        }

        for &(u, v) in edge_set.clone().iter() {
            insert_live_edge!(store, u, v);
        }

        for &loose in &loose_ends {
            // Skip ends an earlier bridge already closed.
            if adjacency
                .get(&loose)
                .map(|n| n.iter().collect::<HashSet<_>>().len())
                != Some(1)
            {
                continue;
            }
            let p = store.nodes[loose];
            let neighbours: HashSet<usize> = adjacency
                .get(&loose)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();

            // Nearest non-adjacent node within the bridge radius. Candidates closer than
            // the snap epsilon are skipped: they are effectively the same point, and
            // `add_edge` refuses such a degenerate edge anyway.
            let mut best_node: Option<usize> = None;
            let mut best_node_distance = f64::INFINITY;
            store.for_each_in_radius(p, bridge_radius, |n| {
                if n == loose || neighbours.contains(&n) {
                    return;
                }
                let distance = point_distance(p, store.nodes[n]);
                if distance >= eps && distance <= bridge_radius && distance < best_node_distance {
                    best_node_distance = distance;
                    best_node = Some(n);
                }
            });

            // Nearest edge, not incident to this end, whose interior the end projects onto.
            let mut best_edge: Option<(usize, usize)> = None;
            let mut best_edge_distance = f64::INFINITY;
            let mut best_edge_t = 0.0;
            let loose_cx = (p.x / edge_cell_size).floor() as i64;
            let loose_cy = (p.y / edge_cell_size).floor() as i64;
            // One edge spans several cells, so it can show up in several of the nine.
            let mut seen_edges: HashSet<(usize, usize)> = HashSet::new();
            for dcx in -1..=1 {
                for dcy in -1..=1 {
                    let Some(bucket) = edge_grid.get(&(loose_cx + dcx, loose_cy + dcy)) else {
                        continue;
                    };
                    for &(eu, ev) in bucket {
                        let key = edge_key(eu, ev);
                        if !seen_edges.insert(key) {
                            continue;
                        }
                        if !edge_set.contains(&key) || eu == loose || ev == loose {
                            continue;
                        }
                        let (pu, pv) = (store.nodes[eu], store.nodes[ev]);
                        let t = project_param(pu, pv, p);
                        if t <= 0.0 || t >= 1.0 {
                            continue;
                        }
                        let distance = distance_to_segment(p.x, p.y, pu.x, pu.y, pv.x, pv.y);
                        if distance <= bridge_radius && distance < best_edge_distance {
                            best_edge_distance = distance;
                            best_edge = Some((eu, ev));
                            best_edge_t = t;
                        }
                    }
                }
            }

            // Every node gets a source when it is created; the fallback is defensive.
            let bridge_element = node_element
                .get(&loose)
                .cloned()
                .unwrap_or_else(|| segments[0].element_id.clone());

            match best_edge {
                Some((eu, ev)) if best_edge_distance < best_node_distance => {
                    // Split the edge at the projection — a point that lies on the stroke —
                    // and connect the loose end to it.
                    let projection = store.get_or_create(point_at_param(
                        store.nodes[eu],
                        store.nodes[ev],
                        best_edge_t,
                    ));
                    if projection == eu || projection == ev {
                        // The projection merged into an endpoint: a plain node bridge.
                        add_edge!(store, loose, projection, &bridge_element);
                        insert_live_edge!(store, loose, projection);
                    } else {
                        let key = edge_key(eu, ev);
                        node_element.entry(projection).or_insert_with(|| {
                            edge_to_elements
                                .get(&key)
                                .and_then(|owners| owners.iter().next().cloned())
                                .unwrap_or_else(|| bridge_element.clone())
                        });
                        // unlink
                        edge_set.remove(&key);
                        let owners = edge_to_elements.remove(&key).unwrap_or_default();
                        if let Some(list) = adjacency.get_mut(&eu) {
                            if let Some(at) = list.iter().position(|&x| x == ev) {
                                list.remove(at);
                            }
                        }
                        if let Some(list) = adjacency.get_mut(&ev) {
                            if let Some(at) = list.iter().position(|&x| x == eu) {
                                list.remove(at);
                            }
                        }
                        let owners: Vec<String> = if owners.is_empty() {
                            vec![bridge_element.clone()]
                        } else {
                            let mut sorted: Vec<String> = owners.into_iter().collect();
                            sorted.sort();
                            sorted
                        };
                        for owner in &owners {
                            add_edge!(store, eu, projection, owner.as_str());
                            add_edge!(store, projection, ev, owner.as_str());
                        }
                        insert_live_edge!(store, eu, projection);
                        insert_live_edge!(store, projection, ev);
                        add_edge!(store, loose, projection, &bridge_element);
                        insert_live_edge!(store, loose, projection);
                    }
                }
                _ => {
                    if let Some(node) = best_node {
                        add_edge!(store, loose, node, &bridge_element);
                        insert_live_edge!(store, loose, node);
                    }
                }
            }
        }
    }

    // Connected components of the final graph, bridges included — so a bridged island
    // shares a component with whatever it bridged to and cannot become a hole.
    let mut component_of: HashMap<usize, usize> = HashMap::new();
    let mut component_count = 0usize;
    let mut starts: Vec<usize> = adjacency.keys().copied().collect();
    starts.sort_unstable();
    for start in starts {
        if component_of.contains_key(&start) {
            continue;
        }
        let mut queue = vec![start];
        component_of.insert(start, component_count);
        while let Some(node) = queue.pop() {
            for &neighbour in adjacency.get(&node).map(|v| v.as_slice()).unwrap_or(&[]) {
                if let std::collections::hash_map::Entry::Vacant(slot) =
                    component_of.entry(neighbour)
                {
                    slot.insert(component_count);
                    queue.push(neighbour);
                }
            }
        }
        component_count += 1;
    }

    // Sort each node's outgoing half-edges by angle.
    let mut sorted_out: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut position_of: HashMap<usize, HashMap<usize, usize>> = HashMap::new();
    for (&node, neighbours) in &adjacency {
        let mut unique: Vec<usize> = Vec::new();
        let mut seen = HashSet::new();
        for &n in neighbours {
            if seen.insert(n) {
                unique.push(n);
            }
        }
        let from = store.nodes[node];
        unique.sort_by(|&p, &q| {
            let ap = (store.nodes[p].y - from.y).atan2(store.nodes[p].x - from.x);
            let aq = (store.nodes[q].y - from.y).atan2(store.nodes[q].x - from.x);
            ap.partial_cmp(&aq).unwrap_or(std::cmp::Ordering::Equal)
        });
        let positions = unique.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        sorted_out.insert(node, unique);
        position_of.insert(node, positions);
    }

    // Walk half-edges into face rings.
    let mut visited: HashSet<(usize, usize)> = HashSet::new();
    let mut faces: Vec<Face> = Vec::new();
    let max_steps = edge_set.len() * 2 + 4;
    let mut nodes_in_order: Vec<usize> = adjacency.keys().copied().collect();
    nodes_in_order.sort_unstable();
    for node in nodes_in_order {
        let firsts = adjacency.get(&node).cloned().unwrap_or_default();
        for first in firsts {
            if visited.contains(&(node, first)) {
                continue;
            }
            let mut ring: Vec<usize> = Vec::new();
            let mut from = node;
            let mut to = first;
            let mut steps = 0;
            while steps <= max_steps {
                steps += 1;
                visited.insert((from, to));
                ring.push(from);
                let outs = &sorted_out[&to];
                let twin_position = position_of[&to][&from];
                // The next edge of the face is the one immediately clockwise from the
                // reverse (twin) direction.
                let next_position = (twin_position + outs.len() - 1) % outs.len();
                let next = outs[next_position];
                from = to;
                to = next;
                if from == node && to == first {
                    break;
                }
            }
            if ring.len() >= 3 {
                let mut contributors = HashSet::new();
                for k in 0..ring.len() {
                    let key = edge_key(ring[k], ring[(k + 1) % ring.len()]);
                    if let Some(owners) = edge_to_elements.get(&key) {
                        for id in owners {
                            contributors.insert(id.clone());
                        }
                    }
                }
                faces.push(Face {
                    ring: ring.iter().map(|&i| store.nodes[i]).collect(),
                    contributors,
                    component_id: component_of.get(&node).copied().unwrap_or(usize::MAX),
                });
            }
        }
    }

    Some(faces)
}

struct FaceSelection {
    face: Face,
    holes: Vec<Face>,
}

/// Pick the smallest bounded face containing the click, and the islands inside it.
///
/// The walk rule — "next is the neighbour immediately clockwise of the twin", in screen
/// coordinates with y pointing down — traces every **bounded** face with negative shoelace
/// area and every component's **outside contour** with positive area. That is a structural
/// invariant of the walk, and bounded-versus-unbounded is decided by that sign *alone*.
///
/// Not by comparing areas: the outermost outline's interior face and its outside contour
/// trace the same polygon with the same absolute area, so any size-based rule degenerates
/// into a coin flip decided by enumeration order — which is exactly how hole detection
/// ends up depending on the order elements happen to sit in the scene.
fn select_face_from_arrangement(
    faces: &[Face],
    point: Point,
    options: &BucketFillOptions,
) -> Option<FaceSelection> {
    if faces.is_empty() {
        return None;
    }
    // Negated: `polygon_signed_area` is CCW-positive, while the invariant above is stated
    // the other way round, with bounded faces negative.
    let areas: Vec<f64> = faces
        .iter()
        .map(|face| -polygon_signed_area(&face.ring, 0.0))
        .collect();

    let mut best: Option<usize> = None;
    let mut best_area = f64::INFINITY;
    for (i, face) in faces.iter().enumerate() {
        let area = areas[i];
        if area > 0.0 {
            continue;
        }
        let abs_area = area.abs();
        if abs_area < options.min_area {
            continue;
        }
        if !polygon_includes_point_non_zero(point, &face.ring) {
            continue;
        }
        if abs_area < best_area {
            best_area = abs_area;
            best = Some(i);
        }
    }

    let selected = best?;
    let selected_component = faces[selected].component_id;

    let candidates: Vec<usize> = (0..faces.len())
        .filter(|&i| {
            faces[i].component_id != selected_component
                // A component's outside contour shares the unbounded face's orientation.
                && areas[i] > 0.0
                && areas[i].abs() >= options.min_area
                // The clicked face never sits inside one of its own holes. Defensive: the
                // smallest-containing-face rule above already guarantees it.
                && !polygon_includes_point_non_zero(point, &faces[i].ring)
                // Components never intersect — a crossing would have merged them — so one
                // vertex settles containment.
                && polygon_includes_point_non_zero(faces[i].ring[0], &faces[selected].ring)
        })
        .collect();

    // Only outermost islands count: one nested inside another already sits in a hole.
    let holes: Vec<Face> = candidates
        .iter()
        .filter(|&&hole| {
            !candidates.iter().any(|&other| {
                other != hole
                    && polygon_includes_point_non_zero(faces[hole].ring[0], &faces[other].ring)
            })
        })
        .map(|&i| faces[i].clone())
        .collect();

    Some(FaceSelection {
        face: faces[selected].clone(),
        holes,
    })
}

// -----------------------------------------------------------------------------
// simplification
// -----------------------------------------------------------------------------

fn dedupe_consecutive(pts: &[Point], eps: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for &p in pts {
        if out
            .last()
            .map(|&last| point_distance(last, p) >= eps)
            .unwrap_or(true)
        {
            out.push(p);
        }
    }
    while out.len() > 1 && point_distance(out[0], out[out.len() - 1]) < eps {
        out.pop();
    }
    out
}

fn remove_collinear(pts: &[Point], tolerance: f64) -> Vec<Point> {
    if pts.len() <= 3 {
        return pts.to_vec();
    }
    let mut out: Vec<Point> = Vec::new();
    for i in 0..pts.len() {
        let prev = *out.last().unwrap_or(&pts[pts.len() - 1]);
        let next = pts[(i + 1) % pts.len()];
        if perpendicular_distance(pts[i], prev, next) >= tolerance {
            out.push(pts[i]);
        }
    }
    if out.len() >= 3 {
        out
    } else {
        pts.to_vec()
    }
}

/// Simplify an open ring under a point budget.
///
/// `None` when it collapses below a triangle, or cannot be made to fit.
fn simplify_ring(ring: &[Point], options: &BucketFillOptions, budget: usize) -> Option<Vec<Point>> {
    let pts = dedupe_consecutive(ring, options.snap_epsilon);
    if pts.len() < 3 {
        return None;
    }

    // Ramer–Douglas–Peucker at the same default tolerance the freedraw renderer uses,
    // escalating until it fits the cap.
    let mut tolerance = 0.75;
    let mut simplified = simplify_path(&pts, tolerance);
    while simplified.len() > budget && tolerance < 1e6 {
        tolerance *= 2.0;
        simplified = simplify_path(&pts, tolerance);
    }
    if simplified.len() > budget {
        return None;
    }

    let simplified = remove_collinear(&simplified, 0.05);
    if simplified.len() >= 3 {
        Some(simplified)
    } else {
        None
    }
}

/// Splice a hole into the ring as a zero-width keyhole.
///
/// Walk the ring to the attachment vertex, bridge across to the hole, go round it, and
/// bridge back along the same line. The doubled bridge cancels under the even-odd rule the
/// renderer fills with, so the hole's interior stays unpainted while the result is still a
/// single closed polygon — which is what lets a fill with holes be one ordinary element.
fn splice_hole_into_ring(ring: &[Point], hole: &[Point]) -> Vec<Point> {
    // Opposite winding. Even-odd does not care, but this keeps the path correct under the
    // non-zero rule too, which is what an SVG export may well use.
    let oriented: Vec<Point> =
        if polygon_signed_area(hole, 0.0).signum() == polygon_signed_area(ring, 0.0).signum() {
            hole.iter().rev().copied().collect()
        } else {
            hole.to_vec()
        };

    // The shortest bridge, so the invisible channel never spans the region.
    let mut best_ring_index = 0;
    let mut best_hole_index = 0;
    let mut best_distance = f64::INFINITY;
    for (i, &r) in ring.iter().enumerate() {
        for (j, &h) in oriented.iter().enumerate() {
            let distance = point_distance(r, h);
            if distance < best_distance {
                best_distance = distance;
                best_ring_index = i;
                best_hole_index = j;
            }
        }
    }

    let mut out: Vec<Point> = ring[..=best_ring_index].to_vec();
    // The full hole loop, ending back on its attachment vertex.
    for k in 0..=oriented.len() {
        out.push(oriented[(best_hole_index + k) % oriented.len()]);
    }
    out.push(ring[best_ring_index]);
    out.extend_from_slice(&ring[best_ring_index + 1..]);
    out
}

/// Simplify the face ring and its holes, splice the holes in, and close the result.
///
/// Each ring is simplified *independently*, so the keyhole bridges stay exactly
/// zero-width. Holes that cannot fit the remaining budget are dropped — the island then
/// simply gets painted over, which is the behaviour from before holes were supported and
/// is better than refusing to fill at all.
fn finalize_polygon(
    outer_ring: &[Point],
    hole_rings: &[Vec<Point>],
    options: &BucketFillOptions,
) -> Option<(Vec<Point>, Vec<usize>)> {
    let mut ring = simplify_ring(outer_ring, options, options.max_generated_points)?;

    // Largest first, so a tight budget drops the least visible islands.
    let mut by_area: Vec<(usize, &Vec<Point>)> = hole_rings.iter().enumerate().collect();
    by_area.sort_by(|a, b| {
        polygon_area(b.1)
            .partial_cmp(&polygon_area(a.1))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut spliced = Vec::new();
    for (index, hole_ring) in by_area {
        // A spliced hole costs its own points plus the two bridge duplicates.
        let budget = options.max_generated_points.saturating_sub(ring.len() + 2);
        if budget < 3 {
            break;
        }
        let Some(hole) = simplify_ring(hole_ring, options, budget) else {
            continue;
        };
        ring = splice_hole_into_ring(&ring, &hole);
        spliced.push(index);
    }

    // Close the polygon exactly once.
    let first = ring[0];
    ring.push(first);
    Some((ring, spliced))
}

// -----------------------------------------------------------------------------
// public API
// -----------------------------------------------------------------------------

fn find_owner<'a>(point: Point, elements: &'a [&'a DrawElement]) -> Option<&'a DrawElement> {
    for element in elements.iter().rev() {
        // Fill-compatible paint is deliberately never an *owner*, unlike its boundary and
        // coverer roles: re-clicking a filled region should either restyle that paint or
        // re-derive the region from the actual strokes, not from the previous fill's ring
        // — and a keyhole ring makes a degenerate owner outline.
        //
        // An element that renders no pixels cannot own either, or a click on what looks
        // like empty canvas would conjure a fill out of an invisible shape.
        if !renders_any_mark(element) || is_bucket_fill_compatible(element) {
            continue;
        }
        if !is_closed_owner_candidate(element) {
            continue;
        }
        if is_point_in_element(point, element) {
            return Some(element);
        }
    }
    None
}

/// The region under `point`, as a polygon ready to become an element.
///
/// `elements` must be in scene order, bottom-most first, and hold no deleted elements.
pub fn compute_bucket_fill(
    point: Point,
    elements: &[&DrawElement],
    options: &BucketFillOptions,
) -> Result<BucketFill, BucketFillFailure> {
    // 1. The owner under the pointer — the fast common case. Without one (a region formed
    // by open lines, or a self-intersecting outline whose hit test fails) the owner-less
    // fallback below builds the arrangement from everything near the click instead.
    let owner = find_owner(point, elements);
    let owner_id: Option<String> = owner.map(|o| o.id.clone());

    let index_of: HashMap<&str, usize> = elements
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();

    // 2 + 3. Collect boundary segments inside `bounds`, tagged with their source and
    // clipped to their visible parts — what is hidden behind opaque paint is not a
    // boundary, because only what a person can see should stop a fill. `None` means the
    // segment cap was hit.
    let collect_segments = |bounds: WorldBounds,
                            primary: Option<&DrawElement>|
     -> Option<Vec<SourceSegment>> {
        let in_range = |element: &DrawElement| bounds_intersect(bounds, element_bounds(element));
        let boundaries: Vec<&&DrawElement> = elements
            .iter()
            .filter(|element| {
                Some(element.id.as_str()) != primary.map(|p| p.id.as_str())
                    && is_eligible_boundary(element)
                    && in_range(element)
            })
            .collect();
        // The opaque elements that hide outlines under them — existing fills included: an
        // outline buried under one is invisible, so it must not stop a new fill either.
        let coverers: Vec<&&DrawElement> = elements
            .iter()
            .filter(|element| renders_opaque_fill(element) && in_range(element))
            .collect();

        let mut raw: Vec<SourceSegment> = Vec::new();
        let collect = |element: &DrawElement, raw: &mut Vec<SourceSegment>| {
            let element_index = index_of.get(element.id.as_str()).copied().unwrap_or(0);
            let coverers_above: Vec<&DrawElement> = coverers
                .iter()
                .filter(|c| {
                    c.id != element.id
                        && index_of.get(c.id.as_str()).copied().unwrap_or(0) > element_index
                })
                .map(|c| **c)
                .collect();
            // Half this element's stroke still renders when its centreline lies on a
            // coverer's edge, so that band is never hidden.
            let clip_margin = options
                .snap_epsilon
                .max(element.stroke_width / 2.0)
                .max(BUCKET_FILL_COVER_MARGIN);

            let mut segments = element_segments(element);
            // A freedraw or a non-polygon line renders closed once its ends are within
            // `LINE_CONFIRM_THRESHOLD`, but its segment chain leaves that closure gap
            // open — so close it explicitly. Deliberately the renderer's own rule and not
            // `gap_tolerance`: whether an element paints its background has to match
            // whether a fill treats it as closed.
            if matches!(
                element.kind,
                DrawElementType::Freedraw | DrawElementType::Line
            ) && is_path_a_loop(element_points(element))
                && !segments.is_empty()
            {
                let first = segments[0].0;
                let last = segments[segments.len() - 1].1;
                if point_distance(first, last) >= options.snap_epsilon {
                    segments.push((last, first));
                }
            }

            for (a, b) in segments {
                // Enforce the budget *while* collecting: without this a huge scene does
                // all the expensive visibility clipping before the cap is ever consulted.
                if raw.len() > options.max_boundary_segments {
                    return;
                }
                // Sub-epsilon segments stay — `build_faces` collapses them through node
                // merging without breaking the chain. Only true zero-length ones are noise.
                if point_distance(a, b) == 0.0 {
                    continue;
                }
                if coverers_above.is_empty() {
                    raw.push(SourceSegment {
                        a,
                        b,
                        element_id: element.id.clone(),
                    });
                    continue;
                }
                for (ca, cb) in clip_segment_to_visible(
                    a,
                    b,
                    &coverers_above,
                    options.snap_epsilon,
                    clip_margin,
                ) {
                    raw.push(SourceSegment {
                        a: ca,
                        b: cb,
                        element_id: element.id.clone(),
                    });
                }
            }
        };

        if let Some(primary) = primary {
            collect(primary, &mut raw);
        }
        for element in boundaries {
            if raw.len() > options.max_boundary_segments {
                break;
            }
            collect(element, &mut raw);
        }
        if raw.len() > options.max_boundary_segments {
            None
        } else {
            Some(raw)
        }
    };

    // 4. Resolve the face under the click.
    let selection: FaceSelection = if let Some(owner) = owner {
        let pad = options.gap_tolerance + 2.0 + owner.stroke_width.max(1.0);
        let search_bounds = expand_bounds(element_bounds(owner), pad);
        let raw =
            collect_segments(search_bounds, Some(owner)).ok_or(BucketFillFailure::TooComplex)?;
        let faces = build_faces(&raw, options).ok_or(BucketFillFailure::TooComplex)?;
        select_face_from_arrangement(&faces, point, options).ok_or(BucketFillFailure::OpenRegion)?
    } else {
        // Owner-less fallback: search an expanding box around the click. A face is final
        // once its ring stays clear of the box frontier — anything that could still
        // subdivide it would intersect the box and is already included, whereas a ring
        // touching the frontier may be missing boundaries further out.
        let total_eligible = elements.iter().filter(|e| is_eligible_boundary(e)).count();
        if total_eligible == 0 {
            return Err(BucketFillFailure::NoOwner);
        }
        let mut found: Option<FaceSelection> = None;
        let mut radius = options.fallback_search_radius;
        for attempt in 0..4 {
            if found.is_some() {
                break;
            }
            let box_ = WorldBounds {
                min_x: point.x - radius,
                min_y: point.y - radius,
                max_x: point.x + radius,
                max_y: point.y + radius,
            };
            let in_range = elements
                .iter()
                .filter(|e| is_eligible_boundary(e) && bounds_intersect(box_, element_bounds(e)))
                .count();
            // Once every eligible element is in range, whatever this box yields is final:
            // a bigger box would rebuild the identical arrangement, and the frontier check
            // below only exists to approximate exactly this condition.
            let is_final_attempt = attempt == 3 || in_range == total_eligible;
            radius *= 2.0;
            if in_range == 0 {
                // Empty at this radius, but the region may still lie further out — a
                // distant enclosure whose strokes are all beyond the first box.
                continue;
            }
            // The fallback is speculative, so its failures are never "too complex" —
            // clicking open canvas should not complain about scene size.
            let Some(raw) = collect_segments(box_, None) else {
                return Err(BucketFillFailure::NoOwner);
            };
            let Some(faces) = build_faces(&raw, options) else {
                return Err(BucketFillFailure::NoOwner);
            };
            let selected = select_face_from_arrangement(&faces, point, options);
            if let Some(selected) = selected {
                let clear_of_frontier = selected.face.ring.iter().all(|p| {
                    p.x > box_.min_x + options.gap_tolerance
                        && p.y > box_.min_y + options.gap_tolerance
                        && p.x < box_.max_x - options.gap_tolerance
                        && p.y < box_.max_y - options.gap_tolerance
                });
                if is_final_attempt || clear_of_frontier {
                    found = Some(selected);
                }
            }
            if found.is_none() && is_final_attempt {
                break;
            }
        }
        // No enclosed region under the pointer. Stays silent like `NoOwner` — clicking
        // open canvas should not nag.
        found.ok_or(BucketFillFailure::NoOwner)?
    };

    let FaceSelection { face, holes } = selection;

    // 5. Simplify and validate. The ring may be a keyhole path, in which case its signed
    // area is the *net* filled area — which is exactly what the size check should measure.
    let hole_rings: Vec<Vec<Point>> = holes.iter().map(|h| h.ring.clone()).collect();
    let (scene_points, spliced) = finalize_polygon(&face.ring, &hole_rings, options)
        .ok_or(BucketFillFailure::InvalidPolygon)?;
    if polygon_area(&scene_points) < options.min_area {
        return Err(BucketFillFailure::TooSmall);
    }

    // Spliced islands genuinely bound the visible fill, so they join the boundary metadata.
    let mut contributors = face.contributors.clone();
    for &index in &spliced {
        for id in &holes[index].contributors {
            contributors.insert(id.clone());
        }
    }

    // 6. Z-order. Two constraints, over *all* scene elements rather than just the region's
    // participants:
    //
    // - the fill goes **above** the topmost opaque element whose paint overlaps the region
    //   anywhere. After a fill the region should read uniformly filled — that is what a
    //   raster flood would have produced — so older paint must not poke through it;
    // - otherwise it goes **below** the lowest element that has to stay visible: every
    //   participant, plus any visible non-participant whose mark lies inside the region. A
    //   floating label or icon would otherwise be buried. A stroke merely *crossing* the
    //   region would have subdivided the face and be a participant already, so sampling
    //   centres and path points catches the wholly-inside marks that remain.
    //
    // When the two conflict, above-the-coverer wins: a mark under opaque paint was mostly
    // hidden already, whereas paint poking through a fresh fill always reads as failure.
    let mut participant_ids: HashSet<String> = contributors.clone();
    if let Some(id) = &owner_id {
        participant_ids.insert(id.clone());
    }
    for hole in &holes {
        for id in &hole.contributors {
            participant_ids.insert(id.clone());
        }
    }

    let region_bounds = {
        let mut b = WorldBounds {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        };
        for p in &scene_points {
            b.min_x = b.min_x.min(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_x = b.max_x.max(p.x);
            b.max_y = b.max_y.max(p.y);
        }
        b
    };

    let mark_inside_region = |element: &DrawElement| -> bool {
        if !bounds_intersect(region_bounds, element_bounds(element)) {
            return false;
        }
        let b = element_bounds(element);
        let mut samples = vec![Point {
            x: (b.min_x + b.max_x) / 2.0,
            y: (b.min_y + b.max_y) / 2.0,
        }];
        if matches!(
            element.kind,
            DrawElementType::Line | DrawElementType::Arrow | DrawElementType::Freedraw
        ) {
            let world = crate::selection::linear::world_points(element);
            let step = (world.len() / 8).max(1);
            samples.extend(world.iter().step_by(step).copied());
        }
        samples
            .iter()
            .any(|&s| polygon_includes_point(s, &scene_points))
    };

    // Whether the element's paint overlaps the region anywhere. Sampled both ways —
    // outline inside the region, region ring inside the element — so partial overlap and
    // containment either way are caught. Exactness does not matter for a z-order call.
    let paint_overlaps_region = |element: &DrawElement| -> bool {
        if !bounds_intersect(region_bounds, element_bounds(element)) {
            return false;
        }
        if is_point_in_element(point, element) {
            return true;
        }
        let outline = element_segments(element);
        let outline_step = (outline.len() / 16).max(1);
        if outline
            .iter()
            .step_by(outline_step)
            .any(|&(a, _)| polygon_includes_point(a, &scene_points))
        {
            return true;
        }
        let ring_step = (scene_points.len() / 16).max(1);
        scene_points
            .iter()
            .step_by(ring_step)
            .any(|&p| is_point_in_element(p, element))
    };

    let mut lowest_above: Option<&DrawElement> = None;
    let mut covering: Option<&DrawElement> = None;
    for element in elements {
        // The coverer constraint is evaluated for every element, not just the ones that
        // must stay above: an opaque element overlapping the region away from the click —
        // or straddling its edge, centre outside — is neither a participant nor a sampled
        // in-region mark, and missing it would put the fill underneath it and let its
        // paint show through.
        if renders_opaque_fill(element) && paint_overlaps_region(element) {
            covering = Some(element);
        }
        let must_stay_above = participant_ids.contains(&element.id)
            || (renders_any_mark(element) && mark_inside_region(element));
        if must_stay_above && lowest_above.is_none() {
            lowest_above = Some(element);
        }
    }

    // A face always has contributors drawn from `elements`, so a participant exists; the
    // last-element fallback is defensive only.
    let insertion = match covering {
        Some(element) => BucketFillInsertion {
            placement: Placement::Above,
            element_id: element.id.clone(),
        },
        None => BucketFillInsertion {
            placement: Placement::Below,
            element_id: lowest_above
                .or_else(|| elements.last().copied())
                .map(|e| e.id.clone())
                .ok_or(BucketFillFailure::NoOwner)?,
        },
    };

    let mut boundary_element_ids: Vec<String> = contributors
        .into_iter()
        .filter(|id| Some(id.as_str()) != owner_id.as_deref())
        .collect();
    // A set has no order of its own, and this list reaches the host and the wire.
    boundary_element_ids.sort();

    Ok(BucketFill {
        owner_id,
        boundary_element_ids,
        scene_points,
        insertion,
    })
}

/// Whether a click landed on a fill the bucket should restyle in place, rather than
/// stacking an identical one on top.
///
/// Fill-compatibility is decided by shape, so a fill someone gave a stroke to has been
/// repurposed into an outline and bounds the new fill instead of being recoloured.
///
/// The area and bounds guard on top is what stops a click inside a shape drawn *over* a
/// filled region from recolouring the fill underneath it, and makes a click on a
/// since-subdivided part of an old fill create a new, smaller fill instead.
pub fn is_restylable_fill(hit: &DrawElement, scene_points: &[Point]) -> bool {
    if !is_bucket_fill_compatible(hit) {
        return false;
    }
    let ring = crate::selection::linear::world_points(hit);
    if ring.len() < 3 {
        return false;
    }
    let tolerance = BUCKET_FILL_REGION_MATCH_TOLERANCE;
    let area_a = polygon_area(&ring);
    let area_b = polygon_area(scene_points);
    // Relative, because a region-sized absolute tolerance would be far too tight on a
    // large fill and far too loose on a small one.
    if (area_a - area_b).abs() > tolerance * (area_a.max(area_b)).sqrt().max(1.0) {
        return false;
    }
    let bounds_of = |pts: &[Point]| {
        pts.iter().fold(
            WorldBounds {
                min_x: f64::INFINITY,
                min_y: f64::INFINITY,
                max_x: f64::NEG_INFINITY,
                max_y: f64::NEG_INFINITY,
            },
            |b, p| WorldBounds {
                min_x: b.min_x.min(p.x),
                min_y: b.min_y.min(p.y),
                max_x: b.max_x.max(p.x),
                max_y: b.max_y.max(p.y),
            },
        )
    };
    let a = bounds_of(&ring);
    let b = bounds_of(scene_points);
    (a.min_x - b.min_x).abs() <= tolerance
        && (a.min_y - b.min_y).abs() <= tolerance
        && (a.max_x - b.max_x).abs() <= tolerance
        && (a.max_y - b.max_y).abs() <= tolerance
}
