//! Elbow arrows: routed around the shapes they connect in horizontal and vertical runs.
//!
//! A port of Excalidraw's router at the pin (`packages/element/src/elbowArrow.ts@1118751f`
//! and what it reaches): the same arithmetic in the same order with the same constants, so
//! a board routes to the same points here as there. `tests/ci_elbow_oracle.rs` holds it to
//! routes recorded from the oracle's own code (`tools/elbow-oracle`) within 1e-9.
//!
//! An elbow arrow is an arrow with `elbowed: true`. Its points are the route's corners; it
//! is never turned (its angle stays 0); both of its bindings orbit, anchored where the end
//! snapped onto the outline. Segments the person has dragged are kept in
//! `fixed_segments` — the segment ending at point `index`, with its two ends — and the rest
//! of the route bends around them; `start_is_special`/`end_is_special` mark an extra
//! corner put in beside an end so a fixed route can still leave its shape square on.
//!
//! [`update`] is `updateElbowArrowPoints` and [`mutate`] the elbow branch of the oracle's
//! `mutateElement`, through which every change to an elbow arrow goes; the rest of this
//! module is the handful of editor gestures built on it.

mod heading;
mod outline;
mod route;
mod segments;
mod snap;

use serde::{Deserialize, Serialize};

pub use heading::Heading;
pub use outline::{hypot, Target};
pub use snap::{normalize_fixed_point, BASE_BINDING_GAP};

use crate::scene::binding::{anchor, set_anchor, Anchor, End};
use crate::scene::element::{Arrowhead, BindMode, DrawElement, DrawElementType};
use outline::Pt;

/// `BASE_PADDING`: how far a route keeps from the shapes it leaves and reaches.
pub const BASE_PADDING: f64 = 40.0;

/// A segment the person moved and the router must keep: Excalidraw's `FixedSegment`.
/// `index` is the index of the point it ends at, so the first segment is 1; `start` and
/// `end` are relative to the arrow's `x`/`y`, like its points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FixedSegment {
    pub index: usize,
    pub start: [f64; 2],
    pub end: [f64; 2],
}

/// Where one end of an elbow arrow is bound: a shape, and a point on it as a ratio of its
/// unrotated box (an elbow binding always orbits).
#[derive(Clone, Debug, PartialEq)]
pub struct ElbowBinding {
    pub element_id: String,
    pub fixed_point: [f64; 2],
}

/// The elbow arrow as the router reads it.
#[derive(Clone, Debug)]
pub(crate) struct Arrow {
    pub x: f64,
    pub y: f64,
    pub points: Vec<Pt>,
    pub start_binding: Option<ElbowBinding>,
    pub end_binding: Option<ElbowBinding>,
    /// Whether each end carries an arrowhead, which keeps the route six gaps clear of the
    /// shape rather than two.
    pub start_arrowhead: bool,
    pub end_arrowhead: bool,
    pub fixed_segments: Option<Vec<FixedSegment>>,
    pub start_is_special: Option<bool>,
    pub end_is_special: Option<bool>,
}

impl Arrow {
    pub(crate) fn of(element: &DrawElement) -> Self {
        let binding = |end| {
            anchor(element, end).map(|a| ElbowBinding {
                element_id: a.element_id,
                fixed_point: a.fixed_point,
            })
        };
        let head = |h: Option<Arrowhead>| h.is_some_and(|h| h != Arrowhead::None);
        Self {
            x: element.x,
            y: element.y,
            points: element.points.clone().unwrap_or_default(),
            start_binding: binding(End::Start),
            end_binding: binding(End::End),
            start_arrowhead: head(element.start_arrowhead),
            end_arrowhead: head(element.end_arrowhead),
            fixed_segments: element.fixed_segments.clone(),
            start_is_special: element.start_is_special,
            end_is_special: element.end_is_special,
        }
    }
}

/// The board as the router sees it: the shapes an arrow is bound to, and — only while an
/// end is dragged — whatever is under each end.
pub struct Board<'a> {
    /// Bottom of the z-order first.
    elements: Vec<&'a DrawElement>,
    by_id: std::collections::HashMap<&'a str, &'a DrawElement>,
}

