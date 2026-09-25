use crate::camera::Point;
use crate::engine::{DrawEngine, Interaction};
use crate::interaction::constrain_to_angle;
use crate::interaction::{linear_from_drag, rect_from_drag, snap_move};
use crate::scene::binding::{anchor, anchor_for_drop, set_anchor, Anchor, End, EndDrop};
use crate::scene::{scene_bounds, DrawElement, DrawElementType};
use crate::selection::{resize_element_within, rotate_element, HandleKind};

impl DrawEngine {
    /// `invert_snap` is the host's Ctrl/Cmd: it flips object snapping for this move, on
    /// when it is off and off when it is on, exactly as Excalidraw's `isSnappingEnabled`.
    pub fn move_pointer(&mut self, sx: f64, sy: f64, square: bool, invert_snap: bool) {
        self.move_pointer_step(sx, sy, square, invert_snap);
        self.refresh_live();
    }

    fn move_pointer_step(&mut self, sx: f64, sy: f64, square: bool, invert_snap: bool) {
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
        let next = self.advance_interaction(it, sx, sy, world, square, invert_snap);
        self.interaction = next;
    }

    fn advance_interaction(
        &mut self,
        it: Interaction,
        sx: f64,
        sy: f64,
        world: Point,
        square: bool,
        invert_snap: bool,
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
                grab,
            } => {
                let at = self.resize_pointer(sx, sy, grab);
                let elements: Vec<DrawElement> = ids
                    .iter()
                    .filter_map(|id| self.scene.get(id).cloned())
                    .collect();
                let resized = crate::selection::group_transform::resize_group(
                    &elements, frame, handle, at, square,
                );
                let (labels, members): (Vec<DrawElement>, Vec<DrawElement>) = resized
                    .into_iter()
                    .partition(|el| el.container_id.is_some() && el.kind == DrawElementType::Text);
                for next in members {
                    self.scene.put(next);
                }
                let flips =
                    crate::selection::group_transform::resize_group_flips(handle, frame, at);
                for label in labels {
                    self.relay_resized_label(label, handle, flips);
                }
                self.release_ends_outside(ids);
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
                self.release_ends_outside(ids);
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
            Interaction::Linear { ref id, start, .. } => {
                self.move_linear(id, start, world, square);
                Some(Interaction::Linear {
                    id: id.clone(),
                    start,
                    pointer: world,
                })
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
                self.mark_along(last, at);
                Some(Interaction::Erase { last: at })
            }
            Interaction::Pan { last_x, last_y } => {
                self.pan_by(sx - last_x, sy - last_y);
                Some(Interaction::Pan {
                    last_x: sx,
                    last_y: sy,
                })
            }
            Interaction::Move { .. } => Some(self.move_selection(it, world, invert_snap)),
            Interaction::CornerRadius {
                ref id,
                corner,
                start_radius,
                grab,
            } => {
                self.move_corner_radius(id, corner, start_radius, grab, world);
                Some(it)
            }
            Interaction::Resize {
                ref id,
                handle,
                ratio,
                origin,
                ref origin_points,
                grab,
                ref label_font,
            } => {
                let points = origin_points.clone();
                let at = self.resize_pointer(sx, sy, grab);
                self.move_resize(
                    id,
                    at,
                    square,
                    ResizeDrag {
                        handle,
                        ratio,
                        origin,
                        origin_points: points.as_deref(),
                        label_font: label_font.as_ref(),
                    },
                );
                Some(it)
            }
            Interaction::Rotate { ref id } => {
                if let Some(mut element) = self.scene.get(id).cloned() {
                    element.angle = rotate_element(&element, world.x, world.y);
                    // An arrow turned on its own lets go of both ends, as in Excalidraw
                    // (`packages/element/src/resizeElements.ts:241-252`): its ends are being
                    // placed by the turn, and holding them to shapes would fight it.
                    if crate::scene::binding::is_binding_element(&element) {
                        set_anchor(&mut element, End::Start, None);
                        set_anchor(&mut element, End::End, None);
                    }
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
            Interaction::Marquee { start, base, .. } => {
                // The rubber band is chrome: nothing in the scene changes as it grows, so
                // nothing else will mark the frame dirty. Without this the rectangle was
                // never painted at all — the state followed the pointer perfectly and the
                // screen showed nothing until release, while the lasso arm above, which
                // does ask, drew its trail the whole way.
                self.request_draw();
                Some(Interaction::Marquee {
                    start,
                    current: world,
                    base,
                })
            }
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
        let moved = self.rebind_endpoint(moved, handle, square);

        self.scene.put(moved);
        crate::scene::binding::refresh_binding_of(&mut self.scene, id);
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
    /// Only the two ends of an arrow bind; a point in the middle of a path is not an end,
    /// and a line binds to nothing. Dragging an end onto a shape binds it, dragging it
    /// clear releases it — so the gesture is reversible.
    fn rebind_endpoint(
        &mut self,
        mut element: crate::scene::DrawElement,
        handle: crate::selection::LinearHandle,
        angle_locked: bool,
    ) -> crate::scene::DrawElement {
        let points = element.points.as_ref().map(Vec::len).unwrap_or(0);
        let which = match handle {
            crate::selection::LinearHandle::Point(0) => Some(End::Start),
            crate::selection::LinearHandle::Point(i) if i + 1 == points => Some(End::End),
            _ => None,
        };
        let Some(end) = which.filter(|_| crate::scene::binding::is_binding_element(&element))
        else {
            self.binding_highlight = None;
            self.binding_point = None;
            return element;
        };

        let world = crate::selection::linear::world_points(&element);
        let tip = match end {
            End::Start => world.first(),
            End::End => world.last(),
        };
        if let Some(&tip) = tip {
            self.bind_dropped_end(&mut element, end, tip, angle_locked);
        }
        element
    }

    /// How far from a shape an endpoint may be dropped and still attach, in world units:
    /// Excalidraw's reach at this zoom ([`crate::scene::binding::max_binding_distance`]).
    /// The same number for the hover outline, the press that starts an arrow and the drop
    /// that ends it, so the three agree on what counts as near.
    pub(crate) fn binding_tolerance(&self) -> f64 {
        crate::scene::binding::max_binding_distance(self.camera.scale)
    }

    /// What dropping one end of `element` at `world` binds it to, with the modifiers
    /// held: nothing at all while Ctrl/Cmd is down, exactly where it is with Alt.
    ///
    /// Returns that end's binding and, when a drop on the other end's shape changes it,
    /// the other end's. See [`anchor_for_drop`].
    pub(crate) fn drop_binding(
        &self,
        element: &DrawElement,
        end: End,
        world: Point,
        angle_locked: bool,
    ) -> (Option<Anchor>, Option<Anchor>) {
        if self.ctrl_held || !crate::scene::binding::is_binding_element(element) {
            return (None, None);
        }
        let drop = EndDrop {
            pointer: world,
            tolerance: self.binding_tolerance(),
            snap: super::MIDPOINT_SNAP_PX / self.camera.scale,
            exact: self.alt_held,
            angle_locked,
            pixel: 1.0 / self.camera.scale,
            grid: self.grid.enabled && self.grid.snap,
        };
        let scene = &self.scene;
        anchor_for_drop(
            scene.iter_ordered().rev(),
            &|id| scene.get(id),
            element,
            end,
            &drop,
        )
    }

    /// Binds the end of `element` being dragged to wherever `world` is, and shows it.
    ///
    /// Dropping an end on the shape the other end is bound to puts both inside it — but
    /// only while it is there. The other end's binding as this drag found it is
    /// remembered, and given back the moment the drag moves on, so passing across the
    /// far shape on the way somewhere else leaves no trace. (Excalidraw keeps the other
    /// end inside for the rest of a new arrow's drag; nobody drawing across a shape means
    /// to pin the arrow's start to wherever it happened to begin.)
    fn bind_dropped_end(
        &mut self,
        element: &mut DrawElement,
        end: End,
        world: Point,
        angle_locked: bool,
    ) {
        let same_drag = self
            .bind_drag_origin
            .as_ref()
            .is_some_and(|(id, dragged, _)| id == &element.id && *dragged == end);
        if !same_drag {
            self.bind_drag_origin = Some((element.id.clone(), end, anchor(element, end.other())));
        }
        let original_other = self
            .bind_drag_origin
            .as_ref()
            .and_then(|(_, _, other)| other.clone());
        set_anchor(element, end.other(), original_other);

        let (this, other) = self.drop_binding(element, end, world, angle_locked);
        self.binding_snaps = Some(self.drop_snaps(this.as_ref(), world));
        self.binding_highlight = this.as_ref().map(|a| a.element_id.clone());
        self.binding_point = Some(world);
        set_anchor(element, end, this);
        if let Some(other) = other {
            set_anchor(element, end.other(), Some(other));
        }
    }

    /// Lets go of every arrow end in `ids` bound to a shape that is not in `ids`.
    ///
    /// For a group being resized or turned: its arrows move with it, rigidly, so an end
    /// bound to a shape left outside it can no longer be where that binding says. Excalidraw
    /// unbinds it (`packages/element/src/resizeElements.ts:464-475`, `1550-1569`) rather
    /// than bend the arrow back to a shape the transform has carried it away from.
    fn release_ends_outside(&mut self, ids: &[String]) {
        let inside: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        let released: Vec<DrawElement> = ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| crate::scene::binding::is_binding_element(el))
            .filter_map(|el| {
                let outside = |end: End| {
                    anchor(el, end).is_some_and(|a| !inside.contains(a.element_id.as_str()))
                };
                let (start, finish) = (outside(End::Start), outside(End::End));
                if !start && !finish {
                    return None;
                }
                let mut next = el.clone();
                if start {
                    set_anchor(&mut next, End::Start, None);
                }
                if finish {
                    set_anchor(&mut next, End::End, None);
                }
                Some(next)
            })
            .collect();
        for element in released {
            self.scene.put(element);
        }
    }

    /// Extends the arrow or line currently being drawn to the pointer.
    ///
    /// The end binds on the same terms as one dragged later — see [`Self::bind_dropped_end`]
    /// — so an arrow drawn onto a shape attaches exactly as it would if its end were
    /// dragged there afterwards. The shape it would attach to is recorded for the painter,
    /// so the outline lights up **while** the arrow is being drawn, not only afterwards.
    fn move_linear(&mut self, id: &str, start: Point, world: Point, square: bool) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let drag = linear_from_drag(start.x, start.y, world.x, world.y, square);
        element.x = drag.x;
        element.y = drag.y;
        element.width = drag.width;
        element.height = drag.height;
        element.points = Some(drag.points);
        if crate::scene::binding::is_binding_element(&element) {
            let tip = crate::selection::linear::world_points(&element)
                .last()
                .copied()
                .unwrap_or(world);
            self.bind_dropped_end(&mut element, End::End, tip, square);
        }
        self.scene.put(element);
        crate::scene::binding::refresh_binding_of(&mut self.scene, id);
        self.request_draw();
    }

    /// Detaches each end of a moving arrow whose shape is not moving with it.
    ///
    /// Without this a bound arrow could not be moved at all. The move put it somewhere
    /// new, then `apply_bindings` re-resolved its ends onto the shapes they were bound to
    /// and put it straight back — every frame — so from the outside it read as blocked.
    ///
    /// Excalidraw's rule, `packages/element/src/dragElements.ts:110-167`: an end whose
    /// shape is also being dragged stays bound and the assembly moves as one; an end whose
    /// shape stays behind lets go, "otherwise we would have weird situations, like 0
    /// length arrow when the user moves the arrow outside a filled shape".
    ///
    /// One divergence: the oracle applies [`super::DRAGGING_THRESHOLD_PX`] only when the
    /// arrow is the *single* element dragged, and unbinds a multi-selection on the first
    /// pixel. Here the threshold always applies. The only observable difference is that a
    /// sub-ten-pixel wobble of a multi-selection no longer detaches anything — which is
    /// the outcome nobody who wobbled wanted.
    ///
    /// The moving set is `origins`, not the selection, because a frame carries its
    /// children: an arrow bound to a shape inside a frame being dragged stays bound.
    fn release_arrows_left_behind(
        &mut self,
        origins: &std::collections::HashMap<String, Point>,
        dx: f64,
        dy: f64,
    ) {
        let travelled = dx.abs().max(dy.abs()) * self.camera.scale;
        if travelled <= super::DRAGGING_THRESHOLD_PX {
            return;
        }
        let released: Vec<DrawElement> = origins
            .keys()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| crate::scene::binding::is_binding_element(el))
            .filter_map(|el| {
                let left_behind = |bound: &Option<String>| {
                    bound.as_ref().is_some_and(|b| !origins.contains_key(b))
                };
                let (drop_start, drop_end) =
                    (left_behind(&el.start_binding), left_behind(&el.end_binding));
                if !drop_start && !drop_end {
                    return None;
                }
                let mut next = el.clone();
                if drop_start {
                    set_anchor(&mut next, End::Start, None);
                }
                if drop_end {
                    set_anchor(&mut next, End::End, None);
                }
                Some(next)
            })
            .collect();
        for element in released {
            self.scene.put(element);
        }
    }

