//! The route itself: `getElbowArrowData`, `routeElbowArrow` and the A* under it
//! (`elbowArrow.ts@1118751f:1192-2212`).
//!
//! The search runs on a grid made of the lines through both ends' boxes, so it only ever
//! turns where a box edge is. A turn costs the cube of the ends' Manhattan distance and
//! the estimate charges its square per turn still to come, so the fewest turns win and
//! length only breaks ties. The binary heap is the oracle's own, quirks included: equal
//! scores are served in the order its sift happens to leave them, and that order is the
//! difference between two equally short routes.

use super::heading::{vector_to_heading, Heading};
use super::outline::{
    aabb, cubed, distance, inside_bounds, js_max, js_min, scale_from, squared, Bounds, Pt, Target,
};
use super::snap::{
    binding_gap, global_fixed_point, heading_for_snap, snap_to_outline, BASE_BINDING_GAP,
};
use super::{Arrow, Board, Options, BASE_PADDING};

/// `DEDUP_TRESHOLD`: a segment shorter than this is folded away.
pub const DEDUP_TRESHOLD: f64 = 1.0;

/// Everything the search needs about the two ends.
pub(super) struct Ends<'a> {
    pub dynamic_aabbs: [Bounds; 2],
    pub start_dongle: Pt,
    pub start: Pt,
    pub start_heading: Heading,
    pub end_dongle: Pt,
    pub end: Pt,
    pub end_heading: Heading,
    pub common: Bounds,
    pub start_target: Option<Target<'a>>,
    pub end_target: Option<Target<'a>>,
}

/// `offsetFromHeading`: `[up, right, down, left]`, `head` on the heading's side.
fn offset_from_heading(heading: Heading, head: f64, side: f64) -> [f64; 4] {
    match heading {
        Heading::Up => [head, side, side, side],
        Heading::Right => [side, head, side, side],
        Heading::Down => [side, side, head, side],
        Heading::Left => [side, side, side, head],
    }
}

/// `commonAABB`.
fn common_aabb(list: &[Bounds]) -> Bounds {
    [
        js_min(&list.iter().map(|b| b[0]).collect::<Vec<_>>()),
        js_min(&list.iter().map(|b| b[1]).collect::<Vec<_>>()),
        js_max(&list.iter().map(|b| b[2]).collect::<Vec<_>>()),
        js_max(&list.iter().map(|b| b[3]).collect::<Vec<_>>()),
    ]
}

