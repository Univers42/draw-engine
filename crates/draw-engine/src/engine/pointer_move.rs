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
            return;
        };
        let world = self.screen_to_world(sx, sy);
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
            Interaction::Draft { ref id, start } => {
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
            Interaction::Freedraw { ref id, start } => {
                if let Some(mut element) = self.scene.get(id).cloned() {
                    if let Some(mut points) = element.points.clone() {
                        points.push([world.x - start.x, world.y - start.y]);
                        element.points = Some(points);
                        self.scene.put(element);
                        self.request_draw();
                    }
                }
                Some(it)
            }
            Interaction::Erase => {
                self.erase_at(sx, sy);
                Some(it)
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
            } => {
                self.move_resize(id, handle, world, square, ratio);
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
        let over = bindable_among(
            self.scene.iter_ordered().rev(),
            world.x,
            world.y,
            tolerance,
            Some(id),
        )
        .map(|el| el.id.clone());

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
        if !bypass_snap {
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

    fn move_resize(
        &mut self,
        id: &str,
        handle: HandleKind,
        world: Point,
        square: bool,
        ratio: Option<f64>,
    ) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let geom = resize_element(
            &element,
            handle,
            world.x,
            world.y,
            1.0,
            if square { ratio } else { None },
        );
        element.x = geom.x;
        element.y = geom.y;
        element.width = geom.width;
        element.height = geom.height;
        self.scene.put(element);
        self.apply_bindings();
        self.request_draw();
    }
}