impl<'a> Board<'a> {
    /// `elements` bottom of the z-order first, as a scene iterates them.
    pub fn new(elements: impl IntoIterator<Item = &'a DrawElement>) -> Self {
        let elements: Vec<&'a DrawElement> =
            elements.into_iter().filter(|e| !e.is_deleted).collect();
        let by_id = elements.iter().map(|e| (e.id.as_str(), *e)).collect();
        Self { elements, by_id }
    }

    pub fn get(&self, id: &str) -> Option<&'a DrawElement> {
        self.by_id.get(id).copied()
    }

    /// `getBindableElementForId`.
    fn bindable(&self, id: &str) -> Option<Target<'a>> {
        self.get(id).and_then(Target::of)
    }

    /// `getHoveredElementForBinding` (`collision.ts@1118751f:301-478`), measured on this
    /// module's outline. The engine's own [`crate::scene::binding::arrow_target_among`]
    /// makes the same choice on the exact outline, which near a diamond's tip or a rounded
    /// corner is a different shape by up to a unit — and a different shape under an end
    /// is a different route.
    fn hovered(&self, p: Pt, zoom: f64) -> Option<Target<'a>> {
        let tolerance = crate::scene::binding::max_binding_distance(zoom);
        // (shape, signed distance to its outline: positive inside), top first.
        let mut found: Vec<(Target<'a>, f64)> = Vec::new();
        for &element in self.elements.iter().rev() {
            let Some(t) = Target::of(element) else {
                continue;
            };
            let d = self.border_distance_if_close(&t, p, tolerance);
            if d > -tolerance {
                if element.locked != Some(true) {
                    found.push((t, d));
                }
                if d >= 0.0 && crate::scene::binding::occludes(element) {
                    break;
                }
            }
        }
        // Stable, as `Array.prototype.sort` is: equals keep the top of the stack first.
        found.sort_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
        let (nearest, distance) = found.first()?;
        let area =
            |[x1, y1, x2, y2]: outline::Bounds| ((x2 - x1).abs() * (y2 - y1).abs()).max(0.00001);
        let outer = outline::element_bounds(nearest);
        let outer_area = area(outer);
        let nested = found[1..].iter().find(|(t, d)| {
            if *d < 0.0 {
                return false;
            }
            let b = outline::element_bounds(t);
            let w = (b[2].min(outer[2]) - b[0].max(outer[0])).max(0.0);
            let h = (b[3].min(outer[3]) - b[1].max(outer[1])).max(0.0);
            let own = area(b);
            (h * w) / own > 0.25 && own / outer_area < 0.75
        });
        match nested {
            Some((t, _)) if *distance >= 0.0 => Some(*t),
            _ => Some(*nearest),
        }
    }

    /// `bindableElementBorderDistanceIfClose`: the distance to the outline, positive
    /// inside, or `-∞` when the point is beyond `tolerance`, clipped from view by the
    /// shape's frame, or inside a frame (bound only from outside).
    fn border_distance_if_close(&self, t: &Target, p: Pt, tolerance: f64) -> f64 {
        let reach = tolerance.max(1.0);
        let near = [p[0] - reach, p[1] - reach, p[0] + reach, p[1] + reach];
        if !outline::bounds_intersect(near, outline::element_bounds(t)) {
            return f64::NEG_INFINITY;
        }
        let frame = t.element.frame_id.as_deref().and_then(|id| self.get(id));
        let clipped = frame
            .filter(|f| f.kind == DrawElementType::Frame)
            .and_then(Target::of)
            .is_some_and(|f| !outline::inside_bounds(p, outline::element_bounds(&f)));
        if clipped {
            return f64::NEG_INFINITY;
        }
        let inside = outline::contains(t, p);
        if inside && t.element.kind == DrawElementType::Frame {
            return f64::NEG_INFINITY;
        }
        let distance = outline::distance_to(t, p);
        if inside {
            distance
        } else if distance > tolerance {
            f64::NEG_INFINITY
        } else {
            -distance
        }
    }
}