/// `getElbowArrowData(arrow, map, nextPoints, options)`.
pub(super) fn ends<'a>(
    arrow: &Arrow,
    board: &Board<'a>,
    next: &[Pt],
    options: &Options,
) -> Ends<'a> {
    let at = |p: Pt| [p[0] + arrow.x, p[1] + arrow.y];
    let orig_start = at(next[0]);
    let orig_end = at(next[next.len() - 1]);
    let zoom = options.zoom;
    let (start_target, end_target) = if options.is_dragging && options.binding_enabled {
        (
            board.hovered(orig_start, zoom),
            board.hovered(orig_end, zoom),
        )
    } else {
        (
            arrow
                .start_binding
                .as_ref()
                .and_then(|b| board.bindable(&b.element_id)),
            arrow
                .end_binding
                .as_ref()
                .and_then(|b| board.bindable(&b.element_id)),
        )
    };
    // `getGlobalPoint`.
    let global = |target: &Option<Target>, fixed: Option<[f64; 2]>, orig: Pt| {
        if options.is_dragging {
            return match target {
                Some(t) if options.binding_enabled => {
                    snap_to_outline(orig, next.len(), t, zoom, options.midpoint_snapping)
                }
                _ => orig,
            };
        }
        match target {
            Some(t) => global_fixed_point(fixed.unwrap_or([0.0, 0.0]), t),
            None => orig,
        }
    };
    let start = global(
        &start_target,
        arrow.start_binding.as_ref().map(|b| b.fixed_point),
        orig_start,
    );
    let end = global(
        &end_target,
        arrow.end_binding.as_ref().map(|b| b.fixed_point),
        orig_end,
    );
    let start_heading = heading_for_snap(start, end, start_target.as_ref(), orig_start, zoom);
    let end_heading = heading_for_snap(end, start, end_target.as_ref(), orig_end, zoom);
    let point_bounds = |p: Pt| [p[0] - 2.0, p[1] - 2.0, p[0] + 2.0, p[1] + 2.0];
    let (start_point_bounds, end_point_bounds) = (point_bounds(start), point_bounds(end));
    let element_bounds =
        |target: &Option<Target>, heading: Heading, head: bool, fallback: Bounds| match target {
            Some(t) => {
                let gap = binding_gap(t);
                aabb(
                    t,
                    Some(offset_from_heading(
                        heading,
                        if head { gap * 6.0 } else { gap * 2.0 },
                        1.0,
                    )),
                )
            }
            None => fallback,
        };
    let start_element_bounds = element_bounds(
        &start_target,
        start_heading,
        arrow.start_arrowhead,
        start_point_bounds,
    );
    let end_element_bounds = element_bounds(
        &end_target,
        end_heading,
        arrow.end_arrowhead,
        end_point_bounds,
    );
    let padded = |target: &Option<Target>, heading: Heading, fallback: Bounds| match target {
        Some(t) => aabb(
            t,
            Some(offset_from_heading(heading, BASE_PADDING, BASE_PADDING)),
        ),
        None => fallback,
    };
    let overlap = inside_bounds(start, padded(&end_target, end_heading, end_point_bounds))
        || inside_bounds(
            end,
            padded(&start_target, start_heading, start_point_bounds),
        );
    let common = if overlap {
        common_aabb(&[start_point_bounds, end_point_bounds])
    } else {
        common_aabb(&[start_element_bounds, end_element_bounds])
    };
    let unbound = start_target.is_none() && end_target.is_none();
    let difference = |heading: Heading, head: bool| {
        if overlap {
            offset_from_heading(heading, if unbound { 0.0 } else { BASE_PADDING }, 0.0)
        } else {
            let head = if unbound {
                0.0
            } else {
                BASE_PADDING
                    - if head {
                        BASE_BINDING_GAP * 6.0
                    } else {
                        BASE_BINDING_GAP * 2.0
                    }
            };
            offset_from_heading(heading, head, BASE_PADDING)
        }
    };
    let dynamic_aabbs = dynamic_aabbs(
        if overlap {
            start_point_bounds
        } else {
            start_element_bounds
        },
        if overlap {
            end_point_bounds
        } else {
            end_element_bounds
        },
        common,
        difference(start_heading, arrow.start_arrowhead),
        difference(end_heading, arrow.end_arrowhead),
        overlap,
        start_target.as_ref().map(|t| aabb(t, None)),
        end_target.as_ref().map(|t| aabb(t, None)),
    );
    Ends {
        start_dongle: dongle(dynamic_aabbs[0], start_heading, start),
        end_dongle: dongle(dynamic_aabbs[1], end_heading, end),
        dynamic_aabbs,
        start,
        start_heading,
        end,
        end_heading,
        common,
        start_target,
        end_target,
    }
}

/// `getDonglePosition`: where the route leaves `p`'s box, straight out along `heading`.
fn dongle(bounds: Bounds, heading: Heading, p: Pt) -> Pt {
    match heading {
        Heading::Up => [p[0], bounds[1]],
        Heading::Right => [bounds[2], p[1]],
        Heading::Down => [p[0], bounds[3]],
        Heading::Left => [bounds[0], p[1]],
    }
}

