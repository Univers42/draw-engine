//! Build-a-diagram-by-keyboard: Ctrl/Cmd+Arrow grows a connected node off the selected
//! shape, Alt+Arrow walks the connections. A port of Excalidraw's
//! `packages/element/src/flowchart.ts` and `packages/excalidraw/components/App.flowchart.ts`
//! (`@1118751f`), held to the oracle's own output by `tests/ci_flowchart_oracle.rs`, which
//! replays what that code produces (`tools/flowchart-oracle`). One extra the oracle lacks:
//! while Ctrl/Cmd is held, 1/2/3 chooses the pending nodes' shape.
//!
//! # Preview, then one commit
//!
//! Ctrl/Cmd+Arrow computes a cluster of nodes and binding arrows and holds them in
//! [`FlowchartCreator::pending`] — painted at a fifth of their opacity over the board
//! (`wasm/paint.rs`, as the oracle's `pendingFlowchartNodes`) but **not** added to the scene,
//! so `get_scene` and history never see them until Ctrl/Cmd is released. A repeat press in
//! the same direction grows the cluster by one and recomputes it from scratch, as the
//! oracle's `numberOfNodes` does; a different direction restarts it at one. Every press
//! reveals the cluster if any of it is off screen — the camera zooms out to hold it, or
//! in, up to 100%, to a small one ([`DrawEngine::reveal_if_hidden`]). [`DrawEngine::
//! flowchart_commit`] inserts the cluster as `insertNewElements` does — into its frame's
//! children when it joined one —, selects the first new node, reveals it, and takes **one**
//! step of history. Escape ([`DrawEngine::flowchart_cancel`]) drops what was never added.
//!
//! # Placement
//!
//! [`place_cluster`] is `placeCluster` (`flowchart.ts@1118751f:161-230`): the cluster sits
//! one gap away from the parent on the creation axis, and slides along the cross axis to
//! the free slot nearest the parent's centre, dodging every node reachable from the parent
//! by a bound arrow ([`connected_flowchart_obstacles`], `getConnectedFlowchartNodes`). A
//! `sticky_cross_start` anchors an already-visible pending cluster so growing it does not
//! shuffle the nodes already on screen (`flowchart.ts@1118751f:203-212`).
//!
//! # The arrow
//!
//! Every link is an elbow arrow, as the oracle's (`createBindingArrow`,
//! `flowchart.ts@1118751f:310-450`): [`binding_arrow`] starts it a padding off the facing
//! sides, binds both ends in orbit and routes it with the elbow router
//! ([`crate::scene::elbow::bind_and_route`]), which also snaps each anchor to the node's
//! outline — a diamond's and a turned node's included. Only elbow arrows link nodes
//! ([`is_flowchart_link`]), for placement and for navigation alike.
//!
//! # Navigation
//!
//! [`FlowchartNavigator`] is `FlowChartNavigator` (`flowchart.ts@1118751f:452-680`): explore
//! one direction, and a repeat press in the same direction cycles same-level nodes; run out
//! and it falls back to an unvisited node in any other direction, for faster hopping around
//! a diagram. A link's direction is the side of the node its arrow leaves or arrives at —
//! [`heading_for_point_from_element`], `headingForPointFromElement` — successors first,
//! then predecessors.
//!
//! # Cost
//!
//! One pass over the scene per press, whatever its size: the obstacle walk builds its
//! adjacency once rather than rescanning every arrow per node reached, and a navigation
//! step reads each arrow once per direction asked. The preview is painted over the cached
//! board, which a press leaves alone. Each new arrow is routed as it is built, against
//! its two nodes alone. Measured in `benches/editing.rs` (`flowchart/*`): beside a
//! 200-node diagram on a 20,000-shape board, a press costs about 150µs, ten presses in a
//! row — each laying out one more node than the last, 55 arrows routed in all — 3.8ms, a
//! walk step 120µs and the release 7µs. The scene pass is most of a press on a large
//! board; the oracle reads each node's `boundElements` instead, which this scene does
//! not index.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::camera::{Point, WorldBounds};
use crate::engine::DrawEngine;
use crate::scene::{
    create_element, elbow, element_overlaps_frame, element_rotated_bounds, is_frame,
    is_target_kind, linear_from_endpoints, scene_outline_bounds, Arrowhead, DrawElement,
    DrawElementStyle, DrawElementType, FillStyle, Geometry, Scene,
};

/// How far off a node's side a new link starts and ends, before routing moves it —
/// `createBindingArrow`'s `PADDING` (`flowchart.ts@1118751f:318`).
const ARROW_PADDING: f64 = 6.0;

