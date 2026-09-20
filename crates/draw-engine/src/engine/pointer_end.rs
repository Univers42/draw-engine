use crate::edit::expand_to_groups;
use crate::engine::{DrawEngine, Interaction};
use crate::freehand::points_bounds;
use crate::interaction::{is_degenerate_linear, DrawTool};
use crate::selection::{elements_in_marquee, marquee_rect};

impl DrawEngine {
    pub fn end_pointer(&mut self) {
        let Some(it) = self.interaction.take() else {
            return;
        };
        self.snap_guides.clear();
        match it {
            Interaction::Draft { id, .. } => self.end_draft(&id),
            Interaction::Linear { id, .. } => self.end_linear(&id),
            Interaction::Freedraw { id, .. } => self.end_freedraw(&id),
            Interaction::Marquee {
                start,
                current,
                base,
            } => {
                let hits = elements_in_marquee(
                    &self.selectable(),
                    marquee_rect(start.x, start.y, current.x, current.y),
                );
                let mut ids = base;
                ids.extend(hits);
                self.set_selection(expand_to_groups(&self.scene.ordered_cloned(), ids));
            }
            _ => {
                self.apply_bindings();
                self.push_history();
                self.request_draw();
            }
        }
    }

    fn end_draft(&mut self, id: &str) {
        if let Some(element) = self.scene.get(id) {
            if element.width < 2.0 && element.height < 2.0 {
                self.scene.discard(id);
                self.set_tool(DrawTool::Select);
                self.request_draw();
                return;
            }
        }
        self.settle_tool();
        self.set_selection(vec![id.to_string()]);
        self.push_history();
    }

    fn end_linear(&mut self, id: &str) {
        let degenerate = self
            .scene
            .get(id)
            .map(|el| is_degenerate_linear(el.width, el.height, 4.0))
            .unwrap_or(true);
        if degenerate {
            self.scene.discard(id);
            self.set_tool(DrawTool::Select);
            self.request_draw();
            return;
        }
        self.settle_tool();
        self.apply_bindings();
        self.set_selection(vec![id.to_string()]);
        self.push_history();
    }

    fn end_freedraw(&mut self, id: &str) {
        if let Some(mut element) = self.scene.get(id).cloned() {
            if let Some(points) = element.points.clone() {
                if points.len() > 1 {
                    let [min_x, min_y, max_x, max_y] = points_bounds(&points);
                    let shifted: Vec<[f64; 2]> = points
                        .iter()
                        .map(|&[px, py]| [px - min_x, py - min_y])
                        .collect();
                    element.x += min_x;
                    element.y += min_y;
                    element.width = (max_x - min_x).max(1.0);
                    element.height = (max_y - min_y).max(1.0);
                    element.points = Some(shifted);
                    self.scene.put(element.clone());
                    self.settle_tool();
                    self.set_selection(vec![element.id]);
                    self.push_history();
                    return;
                }
            }
            self.scene.discard(id);
        }
        self.set_tool(DrawTool::Select);
        self.request_draw();
    }

    pub fn cancel_pointer(&mut self) {
        let it = self.interaction.take();
        self.snap_guides.clear();
        if let Some(
            Interaction::Draft { id, .. }
            | Interaction::Freedraw { id, .. }
            | Interaction::Linear { id, .. },
        ) = it
        {
            self.scene.discard(&id);
            self.set_tool(DrawTool::Select);
        }
        self.clear_selection();
        self.request_draw();
    }
}
