//! Build-a-diagram-by-keyboard: Ctrl/Cmd+Arrow grows a connected node off the selected
//! shape, Alt+Arrow walks the connections. A port of Excalidraw's
//! `packages/element/src/flowchart.ts` and `packages/excalidraw/components/App.flowchart.ts`
//! (`@1118751f`), with one extra the oracle lacks: while Ctrl/Cmd is held, 1/2/3 chooses
//! the pending nodes' shape.
//!
//! # Preview, then one commit
//!
//! Ctrl/Cmd+Arrow computes a cluster of nodes and binding arrows and holds them in
//! [`FlowchartCreator::pending`] — painted (`engine/frame.rs`, alongside a peer's preview)
//! but **not** added to the scene, so `get_scene` and history never see them until Ctrl/Cmd
//! is released. A repeat press in the same direction grows the cluster by one and
//! recomputes it from scratch, as the oracle's `numberOfNodes` does; a different direction
//! restarts it at one. [`DrawEngine::flowchart_commit`] adds every pending element with
//! `Scene::add`, selects the first new node, and takes **one** step of history — the
//! oracle's `captureUpdate: IMMEDIATELY` after `insertNewElements`. Escape
//! ([`DrawEngine::flowchart_cancel`]) just drops what was never added: nothing to undo.
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
//! # Binding, simplified
//!
//! The oracle's arrow is always elbow-routed (`elbowed: true`) with an explicit padding of
//! 6 units past each edge (`createBindingArrow`, `flowchart.ts@1118751f:310-450`). This
//! engine does not route elbow arrows at all (`engine/selection_style.rs`) — an existing,
//! documented gap — so [`binding_arrow`] builds an ordinary straight arrow, bound at both
//! ends to each shape's centre in [`BindMode::Orbit`] and left to
//! [`crate::scene::binding::refresh_bindings_in_place`] (already run for every other bound
//! arrow) to resolve onto the outline at [`crate::scene::binding::binding_gap`] — the same
//! rule a hand-drawn bound arrow gets. A deliberate divergence, not an oversight.
//!
//! Obstacle membership follows the oracle's `isElbowArrow` filter with the equivalent this
//! engine has: **any** bound arrow, elbow or not, since every arrow here would have been
//! elbow there. So two shapes joined by an ordinary drawn arrow are one flowchart for
//! placement purposes too.
//!
//! # Navigation
//!
//! [`FlowchartNavigator`] is `FlowChartNavigator` (`flowchart.ts@1118751f:452-680`): explore
//! one direction, and a repeat press in the same direction cycles same-level nodes; run out
//! and it falls back to an unvisited node in any other direction, for faster hopping around
//! a diagram. [`heading_from_center`] classifies a neighbour by which side of the node's
//! centre it falls on — the oracle instead classifies the *arrow's own endpoint* against the
//! node's bounding box (`headingForPointFromElement`), which matters for a bent elbow arrow.
//! Every arrow this engine draws is straight and every flowchart node is offset cleanly
//! along one axis, so comparing centres lands the same answer without a second geometry
//! primitive — a deliberate simplification, not a partial port.

use std::collections::HashSet;

use crate::camera::{Point, WorldBounds};
use crate::engine::DrawEngine;
use crate::scene::{
    attach_point, binding_gap, create_element, element_center, element_rotated_bounds,
    is_bindable_element, linear_from_endpoints, set_anchor, Anchor, BindMode, DrawElement,
    DrawElementStyle, DrawElementType, End, Geometry, Scene,
};

/// One gap in scene units, on both axes — Excalidraw's `VERTICAL_OFFSET` /
/// `HORIZONTAL_OFFSET` (`flowchart.ts@1118751f:57-58`), both 100.
const VERTICAL_OFFSET: f64 = 100.0;
const HORIZONTAL_OFFSET: f64 = 100.0;

