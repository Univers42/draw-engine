use crate::camera::Point;
use crate::edit::{expand_within, is_in_group};
use crate::engine::{DrawEngine, Interaction};
use crate::interaction::{is_linear_tool, is_shape_tool, DrawTool};
use crate::scene::binding::bindable_among;
use crate::scene::{
    create_element, default_element_style, element_bounds, merge_style, DrawElementType, Geometry,
};
use crate::selection::{hit_handle, selection_handles, HandleKind};

impl DrawEngine {
    pub fn begin_pointer(&mut self, sx: f64, sy: f64, additive: bool, duplicate: bool) {
        // Snapped once, here, so every gesture that starts from a pointer position lands
        // on the grid together. Applying it per-tool is how one of them ends up exempt.
        let world = self.snap(self.screen_to_world(sx, sy));
        if is_shape_tool(self.tool) {
            self.begin_shape(world);
            return;
        }
        if is_linear_tool(self.tool) {
            self.begin_linear(world);
            return;
        }
        match self.tool {
            DrawTool::Eraser => {
                let at = self.screen_to_world(sx, sy);
                self.interaction = Some(Interaction::Erase { last: at });
                self.erase_along(at, at);
            }
            DrawTool::Freedraw | DrawTool::AutoShape => self.begin_freedraw(world),
            DrawTool::Text => self.begin_text(sx, sy, world),
            DrawTool::Lasso => {
                self.interaction = Some(Interaction::Lasso {
                    path: vec![world],
                    base: if additive {
                        self.selected_ids.clone()
                    } else {
                        Default::default()
                    },
                });
                if !additive {
                    self.clear_selection();
                }
                self.request_draw();
            }
            DrawTool::Frame => self.begin_frame(world),
            // Nothing to do with a pointer. The image arrives from a file picker, and
            // until it does there is nothing to place — clicking must not start a
            // marquee either, or the selection changes behind the open dialog.
            DrawTool::Image | DrawTool::Embed => {}
            // The click *is* the gesture: there is nothing to drag, and starting a
            // marquee would change the selection under a fill that just happened.
            DrawTool::BucketFill => {
                if let Err(failure) = self.bucket_fill_at(sx, sy) {
                    self.report_fill_failure(failure);
                }
            }
            DrawTool::Laser => {
                // Deliberately the unsnapped point. A laser follows the cursor; snapping
                // it to the grid would make the beam jump between intersections while the
                // hand it is meant to be tracking moves smoothly.
                let exact = self.screen_to_world(sx, sy);
                self.interaction = Some(Interaction::Laser);
                self.laser.start(exact.x, exact.y, self.now_ms);
                self.request_draw();
            }
            DrawTool::Hand => {
                self.interaction = Some(Interaction::Pan {
                    last_x: sx,
                    last_y: sy,
                });
            }
            _ => self.begin_select(sx, sy, world, additive, duplicate),
        }
    }

    /// A frame is dragged out like a shape, but it is chrome rather than a drawing.
    ///
    /// It takes none of the current element style: a frame is always the same grey, at
    /// the same weight, with the same corners, so it reads as a boundary rather than as
    /// something someone drew. That is why this cannot just be another shape tool.
    fn begin_frame(&mut self, world: Point) {
        let mut element = create_element(
            DrawElementType::Frame,
            Geometry {
                x: world.x,
                y: world.y,
                width: 0.0,
                height: 0.0,
            },
            crate::scene::frame_style(),
            self.now_ms,
        );
        element.name = Some(crate::scene::default_frame_name(self.scene.iter_ordered()));
        let id = element.id.clone();
        self.scene.add(element);
        self.interaction = Some(Interaction::Draft { id, start: world });
        self.request_draw();
    }

    fn begin_shape(&mut self, world: Point) {
        let Some(kind) = crate::interaction::tool_to_element_type(self.tool) else {
            return;
        };
        let style = merge_style(&default_element_style(), &self.next_style);
        let element = create_element(
            kind,
            Geometry {
                x: world.x,
                y: world.y,
                width: 0.0,
                height: 0.0,
            },
            style,
            self.now_ms,
        );
        let id = element.id.clone();
        self.scene.add(element);
        self.interaction = Some(Interaction::Draft { id, start: world });
        self.request_draw();
    }