/// `updateElbowArrowPoints`'s options.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// An end is being dragged: each end snaps onto the outline of whatever is under it,
    /// found afresh, rather than sitting at its binding's anchor.
    pub is_dragging: bool,
    pub binding_enabled: bool,
    pub midpoint_snapping: bool,
    /// The oracle routes through `mutateElement`, which never carries the zoom, so every
    /// route measures at 1 (`elbowArrow.ts@1118751f:1220-1222`).
    pub zoom: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            is_dragging: false,
            binding_enabled: true,
            midpoint_snapping: true,
            zoom: 1.0,
        }
    }
}

/// What a change to an elbow arrow asks for: `ElementUpdate<ExcalidrawElbowArrowElement>`
/// restricted to the keys the router reads. `None` is a key left out; `Some(None)` is an
/// explicit `null`.
#[derive(Clone, Debug, Default)]
pub struct Updates {
    pub x: Option<f64>,
    pub y: Option<f64>,
    /// Relative to the arrow's `x`/`y`. Two points move only the ends.
    pub points: Option<Vec<[f64; 2]>>,
    pub fixed_segments: Option<Option<Vec<FixedSegment>>>,
    pub start_binding: Option<Option<ElbowBinding>>,
    pub end_binding: Option<Option<ElbowBinding>>,
}

/// What the router returns: each field `None` when it has nothing to say about it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Routed {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub points: Option<Vec<[f64; 2]>>,
    pub fixed_segments: Option<Option<Vec<FixedSegment>>>,
    pub start_is_special: Option<Option<bool>>,
    pub end_is_special: Option<Option<bool>>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

/// `clamp(value, -1e6, 1e6)`. `f64::clamp` keeps a NaN a NaN, as `Math.max` does.
fn clamp_position(value: f64) -> f64 {
    value.clamp(-1e6, 1e6)
}

/// `normalizeArrowElementUpdate`: global points made relative to the first, which becomes
/// the arrow's position. An empty route — no grid path, where the oracle throws — says
/// nothing, and the arrow keeps what it had.
pub(crate) fn normalize(
    global: Vec<Pt>,
    fixed_segments: Option<Vec<FixedSegment>>,
    start_is_special: Option<bool>,
    end_is_special: Option<bool>,
) -> Routed {
    let Some(&[ox, oy]) = global.first() else {
        return Routed::default();
    };
    let points: Vec<Pt> = global
        .iter()
        .map(|p| [clamp_position(p[0] - ox), clamp_position(p[1] - oy)])
        .collect();
    let xs: Vec<f64> = points.iter().map(|p| p[0]).collect();
    let ys: Vec<f64> = points.iter().map(|p| p[1]).collect();
    Routed {
        x: Some(clamp_position(ox)),
        y: Some(clamp_position(oy)),
        width: Some(outline::js_max(&xs) - outline::js_min(&xs)),
        height: Some(outline::js_max(&ys) - outline::js_min(&ys)),
        points: Some(points),
        fixed_segments: Some(fixed_segments.filter(|s| !s.is_empty())),
        start_is_special: Some(start_is_special),
        end_is_special: Some(end_is_special),
    }
}

/// `validateElbowPoints`: every segment horizontal or vertical to within a unit.
pub fn validate_points(points: &[[f64; 2]]) -> bool {
    points.windows(2).all(|w| {
        (w[1][0] - w[0][0]).abs() < route::DEDUP_TRESHOLD
            || (w[1][1] - w[0][1]).abs() < route::DEDUP_TRESHOLD
    })
}

