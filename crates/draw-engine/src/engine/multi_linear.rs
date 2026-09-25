//! Placing a line or an arrow point by point.
//!
//! A linear tool offers two gestures, and which one is happening is only knowable on
//! release. **Drag** — press, pull, let go — draws one segment and ends; that is
//! [`super::pointer_end::DrawEngine::end_linear`]'s business and it has not changed.
//! **Click** starts a *path*, which is what this module is: the element stays live
//! between gestures, a preview segment follows the cursor, every click fixes another
//! point, and a click back on the point just placed — or back on the very first one —
//! ends it.
//!
//! Transcribed from Excalidraw's `multiElement` handling (`App.tsx@1118751f:7981-8066` for the
//! preview, `10108-10215` for the press, `11705-11745` for the fork on release,
//! `actionFinalize.tsx@1118751f:250-325` for the end) at the SHA pinned in
//! `scripts/oracle-sha.txt`.
//!
//! # The preview point
//!
//! The path carries at most one *uncommitted* point, which sits at the end of the list
//! and tracks the cursor. [`MultiLinear::committed`](super::types::MultiLinear) counts
//! the points that were actually placed, so the preview — when there is one — is always
//! at exactly that index. Counting rather than holding the point is what makes "throw the
//! preview away" a `truncate`, with no arithmetic that could drop a point somebody meant.
//!
//! The preview only exists once the cursor has left the last point's commit zone. Inside
//! that radius a click means *finish*, so a segment growing there would have the gesture
//! that ends a path also extending it — and you could never stop.

use crate::camera::Point;
use crate::engine::types::MultiLinear;
use crate::engine::{DrawEngine, Interaction};
use crate::interaction::constrain_to_angle;
use crate::scene::binding::{anchor, is_inside, set_anchor, Anchor, End};
use crate::scene::{
    bump_version, is_path_a_loop_within, is_valid_polygon, DrawElement, DrawElementType,
    LINE_CONFIRM_THRESHOLD,
};

impl DrawEngine {
    /// The id of the path being placed, if one is.
    ///
    /// Public because the host needs it for more than curiosity: while a path is open the
    /// canvas has to keep receiving pointer moves with no button held, which is not
    /// otherwise true, and Escape and Enter have to mean *finish this* rather than
    /// whatever they mean the rest of the time.
    pub fn linear_in_progress(&self) -> Option<String> {
        self.multi_linear.as_ref().map(|state| state.id.clone())
    }

    /// Ends the path being placed, keeping it.
    ///
    /// Escape's binding, and Enter's. Deliberately **not** a discard: Excalidraw binds
    /// both keys to `actionFinalize`, and the difference matters because a path of six
    /// points thrown away by a reflexive Escape is six points of work gone. Cancelling is
    /// what undo is for.
    pub fn finish_linear(&mut self) {
        self.finish_linear_step();
        self.refresh_live();
    }

    fn finish_linear_step(&mut self) {
        if self.finish_multi_linear() {
            self.settle_tool();
        }
    }