    fn begin_linear(&mut self, world: Point) {
        // A path is already being placed: this press extends or ends it rather than
        // starting a second one on top.
        if self.multi_linear.is_some() {
            self.press_multi_linear(world);
            return;
        }
        let Some(kind) = crate::interaction::tool_to_element_type(self.tool) else {
            return;
        };
        let style = merge_style(&default_element_style(), &self.next_style);
        let mut element = create_element(
            kind,
            Geometry {
                x: world.x,
                y: world.y,
                width: 0.0,
                height: 0.0,
            },
            style,
            self.now_ms,
        );
        // Same tolerance the far end gets, so both ends of an arrow attach on the same
        // terms. A zero tolerance here meant the tail only bound when the gesture started
        // strictly inside a shape.
        let tolerance = self.binding_tolerance();
        let anchor = crate::scene::binding::is_binding_element(&element)
            .then(|| {
                bindable_among(
                    self.scene.iter_ordered().rev(),
                    world.x,
                    world.y,
                    tolerance,
                    None,
                )
                .map(|el| el.id.clone())
            })
            .flatten();
        element.points = Some(vec![[0.0, 0.0], [0.0, 0.0]]);
        element.start_binding = anchor;
        element.end_binding = None;
        let id = element.id.clone();
        self.scene.add(element);
        self.interaction = Some(Interaction::Linear { id, start: world });
        self.request_draw();
    }

    fn begin_freedraw(&mut self, world: Point) {
        let style = merge_style(&default_element_style(), &self.next_style);
        let mut element = create_element(
            DrawElementType::Freedraw,
            Geometry {
                x: world.x,
                y: world.y,
                width: 0.0,
                height: 0.0,
            },
            style,
            self.now_ms,
        );
        element.points = Some(vec![[0.0, 0.0]]);
        let id = element.id.clone();
        self.scene.add(element);
        self.interaction = Some(Interaction::Freedraw { id, start: world });
        self.request_draw();
    }

    /// Starts a text gesture, without yet knowing which of the two it is.
    ///
    /// A click makes text that grows with what you type; a drag makes a column fixed to
    /// the width you dragged. Which one it was is only knowable on release, so the
    /// editor cannot open here the way it used to — `end_text` opens it, once the
    /// gesture has said how wide the thing is.
    fn begin_text(&mut self, _sx: f64, _sy: f64, world: Point) {
        let style = merge_style(&default_element_style(), &self.next_style);
        let mut element = create_element(
            DrawElementType::Text,
            Geometry {
                x: world.x,
                y: world.y,
                width: 0.0,
                height: 0.0,
            },
            style,
            self.now_ms,
        );
        element.text = Some(String::new());
        element.font_size = Some(self.next_font_size);
        element.text_align = self.next_text_align;
        element.vertical_align = self.next_vertical_align;
        let id = element.id.clone();
        self.scene.add(element);
        self.interaction = Some(Interaction::TextDraft { id, start: world });
        self.request_draw();
    }

    pub fn begin_pan(&mut self, sx: f64, sy: f64) {
        self.interaction = Some(Interaction::Pan {
            last_x: sx,
            last_y: sy,
        });
    }

    /// The unlocked members of the current multi-selection, with their shared frame.
    fn group_frame(&self) -> Option<(Vec<String>, crate::selection::GroupFrame)> {
        let ids: Vec<String> = self.selected_ids.iter().cloned().collect();
        let elements: Vec<crate::scene::DrawElement> = ids
            .iter()
            .filter_map(|id| self.scene.get(id).cloned())
            .filter(|e| !e.locked())
            .collect();
        if elements.len() < 2 {
            return None;
        }
        let frame = crate::selection::GroupFrame::capture(elements.iter())?;
        Some((ids, frame))
    }