/// `generateDynamicAABBs`: the two ends' boxes grown until they touch halfway, with the
/// oracle's "side hack" splitting them along the diagonal they would otherwise overlap on.
#[allow(clippy::too_many_arguments)]
fn dynamic_aabbs(
    a: Bounds,
    b: Bounds,
    common: Bounds,
    start_difference: [f64; 4],
    end_difference: [f64; 4],
    disable_side_hack: bool,
    start_element: Option<Bounds>,
    end_element: Option<Bounds>,
) -> [Bounds; 2] {
    let start_el = start_element.unwrap_or(a);
    let end_el = end_element.unwrap_or(b);
    let [start_up, start_right, start_down, start_left] = start_difference;
    let [end_up, end_right, end_down, end_left] = end_difference;
    let first = [
        if a[0] > b[2] {
            if a[1] > b[3] || a[3] < b[1] {
                js_min(&[(start_el[0] + end_el[2]) / 2.0, a[0] - start_left])
            } else {
                (start_el[0] + end_el[2]) / 2.0
            }
        } else if a[0] > b[0] {
            a[0] - start_left
        } else {
            common[0] - start_left
        },
        if a[1] > b[3] {
            if a[0] > b[2] || a[2] < b[0] {
                js_min(&[(start_el[1] + end_el[3]) / 2.0, a[1] - start_up])
            } else {
                (start_el[1] + end_el[3]) / 2.0
            }
        } else if a[1] > b[1] {
            a[1] - start_up
        } else {
            common[1] - start_up
        },
        if a[2] < b[0] {
            if a[1] > b[3] || a[3] < b[1] {
                js_max(&[(start_el[2] + end_el[0]) / 2.0, a[2] + start_right])
            } else {
                (start_el[2] + end_el[0]) / 2.0
            }
        } else if a[2] < b[2] {
            a[2] + start_right
        } else {
            common[2] + start_right
        },
        if a[3] < b[1] {
            if a[0] > b[2] || a[2] < b[0] {
                js_max(&[(start_el[3] + end_el[1]) / 2.0, a[3] + start_down])
            } else {
                (start_el[3] + end_el[1]) / 2.0
            }
        } else if a[3] < b[3] {
            a[3] + start_down
        } else {
            common[3] + start_down
        },
    ];
    let second = [
        if b[0] > a[2] {
            if b[1] > a[3] || b[3] < a[1] {
                js_min(&[(end_el[0] + start_el[2]) / 2.0, b[0] - end_left])
            } else {
                (end_el[0] + start_el[2]) / 2.0
            }
        } else if b[0] > a[0] {
            b[0] - end_left
        } else {
            common[0] - end_left
        },
        if b[1] > a[3] {
            if b[0] > a[2] || b[2] < a[0] {
                js_min(&[(end_el[1] + start_el[3]) / 2.0, b[1] - end_up])
            } else {
                (end_el[1] + start_el[3]) / 2.0
            }
        } else if b[1] > a[1] {
            b[1] - end_up
        } else {
            common[1] - end_up
        },
        if b[2] < a[0] {
            if b[1] > a[3] || b[3] < a[1] {
                js_max(&[(end_el[2] + start_el[0]) / 2.0, b[2] + end_right])
            } else {
                (end_el[2] + start_el[0]) / 2.0
            }
        } else if b[2] < a[2] {
            b[2] + end_right
        } else {
            common[2] + end_right
        },
        if b[3] < a[1] {
            if b[0] > a[2] || b[2] < a[0] {
                js_max(&[(end_el[3] + start_el[1]) / 2.0, b[3] + end_down])
            } else {
                (end_el[3] + start_el[1]) / 2.0
            }
        } else if b[3] < a[3] {
            b[3] + end_down
        } else {
            common[3] + end_down
        },
    ];

    let c = common_aabb(&[first, second]);
    if !disable_side_hack
        && first[2] - first[0] + second[2] - second[0] > c[2] - c[0] + 0.00000000001
        && first[3] - first[1] + second[3] - second[1] > c[3] - c[1] + 0.00000000001
    {
        let (end_cx, end_cy) = ((second[0] + second[2]) / 2.0, (second[1] + second[3]) / 2.0);
        let turn = |p: Pt, q: Pt| {
            super::outline::cross(
                [p[0] - end_cx, p[1] - end_cy],
                [q[0] - end_cx, q[1] - end_cy],
            ) > 0.0
        };
        if b[0] > a[2] && a[1] > b[3] {
            // Bottom left.
            let cx = first[2] + (second[0] - first[2]) / 2.0;
            let cy = second[3] + (first[1] - second[3]) / 2.0;
            if turn([a[2], a[1]], [a[0], a[3]]) {
                return [
                    [first[0], first[1], cx, first[3]],
                    [cx, second[1], second[2], second[3]],
                ];
            }
            return [
                [first[0], cy, first[2], first[3]],
                [second[0], second[1], second[2], cy],
            ];
        } else if a[2] < b[0] && a[3] < b[1] {
            // Top left.
            let cx = first[2] + (second[0] - first[2]) / 2.0;
            let cy = first[3] + (second[1] - first[3]) / 2.0;
            if turn([a[0], a[1]], [a[2], a[3]]) {
                return [
                    [first[0], first[1], first[2], cy],
                    [second[0], cy, second[2], second[3]],
                ];
            }
            return [
                [first[0], first[1], cx, first[3]],
                [cx, second[1], second[2], second[3]],
            ];
        } else if a[0] > b[2] && a[3] < b[1] {
            // Top right.
            let cx = second[2] + (first[0] - second[2]) / 2.0;
            let cy = first[3] + (second[1] - first[3]) / 2.0;
            if turn([a[2], a[1]], [a[0], a[3]]) {
                return [
                    [cx, first[1], first[2], first[3]],
                    [second[0], second[1], cx, second[3]],
                ];
            }
            return [
                [first[0], first[1], first[2], cy],
                [second[0], cy, second[2], second[3]],
            ];
        } else if a[0] > b[2] && a[1] > b[3] {
            // Bottom right.
            let cx = second[2] + (first[0] - second[2]) / 2.0;
            let cy = second[3] + (first[1] - second[3]) / 2.0;
            if turn([a[0], a[1]], [a[2], a[3]]) {
                return [
                    [cx, first[1], first[2], first[3]],
                    [second[0], second[1], cx, second[3]],
                ];
            }
            return [
                [first[0], cy, first[2], first[3]],
                [second[0], second[1], second[2], cy],
            ];
        }
    }
    [first, second]
}

