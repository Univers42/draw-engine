use crate::camera::Point;
use crate::engine::{DrawEngine, Interaction};
use crate::interaction::constrain_to_angle;
use crate::interaction::{linear_from_drag, rect_from_drag, snap_move};
use crate::scene::binding::bindable_among;
use crate::scene::{scene_bounds, DrawElement};
use crate::selection::{resize_element, rotate_element, HandleKind};

impl DrawEngine {
    pub fn move_pointer(&mut self, sx: f64, sy: f64, square: bool, bypass_snap: bool) {
        let Some(it) = self.interaction.take() else {
            // No gesture in progress — but a path being placed point by point still
            // follows the cursor between its clicks, which is the whole of how it is
            // aimed. It is the only thing in the engine that moves with no button held.
            if self.multi_linear.is_some() {
                let world = self.snap(self.screen_to_world(sx, sy));
                self.track_multi_linear(world, square);
            }
            return;
        };
        let world = self.snap(self.screen_to_world(sx, sy));
        let next = self.advance_interaction(it, sx, sy, world, square, bypass_snap);
        self.interaction = next;
    }

    fn advance_interaction(
        &mut self,
        it: Interaction,
        sx: f64,
        sy: f64,
        world: Point,
        square: bool,
        bypass_snap: bool,
    ) -> Option<Interaction> {
        match it {
            // Same rubber-band as a shape: the box you drag out is the column you get.
            Interaction::TextDraft { ref id, start } | Interaction::Draft { ref id, start } => {
                if let Some(mut element) = self.scene.get(id).cloned() {
                    let rect = rect_from_drag(start.x, start.y, world.x, world.y, square);
                    element.x = rect.x;
                    element.y = rect.y;
                    element.width = rect.width;
                    element.height = rect.height;
                    self.scene.put(element);
                    self.request_draw();
                }
                Some(it)
            }
            Interaction::ResizeGroup {
                ref ids,
                handle,
                ref frame,
            } => {
                let elements: Vec<DrawElement> = ids
                    .iter()
                    .filter_map(|id| self.scene.get(id).cloned())
                    .collect();
                for next in crate::selection::group_transform::resize_group(
                    &elements, frame, handle, world, square,
                ) {
                    self.scene.put(next);
                }
                // Bound arrows have to keep up with the shapes they point at. Without
                // this they held their old attachment for the whole gesture and jumped
                // only on release, so a group could be scaled while its arrows sat
                // detached in mid-air.
                self.apply_bindings();
                self.request_draw();
                Some(it)
            }
            Interaction::RotateGroup { ref ids, ref frame } => {
                let elements: Vec<DrawElement> = ids
                    .iter()
                    .filter_map(|id| self.scene.get(id).cloned())
                    .collect();
                for next in crate::selection::group_transform::rotate_group(&elements, frame, world)
                {
                    self.scene.put(next);
                }
                self.apply_bindings();
                self.request_draw();
                Some(it)
            }
            Interaction::LinearPoint { ref id, handle } => {
                self.move_linear_point(id, handle, world, square);
                // A midpoint drag inserts a point and then *becomes* a drag of that new
                // point, or the next pointer move would insert another one.
                let next = match handle {
                    crate::selection::LinearHandle::Midpoint(i) => {
                        crate::selection::LinearHandle::Point(i + 1)
                    }
                    point => point,
                };
                Some(Interaction::LinearPoint {
                    id: id.clone(),
                    handle: next,
                })
            }
            Interaction::Linear { ref id, start } => {
                self.move_linear(id, start, world, square);
                Some(it)
            }
            // Dragging with the button still down after a press that placed a point: the
            // preview keeps tracking, so a point can be nudged before the release fixes
            // it rather than having to be placed and then dragged again.
            Interaction::MultiLinearPress => {
                self.track_multi_linear(world, square);
                Some(it)
            }
            Interaction::Freedraw { ref id, start } => {
                if let Some(mut element) = self.scene.get(id).cloned() {
                    if let Some(mut points) = element.points.clone() {
                        // Streamlined as it arrives rather than smoothed afterwards, so
                        // what is stored is what is drawn and what is exported — a
                        // stroke smoothed only in the painter would change shape the
                        // moment it was saved and reloaded.
                        let raw = [world.x - start.x, world.y - start.y];
                        let next = match points.last() {
                            Some(&previous) => crate::freehand::streamline(
                                previous,
                                raw,
                                crate::freehand::STREAMLINE,
                            ),
                            None => raw,
                        };
                        points.push(next);
                        element.points = Some(points);
                        self.scene.put(element);
                        self.request_draw();
                    }
                }
                Some(it)
            }
            Interaction::Erase { last } => {
                let at = self.screen_to_world(sx, sy);
                self.erase_along(last, at);
                Some(Interaction::Erase { last: at })
            }
            Interaction::Pan { last_x, last_y } => {
                self.pan_by(sx - last_x, sy - last_y);
                Some(Interaction::Pan {
                    last_x: sx,
                    last_y: sy,
                })
            }
            Interaction::Move { .. } => Some(self.move_selection(it, world, bypass_snap)),
            Interaction::Resize {
                ref id,
                handle,
                ratio,
                origin,
                ref origin_points,
            } => {
                let points = origin_points.clone();
                self.move_resize(
                    id,
                    world,
                    square,
                    ResizeDrag {
                        handle,
                        ratio,
                        origin,
                        origin_points: points.as_deref(),
                    },
                );
                Some(it)
            }
            Interaction::Rotate { ref id } => {
                if let Some(mut element) = self.scene.get(id).cloned() {
                    element.angle = rotate_element(&element, world.x, world.y);
                    self.scene.put(element);
                    // Turning a shape moves the perimeter its arrows attach to.
                    self.apply_bindings();
                    self.request_draw();
                }
                Some(it)
            }
            Interaction::Lasso { mut path, base } => {
                // Sub-pixel moves add nothing the simplifier would keep, and a
                // high-polling pointer emits a great many of them.
                let keep = path
                    .last()
                    .is_none_or(|last| (last.x - world.x).hypot(last.y - world.y) >= 0.5);
                if keep {
                    path.push(world);
                }
                self.request_draw();
                Some(Interaction::Lasso { path, base })
            }
            Interaction::Laser => {
                // Unsnapped, and every sample kept: the trail's smoothing wants the raw
                // pointer stream, and thinning it here would flatten the curves the
                // streamlining exists to produce.
                let exact = self.screen_to_world(sx, sy);
                self.laser.add(exact.x, exact.y, self.now_ms);
                self.request_draw();
                Some(Interaction::Laser)
            }
            Interaction::Marquee { start, base, .. } => Some(Interaction::Marquee {
                start,
                current: world,
                base,
            }),
        }
    }