    /// Which handle of the multi-selection's frame sits under `world`.
    ///
    /// Shared with the hover cursor, so what the pointer reports and what a press
    /// actually starts are decided by one piece of code.
    pub(crate) fn group_handle_at(&self, world: Point) -> Option<HandleKind> {
        let (_, frame) = self.group_frame()?;
        let b = frame.bounds;
        let layout = self.handle_layout();
        // Offset exactly as the painter offsets them, and exactly as a single shape's
        // are, so the inside of a group stays a move target.
        let (min_x, min_y) = (
            b.min_x - layout.handle_offset,
            b.min_y - layout.handle_offset,
        );
        let (max_x, max_y) = (
            b.max_x + layout.handle_offset,
            b.max_y + layout.handle_offset,
        );

        let candidates = [
            (HandleKind::Nw, min_x, min_y),
            (HandleKind::Ne, max_x, min_y),
            (HandleKind::Se, max_x, max_y),
            (HandleKind::Sw, min_x, max_y),
            (
                HandleKind::Rotate,
                (min_x + max_x) / 2.0,
                min_y - layout.rotate_gap,
            ),
        ];

        candidates
            .into_iter()
            .find(|&(_, hx, hy)| (world.x - hx).hypot(world.y - hy) <= layout.hit)
            .map(|(kind, _, _)| kind)
    }

    /// Corner and rotation handles for a multi-element selection.
    ///
    /// Without this a group could only be moved: dragging its corner fell through to
    /// the hit test and started a marquee instead, so a multi-selection could never be
    /// scaled or turned.
    fn begin_group_transform(&self, world: Point) -> Option<Interaction> {
        let kind = self.group_handle_at(world)?;
        let (ids, frame) = self.group_frame()?;
        Some(if kind == HandleKind::Rotate {
            Interaction::RotateGroup { ids, frame }
        } else {
            Interaction::ResizeGroup {
                ids,
                handle: kind,
                frame,
            }
        })
    }

