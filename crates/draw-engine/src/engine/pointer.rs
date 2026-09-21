use crate::camera::Point;
use crate::edit::expand_to_groups;
use crate::engine::{DrawEngine, Interaction};
use crate::interaction::{is_linear_tool, is_shape_tool, DrawTool};
use crate::scene::{
    bindable_at, create_element, default_element_style, element_bounds, merge_style,
    DrawElementType, Geometry,
};
use crate::selection::{hit_handle, selection_handle_points, HandleKind};

impl DrawEngine {
    pub fn begin_pointer(&mut self, sx: f64, sy: f64, additive: bool, duplicate: bool) {
        let world = self.screen_to_world(sx, sy);
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
            DrawTool::Hand => {
                self.interaction = Some(Interaction::Pan {
                    last_x: sx,
                    last_y: sy,
                });
            }
            _ => self.begin_select(sx, sy, world, additive, duplicate),
        }
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
        let ordered = self.scene.ordered_cloned();
        let anchor = bindable_at(&ordered, world.x, world.y, 0.0, None);
        element.points = Some(vec![[0.0, 0.0], [0.0, 0.0]]);
        element.start_binding = anchor.map(|el| el.id.clone());
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

                let gap = super::ROTATE_GAP_PX / self.camera.scale;
                let handle = if crate::selection::linear::is_point_edited(&single) {
                    None
                } else {
                    hit_handle(
                        &selection_handle_points(&single, gap),
                        world.x,
                        world.y,
                        world_tol,
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
                    self.interaction = Some(Interaction::Resize {
                        id: single.id,
                        handle,
                        ratio,
                    });
                    return;
                }
            }
        }
        if let Some(hit) = self.selectable_hit(sx, sy, 2.0) {
            let hit_ids = expand_to_groups(&self.scene.ordered_cloned(), [hit.id.clone()]);
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
        for element in self.get_selected_elements() {
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
        let static_bounds = self
            .scene
            .ordered_cloned()
            .into_iter()
            .filter(|el| {
                !moving.contains(&el.id)
                    && el
                        .container_id
                        .as_ref()
                        .is_none_or(|id| !moving.contains(id))
            })
            .map(|el| element_bounds(&el))
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
        if let Some(hit) = self.selectable_hit(sx, sy, 4.0) {
            self.scene.remove(&hit.id, self.now_ms);
            if self.selected_ids.remove(&hit.id) {
                self.events.selection = Some(self.get_selection());
            }
            self.request_draw();
        }
    }
}