/// One gap in scene units, on both axes — Excalidraw's `VERTICAL_OFFSET` /
/// `HORIZONTAL_OFFSET` (`flowchart.ts@1118751f:57-58`), both 100.
const VERTICAL_OFFSET: f64 = 100.0;
const HORIZONTAL_OFFSET: f64 = 100.0;

/// Which way an arrow points, or a person navigates — Excalidraw's `LinkDirection`, and
/// its `Heading` too: the four are the same set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkDirection {
    Up,
    Down,
    Left,
    Right,
}

impl LinkDirection {
    /// From the wasm boundary's plain string; `None` for anything else.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

/// Whether `element` can start or extend a flowchart — Excalidraw's
/// `isFlowchartNodeElement` (`typeChecks.ts@1118751f:286-295`): a rectangle, a diamond, an
/// ellipse, or a sticky note — and here a figure, the parametric shape the oracle lacks.
fn is_flowchart_node(element: &DrawElement) -> bool {
    matches!(
        element.kind,
        DrawElementType::Rectangle
            | DrawElementType::Diamond
            | DrawElementType::Ellipse
            | DrawElementType::StickyNote
            | DrawElementType::Figure
    )
}

/// Whether `element` links two flowchart nodes: the oracle's `isElbowArrow`
/// (`flowchart.ts@1118751f:119`, `:595`).
fn is_flowchart_link(element: &DrawElement) -> bool {
    crate::scene::elbow::is_elbow(element)
}

// --------------------------------------------------------------------------- placement

struct Interval {
    start: f64,
    end: f64,
}

/// Sorted, non-overlapping. `mergeIntervals` (`flowchart.ts@1118751f:62-76`).
fn merge_intervals(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut merged: Vec<Interval> = Vec::with_capacity(intervals.len());
    for interval in intervals {
        match merged.last_mut() {
            Some(last) if interval.start <= last.end => last.end = last.end.max(interval.end),
            _ => merged.push(interval),
        }
    }
    merged
}

fn interval_is_free(start: f64, size: f64, occupied: &[Interval]) -> bool {
    occupied
        .iter()
        .all(|o| start + size <= o.start || start >= o.end)
}

/// Nearest free `start` for a segment of `size`, searching both sides of `ideal`,
/// ties toward the positive side. `findNearestFreeSlot` (`flowchart.ts@1118751f:83-110`).
/// `occupied` must already be [`merge_intervals`]d.
fn find_nearest_free_slot(ideal: f64, size: f64, occupied: &[Interval]) -> f64 {
    if interval_is_free(ideal, size, occupied) {
        return ideal;
    }
    let mut gap_starts = vec![f64::NEG_INFINITY];
    gap_starts.extend(occupied.iter().map(|o| o.end));
    let mut gap_ends: Vec<f64> = occupied.iter().map(|o| o.start).collect();
    gap_ends.push(f64::INFINITY);

    let mut best = ideal;
    let mut best_distance = f64::INFINITY;
    for i in 0..gap_starts.len() {
        if gap_ends[i] - gap_starts[i] < size {
            continue;
        }
        let start = ideal.clamp(gap_starts[i], gap_ends[i] - size);
        let distance = (start - ideal).abs();
        if distance <= best_distance {
            best = start;
            best_distance = distance;
        }
    }
    best
}

/// Every node reachable from `start` by a bound link — the connected component that acts
/// as the obstacle set while placing a new cluster, each by its turned box
/// (`aabbForElement`). `getConnectedFlowchartNodes` (`flowchart.ts@1118751f:115-151`): a
/// neighbour is marked visited when first met, and walked on only if an arrow can bind it.
///
/// The adjacency is built in one pass over the scene; the oracle rescans every arrow for
/// each node it reaches, which is quadratic on a large diagram. The walk and its order are
/// the same.
fn connected_flowchart_obstacles(scene: &Scene, start: &DrawElement) -> Vec<WorldBounds> {
    let mut neighbours: HashMap<&str, Vec<&str>> = HashMap::new();
    for arrow in scene.iter_ordered().filter(|el| is_flowchart_link(el)) {
        let (from, to) = (arrow.start_binding.as_deref(), arrow.end_binding.as_deref());
        // An end bound to nothing is a neighbour of nothing: `!neighborId` in the oracle.
        if let (Some(from), Some(to)) = (from, to) {
            neighbours.entry(from).or_default().push(to);
            // An arrow from a node to itself is met once, as `startId === currentId` first.
            if from != to {
                neighbours.entry(to).or_default().push(from);
            }
        }
    }
    let mut visited: HashSet<&str> = HashSet::from([start.id.as_str()]);
    let mut queue: VecDeque<&str> = VecDeque::from([start.id.as_str()]);
    let mut obstacles = Vec::new();
    while let Some(current) = queue.pop_front() {
        for &neighbour in neighbours.get(current).map_or(&[][..], Vec::as_slice) {
            if !visited.insert(neighbour) {
                continue;
            }
            if let Some(node) = scene.get(neighbour).filter(|el| !el.is_deleted) {
                if is_target_kind(node) {
                    obstacles.push(element_rotated_bounds(node));
                    queue.push_back(neighbour);
                }
            }
        }
    }
    obstacles
}

/// Where a cluster of `count` equally-sized nodes goes next to `parent`. `placeCluster`
/// (`flowchart.ts@1118751f:161-230`). Returns each node's `(x, y)` and the cross-axis start
/// the cluster settled at, for [`DrawEngine::flowchart_create`] to anchor the next grow.
fn place_cluster(
    parent: &DrawElement,
    direction: LinkDirection,
    count: usize,
    obstacles: &[WorldBounds],
    sticky_cross_start: Option<f64>,
) -> (Vec<(f64, f64)>, f64) {
    let horizontal = matches!(direction, LinkDirection::Left | LinkDirection::Right);
    let node_primary_size = if horizontal {
        parent.width
    } else {
        parent.height
    };
    let node_cross_size = if horizontal {
        parent.height
    } else {
        parent.width
    };
    let primary_gap = if horizontal {
        HORIZONTAL_OFFSET
    } else {
        VERTICAL_OFFSET
    };
    let cross_gap = if horizontal {
        VERTICAL_OFFSET
    } else {
        HORIZONTAL_OFFSET
    };

    let parent_primary_start = if horizontal { parent.x } else { parent.y };
    let parent_cross_center = if horizontal {
        parent.y + parent.height / 2.0
    } else {
        parent.x + parent.width / 2.0
    };

    let primary_start = match direction {
        LinkDirection::Right | LinkDirection::Down => {
            parent_primary_start + node_primary_size + primary_gap
        }
        LinkDirection::Left | LinkDirection::Up => {
            parent_primary_start - primary_gap - node_primary_size
        }
    };

    let occupied = merge_intervals(
        obstacles
            .iter()
            .filter(|bounds| {
                let (start, end) = if horizontal {
                    (bounds.min_x, bounds.max_x)
                } else {
                    (bounds.min_y, bounds.max_y)
                };
                start < primary_start + node_primary_size && end > primary_start
            })
            .map(|bounds| {
                let (start, end) = if horizontal {
                    (bounds.min_y - cross_gap, bounds.max_y + cross_gap)
                } else {
                    (bounds.min_x - cross_gap, bounds.max_x + cross_gap)
                };
                Interval { start, end }
            })
            .collect(),
    );

    let step = node_cross_size + cross_gap;
    let cluster_cross_size = count as f64 * node_cross_size + (count as f64 - 1.0) * cross_gap;

    let anchored_start = sticky_cross_start.and_then(|start| {
        let mut candidates: Vec<f64> = [start, start - step]
            .into_iter()
            .filter(|&c| interval_is_free(c, cluster_cross_size, &occupied))
            .collect();
        // Stable, as `Array.prototype.sort` is: a tie keeps `start` first.
        candidates.sort_by(|a, b| {
            let da = (a + cluster_cross_size / 2.0 - parent_cross_center).abs();
            let db = (b + cluster_cross_size / 2.0 - parent_cross_center).abs();
            da.total_cmp(&db)
        });
        candidates.first().copied()
    });

    let cross_start = anchored_start.unwrap_or_else(|| {
        find_nearest_free_slot(
            parent_cross_center - cluster_cross_size / 2.0,
            cluster_cross_size,
            &occupied,
        )
    });

    let positions = (0..count)
        .map(|index| {
            let cross = cross_start + index as f64 * step;
            if horizontal {
                (primary_start, cross)
            } else {
                (cross, primary_start)
            }
        })
        .collect();

    (positions, cross_start)
}

// ----------------------------------------------------------------------------- building

/// A new node at `(x, y)`, copying `template`'s size and style — `cloneFlowchartNode`
/// (`flowchart.ts@1118751f:232-270`): the size, the corners (`roundness`, radius included),
/// roughness, colours, stroke, opacity and fill, a sticky note's base height; never the
/// turn. `kind` need not match `template`'s: the digit-key shape choice clones the
/// template's *style*, never its type.
fn clone_flowchart_node(
    template: &DrawElement,
    kind: DrawElementType,
    x: f64,
    y: f64,
    now: f64,
) -> DrawElement {
    let style = DrawElementStyle {
        stroke_color: template.stroke_color.clone(),
        background_color: template.background_color.clone(),
        fill_style: template.fill_style,
        stroke_width: template.stroke_width,
        stroke_style: template.stroke_style,
        roughness: template.roughness,
        opacity: template.opacity,
        roundness: template.roundness,
    };
    let mut node = create_element(
        kind,
        Geometry {
            x,
            y,
            width: template.width,
            height: template.height,
        },
        style,
        now,
    );
    node.corner_radius = template.corner_radius;
    if kind == DrawElementType::StickyNote {
        // `newStickyNoteElement`'s `baseHeight ?? height` (`newElement.ts@1118751f:241`).
        node.base_height = Some(template.base_height.unwrap_or(template.height));
    }
    if kind == DrawElementType::Figure {
        // Only reachable extending a figure chain with no digit override — the digit
        // picker never offers a figure (`flowchart_set_shape`) — so `template.figure` is
        // always there to copy.
        node.figure = template.figure.clone();
    }
    node
}

/// The arrow joining `source` to a new node `target` placed toward `direction`.
/// `createBindingArrow` (`flowchart.ts@1118751f:310-450`): the source's stroke colour,
/// stroke style, stroke width, opacity and roughness; every other style the default a new
/// arrow gets — a solid fill, no background, sharp; no tail, and the head the next arrow
/// would get. It starts [`ARROW_PADDING`] off the middle of `source`'s side facing
/// `direction` and ends as far off `target`'s opposite side, then is bound at both ends in
/// orbit and routed ([`crate::scene::elbow::bind_and_route`]) at `zoom`.
fn binding_arrow(
    source: &DrawElement,
    target: &DrawElement,
    direction: LinkDirection,
    end_arrowhead: Arrowhead,
    zoom: f64,
    now: f64,
) -> DrawElement {
    let style = DrawElementStyle {
        stroke_color: source.stroke_color.clone(),
        background_color: "transparent".to_string(),
        fill_style: FillStyle::Solid,
        stroke_width: source.stroke_width,
        stroke_style: source.stroke_style,
        roughness: source.roughness,
        opacity: source.opacity,
        roundness: None,
    };
    let pad = ARROW_PADDING;
    let (s, t) = (source, target);
    let at = |x, y| Point { x, y };
    let start = match direction {
        LinkDirection::Up => at(s.x + s.width / 2.0, s.y - pad),
        LinkDirection::Down => at(s.x + s.width / 2.0, s.y + s.height + pad),
        LinkDirection::Right => at(s.x + s.width + pad, s.y + s.height / 2.0),
        LinkDirection::Left => at(s.x - pad, s.y + s.height / 2.0),
    };
    let end = match direction {
        LinkDirection::Up => at(t.x + t.width / 2.0, t.y + t.height + pad),
        LinkDirection::Down => at(t.x + t.width / 2.0, t.y - pad),
        LinkDirection::Right => at(t.x - pad, t.y + t.height / 2.0),
        LinkDirection::Left => at(t.x + t.width + pad, t.y + t.height / 2.0),
    };
    let arrow = create_element(DrawElementType::Arrow, Geometry::default(), style, now);
    let mut arrow = linear_from_endpoints(arrow, start, end);
    arrow.elbowed = Some(true);
    arrow.start_arrowhead = None;
    arrow.end_arrowhead = Some(end_arrowhead);
    // The oracle routes over the scene, which holds `source` and not yet `target`. Its
    // router reads nothing there but the shapes an arrow is bound to — the rest only while
    // an end is dragged — so `source` alone is that scene, without indexing a whole board
    // per arrow per press.
    elbow::bind_and_route(
        &mut arrow,
        source,
        target,
        &elbow::Board::new([source]),
        zoom,
    );
    arrow
}

/// The cluster being previewed while Ctrl/Cmd is held. `FlowChartCreator`
/// (`flowchart.ts@1118751f:682-754`).
pub(crate) struct FlowchartCreator {
    direction: LinkDirection,
    number_of_nodes: usize,
    cluster_cross_start: Option<f64>,
    shape: DrawElementType,
    pub(crate) pending: Vec<DrawElement>,
}

// ---------------------------------------------------------------------------- navigation

/// `vectorToHeading` (`heading.ts@1118751f:38-50`), ties included.
fn vector_to_heading(x: f64, y: f64) -> LinkDirection {
    let (abs_x, abs_y) = (x.abs(), y.abs());
    if x > abs_y {
        LinkDirection::Right
    } else if x <= -abs_y {
        LinkDirection::Left
    } else if y > abs_x {
        LinkDirection::Down
    } else {
        LinkDirection::Up
    }
}

fn heading_for_point(p: Point, origin: Point) -> LinkDirection {
    vector_to_heading(p.x - origin.x, p.y - origin.y)
}

/// `pointRotateRads`, which skips a zero angle.
fn rotate(p: Point, centre: Point, angle: f64) -> Point {
    if angle == 0.0 {
        return p;
    }
    let (sin, cos) = angle.sin_cos();
    Point {
        x: (p.x - centre.x) * cos - (p.y - centre.y) * sin + centre.x,
        y: (p.x - centre.x) * sin + (p.y - centre.y) * cos + centre.y,
    }
}

/// `vectorCross` of `a - b` and `c - d`.
fn cross(a: Point, b: Point, c: Point, d: Point) -> f64 {
    (a.x - b.x) * (c.y - d.y) - (c.x - d.x) * (a.y - b.y)
}

/// `triangleIncludesPoint` (`math/src/triangle.ts@1118751f:14-28`), edges included.
fn triangle_includes_point([a, b, c]: [Point; 3], p: Point) -> bool {
    let sign = |p1: Point, p2: Point, p3: Point| {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let (d1, d2, d3) = (sign(p, a, b), sign(p, b, c), sign(p, c, a));
    let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(negative && positive)
}

/// `headingForPointFromDiamondElement` (`heading.ts@1118751f:73-222`): the vertex regions
/// first, then the sides, each side handing its point to whichever vertex the diamond's
/// longer axis runs to.
fn heading_for_point_from_diamond(
    element: &DrawElement,
    mid: Point,
    point: Point,
) -> LinkDirection {
    /// Rounded elements tolerance.
    const SHRINK: f64 = 0.95;
    let corner = |x: f64, y: f64| {
        let r = rotate(Point { x, y }, mid, element.angle);
        Point {
            x: (r.x - mid.x) * SHRINK + mid.x,
            y: (r.y - mid.y) * SHRINK + mid.y,
        }
    };
    let (x, y, w, h) = (element.x, element.y, element.width, element.height);
    let top = corner(x + w / 2.0, y);
    let right = corner(x + w, y + h / 2.0);
    let bottom = corner(x + w / 2.0, y + h);
    let left = corner(x, y + h / 2.0);

    if cross(point, top, top, right) <= 0.0 && cross(point, top, top, left) > 0.0 {
        return heading_for_point(top, mid);
    } else if cross(point, right, right, bottom) <= 0.0 && cross(point, right, right, top) > 0.0 {
        return heading_for_point(right, mid);
    } else if cross(point, bottom, bottom, left) <= 0.0 && cross(point, bottom, bottom, right) > 0.0
    {
        return heading_for_point(bottom, mid);
    } else if cross(point, left, left, top) <= 0.0 && cross(point, left, left, bottom) > 0.0 {
        return heading_for_point(left, mid);
    }

    let wide = w > h;
    let p = if cross(point, mid, top, mid) <= 0.0 && cross(point, mid, right, mid) > 0.0 {
        if wide {
            top
        } else {
            right
        }
    } else if cross(point, mid, right, mid) <= 0.0 && cross(point, mid, bottom, mid) > 0.0 {
        if wide {
            bottom
        } else {
            right
        }
    } else if cross(point, mid, bottom, mid) <= 0.0 && cross(point, mid, left, mid) > 0.0 {
        if wide {
            bottom
        } else {
            left
        }
    } else if wide {
        top
    } else {
        left
    };
    heading_for_point(p, mid)
}

/// Which side of `element` the point `p` is on. `headingForPointFromElement`
/// (`heading.ts@1118751f:227-278`): four search cones from the centre of the node's turned
/// box, twice its size; a diamond by its own rule.
fn heading_for_point_from_element(element: &DrawElement, p: Point) -> LinkDirection {
    /// `SEARCH_CONE_MULTIPLIER`.
    const CONE: f64 = 2.0;
    let aabb = element_rotated_bounds(element);
    let mid = Point {
        x: aabb.min_x + (aabb.max_x - aabb.min_x) / 2.0,
        y: aabb.min_y + (aabb.max_y - aabb.min_y) / 2.0,
    };
    if element.kind == DrawElementType::Diamond {
        return heading_for_point_from_diamond(element, mid, p);
    }
    let scaled = |x: f64, y: f64| Point {
        x: mid.x + (x - mid.x) * CONE,
        y: mid.y + (y - mid.y) * CONE,
    };
    let top_left = scaled(aabb.min_x, aabb.min_y);
    let top_right = scaled(aabb.max_x, aabb.min_y);
    let bottom_left = scaled(aabb.min_x, aabb.max_y);
    let bottom_right = scaled(aabb.max_x, aabb.max_y);
    if triangle_includes_point([top_left, top_right, mid], p) {
        LinkDirection::Up
    } else if triangle_includes_point([top_right, bottom_right, mid], p) {
        LinkDirection::Right
    } else if triangle_includes_point([bottom_right, bottom_left, mid], p) {
        LinkDirection::Down
    } else {
        LinkDirection::Left
    }
}

/// The nodes linked to `node` whose link leaves (`successors`) or arrives at (not) the
/// side facing `direction`, in scene order. `getNodeRelatives` (`flowchart.ts@1118751f:
/// 582-650`): a successor is the far end of an arrow starting at `node`, judged by the
/// arrow's first point; a predecessor the near end of one finishing there, by its last.
fn node_relatives(
    successors: bool,
    node: &DrawElement,
    scene: &Scene,
    direction: LinkDirection,
) -> Vec<String> {
    let mut relatives = Vec::new();
    for arrow in scene.iter_ordered().filter(|el| is_flowchart_link(el)) {
        let (own, opposite) = if successors {
            (&arrow.start_binding, &arrow.end_binding)
        } else {
            (&arrow.end_binding, &arrow.start_binding)
        };
        let Some(opposite) = opposite else {
            continue;
        };
        if own.as_deref() != Some(node.id.as_str()) {
            continue;
        }
        let Some(relative) = scene.get(opposite).filter(|el| !el.is_deleted) else {
            continue;
        };
        let points = arrow.points.as_deref().unwrap_or_default();
        let edge = if successors {
            Some([0.0, 0.0])
        } else {
            points.last().copied()
        };
        let Some([ex, ey]) = edge else {
            continue;
        };
        let at = Point {
            x: ex + arrow.x,
            y: ey + arrow.y,
        };
        if heading_for_point_from_element(node, at) == direction {
            relatives.push(relative.id.clone());
        }
    }
    relatives
}

/// Successors, then predecessors — `[...getSuccessors, ...getPredecessors]`.
fn linked_nodes(node: &DrawElement, scene: &Scene, direction: LinkDirection) -> Vec<String> {
    let mut nodes = node_relatives(true, node, scene, direction);
    nodes.extend(node_relatives(false, node, scene, direction));
    nodes
}

/// Alt+Arrow's session state: which direction is being explored, the nodes at that level,
/// and what has already been visited so a run of presses does not loop on itself.
/// `FlowChartNavigator` (`flowchart.ts@1118751f:452-680`).
#[derive(Default)]
pub(crate) struct FlowchartNavigator {
    pub(crate) exploring: bool,
    same_level: Vec<String>,
    index: usize,
    direction: Option<LinkDirection>,
    visited: HashSet<String>,
}

impl FlowchartNavigator {
    pub(crate) fn clear(&mut self) {
        self.exploring = false;
        self.same_level.clear();
        self.index = 0;
        self.direction = None;
        self.visited.clear();
    }

    /// One Alt+Arrow press: the id to select next, or `None` when there is nowhere to go.
    /// `exploreByDirection` (`flowchart.ts@1118751f:470-580`).
    fn explore(
        &mut self,
        element: &DrawElement,
        scene: &Scene,
        direction: LinkDirection,
    ) -> Option<String> {
        if !is_target_kind(element) {
            return None;
        }
        if Some(direction) != self.direction {
            self.clear();
        }
        self.visited.insert(element.id.clone());

        // Already exploring this direction with more than one node at this level: loop
        // through them.
        if self.exploring && Some(direction) == self.direction && self.same_level.len() > 1 {
            self.index = (self.index + 1) % self.same_level.len();
            return Some(self.same_level[self.index].clone());
        }

        // Starting fresh in this direction: go to the first node there.
        let nodes = linked_nodes(element, scene, direction);
        if let Some(first) = nodes.first().cloned() {
            self.index = 0;
            self.exploring = true;
            self.same_level = nodes;
            self.direction = Some(direction);
            self.visited.insert(first.clone());
            return Some(first);
        }

        // Nothing at this level: hop to any other unvisited linked node, for speedier
        // navigation without switching arrow keys.
        if Some(direction) == self.direction || !self.exploring {
            for other in [
                LinkDirection::Up,
                LinkDirection::Right,
                LinkDirection::Down,
                LinkDirection::Left,
            ] {
                if other == direction {
                    continue;
                }
                for candidate in linked_nodes(element, scene, other) {
                    if self.visited.insert(candidate.clone()) {
                        self.exploring = true;
                        self.direction = Some(direction);
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }
}

impl DrawEngine {
    /// Ctrl/Cmd+Arrow: previews a cluster of new nodes off the single selected flowchart
    /// node, growing it by one on a repeat press in the same direction, and reveals the
    /// cluster if any of it is off screen. The key is the oracle's whether or not a node is
    /// selected (`App.flowchart.ts@1118751f:112-131`): with no exactly-one flowchart node
    /// selected nothing grows, and a cluster already pending is revealed again.
    pub fn flowchart_create(&mut self, direction: LinkDirection) {
        let selected = self.get_selected_elements();
        if let [start] = selected.as_slice() {
            if is_flowchart_node(start) {
                let (count, cross_start, shape) = match &self.flowchart_creator {
                    Some(creator) if creator.direction == direction => (
                        creator.number_of_nodes + 1,
                        creator.cluster_cross_start,
                        creator.shape,
                    ),
                    // A different direction restarts the cluster, as the oracle's does
                    // (`flowchart.ts@1118751f:699-704`) — but keeps the shape already
                    // chosen this gesture, rather than reverting to the start node's own.
                    Some(creator) => (1, None, creator.shape),
                    None => (1, None, start.kind),
                };
                self.set_pending_flowchart(start, direction, count, shape, cross_start);
            }
        }
        let bounds = self
            .flowchart_creator
            .as_ref()
            .and_then(|creator| scene_outline_bounds(&creator.pending));
        if let Some(bounds) = bounds {
            self.reveal_if_hidden(bounds);
        }
    }

    /// The pending cluster for one direction press: `count` node-and-arrow pairs
    /// (`[node0, arrow0, node1, arrow1, …]`, the oracle's interleaving, so index 0 is always
    /// the first node), and the cross-axis start the next grow anchors to. `addNewNodes`
    /// (`flowchart.ts@1118751f:272-308`), then `createNodes`' frame rule (`:720-744`): the
    /// cluster joins the source's frame when every piece of it is inside or overlapping it.
    fn build_pending(
        &self,
        start: &DrawElement,
        direction: LinkDirection,
        count: usize,
        shape: DrawElementType,
        sticky_cross_start: Option<f64>,
    ) -> (Vec<DrawElement>, f64) {
        let obstacles = connected_flowchart_obstacles(&self.scene, start);
        let (positions, cross_start) =
            place_cluster(start, direction, count, &obstacles, sticky_cross_start);
        let head = self.next_end_arrowhead.unwrap_or(Arrowhead::Arrow);
        let mut pending = Vec::with_capacity(count * 2);
        for (x, y) in positions {
            let node = clone_flowchart_node(start, shape, x, y, self.now_ms);
            let arrow = binding_arrow(
                start,
                &node,
                direction,
                head,
                self.camera.scale,
                self.now_ms,
            );
            pending.push(node);
            pending.push(arrow);
        }
        let frame = start
            .frame_id
            .as_deref()
            .and_then(|id| self.scene.get(id))
            .filter(|frame| !frame.is_deleted && is_frame(frame));
        if let Some(frame) = frame {
            if pending
                .iter()
                .all(|element| element_overlaps_frame(element, frame))
            {
                for element in &mut pending {
                    element.frame_id = Some(frame.id.clone());
                }
            }
        }
        (pending, cross_start)
    }

    /// Computes and holds the pending cluster. Shared by a direction press and a shape pick.
    fn set_pending_flowchart(
        &mut self,
        start: &DrawElement,
        direction: LinkDirection,
        count: usize,
        shape: DrawElementType,
        cross_start: Option<f64>,
    ) {
        let (pending, cross_start) =
            self.build_pending(start, direction, count, shape, cross_start);
        self.flowchart_creator = Some(FlowchartCreator {
            direction,
            number_of_nodes: count,
            cluster_cross_start: Some(cross_start),
            shape,
            pending,
        });
        self.request_draw();
    }

    /// While Ctrl/Cmd is held, 1/2/3 chooses the pending nodes' shape — rectangle, diamond,
    /// ellipse — and the preview recomputes at once. An extra this engine offers that the
    /// oracle does not. No-op with no creation in progress, or a shape that is not one of
    /// the three.
    pub fn flowchart_set_shape(&mut self, shape: DrawElementType) {
        if !matches!(
            shape,
            DrawElementType::Rectangle | DrawElementType::Diamond | DrawElementType::Ellipse
        ) {
            return;
        }
        let Some(creator) = &self.flowchart_creator else {
            return;
        };
        let (direction, count, cross_start) = (
            creator.direction,
            creator.number_of_nodes,
            creator.cluster_cross_start,
        );
        let selected = self.get_selected_elements();
        let [start] = selected.as_slice() else {
            return;
        };
        if !is_flowchart_node(start) {
            return;
        }
        self.set_pending_flowchart(start, direction, count, shape, cross_start);
    }

    /// Releasing Ctrl/Cmd: inserts the pending cluster, selects the first new node,
    /// reveals it, and records **one** step of history — `insertNewElements`, then
    /// `selectAndReveal`, then `captureUpdate: IMMEDIATELY` (`App.flowchart.ts@1118751f:
    /// 78-90`).
    ///
    /// Inserted in runs of one frame: a run that joined a frame goes where
    /// `getFrameChildrenInsertionIndex` puts it (`frame.ts@1118751f:521-537`) — above the
    /// frame's topmost child, or directly under the frame when it has none — and any other
    /// on top of the board (`App.tsx@1118751f:7754-7782`).
    pub fn flowchart_commit(&mut self) {
        let Some(creator) = self.flowchart_creator.take() else {
            return;
        };
        let Some(first) = creator.pending.first() else {
            self.request_draw();
            return;
        };
        let first_node_id = first.id.clone();
        let created: Vec<String> = creator.pending.iter().map(|el| el.id.clone()).collect();
        let mut runs: Vec<(Option<String>, Vec<String>)> = Vec::new();
        for element in &creator.pending {
            match runs.last_mut() {
                Some((frame, ids)) if *frame == element.frame_id => ids.push(element.id.clone()),
                _ => runs.push((element.frame_id.clone(), vec![element.id.clone()])),
            }
        }
        for (frame, ids) in runs {
            let anchor = frame.as_deref().and_then(|frame| {
                self.scene
                    .iter_ordered()
                    .rev()
                    .find(|el| el.id == frame || el.frame_id.as_deref() == Some(frame))
                    .map(|el| (el.id.clone(), el.id == frame))
            });
            for id in &ids {
                if let Some(element) = creator.pending.iter().find(|el| &el.id == id) {
                    self.scene.add(element.clone());
                }
            }
            match anchor {
                Some((frame, true)) => self.scene.place_below(&ids, &frame),
                Some((child, false)) => self.scene.place_above(&ids, &child),
                None => {}
            }
        }
        // No binding pass: each arrow was routed against these very nodes when it was
        // built, and `insertNewElements` routes nothing again. Re-routing from the anchors
        // differs where an end starts off a turned node's outline.
        self.set_selection(vec![first_node_id.clone()]);
        // The cluster's frame was decided by overlap when it was built, not by where each
        // piece landed: a node straddling the frame's edge still joins it.
        self.push_history_keeping_frames(&created);
        if let Some(bounds) = self
            .scene
            .get(&first_node_id)
            .and_then(|node| scene_outline_bounds([node]))
        {
            self.reveal_if_hidden(bounds);
        }
        self.request_draw();
    }

    /// Escape while creating: the pending cluster was never in the scene, so cancelling it
    /// leaves no trace and nothing to undo (`App.flowchart.ts@1118751f:103-106`).
    pub fn flowchart_cancel(&mut self) {
        if self.flowchart_creator.take().is_some() {
            self.request_draw();
        }
    }

    pub fn is_creating_flowchart(&self) -> bool {
        self.flowchart_creator.is_some()
    }

    /// The cluster being previewed, painted but not in `get_scene` — empty between
    /// gestures.
    pub fn pending_flowchart_elements(&self) -> Vec<DrawElement> {
        self.flowchart_creator
            .as_ref()
            .map(|creator| creator.pending.clone())
            .unwrap_or_default()
    }

    /// Alt+Arrow: selects the node linked in that direction, cycling same-level nodes on a
    /// repeat press, and reveals it if it is off screen — `selectAndReveal`
    /// (`App.flowchart.ts@1118751f:166-176`). Returns the id selected, or `None` with no
    /// exactly-one selection an arrow can bind to, or nothing linked that way.
    pub fn flowchart_navigate(&mut self, direction: LinkDirection) -> Option<String> {
        let selected = self.get_selected_elements();
        let [element] = selected.as_slice() else {
            return None;
        };
        let id = self
            .flowchart_navigator
            .explore(element, &self.scene, direction)?;
        let node = self.scene.get(&id).filter(|el| !el.is_deleted)?;
        let bounds = scene_outline_bounds([node]);
        self.set_selection(vec![id.clone()]);
        if let Some(bounds) = bounds {
            self.reveal_if_hidden(bounds);
        }
        self.request_draw();
        Some(id)
    }

    /// Alt released: ends the exploration, so the next Alt+Arrow starts fresh rather than
    /// continuing a cycle through stale same-level nodes.
    pub fn flowchart_navigation_end(&mut self) {
        if self.flowchart_navigator.exploring {
            self.flowchart_navigator.clear();
        }
    }
}