    /// Moves one point of a line or arrow to the pointer.
    ///
    /// Shift constrains the segment leading into the point to 45 degree steps, matching
    /// what drawing one does — so an arrow can be made exactly horizontal after the
    /// fact, not only while it is first drawn.
    fn move_linear_point(
        &mut self,
        id: &str,
        handle: crate::selection::LinearHandle,
        world: Point,
        square: bool,
    ) {
        let Some(element) = self.scene.get(id).cloned() else {
            return;
        };

        let target = if square {
            self.constrain_point(&element, handle, world)
        } else {
            world
        };

        let moved = crate::selection::linear::move_handle(&element, handle, target);

        // Re-resolve the binding for whichever end moved, so dropping an endpoint on a
        // shape attaches it and dragging it away releases it.
        let moved = self.rebind_endpoint(moved, handle);

        self.scene.put(moved);
        self.refresh_binding_highlight(handle, target);
        self.request_draw();
    }

    /// Quantises the dragged point to 45 degrees from its neighbour.
    fn constrain_point(
        &self,
        element: &crate::scene::DrawElement,
        handle: crate::selection::LinearHandle,
        world: Point,
    ) -> Point {
        let points = crate::selection::linear::world_points(element);
        let anchor = match handle {
            crate::selection::LinearHandle::Point(0) => points.get(1),
            crate::selection::LinearHandle::Point(i) => points.get(i.wrapping_sub(1)),
            crate::selection::LinearHandle::Midpoint(i) => points.get(i),
        };
        let Some(anchor) = anchor else {
            return world;
        };

        let (dx, dy) = constrain_to_angle(world.x - anchor.x, world.y - anchor.y);
        Point {
            x: anchor.x + dx,
            y: anchor.y + dy,
        }
    }