/// Screen-space clearance a reveal keeps around the node, matching the oracle's own
/// `revealIfHidden` under `offsets: { ui: true }` in spirit — this engine has no chrome
/// layout to read, so a fixed margin stands in for it.
const REVEAL_PADDING: f64 = 48.0;

/// Which way an arrow points, or a person navigates — Excalidraw's `LinkDirection`.
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
/// `isFlowchartNodeElement` (`typeChecks.ts@1118751f:286-293`): a rectangle, a diamond, an
/// ellipse, or a sticky note.
fn is_flowchart_node(element: &DrawElement) -> bool {
    matches!(
        element.kind,
        DrawElementType::Rectangle
            | DrawElementType::Diamond
            | DrawElementType::Ellipse
            | DrawElementType::StickyNote
    )
}

// --------------------------------------------------------------------------- placement

struct Interval {
    start: f64,
    end: f64,
}

/// Sorted, non-overlapping. `mergeIntervals` (`flowchart.ts@1118751f:62-76`).
fn merge_intervals(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
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

/// Every node reachable from `start` by a bound arrow — the connected component that acts
/// as the obstacle set while placing a new cluster. `getConnectedFlowchartNodes`
/// (`flowchart.ts@1118751f:115-151`); see the module doc for the `isElbowArrow` divergence.
fn connected_flowchart_obstacles(scene: &Scene, start: &DrawElement) -> Vec<WorldBounds> {
    let arrows: Vec<&DrawElement> = scene
        .iter_ordered()
        .filter(|el| el.kind == DrawElementType::Arrow)
        .collect();
    let mut visited: HashSet<String> = HashSet::from([start.id.clone()]);
    let mut queue: Vec<String> = vec![start.id.clone()];
    let mut obstacles = Vec::new();
    let mut i = 0;
    while i < queue.len() {
        let current = queue[i].clone();
        i += 1;
        for arrow in &arrows {
            let neighbor_id = if arrow.start_binding.as_deref() == Some(current.as_str()) {
                arrow.end_binding.clone()
            } else if arrow.end_binding.as_deref() == Some(current.as_str()) {
                arrow.start_binding.clone()
            } else {
                None
            };
            let Some(neighbor_id) = neighbor_id else {
                continue;
            };
            if !visited.insert(neighbor_id.clone()) {
                continue;
            }
            if let Some(node) = scene.get(&neighbor_id) {
                if !node.is_deleted {
                    obstacles.push(element_rotated_bounds(node));
                    queue.push(neighbor_id);
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
        candidates.sort_by(|a, b| {
            let da = (a + cluster_cross_size / 2.0 - parent_cross_center).abs();
            let db = (b + cluster_cross_size / 2.0 - parent_cross_center).abs();
            da.partial_cmp(&db).unwrap()
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
/// (`flowchart.ts@1118751f:232-270`). `kind` need not match `template`'s: the digit-key
/// shape choice clones the template's *style*, never its type.
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
    if kind == DrawElementType::StickyNote {
        node.base_height = template.base_height;
    }
    node
}

/// A straight arrow from `source` to `target`, bound at both ends — see the module doc for
/// why this engine draws a straight line where the oracle's is elbow-routed. The two ends
/// are anchored at each shape's centre in [`BindMode::Orbit`]; the visible points here are
/// only a bootstrap; the shape they settle at is whatever
/// [`crate::scene::binding::refresh_bindings_in_place`] gives every other bound arrow, run
/// once the cluster is committed to the scene ([`DrawEngine::flowchart_commit`]).
fn binding_arrow(source: &DrawElement, target: &DrawElement, now: f64) -> DrawElement {
    let start_pt = attach_point(source, element_center(target), binding_gap(source));
    let end_pt = attach_point(target, element_center(source), binding_gap(target));
    let style = DrawElementStyle {
        stroke_color: source.stroke_color.clone(),
        background_color: "transparent".to_string(),
        fill_style: source.fill_style,
        stroke_width: source.stroke_width,
        stroke_style: source.stroke_style,
        roughness: source.roughness,
        opacity: source.opacity,
        roundness: None,
    };
    let arrow = create_element(DrawElementType::Arrow, Geometry::default(), style, now);
    let mut arrow = linear_from_endpoints(arrow, start_pt, end_pt);
    set_anchor(
        &mut arrow,
        End::Start,
        Some(Anchor {
            element_id: source.id.clone(),
            fixed_point: [0.5, 0.5],
            mode: BindMode::Orbit,
        }),
    );
    set_anchor(
        &mut arrow,
        End::End,
        Some(Anchor {
            element_id: target.id.clone(),
            fixed_point: [0.5, 0.5],
            mode: BindMode::Orbit,
        }),
    );
    arrow
}

/// The pending cluster for one direction press: `count` node-and-arrow pairs
/// (`[node0, arrow0, node1, arrow1, …]`, matching the oracle's interleaving so index 0 is
/// always the first node), and the cross-axis start the next grow anchors to.
fn build_pending(
    scene: &Scene,
    start: &DrawElement,
    direction: LinkDirection,
    count: usize,
    shape: DrawElementType,
    sticky_cross_start: Option<f64>,
    now: f64,
) -> (Vec<DrawElement>, f64) {
    let obstacles = connected_flowchart_obstacles(scene, start);
    let (positions, cross_start) =
        place_cluster(start, direction, count, &obstacles, sticky_cross_start);
    let mut pending = Vec::with_capacity(count * 2);
    for (x, y) in positions {
        let node = clone_flowchart_node(start, shape, x, y, now);
        let arrow = binding_arrow(start, &node, now);
        pending.push(node);
        pending.push(arrow);
    }
    (pending, cross_start)
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

/// Which side of `node`'s centre `other` falls on. See the module doc for why this stands
/// in for the oracle's per-endpoint `headingForPointFromElement`.
fn heading_from_center(node: &DrawElement, other: Point) -> LinkDirection {
    let center = element_center(node);
    let dx = other.x - center.x;
    let dy = other.y - center.y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            LinkDirection::Right
        } else {
            LinkDirection::Left
        }
    } else if dy >= 0.0 {
        LinkDirection::Down
    } else {
        LinkDirection::Up
    }
}

/// Every node bound to `element` by an arrow, on either end, that lies in `direction` —
/// the union of the oracle's `getSuccessors` and `getPredecessors`
/// (`flowchart.ts@1118751f:582-679`): a link is walked either way.
fn linked_nodes(element: &DrawElement, scene: &Scene, direction: LinkDirection) -> Vec<String> {
    let mut result = Vec::new();
    for arrow in scene
        .iter_ordered()
        .filter(|el| el.kind == DrawElementType::Arrow)
    {
        let other_id = if arrow.start_binding.as_deref() == Some(element.id.as_str()) {
            arrow.end_binding.as_deref()
        } else if arrow.end_binding.as_deref() == Some(element.id.as_str()) {
            arrow.start_binding.as_deref()
        } else {
            None
        };
        let Some(other_id) = other_id else {
            continue;
        };
        let Some(other) = scene.get(other_id).filter(|el| !el.is_deleted) else {
            continue;
        };
        if heading_from_center(element, element_center(other)) == direction {
            result.push(other.id.clone());
        }
    }
    result
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
    fn explore(
        &mut self,
        element: &DrawElement,
        scene: &Scene,
        direction: LinkDirection,
    ) -> Option<String> {
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
        if !nodes.is_empty() {
            self.index = 0;
            self.exploring = true;
            self.same_level = nodes.clone();
            self.direction = Some(direction);
            self.visited.insert(nodes[0].clone());
            return Some(nodes[0].clone());
        }

        // Nothing at this level: hop to any other unvisited linked node, for speedier
        // navigation without switching arrow keys.
        if Some(direction) == self.direction || !self.exploring {
            if !self.exploring {
                self.visited.insert(element.id.clone());
            }
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
    /// node, growing it by one on a repeat press in the same direction. No-op with no
    /// exactly-one flowchart-eligible selection — the guard the oracle's
    /// `resolveKeyboardEventToOperation` runs before calling `createNodes`
    /// (`App.flowchart.ts@1118751f:112-131`).
    pub fn flowchart_create(&mut self, direction: LinkDirection) {
        let selected = self.get_selected_elements();
        let [start] = selected.as_slice() else {
            return;
        };
        if !is_flowchart_node(start) {
            return;
        }
        let (count, cross_start, shape) = match &self.flowchart_creator {
            Some(creator) if creator.direction == direction => (
                creator.number_of_nodes + 1,
                creator.cluster_cross_start,
                creator.shape,
            ),
            // A different direction restarts the cluster, as the oracle's does
            // (`flowchart.ts@1118751f:699-704`) — but keeps the shape already chosen this
            // gesture, rather than reverting to the start node's own.
            Some(creator) => (1, None, creator.shape),
            None => (1, None, start.kind),
        };
        let now = self.now_ms;
        let (pending, cross_start) = build_pending(
            &self.scene,
            start,
            direction,
            count,
            shape,
            cross_start,
            now,
        );
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
        let now = self.now_ms;
        let (pending, cross_start) = build_pending(
            &self.scene,
            start,
            direction,
            count,
            shape,
            cross_start,
            now,
        );
        self.flowchart_creator = Some(FlowchartCreator {
            direction,
            number_of_nodes: count,
            cluster_cross_start: Some(cross_start),
            shape,
            pending,
        });
        self.request_draw();
    }

    /// Releasing Ctrl/Cmd: adds the pending cluster to the scene, selects the first new
    /// node, records **one** step of history — `insertNewElements` followed by
    /// `captureUpdate: IMMEDIATELY` (`App.flowchart.ts@1118751f:78-90`) — and eases the
    /// camera to the new node if it landed off screen (`App.flowchart.ts@1118751f:86`'s own
    /// `selectAndReveal`).
    pub fn flowchart_commit(&mut self) {
        let Some(creator) = self.flowchart_creator.take() else {
            return;
        };
        if creator.pending.is_empty() {
            return;
        }
        let first_node_id = creator.pending[0].id.clone();
        let first_node_bounds = element_rotated_bounds(&creator.pending[0]);
        for element in creator.pending {
            self.scene.add(element);
        }
        self.set_selection(vec![first_node_id]);
        self.apply_bindings();
        self.push_history();
        self.reveal(first_node_bounds, REVEAL_PADDING);
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

    /// Alt+Arrow: selects the node connected in that direction, cycling same-level nodes on
    /// a repeat press, and eases the camera to it if it is off screen. Returns the id
    /// selected, or `None` with no exactly-one bindable selection or nothing linked that way.
    pub fn flowchart_navigate(&mut self, direction: LinkDirection) -> Option<String> {
        let selected = self.get_selected_elements();
        let [element] = selected.as_slice() else {
            return None;
        };
        if !is_bindable_element(element) {
            return None;
        }
        let element = element.clone();
        let id = self
            .flowchart_navigator
            .explore(&element, &self.scene, direction);
        if let Some(id) = &id {
            self.set_selection(vec![id.clone()]);
            let bounds = self.scene.get(id).map(element_rotated_bounds);
            if let Some(bounds) = bounds {
                self.reveal(bounds, REVEAL_PADDING);
            }
        }
        id
    }

    /// Alt released: ends the exploration, so the next Alt+Arrow starts fresh rather than
    /// continuing a cycle through stale same-level nodes.
    pub fn flowchart_navigation_end(&mut self) {
        if self.flowchart_navigator.exploring {
            self.flowchart_navigator.clear();
        }
    }
}
