//! Drawing and editing elbow arrows: the editor's gestures over the router in
//! [`crate::scene::elbow`], as the oracle's `App.tsx` and `LinearElementEditor` drive it.
//!
//! An end being dragged — a new arrow's head, or either end of one selected — only moves:
//! the route snaps it onto the shape it is over, but its bindings stay as they were until
//! the release, which binds it to the shape under the pointer or lets it go
//! (`pointDraggingUpdates`, `linearElementEditor.ts@1118751f:2438-2460`, then
//! `actionFinalize.tsx@1118751f:88-137`). A segment's midpoint drags that segment across and
//! keeps it there (`moveFixedSegment`); a double click on one lets go of it.

use std::collections::HashSet;

use crate::camera::Point;
use crate::engine::DrawEngine;
use crate::interaction::constrain_to_angle;
use crate::scene::binding::{set_anchor, End};
use crate::scene::elbow::{self, Board, Options};
use crate::scene::{DrawElement, DrawElementType};
use crate::selection::linear::{elbow_handle_points, hit_handle};
use crate::selection::LinearHandle;

/// `POINT_HANDLE_SIZE / 2`: how long a segment must be on screen to offer its midpoint
/// (`isSegmentTooShort`, `linearElementEditor.ts@1118751f:943-949`).
pub(super) const SEGMENT_MIN_PX: f64 = 5.0;

/// `INVISIBLY_SMALL_ELEMENT_SIZE` (`constants.ts@1118751f`): a new two-point arrow whose ends
/// are closer than this on both axes is thrown away on release.
const INVISIBLY_SMALL: f64 = 0.1;

impl DrawEngine {
    /// How a gesture routes: binding off while Ctrl/Cmd is held, and no snap to a side's
    /// middle with the angle locked or the grid on (`linearElementEditor.ts@1118751f:383-399`).
    fn elbow_options(&self, dragging: bool, angle_locked: bool) -> Options {
        Options {
            is_dragging: dragging,
            binding_enabled: !self.ctrl_held,
            midpoint_snapping: !angle_locked && !(self.grid.enabled && self.grid.snap),
            ..Options::default()
        }
    }

    /// The shape an elbow arrow's end at `world` binds to at this zoom — none while
    /// Ctrl/Cmd is held (`getHoveredElementForBinding`).
    pub(super) fn elbow_target_at(&self, world: Point) -> Option<String> {
        if self.ctrl_held {
            return None;
        }
        Board::new(self.scene.iter_ordered())
            .shape_at([world.x, world.y], self.camera.scale)
            .map(|shape| shape.id.clone())
    }

    /// A new elbow arrow's first press at `world`: its tail binds to the shape there and
    /// the route leaves that shape's outline, or it starts unbound
    /// (`App.tsx@1118751f:10305-10347`).
    pub(super) fn begin_elbow(&self, element: &mut DrawElement, world: Point) {
        set_anchor(element, End::Start, None);
        set_anchor(element, End::End, None);
        if self.ctrl_held {
            return;
        }
        let board = Board::new(self.scene.iter_ordered());
        if let Some(shape) = board.shape_at([world.x, world.y], self.camera.scale) {
            let snapping = self.elbow_options(false, false).midpoint_snapping;
            elbow::bind(element, shape, End::Start, self.camera.scale, snapping);
            elbow::reroute(element, &board);
        }
    }