/// `updateElbowArrowPoints`.
///
/// Two differences from the oracle, neither reachable from its callers: its "no-op"
/// shortcut fires only when a caller hands back the arrow's own binding objects, which no
/// caller does, so it is not reproduced; and its first case, an empty scene, cannot occur
/// here, where the arrow is always on the board it is routed on.
pub(crate) fn update(arrow: &Arrow, board: &Board, updates: &Updates, options: &Options) -> Routed {
    if arrow.points.len() < 2 {
        return Routed {
            points: Some(
                updates
                    .points
                    .clone()
                    .unwrap_or_else(|| arrow.points.clone()),
            ),
            ..Routed::default()
        };
    }
    let fixed: Vec<FixedSegment> = match &updates.fixed_segments {
        Some(Some(list)) => list.clone(),
        _ => arrow.fixed_segments.clone().unwrap_or_default(),
    };
    let updated: Vec<Pt> = match &updates.points {
        Some(p) if p.len() == 2 => {
            let last = arrow.points.len() - 1;
            arrow
                .points
                .iter()
                .enumerate()
                .map(|(i, &q)| {
                    if i == 0 {
                        p[0]
                    } else if i == last {
                        p[1]
                    } else {
                        q
                    }
                })
                .collect()
        }
        Some(p) => p.clone(),
        None => arrow.points.clone(),
    };
    let start_binding = updates
        .start_binding
        .clone()
        .unwrap_or_else(|| arrow.start_binding.clone());
    let end_binding = updates
        .end_binding
        .clone()
        .unwrap_or_else(|| arrow.end_binding.clone());
    let start_missing = start_binding
        .as_ref()
        .is_some_and(|b| board.bindable(&b.element_id).is_none());
    let end_missing = end_binding
        .as_ref()
        .is_some_and(|b| board.bindable(&b.element_id).is_none());
    let valid = validate_points(&updated);
    let rest_empty = updates.x.is_none()
        && updates.y.is_none()
        && updates.points.is_none()
        && updates.fixed_segments.is_none();
    if ((start_missing || end_missing) && valid) || (rest_empty && (start_missing || end_missing)) {
        return normalize(
            updated
                .iter()
                .map(|p| [arrow.x + p[0], arrow.y + p[1]])
                .collect(),
            arrow.fixed_segments.clone(),
            arrow.start_is_special,
            arrow.end_is_special,
        );
    }

    // 1. Renormalise.
    let falsy = |b: &Option<Option<ElbowBinding>>| !matches!(b, Some(Some(_)));
    if updates.points.is_none()
        && !matches!(updates.fixed_segments, Some(Some(_)))
        && falsy(&updates.start_binding)
        && falsy(&updates.end_binding)
    {
        return segments::renormalize(arrow, board);
    }

    let rebound = Arrow {
        start_binding,
        end_binding,
        ..arrow.clone()
    };
    let e = route::ends(&rebound, board, &updated, options);

    // 2. No fixed segments: route.
    if fixed.is_empty() {
        let corners = route::corners_of(arrow.start_binding.is_some(), &e);
        return normalize(corners, Some(fixed), None, None);
    }
    // 3. A segment released.
    if arrow.fixed_segments.as_ref().map_or(0, Vec::len) > fixed.len() {
        return segments::release(arrow, &fixed, board);
    }
    // 4. A segment moved.
    if updates.points.is_none() {
        return segments::move_segment(
            arrow,
            &fixed,
            e.start_heading,
            e.end_heading,
            e.start_target.as_ref(),
            e.end_target.as_ref(),
        );
    }
    // 5. Resized: taken as given.
    if let Some(Some(list)) = &updates.fixed_segments {
        return Routed {
            x: updates.x,
            y: updates.y,
            points: updates.points.clone(),
            fixed_segments: Some(Some(list.clone())),
            ..Routed::default()
        };
    }
    // 6. An end moved with segments fixed.
    segments::drag_endpoint(
        arrow,
        &updated,
        &fixed,
        e.start_heading,
        e.end_heading,
        e.start,
        e.end,
        e.start_target.as_ref(),
        e.end_target.as_ref(),
    )
}

fn apply_binding(element: &mut DrawElement, end: End, binding: Option<ElbowBinding>) {
    set_anchor(
        element,
        end,
        binding.map(|b| Anchor {
            element_id: b.element_id,
            fixed_point: b.fixed_point,
            mode: BindMode::Orbit,
        }),
    );
}