    /// Turns the element a click just left behind into a path waiting for its next point.
    ///
    /// Called from `end_linear` once the release has shown the gesture was a click. The
    /// drag preview is cut back to the single point that was pressed — the second point
    /// `begin_linear` seeds is there for the drag case, and keeping it would start every
    /// path with a zero-length segment.
    pub(crate) fn begin_multi_linear(&mut self, id: &str) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        element.points = Some(vec![[0.0, 0.0]]);
        element.width = 0.0;
        element.height = 0.0;
        self.scene.put(element);
        self.multi_linear = Some(MultiLinear {
            id: id.to_string(),
            committed: 1,
        });
        self.request_draw();
    }

    /// A press while a path is open: either it ends the path, or it places a point.
    pub(crate) fn press_multi_linear(&mut self, world: Point, screen: Point) {
        let Some(state) = self.multi_linear.clone() else {
            return;
        };
        let Some(element) = self.scene.get(&state.id).cloned() else {
            // The path's element is gone — erased, or undone out from under it. Drop the
            // state rather than carrying an id that resolves to nothing.
            self.multi_linear = None;
            return;
        };
        let points = element.points.clone().unwrap_or_default();
        let local = [world.x - element.x, world.y - element.y];
        let tolerance = self.confirm_tolerance();

        // Back on the first point: the path has closed on itself. Only a line closes —
        // an arrow that met its own tail would be a loop with a head on it, and
        // Excalidraw guards the same way (`App.tsx@1118751f:10125`).
        if element.kind == DrawElementType::Line && is_path_a_loop_within(&points, tolerance) {
            // Commit the preview before finishing, so the closing point survives the
            // trim. Excalidraw does this explicitly for the same reason
            // (`App.tsx@1118751f:10128-10142`).
            self.commit_multi_point();
            self.finish_by_press(&state.id, screen);
            return;
        }

        // Back on the point just placed: that is how a path ends, and it is what the
        // second click of a double click lands on.
        //
        // One divergence from the oracle, which requires `points.length > 1` here
        // (`App.tsx@1118751f:10211`): that guard makes a second click in the same spot do nothing
        // at all, so a double click on an empty board leaves a one-point path live and
        // invisible, and every later click extends it from there. Allowing it at one
        // point costs nothing — the only way to be inside the commit zone with no
        // preview out is to have clicked twice without moving — and `finish` throws a
        // path of fewer than two points away, which is the outcome anyone doing that
        // wanted.
        if state.committed >= 1 {
            if let Some(&last) = points.get(state.committed - 1) {
                if distance(local, last) < tolerance {
                    self.finish_by_press(&state.id, screen);
                    return;
                }
            }
        }

        // Clicking beside a shape the pending point would bind to ends the path there,
        // the same as clicking back on the point just placed: a bind is as deliberate a
        // "done" as a close is. Checked after the click-back case, not before, so
        // re-clicking the same spot still ends the path through that simpler path.
        //
        // Only *beside* one, as in Excalidraw (`boundOutsideFromElsewhere` and
        // `endOutsideSameElement`, `App.tsx@1118751f:10189-10215`): a click inside a
        // shape places a waypoint there, so a path can be routed across shapes — and
        // beside the shape the path started from only when it came back from outside.
        //
        // **Deliberate divergence:** the press is judged where it is, the point the hover
        // just judged for its outline. Excalidraw judges it at its preview point instead
        // (`multiElement.points[last]`, `App.tsx@1118751f:10170`), which its hover has
        // already moved onto the outline gap of the shape it shows; re-tested there, the
        // point often falls inside another shape, and a press made under an orbit outline
        // placed a waypoint — 13 of 48 probes in a Ctrl+D pack on excalidraw.com. Here
        // what the outline shows is what the press does.
        if self.ends_path_at(&element, world) {
            // Same as landing back on the point just placed (above): whatever a hover
            // would have put there, this press puts there too, so the commit below has a
            // real point to fix rather than whatever was left over from before the press
            // arrived — a click with no preceding move at this exact spot is not
            // something the engine can tell apart from one that had it, but it must not
            // matter to either.
            self.track_multi_linear(world, false);
            self.commit_multi_point();
            self.finish_by_press(&state.id, screen);
            return;
        }

        // Otherwise the press is placing a point, and the release is what places it.
        self.interaction = Some(Interaction::MultiLinearPress);
    }

    /// Ends the path because a press at `screen` said so, remembering which path it was:
    /// the press may be either half of a double click, whose `dblclick` arrives next and
    /// is about this path, not about whatever lies under the pointer. Remembered after
    /// the finish, which settles the tool — and a tool change forgets it.
    fn finish_by_press(&mut self, id: &str, screen: Point) {
        self.finish_linear();
        self.finished_by_press = Some((id.to_string(), screen));
    }

    /// What the path's pending point would bind to at `world`: its own anchor and, when
    /// it changes, the start's. Shared by the live highlight, the click-to-bind-and-finish
    /// check, and the end committed at finish, so the three cannot drift apart on what
    /// counts as "close enough".
    fn end_binding_at(
        &self,
        element: &DrawElement,
        world: Point,
    ) -> (Option<Anchor>, Option<Anchor>) {
        self.drop_binding(element, End::End, world, false)
    }

    /// Whether a click at `world` binds the path's end and so finishes it: in orbit round
    /// a shape, or beside — not inside — the shape the path started from.
    fn ends_path_at(&self, element: &DrawElement, world: Point) -> bool {
        let Some(end) = self.end_binding_at(element, world).0 else {
            return false;
        };
        let Some(shape) = self.scene.get(&end.element_id) else {
            return false;
        };
        let from_start =
            anchor(element, End::Start).is_some_and(|a| a.element_id == end.element_id);
        !is_inside(shape, world) && (end.mode == crate::scene::BindMode::Orbit || from_start)
    }

    /// Fixes the preview point in place, so the next move starts a new segment from it.
    pub(crate) fn commit_multi_point(&mut self) {
        let Some(state) = self.multi_linear.as_ref() else {
            return;
        };
        let Some(element) = self.scene.get(&state.id) else {
            self.multi_linear = None;
            return;
        };
        let placed = element
            .points
            .as_ref()
            .map_or(1, |points| points.len())
            .max(1);
        if let Some(state) = self.multi_linear.as_mut() {
            state.committed = placed;
        }
        self.request_draw();
    }

    /// The rubber segment between the last placed point and the cursor.
    ///
    /// Runs on pointer moves with **no button held**, which is the only gesture in the
    /// engine that does.
    pub(crate) fn track_multi_linear(&mut self, world: Point, constrain: bool) {
        let Some(state) = self.multi_linear.clone() else {
            return;
        };
        let Some(mut element) = self.scene.get(&state.id).cloned() else {
            self.multi_linear = None;
            return;
        };

        // Live suggestion for the point about to be placed — the painter outlines
        // whatever shape a click would attach to, the same as a dragged arrow's
        // endpoint (`move_linear`). Visual only: nothing here touches the element,
        // since the pending point is not necessarily the path's actual end until a
        // click on it (`press_multi_linear`) or `finish_multi_linear` says so.
        let end = self.end_binding_at(&element, world).0;
        self.binding_snaps = Some(self.drop_snaps(end.as_ref(), world));
        self.binding_highlight = end.map(|a| a.element_id);
        self.binding_point = self.binding_highlight.as_ref().map(|_| world);

        let Some(mut points) = element.points.clone() else {
            return;
        };
        // Clamped rather than trusted: an undo can shorten the list under the count.
        let committed = state.committed.clamp(1, points.len());
        let anchor = points[committed - 1];

        let mut local = [world.x - element.x, world.y - element.y];
        if constrain {
            // Shift holds the segment to an angle, exactly as it does for a dragged one —
            // measured from the point it starts at, not from the element's origin, or
            // every segment after the first would snap about the wrong centre.
            let (dx, dy) = constrain_to_angle(local[0] - anchor[0], local[1] - anchor[1]);
            local = [anchor[0] + dx, anchor[1] + dy];
        }
        let near_anchor = distance(local, anchor) < self.confirm_tolerance();

        if points.len() == committed {
            // No preview yet. One appears as soon as the cursor leaves the commit zone.
            if near_anchor {
                return;
            }
            points.push(local);
        } else if near_anchor && committed >= 2 {
            // Back inside the commit zone with a preview out: take it away again, so a
            // segment can be backed out of without being placed.
            points.truncate(committed);
        } else {
            points[committed] = local;
            points.truncate(committed + 1);
        }

        element.points = Some(points);
        reseat_points(&mut element);
        self.scene.put(element);
        self.request_draw();
    }

    /// Ends the open path, if there is one. Returns whether there was.
    ///
    /// Split from [`Self::finish_linear`] so that `set_tool` can end a path without
    /// recursing back through `settle_tool` — the caller there is already choosing a tool.
    pub(crate) fn finish_multi_linear(&mut self) -> bool {
        let Some(state) = self.multi_linear.take() else {
            return false;
        };
        // The press that ended the path has no release to wait for.
        if matches!(self.interaction, Some(Interaction::MultiLinearPress)) {
            self.interaction = None;
        }
        let Some(mut element) = self.scene.get(&state.id).cloned() else {
            return false;
        };
        let mut points = element.points.clone().unwrap_or_default();

        // The preview followed the cursor and was never placed.
        if points.len() > state.committed {
            points.truncate(state.committed);
        }

        // One point is not a path. This is what a double click on an empty board leaves,
        // and an invisible one-point element would be unselectable and permanent.
        if points.len() < 2 {
            self.scene.discard(&state.id);
            self.request_draw();
            return true;
        }

        // A loop is shut exactly, not to within a few pixels: a gap of three pixels at
        // this zoom is three hundred at the next one, and the shape comes apart. Finishing
        // a line on its own first point is what turns it into a filled polygon —
        // `actionFinalize.tsx@1118751f:310-334` sets both `points` and `polygon: true` in
        // the same mutation, for the same reason: a loop that closed but stayed unfilled
        // would look identical to one a stray click almost closed.
        if element.kind == DrawElementType::Line
            && is_path_a_loop_within(&points, self.confirm_tolerance())
        {
            let first = points[0];
            *points.last_mut().expect("checked non-empty") = first;
            element.polygon = Some(true);
        }
        // A two-vertex loop — closed back onto itself one click after the start — is a
        // segment, not a shape: nothing it could enclose. `isValidPolygon`
        // (`typeChecks.ts@1118751f:391-401`) requires three, so the `true` just set above
        // is undone rather than left to paint a fill with no area.
        if element.kind == DrawElementType::Line && !is_valid_polygon(&points) {
            element.polygon = Some(false);
        }

        element.points = Some(points);
        reseat_points(&mut element);

        // The path's real end is only known now — unlike a drag, where the point being
        // moved always *is* the end, a waypoint placed mid-path is not. Evaluated once,
        // here, rather than on every click — except when `press_multi_linear` already
        // decided the last point was a bind and finished on the strength of it, where
        // this just confirms the same answer again. The start was bound by `begin_linear`
        // on the very first press, and changes only if the end comes back to its shape.
        if let Some(&last) = element.points.as_ref().and_then(|points| points.last()) {
            let end = Point {
                x: element.x + last[0],
                y: element.y + last[1],
            };
            let (this, start) = self.end_binding_at(&element, end);
            set_anchor(&mut element, End::End, this);
            if let Some(start) = start {
                set_anchor(&mut element, End::Start, Some(start));
            }
        }
        self.clear_binding_suggestion();

        self.scene.put(bump_version(element, self.now_ms));
        self.apply_bindings();
        self.set_selection(vec![state.id]);
        // One history entry for the whole path. Recording every click would make undoing
        // a six-point path six keystrokes, through five intermediate shapes nobody chose.
        self.push_history();
        self.request_draw();
        true
    }

    /// [`LINE_CONFIRM_THRESHOLD`] in world units.
    ///
    /// The same constant three other things already turn on — whether a path paints its
    /// background, whether a fill treats it as a wall, whether a click inside it belongs
    /// to it — divided by the zoom, as Excalidraw divides it (`utils.ts@1118751f:521-523`). The
    /// radius is about how accurately a hand can aim, which is a fact about the screen:
    /// left in world units it would be a tenth of a shape when zoomed in and would
    /// swallow the whole drawing when zoomed out.
    fn confirm_tolerance(&self) -> f64 {
        LINE_CONFIRM_THRESHOLD / self.camera.scale
    }
}

/// Whether a click at `(sx, sy)` on screen lands near enough to one at `at` to be the
/// other half of its double click.
pub(crate) fn is_double_tap(at: Point, sx: f64, sy: f64) -> bool {
    (at.x - sx).hypot(at.y - sy) <= super::DOUBLE_TAP_PX
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Re-seats a path's origin on its own points.
///
/// Points are stored relative to the element, and a path placed point by point can grow
/// in any direction — including back past where it started. Left alone the origin stays
/// at the first click and the extent describes only the first segment, so the element's
/// box stops containing the drawing: the marquee misses it, the eraser sweeps past it and
/// the exporter crops it.
///
/// World positions are preserved exactly — every point moves by the same amount the
/// origin does, in the opposite direction.
fn reseat_points(element: &mut DrawElement) {
    let Some(points) = element.points.as_mut() else {
        return;
    };
    if points.is_empty() {
        return;
    }
    let [min_x, min_y, max_x, max_y] = crate::freehand::points_bounds(points);
    for point in points.iter_mut() {
        point[0] -= min_x;
        point[1] -= min_y;
    }
    element.x += min_x;
    element.y += min_y;
    element.width = max_x - min_x;
    element.height = max_y - min_y;
}