    fn move_selection(&mut self, it: Interaction, world: Point, invert_snap: bool) -> Interaction {
        let Interaction::Move {
            ids,
            start,
            origins,
            static_bounds,
        } = it
        else {
            return it;
        };
        // Any move makes the press a drag, even one that comes back to where it started:
        // the oracle's `drag.hasOccurred` (`App.tsx:10918-10921`). Judged by where the
        // elements ended, a drag home — or one shorter than a grid cell — read as a click.
        self.narrow_on_click = None;
        let mut dx = world.x - start.x;
        let mut dy = world.y - start.y;
        self.snap_guides.clear();
        // Alignment guides pull toward other elements' edges, which is a different answer
        // from the grid's. Running both makes the result depend on which won by a pixel,
        // so the grid takes precedence while it is snapping.
        //
        // Otherwise Excalidraw's rule (`snapping.ts:180-183`): the preference, inverted
        // for as long as Ctrl/Cmd is held. Theirs also refuses the inverted case while the
        // grid is on; here the grid already wins outright, which covers it.
        let grid_snapping = self.grid().enabled && self.grid().snap;
        let snap_to_objects = self.objects_snap != invert_snap;
        if snap_to_objects && !grid_snapping {
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
        self.release_arrows_left_behind(&origins, dx, dy);
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

    /// Where a resize puts the edge it moves: the pointer, less where in the handle it was
    /// taken, snapped (`App.tsx@1118751f:13578-13582` — the grab first, then the grid).
    fn resize_pointer(&self, sx: f64, sy: f64, grab: Point) -> Point {
        let raw = self.screen_to_world(sx, sy);
        self.snap(Point {
            x: raw.x - grab.x,
            y: raw.y - grab.y,
        })
    }

    fn move_resize(&mut self, id: &str, world: Point, square: bool, drag: ResizeDrag<'_>) {
        let ResizeDrag {
            handle,
            ratio,
            origin,
            origin_points,
            label_font,
        } = drag;
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        if element.kind == DrawElementType::Text && element.container_id.is_none() {
            self.resize_text(element, world, square, handle, &origin);
            return;
        }
        let latest = element.clone();
        // Measured from where the element was when the drag started, so the anchor is
        // fixed for the whole gesture. Reading the live element instead let the anchor
        // follow the pointer the moment the element turned through it.
        let mut from = element.clone();
        from.x = origin.x;
        from.y = origin.y;
        from.width = origin.width;
        from.height = origin.height;
        // A path's `x`, `y` is its first point, which need not be a corner of its box: it
        // is resized from the box its handles are drawn on, as the oracle's is
        // (`previousOrigin`, `resizeElements.ts@1118751f:848-851`). From its first point,
        // a handle taken where it is drawn flattened a line whose first point was not its
        // top-left.
        if let Some(points) = origin_points.filter(|points| !points.is_empty()) {
            let ring = ring_box(points);
            (from.x, from.y, from.width, from.height) = (
                origin.x + ring.x,
                origin.y + ring.y,
                ring.width,
                ring.height,
            );
        }
        // Images hold their proportions unless Shift is held; every other shape is the
        // other way round. A photograph stretched by accident is a mistake you often do
        // not notice until much later.
        let lock = crate::scene::locks_aspect_ratio(&from, square);
        let label = element
            .bound_text_id
            .as_deref()
            .and_then(|label| self.scene.get(label))
            .filter(|label| !label.is_deleted && label.kind == DrawElementType::Text)
            .cloned();
        // A shape holding a label is never made smaller than one line of it
        // (`resizeSingleElement`, `resizeElements.ts@1118751f:778-803`) — unless it keeps
        // its proportions, when the label's font scales instead.
        let min_size = match &label {
            Some(label) if !lock => {
                self.with_measure(|measure| crate::text::layout::min_container_size(label, measure))
            }
            _ => (1.0, 1.0),
        };
        let geom = resize_element_within(
            &from,
            handle,
            world.x,
            world.y,
            min_size,
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
        // Resizing an arrow by its box places its ends, so they let go of their shapes, as
        // in Excalidraw (`packages/element/src/resizeElements.ts:930-946`).
        if crate::scene::binding::is_binding_element(&element) {
            set_anchor(&mut element, End::Start, None);
            set_anchor(&mut element, End::End, None);
        }
        let Some(mut label) = label else {
            self.scene.put(element);
            self.apply_bindings();
            self.request_draw();
            return;
        };
        if lock {
            // The label's font follows its room (`:815-833`), or an arrow's width
            // (`:904-915`), from where the last move left them.
            let size = crate::text::layout::font_size_of(&label);
            let scale = if crate::scene::is_linear_element(&element) {
                element.width.abs() / latest.width.abs()
            } else {
                crate::text::layout::bound_text_max_width(&element, size)
                    / crate::text::layout::bound_text_max_width(&latest, size)
            };
            let next = size * scale;
            if !(next.is_finite() && next >= crate::selection::MIN_FONT_SIZE) {
                return;
            }
            label.font_size = Some(next);
        } else if let Some((_, font)) = label_font.filter(|(id, _)| *id == label.id) {
            // Every other move starts from the font the label had at the press (`:805-814`),
            // so letting go of Shift gives it back.
            label.font_size = *font;
        }
        let flips = (
            (geom.width < 0.0) != (from.width < 0.0),
            (geom.height < 0.0) != (from.height < 0.0),
        );
        self.scene.put(element);
        self.relay_resized_label(label, handle, flips);
        self.apply_bindings();
        self.request_draw();
    }

    /// `label` laid out again in the shape a resize just changed, and the shape grown back
    /// to hold it from the side the drag holds (`handleBindTextResize`,
    /// `textElement.ts@1118751f:155-247`): a north handle holds the bottom, the others the
    /// top — and the other way round once the drag has turned the shape through its
    /// anchor. A label too wide for its room widens the shape the same way from the side a
    /// west handle holds.
    fn relay_resized_label(&mut self, label: DrawElement, handle: HandleKind, flips: (bool, bool)) {
        if label.is_deleted {
            return;
        }
        let Some(container) = label
            .container_id
            .as_deref()
            .and_then(|id| self.scene.get(id))
            .filter(|container| !container.is_deleted)
            .cloned()
        else {
            return;
        };
        let north = matches!(handle, HandleKind::N | HandleKind::Ne | HandleKind::Nw);
        let west = matches!(handle, HandleKind::W | HandleKind::Nw | HandleKind::Sw);
        let keep = (
            if west != flips.0 { 1.0 } else { 0.0 },
            if north != flips.1 { 1.0 } else { 0.0 },
        );
        let laid = self.with_measure(|measure| {
            crate::text::layout::bound_text_resize(&label, &container, keep, measure)
        });
        if let Some(grown) = laid.container {
            self.scene.put(grown);
        }
        if self.scene.get(&laid.text.id) != Some(&laid.text) {
            self.scene.put(laid.text);
        }
    }

    /// A free text resized (`resizeSingleTextElement`, `resizeElements.ts@1118751f:
    /// 317-409`):
    ///
    /// - a corner, the top or the bottom scales the font and the box by the height asked
    ///   for (`:328-358`) — the lines are the same lines, scaled. A drag that would take
    ///   the font below [`MIN_FONT_SIZE`](crate::selection::MIN_FONT_SIZE), past the
    ///   anchor included, leaves the text as the last move had it, so it never turns
    ///   inside out;
    /// - a side fixes the width and wraps what was typed at it (`:360-408`), never
    ///   narrower than a space and the padding. The width is the one the lines were
    ///   wrapped at, so laying them out again gives the same lines; a glyph wider than
    ///   that hangs out of the box, as it does in the oracle.
    ///
    /// The corner opposite the handle stays put, turned or not (`getResizedOrigin`).
    fn resize_text(
        &mut self,
        latest: DrawElement,
        world: Point,
        square: bool,
        handle: HandleKind,
        origin: &crate::selection::Geometry,
    ) {
        use crate::selection::{next_box_size, resized_origin, MIN_FONT_SIZE};
        let (next_width, next_height) =
            next_box_size(origin, latest.angle, handle, world.x, world.y, square);
        let mut next = latest.clone();
        if matches!(handle, HandleKind::E | HandleKind::W) {
            let laid = self.with_measure(|measure| {
                let min_width = crate::text::layout::min_text_width(&latest, measure);
                let mut fixed = latest.clone();
                fixed.auto_resize = Some(false);
                fixed.width = next_width.max(min_width);
                crate::text::layout::layout_text(&fixed, None, measure).text
            });
            let at = resized_origin(origin, laid.width, laid.height, latest.angle, handle);
            next = laid;
            next.x = at.x;
            next.y = at.y;
        } else {
            if !(latest.height > 0.0 && latest.width.is_finite()) {
                return;
            }
            let ratio = next_height / latest.height;
            let size = crate::text::layout::font_size_of(&latest) * ratio;
            if !(size.is_finite() && size >= MIN_FONT_SIZE) {
                return;
            }
            let width = latest.width * ratio;
            let at = resized_origin(origin, width, next_height, latest.angle, handle);
            next.font_size = Some(size);
            next.width = width;
            next.height = next_height;
            next.x = at.x;
            next.y = at.y;
        }
        self.scene.put(next);
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
    label_font: Option<&'a (String, Option<f64>)>,
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