/// The elbow branch of the oracle's `mutateElement`: the change as asked, the angle
/// straightened, then whatever the router made of it written over both.
pub fn mutate(element: &mut DrawElement, board: &Board, updates: Updates, options: &Options) {
    let mut arrow = Arrow::of(element);
    // `updates.x || element.x`: a zero is not a position here either.
    if let Some(x) = updates.x.filter(|x| *x != 0.0 && !x.is_nan()) {
        arrow.x = x;
    }
    if let Some(y) = updates.y.filter(|y| *y != 0.0 && !y.is_nan()) {
        arrow.y = y;
    }
    let routed = update(&arrow, board, &updates, options);
    if let Some(x) = updates.x {
        element.x = x;
    }
    if let Some(y) = updates.y {
        element.y = y;
    }
    if let Some(points) = updates.points {
        element.points = Some(points);
    }
    if let Some(fixed) = updates.fixed_segments {
        element.fixed_segments = fixed;
    }
    if let Some(binding) = updates.start_binding {
        apply_binding(element, End::Start, binding);
    }
    if let Some(binding) = updates.end_binding {
        apply_binding(element, End::End, binding);
    }
    element.angle = 0.0;
    apply(element, routed);
}

/// Writes what the router returned onto `element`.
fn apply(element: &mut DrawElement, routed: Routed) {
    if let Some(x) = routed.x {
        element.x = x;
    }
    if let Some(y) = routed.y {
        element.y = y;
    }
    if let Some(points) = routed.points {
        element.points = Some(points);
    }
    if let Some(fixed) = routed.fixed_segments {
        element.fixed_segments = fixed;
    }
    if let Some(special) = routed.start_is_special {
        element.start_is_special = special;
    }
    if let Some(special) = routed.end_is_special {
        element.end_is_special = special;
    }
    if let Some(width) = routed.width {
        element.width = width;
    }
    if let Some(height) = routed.height {
        element.height = height;
    }
}

/// Whether `element` is an elbow arrow.
pub fn is_elbow(element: &DrawElement) -> bool {
    element.kind == crate::scene::element::DrawElementType::Arrow && element.elbowed == Some(true)
}

/// Moves an elbow arrow's ends, as the oracle's `LinearElementEditor.movePoints` does:
/// `start`/`end` are relative to its `x`/`y`, `None` leaves that end where it is. The
/// route follows; while `dragging`, each end also snaps to the outline under it.
pub fn move_ends(
    element: &mut DrawElement,
    board: &Board,
    start: Option<[f64; 2]>,
    end: Option<[f64; 2]>,
    dragging: bool,
) {
    let points = element.points.clone().unwrap_or_default();
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else {
        return;
    };
    let updates = Updates {
        points: Some(vec![start.unwrap_or(first), end.unwrap_or(last)]),
        ..Updates::default()
    };
    let options = Options {
        is_dragging: dragging,
        ..Options::default()
    };
    mutate(element, board, updates, &options);
}

/// The anchor an elbow arrow's `end` gets on `shape` where it is now:
/// `calculateFixedPointForElbowArrowBinding`, snapping onto the outline at `zoom`.
pub fn fixed_point_for(
    element: &DrawElement,
    shape: &DrawElement,
    end: End,
    zoom: f64,
    midpoint_snapping: bool,
) -> Option<[f64; 2]> {
    let target = Target::of(shape)?;
    let points = element.points.as_deref().unwrap_or(&[]);
    let local = match end {
        End::Start => points.first(),
        End::End => points.last(),
    }
    .copied()
    .unwrap_or([0.0, 0.0]);
    let point = [element.x + local[0], element.y + local[1]];
    Some(snap::fixed_point_for(
        point,
        points.len(),
        &target,
        zoom,
        true,
        midpoint_snapping,
    ))
}

/// Binds `end` of an elbow arrow to `shape` where it is: `bindBindingElement` for an
/// elbow arrow. The route is left for the caller's next move of the ends.
pub fn bind(
    element: &mut DrawElement,
    shape: &DrawElement,
    end: End,
    zoom: f64,
    midpoint_snapping: bool,
) {
    if let Some(fixed_point) = fixed_point_for(element, shape, end, zoom, midpoint_snapping) {
        apply_binding(
            element,
            end,
            Some(ElbowBinding {
                element_id: shape.id.clone(),
                fixed_point,
            }),
        );
    }
}