    /// Drags `end` of elbow arrow `id` to `world` — Shift holds it to an angle from the
    /// corner beside it. The route follows and snaps the end onto the shape under it,
    /// which lights as the one a release there binds to.
    pub(super) fn drag_elbow_end(&mut self, id: &str, end: End, world: Point, square: bool) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let points = element.points.clone().unwrap_or_default();
        if points.len() < 2 {
            return;
        }
        let at = if square {
            let beside = match end {
                End::Start => points[1],
                End::End => points[points.len() - 2],
            };
            let (x, y) = (element.x + beside[0], element.y + beside[1]);
            let (dx, dy) = constrain_to_angle(world.x - x, world.y - y);
            Point {
                x: x + dx,
                y: y + dy,
            }
        } else {
            world
        };
        let local = [at.x - element.x, at.y - element.y];
        let (start, finish) = match end {
            End::Start => (Some(local), None),
            End::End => (None, Some(local)),
        };
        let options = self.elbow_options(true, square);
        let target = {
            let board = Board::new(self.scene.iter_ordered());
            elbow::move_ends(&mut element, &board, start, finish, &options);
            board
                .shape_at([at.x, at.y], self.camera.scale)
                .filter(|_| options.binding_enabled)
                .map(|shape| shape.id.clone())
        };
        self.binding_highlight = target;
        self.binding_point = Some(at);
        self.binding_snaps = None;
        self.scene.put(element);
        let label = crate::scene::binding::refresh_binding_of(&mut self.scene, id);
        self.rewrap_linear_labels(label);
        self.request_draw();
    }

    /// Lets go of `end` of elbow arrow `id` over `shape`: the route is renormalised, then
    /// the end binds to `shape` where it snapped and the route leaves it square — or, over
    /// nothing, lets go of what it was bound to where it is, which the oracle does not
    /// route (`App.tsx@1118751f:11574-11585`, then `actionFinalize`).
    pub(super) fn drop_elbow_end(&mut self, id: &str, end: End, shape: Option<&str>) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        {
            let board = Board::new(self.scene.iter_ordered());
            elbow::renormalize(&mut element, &board);
            match shape.and_then(|shape| board.get(shape)) {
                Some(shape) => {
                    let snapping = self.elbow_options(false, false).midpoint_snapping;
                    elbow::bind(&mut element, shape, end, self.camera.scale, snapping);
                    elbow::reroute(&mut element, &board);
                }
                None => set_anchor(&mut element, end, None),
            }
        }
        self.scene.put(element);
        let label = crate::scene::binding::refresh_binding_of(&mut self.scene, id);
        self.rewrap_linear_labels(label);
    }

    /// Throws away new elbow arrow `id` if its release left it invisibly small
    /// (`isInvisiblySmallElement`, `actionFinalize.tsx@1118751f:300`). Returns whether it did.
    pub(super) fn discard_invisible_elbow(&mut self, id: &str) -> bool {
        let small = self
            .scene
            .get(id)
            .is_some_and(|element| match element.points.as_deref() {
                Some([a, b]) => {
                    (a[0] - b[0]).abs() < INVISIBLY_SMALL && (a[1] - b[1]).abs() < INVISIBLY_SMALL
                }
                points => points.is_none_or(|p| p.len() < 2),
            });
        if small {
            self.scene.discard(id);
        }
        small
    }

    /// One move of a drag on elbow arrow `id`'s `handle`, and the handle the drag goes on
    /// with: an end is dragged as above, a segment across (`moveFixedSegment`), and a
    /// segment's index can change under it as the route gains or loses corners before it.
    pub(super) fn drag_elbow_handle(
        &mut self,
        id: &str,
        handle: LinearHandle,
        world: Point,
        square: bool,
    ) -> LinearHandle {
        match handle {
            LinearHandle::Point(i) => {
                let end = if i == 0 { End::Start } else { End::End };
                self.drag_elbow_end(id, end, world, square);
                let last = self
                    .scene
                    .get(id)
                    .and_then(|element| element.points.as_ref())
                    .map_or(i, |points| points.len().saturating_sub(1));
                LinearHandle::Point(if i == 0 { 0 } else { last })
            }
            LinearHandle::Midpoint(i) => {
                let Some(mut element) = self.scene.get(id).cloned() else {
                    return handle;
                };
                let next = {
                    let board = Board::new(self.scene.iter_ordered());
                    elbow::move_segment(&mut element, &board, i + 1, world.x, world.y)
                };
                self.scene.put(element);
                let label = crate::scene::binding::refresh_binding_of(&mut self.scene, id);
                self.rewrap_linear_labels(label);
                self.request_draw();
                LinearHandle::Midpoint(next.map_or(i, |index| index.saturating_sub(1)))
            }
        }
    }

    /// The release of a drag on elbow arrow `id`'s `handle`, the pointer last at
    /// `released_at`: an end is dropped (see [`Self::drop_elbow_end`]) on what is there, a
    /// segment stays where it was put and the route is renormalised. A press that moved
    /// nothing changes nothing.
    pub(super) fn end_elbow_handle(
        &mut self,
        id: &str,
        handle: LinearHandle,
        released_at: Option<Point>,
    ) {
        if self.scene.committed(id).is_none() {
            return;
        }
        match (handle, released_at) {
            (LinearHandle::Point(i), Some(at)) => {
                let end = if i == 0 { End::Start } else { End::End };
                let shape = self.elbow_target_at(at);
                self.drop_elbow_end(id, end, shape.as_deref());
            }
            _ => {
                let Some(mut element) = self.scene.get(id).cloned() else {
                    return;
                };
                elbow::renormalize(&mut element, &Board::new(self.scene.iter_ordered()));
                self.scene.put(element);
            }
        }
    }

    /// A double click at `world` on a segment's midpoint of the one elbow arrow selected:
    /// that segment is let go of and routed afresh (`App.tsx@1118751f:7235-7292`). Returns
    /// whether it landed on one.
    pub(super) fn release_elbow_segment_at(&mut self, world: Point) -> bool {
        let Some(element) = self
            .single_selected()
            .filter(|element| elbow::is_elbow(element) && !element.locked())
        else {
            return false;
        };
        let midpoints: Vec<_> = self
            .point_handles(&element)
            .into_iter()
            .filter(|h| matches!(h.handle, LinearHandle::Midpoint(_)))
            .collect();
        let reach = super::HANDLE_HIT_PX / self.camera.scale;
        let Some(LinearHandle::Midpoint(i)) = hit_handle(&midpoints, world.x, world.y, reach)
        else {
            return false;
        };
        let mut next = element;
        elbow::release_segment(&mut next, &Board::new(self.scene.iter_ordered()), i + 1);
        self.scene.put(next);
        self.push_history();
        self.request_draw();
        true
    }

    /// Leaves out of a drag what the oracle's `dragSelectedElements` does not move
    /// (`dragElements.ts@1118751f:45-66`): an elbow arrow dragged alone and bound at either
    /// end, and one bound at both ends whose shapes are not both dragged with it. The
    /// router moves it instead, as its shapes go.
    pub(super) fn pin_elbows(&self, moving: &mut HashSet<String>) {
        let dragged: Vec<&DrawElement> = moving
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| !(el.kind == DrawElementType::Text && el.container_id.is_some()))
            .collect();
        if let [alone] = dragged.as_slice() {
            if elbow::is_elbow(alone)
                && (alone.start_binding.is_some() || alone.end_binding.is_some())
            {
                moving.clear();
                return;
            }
        }
        let pinned: Vec<String> = dragged
            .iter()
            .filter(|el| elbow::is_elbow(el))
            .filter(|el| match (&el.start_binding, &el.end_binding) {
                (Some(start), Some(end)) => !moving.contains(start) || !moving.contains(end),
                _ => false,
            })
            .map(|el| el.id.clone())
            .collect();
        for id in pinned {
            moving.remove(&id);
        }
    }

    /// The end of a drag that moved `ids`: each elbow arrow bound to one of them is
    /// renormalised, as the oracle does for the shape it dragged
    /// (`App.tsx@1118751f:11523-11537`).
    ///
    /// ponytail: every shape dragged, where the oracle takes only the one pressed on; a
    /// route the drag re-routed is already normal, so this differs only for an arrow
    /// carried along. Upgrade path: keep the pressed element on `Interaction::Move`.
    pub(super) fn renormalize_elbows_of(&mut self, ids: &[String]) {
        if !ids.iter().any(|id| self.scene.committed(id).is_some()) {
            return;
        }
        let moved: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let bound =
            |binding: &Option<String>| binding.as_deref().is_some_and(|id| moved.contains(id));
        let arrows: Vec<DrawElement> = {
            let board = Board::new(self.scene.iter_ordered());
            self.scene
                .iter_ordered()
                .filter(|el| {
                    elbow::is_elbow(el) && (bound(&el.start_binding) || bound(&el.end_binding))
                })
                .filter_map(|el| {
                    let mut next = el.clone();
                    elbow::renormalize(&mut next, &board);
                    (&next != el).then_some(next)
                })
                .collect()
        };
        for arrow in arrows {
            self.scene.put(arrow);
        }
    }

    /// The point handles a sole selected element offers at this zoom.
    ///
    /// An elbow arrow offers its ends and segments. Otherwise, points and midpoints in
    /// place of a box when [`Self::shows_point_handles`]. A longer line or arrow keeps its
    /// box and still offers every point under it, without the midpoints: the oracle draws
    /// the circles for any selected linear element that is not locked
    /// (`interactiveScene.ts@1118751f:1790-1815`), gates only the midpoints on the editor
    /// (`:1198`, `linearElementEditor.ts@1118751f:887-893`), and takes a press on a point
    /// after the box handles (`App.tsx@1118751f:9406-9445`). Without them an arrow's
    /// points were out of reach, since a double click on an arrow opens its label rather
    /// than the editor.
    pub(crate) fn point_handles(
        &self,
        element: &DrawElement,
    ) -> Vec<crate::selection::linear::LinearHandlePoint> {
        if elbow::is_elbow(element) {
            return elbow_handle_points(element, SEGMENT_MIN_PX / self.camera.scale);
        }
        if self.shows_point_handles(element) {
            return crate::selection::linear::handle_points(
                element,
                super::LINEAR_MIDPOINT_MIN_PX / self.camera.scale,
            );
        }
        let linear = matches!(
            element.kind,
            crate::scene::DrawElementType::Line | crate::scene::DrawElementType::Arrow
        );
        if !linear || element.locked() {
            return Vec::new();
        }
        // No segment is long enough for a midpoint, which leaves the points.
        crate::selection::linear::handle_points(element, f64::MAX)
    }
}
