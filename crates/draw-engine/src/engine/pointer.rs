use crate::camera::Point;
use crate::edit::{expand_within, is_in_group};
use crate::engine::multi_linear::is_double_tap;
use crate::engine::{DrawEngine, Interaction};
use crate::interaction::{is_linear_tool, is_shape_tool, DrawTool};
use crate::scene::binding::{set_anchor, End};
use crate::scene::{
    create_element, default_element_style, element_bounds, merge_style, DrawElementType, Geometry,
};
use crate::selection::{hit_handle, selection_handles, HandleKind};

impl DrawEngine {
    pub fn begin_pointer(&mut self, sx: f64, sy: f64, additive: bool, duplicate: bool) {
        // What the press selects is settled on the release, as the oracle captures at
        // pointer-up (`App.tsx@1118751f:12451-12464`): see `stamp.rs`.
        self.pointer_open = true;
        self.begin_pointer_step(sx, sy, additive, duplicate);
        self.refresh_live();
    }

    fn begin_pointer_step(&mut self, sx: f64, sy: f64, additive: bool, duplicate: bool) {
        // The press that lands where the finishing one did is the other half of its
        // double click; any other forgets the finished path.
        if !self
            .finished_by_press
            .as_ref()
            .is_some_and(|(_, at)| is_double_tap(*at, sx, sy))
        {
            self.finished_by_press = None;
        }
        // Snapped once, here, so every gesture that starts from a pointer position lands
        // on the grid together. Applying it per-tool is how one of them ends up exempt.
        let world = self.snap(self.screen_to_world(sx, sy));
        if is_shape_tool(self.tool) {
            self.begin_shape(world);
            return;
        }
        if is_linear_tool(self.tool) {
            // Alt at the press, not as of the last move: with no button held, moves are
            // not reported, so that one can be long stale.
            self.alt_held = duplicate;
            self.begin_linear(world, Point { x: sx, y: sy });
            return;
        }
        match self.tool {
            DrawTool::Eraser => {
                let at = self.screen_to_world(sx, sy);
                self.clear_erasing();
                self.interaction = Some(Interaction::Erase { last: at });
                // The press marks what is under it whatever Alt says — a click erases
                // it, Alt held or not, as Excalidraw's pointer-up does. Alt then governs
                // the moves.
                self.alt_held = false;
                self.mark_along(at, at);
                self.alt_held = duplicate;
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
                    // The level being edited outlives the empty selection a loop starts
                    // from, and the release resolves the loop at it (`select_caught`), as
                    // the oracle's does (`lasso/index.ts:72-89`, `:119-127`). Cleared with
                    // the selection, a lasso inside a group took whole top-level groups.
                    let editing = self.editing_group_id.take();
                    self.clear_selection();
                    self.editing_group_id = editing;
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

    fn begin_linear(&mut self, world: Point, screen: Point) {
        // A path is already being placed: this press extends or ends it rather than
        // starting a second one on top.
        if self.multi_linear.is_some() {
            self.press_multi_linear(world, screen);
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
        element.points = Some(vec![[0.0, 0.0], [0.0, 0.0]]);
        // The tail binds on the same terms the head will, as Excalidraw's initial binding
        // does (`packages/excalidraw/components/App.tsx:10317-10339`): inside a shape it
        // sits exactly where the press was, near one it orbits from there.
        let (start, _) = self.drop_binding(&element, End::Start, world, false);
        set_anchor(&mut element, End::Start, start);
        set_anchor(&mut element, End::End, None);
        self.bind_drag_origin = None;
        let id = element.id.clone();
        self.scene.add(element);
        self.interaction = Some(Interaction::Linear {
            id,
            start: world,
            pointer: world,
        });
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
        let element = self.new_text_element(Geometry {
            x: world.x,
            y: world.y,
            width: 0.0,
            height: 0.0,
        });
        let id = element.id.clone();
        self.scene.add(element);
        self.interaction = Some(Interaction::TextDraft { id, start: world });
        self.request_draw();
    }

    pub fn begin_pan(&mut self, sx: f64, sy: f64) {
        self.finished_by_press = None;
        self.interaction = Some(Interaction::Pan {
            last_x: sx,
            last_y: sy,
        });
    }

    /// What a transform of the selection carries: see [`crate::edit::carried_by`].
    ///
    /// Read from the selected elements alone — what is carried is always part of the
    /// selection — because this runs on every hover move over a multi-selection, and a
    /// walk of the whole board there cost the size of the board per move.
    pub(crate) fn carried_selection(&self) -> std::collections::HashSet<String> {
        crate::edit::carried_by(
            self.selected_ids.iter().filter_map(|id| self.scene.get(id)),
            &self.selected_ids,
            self.editing_group_id.as_deref(),
        )
    }

    /// The box a multi-selection's handles sit on: around what it carries, so a loose
    /// locked element is neither framed nor grabbed. `None` below two carried elements.
    pub(crate) fn group_box(&self) -> Option<crate::camera::WorldBounds> {
        let ids = self.carried_selection();
        if ids.len() < 2 {
            return None;
        }
        crate::scene_bounds(ids.iter().filter_map(|id| self.scene.get(id)))
    }

    /// What the current multi-selection carries, with its shared frame.
    fn group_frame(&self) -> Option<(Vec<String>, crate::selection::GroupFrame)> {
        let ids: Vec<String> = self.carried_selection().into_iter().collect();
        if ids.len() < 2 {
            return None;
        }
        let frame =
            crate::selection::GroupFrame::capture(ids.iter().filter_map(|id| self.scene.get(id)))?;
        Some((ids, frame))
    }

    /// Which handle of the multi-selection's frame sits under `world`.
    ///
    /// Shared with the hover cursor, so what the pointer reports and what a press
    /// actually starts are decided by one piece of code.
    pub(crate) fn group_handle_at(&self, world: Point) -> Option<HandleKind> {
        let b = self.group_box()?;
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
        self.narrow_on_click = None;
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
            //
            // By letting go of what is held, shift or not, which lets go of the group
            // too (`App.tsx:9639-9650`): a shift-click that kept the pieces inside held
            // half of one group beside all of another. Not through `leave_group`, which
            // re-derives the selection: this click is about to compute its own, and a
            // re-derivation here would make the hit look already-selected and turn the
            // press into a drag of the wrong thing.
            if self
                .editing_group_id
                .as_deref()
                .is_some_and(|editing| !is_in_group(&hit, editing))
            {
                self.set_selection(Vec::new());
            }
            let editing = self.editing_group_id.clone();
            let hit_ids = expand_within(
                self.scene.iter_ordered(),
                [hit.id.clone()],
                editing.as_deref(),
            );
            if additive {
                let mut next = self.selected_ids.clone();
                let held = next.contains(&hit.id);
                if held {
                    next.retain(|id| !hit_ids.contains(id));
                } else {
                    next.extend(hit_ids);
                }
                if held && !duplicate {
                    // Taken out on the release, and only by a click: the oracle changes
                    // nothing on a shift-press over what is held (`App.tsx:9656-9660`)
                    // and removes it on a release with no drag (`:12183-12260`). Taken
                    // out here, a shift-drag moved the rest and left this one behind.
                    self.narrow_on_click = Some(next);
                } else {
                    // Through the one door, so a shift-click that lets go of the last
                    // thing held lets go of the group being edited with it.
                    self.set_selection(next);
                }
            } else if !self.selected_ids.contains(&hit.id) {
                self.set_selection(hit_ids);
            } else if !duplicate && hit_ids != self.selected_ids {
                self.narrow_on_click = Some(hit_ids);
            }
            if duplicate {
                self.duplicate_selection(0.0, 0.0);
            }
            self.begin_move(world);
            return;
        }
        // Nothing was hit — but the press may still be *inside what is selected*, which
        // the oracle hits as a whole: a selected element by its box, several by their
        // common box (`App.tsx:6782-6806`, `:9783-9806`). A drag from there moves the
        // selection; a release without one was a click on nothing, and lets go of it
        // (`App.tsx:12344-12387`).
        //
        // It matters because a shape with no fill is hit on its outline only, so the
        // middle of a selected empty rectangle is a hole, movable otherwise only by
        // aiming at a two-pixel line. Checked *after* `selectable_hit` so a shape lying
        // over the selection can still be clicked and selected in the normal way.
        if !additive && self.pointer_is_inside_selection(world) {
            if duplicate {
                self.duplicate_selection(0.0, 0.0);
            } else {
                self.narrow_on_click = Some(std::collections::HashSet::new());
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
    /// For one element as for several: the oracle hits a selected element anywhere in
    /// its box (`hitElement`, `App.tsx:6782-6806`) and several in their common box
    /// (`isHittingCommonBoundingBoxOfSelectedElements`, `App.tsx:9783-9806`). Except a
    /// line or arrow edited by its points, which has no box drawn round it and so none to
    /// grab (`hasBoundingBox`, `packages/element/src/transformHandles.ts:328-353`).
    ///
    /// By reference: the hover cursor asks this on every move, and cloning the selection
    /// there cost the size of a select-all per move.
    pub(crate) fn pointer_is_inside_selection(&self, world: Point) -> bool {
        let selected = || {
            self.selected_ids
                .iter()
                .filter_map(|id| self.scene.get(id))
                .filter(|el| !el.is_deleted)
        };
        // Also true of an empty selection.
        if selected().all(|el| el.locked()) {
            return false;
        }
        let mut live = selected();
        if let (Some(single), None) = (live.next(), live.next()) {
            if self.shows_point_handles(single) {
                return false;
            }
        }
        let Some(bounds) = crate::scene_bounds(selected()) else {
            return false;
        };
        let pad = self.handle_layout().frame_pad + self.collision_tolerance();
        world.x >= bounds.min_x - pad
            && world.x <= bounds.max_x + pad
            && world.y >= bounds.min_y - pad
            && world.y <= bounds.max_y + pad
    }

    /// What moving the selection moves, by drag or by arrow key: what it carries, plus
    /// everything a frame in it contains.
    ///
    /// A frame carries what it contains. Expanding the set here rather than moving
    /// children separately means one code path moves everything: the children snap,
    /// re-bind and undo exactly as they would if you had selected them yourself.
    ///
    /// Locked children included, as a group's locked members are: the oracle adds every
    /// child of a dragged frame with no lock filter (`packages/element/src/
    /// dragElements.ts:75-84`), and one left behind would sit outside the frame that
    /// still claims it. A child a peer holds stays where they have it: what a peer holds
    /// is untouchable, and moving it anyway left the two sides stamping the same version.
    pub(crate) fn moving_selection(&self) -> std::collections::HashSet<String> {
        let mut moving = self.carried_selection();
        let children: Vec<String> = moving
            .iter()
            .filter(|id| self.scene.get(id).is_some_and(crate::scene::is_frame))
            .flat_map(|id| crate::scene::frame_children(self.scene.iter_ordered(), id))
            .filter(|child| !self.held.contains_key(child))
            .collect();
        moving.extend(children);
        moving
    }

    fn begin_move(&mut self, world: Point) {
        let moving = self.moving_selection();
        let origins: std::collections::HashMap<String, Point> = moving
            .iter()
            .filter_map(|id| {
                let element = self.scene.get(id)?;
                Some((
                    id.clone(),
                    Point {
                        x: element.x,
                        y: element.y,
                    },
                ))
            })
            .collect();
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
}