// ----------------------------------------------------------------------------- A*

struct Node {
    f: f64,
    g: f64,
    closed: bool,
    visited: bool,
    parent: Option<usize>,
    pos: Pt,
    col: usize,
    row: usize,
}

struct Grid {
    rows: usize,
    cols: usize,
    data: Vec<Node>,
}

impl Grid {
    fn at(&self, col: isize, row: isize) -> Option<usize> {
        if col < 0 || row < 0 || col as usize >= self.cols || row as usize >= self.rows {
            return None;
        }
        Some(row as usize * self.cols + col as usize)
    }

    /// `pointToGridNode`: the node exactly at `p`, if one is.
    fn node_at(&self, p: Pt) -> Option<usize> {
        self.data
            .iter()
            .position(|n| n.pos[0] == p[0] && n.pos[1] == p[1])
    }
}

/// Values in the order a JS `Set` keeps them — `-0` and `0` are one — sorted `a - b`.
fn axis(values: impl IntoIterator<Item = f64>) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for v in values {
        if !out.iter().any(|&u| u == v || (u.is_nan() && v.is_nan())) {
            out.push(v);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// `calculateGrid`.
fn grid(
    aabbs: &[Bounds; 2],
    start: Pt,
    start_heading: Heading,
    end: Pt,
    end_heading: Heading,
    common: Bounds,
) -> Grid {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (p, heading) in [(start, start_heading), (end, end_heading)] {
        if heading.is_horizontal() {
            ys.push(p[1]);
        } else {
            xs.push(p[0]);
        }
    }
    for b in aabbs {
        xs.extend([b[0], b[2]]);
        ys.extend([b[1], b[3]]);
    }
    xs.extend([common[0], common[2]]);
    ys.extend([common[1], common[3]]);
    let (xs, ys) = (axis(xs), axis(ys));
    let mut data = Vec::with_capacity(xs.len() * ys.len());
    for (row, &y) in ys.iter().enumerate() {
        for (col, &x) in xs.iter().enumerate() {
            data.push(Node {
                f: 0.0,
                g: 0.0,
                closed: false,
                visited: false,
                parent: None,
                pos: [x, y],
                col,
                row,
            });
        }
    }
    Grid {
        rows: ys.len(),
        cols: xs.len(),
        data,
    }
}

/// The oracle's `BinaryHeap` (`common/src/binary-heap.ts@1118751f`), scored by `f`. Its
/// `sinkDown` moves toward the root and its `bubbleUp` away from it; `rescoreElement`
/// only ever sinks. A node's slot is tracked rather than searched for — each node is in
/// the heap at most once, so it is the slot `indexOf` would find.
struct Heap {
    content: Vec<usize>,
    slot: Vec<usize>,
}

impl Heap {
    fn place(&mut self, idx: usize, node: usize) {
        self.content[idx] = node;
        self.slot[node] = idx;
    }

    fn sink_down(&mut self, nodes: &[Node], mut idx: usize) {
        let node = self.content[idx];
        let score = nodes[node].f;
        while idx > 0 {
            let parent_n = ((idx + 1) >> 1) - 1;
            let parent = self.content[parent_n];
            if score < nodes[parent].f {
                self.place(idx, parent);
                idx = parent_n;
            } else {
                break;
            }
        }
        self.place(idx, node);
    }

    fn bubble_up(&mut self, nodes: &[Node], mut idx: usize) {
        let length = self.content.len();
        let node = self.content[idx];
        let score = nodes[node].f;
        loop {
            let child1 = ((idx + 1) << 1) - 1;
            let child2 = child1 + 1;
            let mut smallest = idx;
            let mut smallest_score = score;
            if child1 < length {
                let s = nodes[self.content[child1]].f;
                if s < smallest_score {
                    smallest = child1;
                    smallest_score = s;
                }
            }
            if child2 < length && nodes[self.content[child2]].f < smallest_score {
                smallest = child2;
            }
            if smallest == idx {
                break;
            }
            let moved = self.content[smallest];
            self.place(idx, moved);
            idx = smallest;
        }
        self.place(idx, node);
    }

    fn push(&mut self, nodes: &[Node], node: usize) {
        self.content.push(node);
        let last = self.content.len() - 1;
        self.sink_down(nodes, last);
    }

    fn pop(&mut self, nodes: &[Node]) -> Option<usize> {
        let result = *self.content.first()?;
        let end = self.content.pop()?;
        if !self.content.is_empty() {
            self.place(0, end);
            self.bubble_up(nodes, 0);
        }
        Some(result)
    }

    fn rescore(&mut self, nodes: &[Node], node: usize) {
        let idx = self.slot[node];
        self.sink_down(nodes, idx);
    }
}

/// Manhattan distance, `m_dist`.
fn m_dist(a: Pt, b: Pt) -> f64 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs()
}

/// `neighborIndexToHeading`: up, right, down, left.
const NEIGHBOR_HEADINGS: [Heading; 4] = [Heading::Up, Heading::Right, Heading::Down, Heading::Left];

/// `estimateSegmentCount`: the turns still needed from `start`, heading `sh`, to reach
/// `end` arriving along `eh`.
fn estimate_segment_count(start: Pt, end: Pt, sh: Heading, eh: Heading) -> f64 {
    use Heading::*;
    let n = match (eh, sh) {
        (Right, Right) => {
            if start[0] >= end[0] {
                4
            } else if start[1] == end[1] {
                0
            } else {
                2
            }
        }
        (Right, Up) => {
            if start[1] > end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Right, Down) => {
            if start[1] < end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Right, Left) => {
            if start[1] == end[1] {
                4
            } else {
                2
            }
        }
        (Left, Right) => {
            if start[1] == end[1] {
                4
            } else {
                2
            }
        }
        (Left, Up) => {
            if start[1] > end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
        (Left, Down) => {
            if start[1] < end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
        (Left, Left) => {
            if start[0] <= end[0] {
                4
            } else if start[1] == end[1] {
                0
            } else {
                2
            }
        }
        (Up, Right) => {
            if start[1] > end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Up, Up) => {
            if start[1] >= end[1] {
                4
            } else if start[0] == end[0] {
                0
            } else {
                2
            }
        }
        (Up, Down) => {
            if start[0] == end[0] {
                4
            } else {
                2
            }
        }
        (Up, Left) => {
            if start[1] > end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
        (Down, Right) => {
            if start[1] < end[1] && start[0] < end[0] {
                1
            } else {
                3
            }
        }
        (Down, Up) => {
            if start[0] == end[0] {
                4
            } else {
                2
            }
        }
        (Down, Down) => {
            if start[1] <= end[1] {
                4
            } else if start[0] == end[0] {
                0
            } else {
                2
            }
        }
        (Down, Left) => {
            if start[1] < end[1] && start[0] > end[0] {
                1
            } else {
                3
            }
        }
    };
    n as f64
}

/// `astar`: the node path from `start` to `end`, never stepping back along the way it
/// came, nor out of `start` against its heading, nor into `end` along its own.
fn astar(
    grid: &mut Grid,
    start: usize,
    end: usize,
    start_heading: Heading,
    end_heading: Heading,
    aabbs: &[Bounds],
) -> Option<Vec<usize>> {
    let bend = m_dist(grid.data[start].pos, grid.data[end].pos);
    let (bend_cubed, bend_squared) = (cubed(bend), squared(bend));
    let mut open = Heap {
        content: Vec::new(),
        slot: vec![0; grid.data.len()],
    };
    open.push(&grid.data, start);
    while !open.content.is_empty() {
        let Some(current) = open.pop(&grid.data) else {
            continue;
        };
        if grid.data[current].closed {
            continue;
        }
        if current == end {
            return Some(path_to(grid, start, current));
        }
        grid.data[current].closed = true;
        let (col, row) = (
            grid.data[current].col as isize,
            grid.data[current].row as isize,
        );
        let neighbors = [
            grid.at(col, row - 1),
            grid.at(col + 1, row),
            grid.at(col, row + 1),
            grid.at(col - 1, row),
        ];
        let current_pos = grid.data[current].pos;
        for (i, neighbor) in neighbors.into_iter().enumerate() {
            let Some(n) = neighbor else {
                continue;
            };
            if grid.data[n].closed {
                continue;
            }
            let half = scale_from(grid.data[n].pos, current_pos, 0.5);
            if aabbs.iter().any(|b| inside_bounds(half, *b)) {
                continue;
            }
            let heading = NEIGHBOR_HEADINGS[i];
            let previous = match grid.data[current].parent {
                Some(p) => {
                    let pp = grid.data[p].pos;
                    vector_to_heading([current_pos[0] - pp[0], current_pos[1] - pp[1]])
                }
                None => start_heading,
            };
            let at_start = grid.data[start].col == grid.data[n].col
                && grid.data[start].row == grid.data[n].row;
            let at_end =
                grid.data[end].col == grid.data[n].col && grid.data[end].row == grid.data[n].row;
            if previous.flip() == heading
                || (at_start && heading == start_heading)
                || (at_end && heading == end_heading)
            {
                continue;
            }
            let g = grid.data[current].g
                + m_dist(grid.data[n].pos, current_pos)
                + if previous != heading { bend_cubed } else { 0.0 };
            let been = grid.data[n].visited;
            if !been || g < grid.data[n].g {
                let estimate = estimate_segment_count(
                    grid.data[n].pos,
                    grid.data[end].pos,
                    heading,
                    end_heading,
                );
                let h = m_dist(grid.data[end].pos, grid.data[n].pos) + estimate * bend_squared;
                let node = &mut grid.data[n];
                node.visited = true;
                node.parent = Some(current);
                node.g = g;
                node.f = g + h;
                if !been {
                    open.push(&grid.data, n);
                } else {
                    open.rescore(&grid.data, n);
                }
            }
        }
    }
    None
}

/// `pathTo`.
fn path_to(grid: &Grid, start: usize, node: usize) -> Vec<usize> {
    let mut path = vec![node];
    let mut current = node;
    while let Some(parent) = grid.data[current].parent {
        path.push(parent);
        current = parent;
    }
    // The walk ends on the node with no parent and `pathTo` puts `start` there instead.
    path.pop();
    path.push(start);
    path.reverse();
    path
}

/// `routeElbowArrow`: the corner-to-corner route, both ends included, or `None` when no
/// grid path exists (where the oracle would go on to throw).
pub(super) fn route(start_bound: bool, ends: &Ends) -> Option<Vec<Pt>> {
    let mut grid = grid(
        &ends.dynamic_aabbs,
        ends.start_dongle,
        ends.start_heading,
        ends.end_dongle,
        ends.end_heading,
        ends.common,
    );
    let start_dongle = grid.node_at(ends.start_dongle);
    let end_dongle = grid.node_at(ends.end_dongle);
    let end_node = grid.node_at(ends.end);
    if let (Some(n), true) = (end_node, ends.end_target.is_some()) {
        grid.data[n].closed = true;
    }
    let start_node = grid.node_at(ends.start);
    if let (Some(n), true) = (start_node, start_bound) {
        grid.data[n].closed = true;
    }
    let dongle_overlap = match (start_dongle, end_dongle) {
        (Some(s), Some(e)) => {
            inside_bounds(grid.data[s].pos, ends.dynamic_aabbs[1])
                || inside_bounds(grid.data[e].pos, ends.dynamic_aabbs[0])
        }
        _ => false,
    };
    let path = astar(
        &mut grid,
        start_dongle.or(start_node)?,
        end_dongle.or(end_node)?,
        ends.start_heading,
        ends.end_heading,
        if dongle_overlap {
            &[]
        } else {
            &ends.dynamic_aabbs
        },
    )?;
    let mut points: Vec<Pt> = path.into_iter().map(|n| grid.data[n].pos).collect();
    if start_dongle.is_some() {
        points.insert(0, ends.start);
    }
    if end_dongle.is_some() {
        points.push(ends.end);
    }
    Some(points)
}

/// `removeElbowArrowShortSegments`.
pub(super) fn remove_short_segments(points: Vec<Pt>) -> Vec<Pt> {
    if points.len() < 4 {
        return points;
    }
    let last = points.len() - 1;
    (0..points.len())
        .filter(|&i| i == 0 || i == last || distance(points[i - 1], points[i]) > DEDUP_TRESHOLD)
        .map(|i| points[i])
        .collect()
}

/// `getElbowArrowCornerPoints`: drops the points a route passes straight through.
pub(super) fn corner_points(points: Vec<Pt>) -> Vec<Pt> {
    if points.len() <= 1 {
        return points;
    }
    let horizontal = |p: Pt, q: Pt| (p[1] - q[1]).abs() < (p[0] - q[0]).abs();
    let mut previous = horizontal(points[0], points[1]);
    let last = points.len() - 1;
    let mut out = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        if i == 0 || i == last {
            out.push(p);
            continue;
        }
        let next = horizontal(p, points[i + 1]);
        let keep = previous != next;
        previous = next;
        if keep {
            out.push(p);
        }
    }
    out
}

/// The full pipeline of case 2: route, fold short segments, keep the corners.
pub(super) fn corners_of(start_bound: bool, ends: &Ends) -> Vec<Pt> {
    corner_points(remove_short_segments(
        route(start_bound, ends).unwrap_or_default(),
    ))
}
