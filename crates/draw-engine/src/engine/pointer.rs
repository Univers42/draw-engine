use crate::camera::Point;
use crate::edit::expand_to_groups_among;
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
                self.interaction = Some(Interaction::Erase);
                self.erase_at(sx, sy);
            }
            DrawTool::Freedraw => self.begin_freedraw(world),
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
        let anchor = bindable_among(
            self.scene.iter_ordered().rev(),
            world.x,
            world.y,
            tolerance,
            None,
        )
        .map(|el| el.id.clone());
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

    fn begin_text(&mut self, sx: f64, sy: f64, world: Point) {
        let style = merge_style(&default_element_style(), &self.next_style);
        let mut element = create_element(
            DrawElementType::Text,
            Geometry {
                x: world.x,
                y: world.y,
                width: 4.0,
                height: self.next_font_size,
            },
            style,
            self.now_ms,
        );
        element.text = Some(String::new());
        element.font_size = Some(self.next_font_size);
        let id = element.id.clone();
        let color = element.stroke_color.clone();
        self.scene.add(element);
        self.set_selection(vec![id.clone()]);
        self.events.text_edit = Some(crate::engine::TextEditRequest {
            id,
            x: sx,
            y: sy,
            font_size: self.next_font_size,
            color,
            text: String::new(),
        });
        self.settle_tool();
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
                let world_tol = super::HANDLE_HIT_PX / self.camera.scale;

                // A line or arrow is edited by its points, not its bounding box — so
                // its handles are tested first and the box handles never apply to it.
                // Excalidraw does the same: select an arrow there and you get circles
                // on its ends, with no selection rectangle at all.
                if crate::selection::linear::is_point_edited(&single) {
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

                let handle = if crate::selection::linear::is_point_edited(&single) {
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
            let hit_ids = expand_to_groups_among(self.scene.iter_ordered(), [hit.id.clone()]);
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
            .collect();
        self.interaction = Some(Interaction::Move {
            ids: origins.keys().cloned().collect(),
            start: world,
            origins,
            static_bounds,
        });
        self.request_draw();
    }

    pub(crate) fn erase_at(&mut self, sx: f64, sy: f64) {
        if let Some(hit) = self.selectable_hit(sx, sy, self.collision_tolerance()) {
            self.scene.remove(&hit.id, self.now_ms);
            if self.selected_ids.remove(&hit.id) {
                self.events.selection = Some(self.get_selection());
            }
            self.request_draw();
        }
    }
}