/// Re-routes an elbow arrow after a shape it is bound to moved or changed size, as the
/// oracle's `updateBoundElements` does: each bound end is put at its anchor and the route
/// follows.
///
/// ponytail: a bound end is put at its anchor, where `updateBoundPoint` would put it where
/// the line between both anchors crosses the outline. The router reads that point only to
/// tell whether it is within 15 of the outline, and an elbow anchor always is — it is
/// snapped a gap out — so the route is the same. Upgrade path: port `updateBoundPoint`
/// if an anchor can ever be set further out.
pub fn reroute(element: &mut DrawElement, board: &Board) {
    let arrow = Arrow::of(element);
    let (Some(&first), Some(&last)) = (arrow.points.first(), arrow.points.last()) else {
        return;
    };
    // `updateBoundPoint` leaves the ends of a just-pressed arrow where they are.
    let pressed = last[0].abs() < 1e-4 && last[1].abs() < 1e-4;
    let at = |binding: &Option<ElbowBinding>, fallback: Pt| match binding {
        _ if pressed => fallback,
        Some(b) => match board.bindable(&b.element_id) {
            Some(t) => {
                let p = snap::global_fixed_point(b.fixed_point, &t);
                [p[0] - arrow.x, p[1] - arrow.y]
            }
            None => fallback,
        },
        None => fallback,
    };
    let start = at(&arrow.start_binding, first);
    let end = at(&arrow.end_binding, last);
    move_ends(element, board, Some(start), Some(end), false);
}

/// Drags segment `index` (the one ending at point `index`) through `(x, y)`:
/// `LinearElementEditor.moveFixedSegment`. The segment keeps its orientation, so only
/// the coordinate across it moves.
pub fn move_segment(element: &mut DrawElement, board: &Board, index: usize, x: f64, y: f64) {
    let points = element.points.clone().unwrap_or_default();
    if index == 0 || index >= points.len() {
        return;
    }
    let horizontal = heading::heading_for_point_is_horizontal(points[index], points[index - 1]);
    let mut fixed: Vec<FixedSegment> = element.fixed_segments.clone().unwrap_or_default();
    fixed.retain(|s| s.index != index);
    fixed.push(FixedSegment {
        index,
        start: [
            if horizontal {
                points[index - 1][0]
            } else {
                x - element.x
            },
            if horizontal {
                y - element.y
            } else {
                points[index - 1][1]
            },
        ],
        end: [
            if horizontal {
                points[index][0]
            } else {
                x - element.x
            },
            if horizontal {
                y - element.y
            } else {
                points[index][1]
            },
        ],
    });
    // `Object.values` of an index-keyed record, then sorted by index.
    fixed.sort_by_key(|s| s.index);
    let updates = Updates {
        fixed_segments: Some(Some(fixed)),
        ..Updates::default()
    };
    mutate(element, board, updates, &Options::default());
}

/// Lets go of fixed segment `index`: `LinearElementEditor.deleteFixedSegment`. The route
/// between its neighbours is worked out afresh.
pub fn release_segment(element: &mut DrawElement, board: &Board, index: usize) {
    let fixed = element
        .fixed_segments
        .clone()
        .map(|list| list.into_iter().filter(|s| s.index != index).collect());
    let updates = Updates {
        fixed_segments: Some(fixed),
        ..Updates::default()
    };
    mutate(element, board, updates, &Options::default());
}

/// Renormalises an elbow arrow — merges the corners a changed route no longer turns at —
/// as the oracle's empty `mutateElement` does after an indirect change.
pub fn renormalize(element: &mut DrawElement, board: &Board) {
    mutate(element, board, Updates::default(), &Options::default());
}

/// Binds a new elbow arrow to `start` and `end` and routes it between them: the binding
/// and routing half of the oracle's flowchart `createBindingArrow`
/// (`flowchart.ts@1118751f:310-448`). `arrow` comes with its position and its two points
/// already at the two shapes, and `elbowed` set; `board` must hold both shapes.
pub fn bind_and_route(
    arrow: &mut DrawElement,
    start: &DrawElement,
    end: &DrawElement,
    board: &Board,
    zoom: f64,
) {
    bind(arrow, start, End::Start, zoom, true);
    bind(arrow, end, End::End, zoom, true);
    move_ends(arrow, board, None, None, false);
    let updates = Updates {
        points: arrow.points.clone(),
        ..Updates::default()
    };
    mutate(arrow, board, updates, &Options::default());
}
