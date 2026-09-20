use crate::camera::Point;
use crate::engine::{DrawEngine, Interaction};
use crate::interaction::{linear_from_drag, rect_from_drag, snap_move};
use crate::scene::{bindable_at, scene_bounds, DrawElement};
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

    fn move_linear(&mut self, id: &str, start: Point, world: Point, square: bool) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let ordered = self.scene.ordered_cloned();
        let over = bindable_at(&ordered, world.x, world.y, 0.0, Some(id));
        let drag = linear_from_drag(start.x, start.y, world.x, world.y, square);
        element.x = drag.x;
        element.y = drag.y;
        element.width = drag.width;
        element.height = drag.height;
        element.points = Some(drag.points);
        element.end_binding = over
            .filter(|el| Some(&el.id) != element.start_binding.as_ref())
            .map(|el| el.id.clone());
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
