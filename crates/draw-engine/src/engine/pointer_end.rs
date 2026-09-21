use crate::edit::expand_to_groups_among;
use crate::engine::{DrawEngine, Interaction};
use crate::freehand::points_bounds;
use crate::interaction::{is_degenerate_linear, DrawTool};
use crate::selection::{elements_in_lasso, elements_in_marquee_among, marquee_rect, LassoMode};

/// World units the pointer must travel before a drag counts as a marquee rather than a
/// click. Small enough that a deliberate rubber-band always registers, large enough to
/// absorb the jitter of a mouse being clicked.
const MARQUEE_MIN_DRAG: f64 = 2.0;

impl DrawEngine {
    pub fn end_pointer(&mut self) {
        // The binding hint belongs to the drag, not to the document.
        self.binding_highlight = None;
        let Some(it) = self.interaction.take() else {
            return;
        };
        self.snap_guides.clear();
        match it {
            Interaction::Draft { id, .. } => self.end_draft(&id),
            Interaction::Linear { id, .. } => self.end_linear(&id),
            Interaction::Freedraw { id, .. } => self.end_freedraw(&id),
            Interaction::Lasso { path, base } => {
                // The engine decides what the loop caught, not the host: this is
                // geometry, and every frontend on this engine must answer it the same.
                let mut ids = base;
                ids.extend(elements_in_lasso(
                    self.scene.iter_ordered(),
                    &path,
                    LassoMode::Contain,
                ));
                let expanded = expand_to_groups_among(self.scene.iter_ordered(), ids);
                self.set_selection(expanded);
                self.settle_tool();
            }
            Interaction::Laser => {
                // The stroke is finished, not gone: it keeps fading on its own, so the
                // frame loop has to stay awake until it has.
                self.laser.end();
                self.request_draw();
                // Deliberately no `settle_tool`. The laser is a mode you stay in while
                // presenting — Excalidraw keeps it selected too — and a tool that
                // reverted to Select after every flick would be unusable for it.
            }
            Interaction::Marquee {
                start,
                current,
                base,
            } => {
                let rect = marquee_rect(start.x, start.y, current.x, current.y);

                // A click that hit nothing is not a marquee. Without this, releasing
                // without dragging leaves a zero-area rect, and a zero-area rect
                // "overlaps" any bounding box containing the point — so clicking empty
                // space well away from a diagonal line still selected it, because the
                // click fell inside the line's bounding box. Lines and arrows felt
                // grabby for exactly this reason.
                let dragged = (rect.max_x - rect.min_x).abs() > MARQUEE_MIN_DRAG
                    || (rect.max_y - rect.min_y).abs() > MARQUEE_MIN_DRAG;

                let mut ids = base;
                if dragged {
                    // By reference: this used to clone the whole document twice, once to
                    // test the rectangle against it and once to expand groups.
                    ids.extend(elements_in_marquee_among(
                        self.scene.iter_ordered().filter(|el| !el.locked()),
                        rect,
                    ));
                }
                let expanded = expand_to_groups_among(self.scene.iter_ordered(), ids);
                self.set_selection(expanded);
            }
            _ => {
                // Membership follows position, so it is settled once the gesture is:
                // re-deriving it per pointer move would make an element flicker between
                // frames as it crossed a border mid-drag, and would put a history-worthy
                // change behind every sample.
                self.refresh_frame_membership();
                self.apply_bindings();
                self.push_history();
                self.request_draw();
            }
        }
    }

    /// Re-derive which frame owns what, across the whole scene.
    ///
    /// Derived rather than remembered. A drag can change membership in both directions
    /// at once — a shape leaves one frame as the frame it is leaving grows over another
    /// — and tracking only the elements that moved would miss the second half of that.
    pub(crate) fn refresh_frame_membership(&mut self) {
        if !self.scene.iter_ordered().any(crate::scene::is_frame) {
            return;
        }
        let mut changes: Vec<(String, Option<String>)> = Vec::new();
        for element in self.scene.iter_ordered() {
            if element.is_deleted || crate::scene::is_frame(element) {
                continue;
            }
            let owner = crate::scene::frame_for_element(self.scene.iter_ordered(), element);
            if owner != element.frame_id {
                changes.push((element.id.clone(), owner));
            }
        }
        for (id, frame_id) in changes {
            if let Some(mut element) = self.scene.get(&id).cloned() {
                element.frame_id = frame_id;
                self.scene
                    .put(crate::scene::bump_version(element, self.now_ms));
            }
        }
    }

    fn end_draft(&mut self, id: &str) {
        let is_frame = self.scene.get(id).is_some_and(crate::scene::is_frame);
        // A frame needs more than a click's worth of drag to be worth keeping: below
        // this it captures nothing and is almost impossible to grab again.
        let too_small = match self.scene.get(id) {
            Some(element) if is_frame => {
                element.width.abs() < crate::scene::FRAME_MIN_SIZE
                    && element.height.abs() < crate::scene::FRAME_MIN_SIZE
            }
            Some(element) => element.width < 2.0 && element.height < 2.0,
            None => false,
        };
        if too_small {
            self.scene.discard(id);
            self.set_tool(DrawTool::Select);
            self.request_draw();
            return;
        }
        if is_frame {
            // Drawing a frame over existing work adopts it. This is the whole point of
            // the tool, and it is why capture is on containment rather than overlap:
            // a frame that swept in everything it merely touched would take the
            // neighbouring diagram with it.
            self.refresh_frame_membership();
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