    fn begin_select(&mut self, sx: f64, sy: f64, world: Point, additive: bool, duplicate: bool) {
        if let Some(single) = self.single_selected() {
            if !single.locked() {
                // Radius handles first. They sit *inside* the shape, so a press on one of
                // a filled rectangle would otherwise pick the whole shape up and move it.
                if let Some(corner) = self.radius_handle_at(world) {
                    self.begin_corner_radius(&single, corner, world);
                    return;
                }
                let world_tol = super::HANDLE_HIT_PX / self.camera.scale;

                // A line or arrow is edited by its points, not its bounding box — so
                // its handles are tested first and the box handles never apply to it.
                // Excalidraw does the same: select an arrow there and you get circles
                // on its ends, with no selection rectangle at all.
                if self.shows_point_handles(&single) {
                    let min_segment = super::LINEAR_MIDPOINT_MIN_PX / self.camera.scale;
                    let handles = crate::selection::linear::handle_points(&single, min_segment);
                    if let Some(handle) =
                        crate::selection::linear::hit_handle(&handles, world.x, world.y, world_tol)
                    {
                        self.interaction = Some(Interaction::LinearPoint {
                            id: single.id,
                            handle,
                        });
                        return;
                    }
                }

                let handle = if self.shows_point_handles(&single) {
                    None
                } else {
                    // The same layout the painter uses, so a grab can only land on a
                    // handle that is actually on screen — and its own reach, which is
                    // sized to stay clear of the element so the outline still moves it.
                    let layout = self.handle_layout();
                    hit_handle(
                        &selection_handles(&single, layout),
                        world.x,
                        world.y,
                        layout.hit,
                    )
                };
                if handle == Some(HandleKind::Rotate) {
                    self.interaction = Some(Interaction::Rotate { id: single.id });
                    return;
                }
                if let Some(handle) = handle {
                    let ratio = if single.height.abs() > 0.01 {
                        Some((single.width / single.height).abs())
                    } else {
                        None
                    };
                    let origin = crate::selection::Geometry {
                        x: single.x,
                        y: single.y,
                        width: single.width,
                        height: single.height,
                    };
                    self.interaction = Some(Interaction::Resize {
                        id: single.id,
                        handle,
                        ratio,
                        origin,
                        origin_points: single.points.clone(),
                    });
                    return;
                }
            }
        }
        // More than one element selected: the handles belong to the group's frame.
        if self.selected_ids.len() > 1 {
            if let Some(interaction) = self.begin_group_transform(world) {
                self.interaction = Some(interaction);
                return;
            }
        }

        if let Some(hit) = self.selectable_hit(sx, sy, self.collision_tolerance()) {
            // Pressing something outside the group being edited steps back out of it,
            // before the selection is worked out — otherwise the click would be resolved
            // relative to a group it has nothing to do with and select nothing at all.
            if let Some(editing) = self.editing_group_id.clone() {
                if !is_in_group(&hit, &editing) {
                    // Dropped directly rather than through `leave_group`, which also
                    // re-derives the selection: this click is about to compute its own,
                    // and a re-derivation here would make the hit look already-selected
                    // and turn the press into a drag of the wrong thing.
                    self.editing_group_id = None;
                }
            }
            let editing = self.editing_group_id.clone();
            let hit_ids = expand_within(
                self.scene.iter_ordered(),
                [hit.id.clone()],
                editing.as_deref(),
            );
            if additive {
                let has = self.selected_ids.contains(&hit.id);
                for id in hit_ids {
                    if has {
                        self.selected_ids.remove(&id);
                    } else {
                        self.selected_ids.insert(id);
                    }
                }
                self.events.selection = Some(self.get_selection());
            } else if !self.selected_ids.contains(&hit.id) {
                self.set_selection(hit_ids);
            }
            if duplicate {
                self.duplicate_selection(0.0, 0.0);
            }
            self.begin_move(world);
            return;
        }
        // Nothing was hit — but the click may still be *inside what is selected*, and a
        // click there can only sensibly mean "move this".
        //
        // It matters because a shape with no fill is hit on its outline only, so the
        // middle of a selected empty rectangle is a hole. Falling through to a marquee
        // there drops the selection that was just made and leaves the shape movable only
        // by aiming at a two-pixel line. Checked *after* `selectable_hit` so a shape
        // lying over the selection can still be clicked and selected in the normal way.
        if !additive && self.pointer_is_inside_selection(world) {
            if duplicate {
                self.duplicate_selection(0.0, 0.0);
            }
            self.begin_move(world);
            return;
        }

        if !additive {
            self.clear_selection();
        }
        self.interaction = Some(Interaction::Marquee {
            start: world,
            current: world,
            base: self.selected_ids.clone(),
        });
        self.request_draw();
    }

    /// Whether a world point falls within the frame drawn around the current selection.
    ///
    /// The same box the painter outlines, so what you can grab is what you can see. The
    /// collision tolerance is added on top for the same reason it is added everywhere
    /// else: the frame is a line, and a line is not something anyone can aim at exactly.
    ///
    /// A deliberate divergence from Excalidraw, which answers this only for two or more
    /// elements (`isHittingCommonBoundingBoxOfSelectedElements`, `App.tsx:9783`, returns
    /// false below that). One element and two behaving differently is an asymmetry
    /// nobody asks for.
    fn pointer_is_inside_selection(&self, world: Point) -> bool {
        let selected = self.get_selected_elements();
        if selected.is_empty() || selected.iter().all(|el| el.locked()) {
            return false;
        }
        let Some(bounds) = crate::scene_bounds(selected.iter()) else {
            return false;
        };
        let pad = self.handle_layout().frame_pad + self.collision_tolerance();
        world.x >= bounds.min_x - pad
            && world.x <= bounds.max_x + pad
            && world.y >= bounds.min_y - pad
            && world.y <= bounds.max_y + pad
    }