    /// Attaches or releases a binding for whichever end just moved.
    ///
    /// Only the two ends bind; a point in the middle of a path is not an endpoint and
    /// has nothing to attach to. Dragging an end onto a shape binds it, dragging it
    /// clear releases it — so the gesture is reversible, which the previous
    /// bind-on-create-only behaviour was not.
    fn rebind_endpoint(
        &self,
        mut element: crate::scene::DrawElement,
        handle: crate::selection::LinearHandle,
    ) -> crate::scene::DrawElement {
        let points = element.points.as_ref().map(Vec::len).unwrap_or(0);
        let which = match handle {
            crate::selection::LinearHandle::Point(0) => Some(false),
            crate::selection::LinearHandle::Point(i) if i + 1 == points => Some(true),
            _ => None,
        };
        let Some(is_end) = which else {
            return element;
        };

        let world = crate::selection::linear::world_points(&element);
        let Some(tip) = (if is_end { world.last() } else { world.first() }) else {
            return element;
        };

        // Walked by reference. This used to clone the whole document, and so did the
        // highlight refresh immediately after it — two deep copies of every element on
        // the board for every single pointer move while dragging one endpoint.
        let tolerance = self.binding_tolerance();
        let target = crate::scene::binding::bindable_among(
            self.scene.iter_ordered().rev(),
            tip.x,
            tip.y,
            tolerance,
            Some(&element.id),
        )
        .map(|shape| shape.id.clone());

        if is_end {
            element.end_binding = target;
        } else {
            element.start_binding = target;
        }
        element
    }

    /// How far from a shape an endpoint may be dropped and still attach.
    ///
    /// A fixed world tolerance would shrink to nothing when zoomed out, so it is
    /// derived from screen pixels. Excalidraw does the same — you do not have to land
    /// *inside* a shape to bind to it, only near it, which is the difference between
    /// binding feeling helpful and feeling fiddly.
    pub(crate) fn binding_tolerance(&self) -> f64 {
        super::BINDING_HOVER_PX / self.camera.scale
    }

    /// Records which shape, if any, the dragged endpoint would bind to.
    ///
    /// The painter reads this to outline that shape, so you can see the attachment
    /// before you commit to it.
    fn refresh_binding_highlight(&mut self, handle: crate::selection::LinearHandle, at: Point) {
        let is_endpoint = matches!(handle, crate::selection::LinearHandle::Point(_));
        if !is_endpoint {
            self.binding_highlight = None;
            return;
        }
        let tolerance = self.binding_tolerance();
        self.binding_highlight = crate::scene::binding::bindable_among(
            self.scene.iter_ordered().rev(),
            at.x,
            at.y,
            tolerance,
            None,
        )
        .map(|shape| shape.id.clone());
    }

    /// Extends the arrow or line currently being drawn to the pointer.
    ///
    /// The endpoint binds on the same terms as one dragged later: within
    /// [`Self::binding_tolerance`] of a shape, not strictly inside it. It used to require
    /// a tolerance of zero here and a generous one everywhere else, so an arrow drawn
    /// onto a shape refused to attach while the identical gesture performed a moment
    /// later did. The shape it would attach to is recorded for the painter, so the
    /// perimeter lights up **while** the arrow is being drawn, not only afterwards.
    fn move_linear(&mut self, id: &str, start: Point, world: Point, square: bool) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let tolerance = self.binding_tolerance();
        // Only an arrow looks for something to attach to. A line reaching the edge of a
        // shape means nothing more than a line reaching that spot.
        let over = crate::scene::binding::is_binding_element(&element)
            .then(|| {
                bindable_among(
                    self.scene.iter_ordered().rev(),
                    world.x,
                    world.y,
                    tolerance,
                    Some(id),
                )
                .map(|el| el.id.clone())
            })
            .flatten();

