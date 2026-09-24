use crate::edit::expand_within;
use crate::engine::{DrawEngine, Interaction};
use crate::freehand::points_bounds;
use crate::interaction::{is_degenerate_linear, DrawTool};
use crate::selection::{elements_in_lasso, elements_in_marquee_among, marquee_rect, LassoMode};

/// World units the pointer must travel before a drag counts as a marquee rather than a
/// click. Small enough that a deliberate rubber-band always registers, large enough to
/// absorb the jitter of a mouse being clicked.
const MARQUEE_MIN_DRAG: f64 = 2.0;

/// How wide a text gesture has to be before it means "a column this wide" rather than
/// "put the caret here". Wider than [`MARQUEE_MIN_DRAG`] because the consequence of
/// getting it wrong is worse: a marquee that selects nothing costs a click, a column
/// four pixels wide costs a retype.
const TEXT_BOX_MIN_DRAG: f64 = 12.0;

impl DrawEngine {
    pub fn end_pointer(&mut self) {
        self.end_pointer_step();
        self.refresh_live();
    }

    fn end_pointer_step(&mut self) {
        // The binding hint belongs to the drag, not to the document.
        self.binding_highlight = None;
        self.binding_point = None;
        self.bind_drag_origin = None;
        let Some(it) = self.interaction.take() else {
            return;
        };
        self.snap_guides.clear();
        match it {
            Interaction::Draft { id, .. } => self.end_draft(&id),
            Interaction::TextDraft { id, .. } => self.end_text(&id),
            Interaction::Linear { id, .. } => self.end_linear(&id),
            // The release is what places a point. Committing on the press instead would
            // freeze it where the button went down, so it could never be nudged before
            // being let go of.
            Interaction::MultiLinearPress => self.commit_multi_point(),
            Interaction::CornerRadius { id, .. } => self.end_corner_radius(&id),
            Interaction::Freedraw { id, .. } => self.end_freedraw(&id),
            Interaction::Erase { .. } => self.erase_marked(),
            Interaction::Lasso { path, base } => {
                // The engine decides what the loop caught, not the host: this is
                // geometry, and every frontend on this engine must answer it the same.
                let mut ids = base;
                ids.extend(elements_in_lasso(
                    self.scene.iter_ordered(),
                    &path,
                    LassoMode::Contain,
                ));
                let editing = self.editing_group_id.clone();
                let expanded = expand_within(self.scene.iter_ordered(), ids, editing.as_deref());
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
                        self.scene.iter_ordered().filter(|el| !self.untouchable(el)),
                        rect,
                    ));
                }
                let editing = self.editing_group_id.clone();
                let expanded = expand_within(self.scene.iter_ordered(), ids, editing.as_deref());
                self.set_selection(expanded);
            }
            _ => self.settle_gesture(),
        }
    }

    /// Commits what a move, resize, rotation or point drag did to the scene.
    fn settle_gesture(&mut self) {
        // Membership follows position, so it is settled once the gesture is: re-deriving
        // it per pointer move would make an element flicker between frames as it crossed
        // a border mid-drag, and would put a history-worthy change behind every sample.
        self.refresh_frame_membership();
        self.apply_bindings();
        self.push_history();
        self.request_draw();
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

    /// Decides which text gesture just happened, and opens the editor for it.
    ///
    /// Below `TEXT_BOX_MIN_DRAG` the gesture was a click and the element auto-sizes;
    /// above it, the box that was dragged out is the column, fixed to that width.
    ///
    /// The threshold is not cosmetic. A click is never perfectly still, and without it a
    /// two-pixel wobble would produce a two-pixel column that wraps every character onto
    /// its own line — indistinguishable from a click to the person who made it, and the
    /// worst of the available outcomes.
    fn end_text(&mut self, id: &str) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let dragged = element.width.abs() >= TEXT_BOX_MIN_DRAG;
        if dragged {
            element.auto_resize = Some(false);
            element.width = element.width.abs();
            // One line to start with. The height follows the text from here on, because
            // a column's height is a consequence of its width, never a thing you set.
            element.height = self.font_size_of(&element) * crate::TEXT_LINE_HEIGHT;
        } else {
            // Put back the click-sized box `begin_text` could not commit to.
            element.width = 4.0;
            element.height = self.font_size_of(&element);
        }
        self.scene.put(element.clone());
        self.set_selection(vec![id.to_string()]);
        self.request_text_edit(&element);
        self.settle_tool();
    }

    /// The fork between a linear tool's two gestures, decided on release.
    ///
    /// Above [`LINEAR_CLICK_PX`] of travel the gesture was a **drag**: one segment, drawn
    /// and done, which is what this always used to do. Below it, it was a **click**, and a
    /// click starts a path that keeps taking points — so the element it left behind is
    /// handed to `begin_multi_linear` rather than thrown away for being too small.
    ///
    /// Measured in screen pixels off the last point, which is the drag delta, because the
    /// threshold is a statement about the hand rather than about the drawing.
    fn end_linear(&mut self, id: &str) {
        let Some(element) = self.scene.get(id) else {
            return;
        };
        let travelled = element
            .points
            .as_deref()
            .and_then(|points| points.last())
            .map_or(0.0, |&[dx, dy]| dx.hypot(dy))
            * self.camera.scale;
        if travelled < super::LINEAR_CLICK_PX {
            self.begin_multi_linear(id);
            return;
        }

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
                    let id = element.id.clone();
                    self.scene.put(element.clone());
                    // The whole gesture is one history entry: the stroke is never
                    // committed on its own, so one undo takes the shape away rather
                    // than turning it back into a scribble nobody drew.
                    // Auto-shape watches one stroke and hands back a shape, so the
                    // gesture ends with something you will want to place: it settles and
                    // leaves the result selected, like every shape tool.
                    //
                    // The pencil does not. Drawing by hand is many strokes with the pen
                    // lifted between them, so settling would make every stroke after the
                    // first a marquee and you would reach for the tool again between
                    // every mark. Nor is the stroke left selected: a frame and eight
                    // handles over the drawing you are still making would swallow the
                    // next stroke that began inside them.
                    if self.get_tool() == DrawTool::AutoShape {
                        self.convert_to_shape(&id);
                        self.settle_tool();
                        self.set_selection(vec![element.id]);
                    }
                    self.push_history();
                    self.request_draw();
                    return;
                }
            }
            self.scene.discard(id);
        }
        // A tap too short to keep must not take the tool with it either.
        if self.get_tool() != DrawTool::Freedraw {
            self.set_tool(DrawTool::Select);
        }
        self.request_draw();
    }

    pub fn cancel_pointer(&mut self) {
        self.cancel_pointer_step();
        self.refresh_live();
    }

    fn cancel_pointer_step(&mut self) {
        // Escape ends an open path rather than throwing it away, which is Excalidraw's
        // binding too: both Escape and Enter run `actionFinalize`. A path of six points
        // lost to a reflexive Escape is six points of work gone, and undo is the thing
        // that exists for changing your mind.
        if self.multi_linear.is_some() {
            self.finish_linear();
            return;
        }
        // A sweep in progress is backed out of first. Escape is how you change your mind
        // about what the eraser has marked, and stepping out of a group instead — the
        // next rule down — left the sweep running, and the release deleted it all.
        if matches!(self.interaction, Some(Interaction::Erase { .. })) {
            self.interaction = None;
            self.clear_erasing();
            return;
        }
        // Escape steps out of a group before it does anything else. It is the way back
        // up, and without it the only exit is clicking something outside — which is
        // awkward when the group fills the screen.
        if self.leave_group() {
            return;
        }
        let it = self.interaction.take();
        self.snap_guides.clear();
        match it {
            Some(
                Interaction::Draft { id, .. }
                | Interaction::Freedraw { id, .. }
                | Interaction::Linear { id, .. },
            ) => {
                self.scene.discard(&id);
                self.set_tool(DrawTool::Select);
            }
            // A gesture that already changed elements is committed, not abandoned: the
            // elements stay where the drag left them either way, and leaving that
            // uncommitted meant it was never saved — and, pending, it kept refusing
            // every peer's edit of those elements until some unrelated commit came.
            Some(Interaction::CornerRadius { id, .. }) => self.end_corner_radius(&id),
            // Escape mid-sweep lets the marks go: nothing was deleted yet, and the point
            // of marking first is that you can still back out.
            Some(Interaction::Erase { .. }) => self.clear_erasing(),
            Some(
                Interaction::Move { .. }
                | Interaction::Resize { .. }
                | Interaction::Rotate { .. }
                | Interaction::ResizeGroup { .. }
                | Interaction::RotateGroup { .. }
                | Interaction::LinearPoint { .. },
            ) => self.settle_gesture(),
            _ => {}
        }
        self.clear_selection();
        self.request_draw();
    }
}