    fn begin_move(&mut self, world: Point) {
        let mut origins = std::collections::HashMap::new();
        // A frame carries what it contains. Expanding the set here rather than moving
        // children separately means one code path moves everything: the children snap,
        // re-bind and undo exactly as they would if you had selected them yourself.
        let mut moving_elements = self.get_selected_elements();
        let frame_ids: Vec<String> = moving_elements
            .iter()
            .filter(|el| crate::scene::is_frame(el))
            .map(|el| el.id.clone())
            .collect();
        for frame_id in frame_ids {
            for child_id in crate::scene::frame_children(self.scene.iter_ordered(), &frame_id) {
                if origins.contains_key(&child_id) {
                    continue;
                }
                if let Some(child) = self.scene.get(&child_id) {
                    if !moving_elements.iter().any(|el| el.id == child_id) {
                        moving_elements.push(child.clone());
                    }
                }
            }
        }
        for element in moving_elements {
            if !element.locked() {
                origins.insert(
                    element.id.clone(),
                    Point {
                        x: element.x,
                        y: element.y,
                    },
                );
            }
        }
        let moving: std::collections::HashSet<_> = origins.keys().cloned().collect();
        // By reference: this runs once per drag-start but touches every element in the
        // document, and cloning them only to read four numbers off each was the single
        // most expensive thing about picking up a shape on a large board.
        //
        // Culled to the viewport as well. An alignment guide to something off screen is
        // drawn where nobody can see it, so the shape appears to stick for no reason —
        // and gathering candidates from the whole document makes `snap_move` cost the
        // size of the board on *every frame of every drag*. Excalidraw gathers its
        // candidates from the visible elements for the same two reasons.
        //
        // Once, here, rather than per frame: the viewport does not move during a drag.
        let view = crate::visible_world_rect(self.camera, self.width, self.height);
        let static_bounds = self
            .scene
            .iter_ordered()
            .filter(|el| {
                !moving.contains(&el.id)
                    && el
                        .container_id
                        .as_ref()
                        .is_none_or(|id| !moving.contains(id))
            })
            .map(element_bounds)
            .filter(|b| {
                b.min_x <= view.max_x
                    && b.max_x >= view.min_x
                    && b.min_y <= view.max_y
                    && b.max_y >= view.min_y
            })
            .collect();
        self.interaction = Some(Interaction::Move {
            ids: origins.keys().cloned().collect(),
            start: world,
            origins,
            static_bounds,
        });
        self.request_draw();
    }

    /// Erase everything the sweep from `from` to `to` touches.
    ///
    /// Two departures from what this replaced, both of which are why the eraser felt
    /// broken rather than slow:
    ///
    /// - It takes a *segment*. Pointer moves are coalesced to one per animation frame, so
    ///   a quick drag arrives as samples tens of pixels apart, and testing the samples
    ///   steps over everything in between.
    /// - It takes *every* element it touches, not the topmost. A board made by holding
    ///   Ctrl+D is a stack of identical shapes in one place, so taking one per pass meant
    ///   one pass per copy — each of which looked like it had done nothing.
    pub(crate) fn erase_along(&mut self, from: Point, to: Point) {
        let tolerance = self.collision_tolerance();
        let doomed: Vec<String> = self
            .scene
            .iter_ordered()
            .filter(|el| !el.locked() && crate::segment_hits_element(el, from, to, tolerance))
            .map(|el| el.id.clone())
            .collect();
        if doomed.is_empty() {
            return;
        }

        // A label belongs to its container: leaving it behind orphans it against a shape
        // that is no longer there, which only surfaces later when something tries to lay
        // it out.
        let mut removed_any = false;
        let mut selection_changed = false;
        for id in doomed {
            let bound = self.scene.get(&id).and_then(|el| el.bound_text_id.clone());
            for id in std::iter::once(id).chain(bound) {
                if self.scene.get(&id).is_none_or(|el| el.is_deleted) {
                    continue;
                }
                self.scene.remove(&id, self.now_ms);
                removed_any = true;
                selection_changed |= self.selected_ids.remove(&id);
            }
        }

        if selection_changed {
            self.events.selection = Some(self.get_selection());
        }
        if removed_any {
            self.request_draw();
        }
    }
}