        let drag = linear_from_drag(start.x, start.y, world.x, world.y, square);
        element.x = drag.x;
        element.y = drag.y;
        element.width = drag.width;
        element.height = drag.height;
        element.points = Some(drag.points);
        element.end_binding = over.filter(|hit| Some(hit) != element.start_binding.as_ref());
        self.binding_highlight = element.end_binding.clone();
        self.scene.put(element);
        self.request_draw();
    }

    fn move_selection(&mut self, it: Interaction, world: Point, bypass_snap: bool) -> Interaction {
        let Interaction::Move {
            ids,
            start,
            origins,
            static_bounds,
        } = it
        else {
            return it;
        };
        let mut dx = world.x - start.x;
        let mut dy = world.y - start.y;
        self.snap_guides.clear();
        // Alignment guides pull toward other elements' edges, which is a different answer
        // from the grid's. Running both makes the result depend on which won by a pixel,
        // so the grid takes precedence while it is snapping.
        let grid_snapping = self.grid().enabled && self.grid().snap;
        if !bypass_snap && !grid_snapping {
            let moving: Vec<DrawElement> = ids
                .iter()
                .filter_map(|id| self.scene.get(id).cloned())
                .map(|mut el| {
                    if let Some(origin) = origins.get(&el.id) {
                        el.x = origin.x + dx;
                        el.y = origin.y + dy;
                    }
                    el
                })
                .collect();
            if let Some(bounds) = scene_bounds(&moving) {
                let snap = snap_move(bounds, &static_bounds, super::SNAP_PX / self.camera.scale);
                dx += snap.dx;
                dy += snap.dy;
                self.snap_guides = snap.guides;
            }
        }
        for id in &ids {
            if let (Some(mut element), Some(origin)) =
                (self.scene.get(id).cloned(), origins.get(id))
            {
                element.x = origin.x + dx;
                element.y = origin.y + dy;
                self.scene.put(element);
            }
        }
        self.apply_bindings();
        self.request_draw();
        Interaction::Move {
            ids,
            start,
            origins,
            static_bounds,
        }
    }

    fn move_resize(&mut self, id: &str, world: Point, square: bool, drag: ResizeDrag<'_>) {
        let ResizeDrag {
            handle,
            ratio,
            origin,
            origin_points,
        } = drag;
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        // Measured from where the element was when the drag started, so the anchor is
        // fixed for the whole gesture. Reading the live element instead let the anchor
        // follow the pointer the moment the element turned through it.
        let mut from = element.clone();
        from.x = origin.x;
        from.y = origin.y;
        from.width = origin.width;
        from.height = origin.height;
        // Images hold their proportions unless Shift is held; every other shape is the
        // other way round. A photograph stretched by accident is a mistake you often do
        // not notice until much later.
        let lock = crate::scene::locks_aspect_ratio(&from, square);
        let geom = resize_element(
            &from,
            handle,
            world.x,
            world.y,
            1.0,
            if lock { ratio } else { None },
        );
        element.x = geom.x;
        element.y = geom.y;
        element.width = geom.width;
        element.height = geom.height;
        // A shape is generated into its box, so the box is the whole story. A path *is*
        // its points, so the box on its own moves nothing that is drawn — the ring has to
        // be scaled to match, from the ring the drag started with.
        if let Some(points) = origin_points {
            scale_ring(&mut element, points, &geom);
        }
        self.scene.put(element);
        self.apply_bindings();
        self.request_draw();
    }
}

/// Everything a resize drag remembers from the moment it began.
///
/// Grouped rather than passed one by one because they are one thing — the state of a
/// gesture in progress — and because every one of them exists for the same reason: a
/// resize must be measured from where the element *was*, never from where it has got to.
struct ResizeDrag<'a> {
    handle: HandleKind,
    ratio: Option<f64>,
    origin: crate::selection::Geometry,
    origin_points: Option<&'a [[f64; 2]]>,
}

/// Scale a path's points so its ring follows the box the handle just dragged.
///
/// The awkward part is the anchor. A point-based element stores `points[0]` at `[0, 0]`
/// and puts `x`/`y` where that first point sits, so the ring's own box is offset from the
/// element's origin by however far the first point is from the ring's corner — and for a
/// bucket fill that offset is whatever vertex the region walk happened to start on. So
/// the points are scaled about `points[0]`, and then the origin is moved by exactly the
/// amount that scaling shifted the ring's corner, which puts the corner back where the
/// handle asked for it.
fn scale_ring(
    element: &mut DrawElement,
    origin_points: &[[f64; 2]],
    geom: &crate::selection::Geometry,
) {
    if origin_points.is_empty() {
        return;
    }
    // Measured from the ring the drag *started* with, never from the live one. The live
    // ring has already been scaled by every earlier move of this same gesture, so a scale
    // derived from it and then applied to the original points compounds: the second move
    // divides by a span the first move had already stretched, and the ring drifts away
    // from the box under the hand. A single-step drag cannot see this, which is why the
    // first test of it passed.
    let before = ring_box(origin_points);
    // A ring with no extent on an axis has no scale to speak of on it: leave it alone
    // rather than divide by zero and send every point to infinity.
    let sx = if before.width.abs() > f64::EPSILON {
        geom.width / before.width
    } else {
        1.0
    };
    let sy = if before.height.abs() > f64::EPSILON {
        geom.height / before.height
    } else {
        1.0
    };
    element.points = Some(
        origin_points
            .iter()
            .map(|p| [p[0] * sx, p[1] * sy])
            .collect(),
    );
    element.x = geom.x - before.x * sx;
    element.y = geom.y - before.y * sy;
    element.width = geom.width;
    element.height = geom.height;
}

/// The box a ring covers, relative to its own first point.
fn ring_box(points: &[[f64; 2]]) -> crate::scene::geometry::Rect {
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    crate::scene::geometry::Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}
