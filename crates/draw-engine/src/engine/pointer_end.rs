use crate::camera::Point;
use crate::engine::{DrawEngine, Interaction};
use crate::freehand::points_bounds;
use crate::interaction::DrawTool;
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
        self.pointer_open = false;
        self.settle_selection();
        self.refresh_live();
    }

    fn end_pointer_step(&mut self) {
        // Where an elbow arrow's end was dragged to, which its release binds on.
        let released_at = self.binding_point;
        // The binding hint belongs to the drag, not to the document.
        self.clear_binding_suggestion();
        let Some(it) = self.interaction.take() else {
            return;
        };
        self.snap_guides.clear();
        match it {
            Interaction::Draft {
                id,
                start,
                press,
                shift,
            } => {
                if self
                    .scene
                    .get(&id)
                    .is_some_and(crate::scene::sticky::is_sticky_note)
                {
                    self.end_sticky(&id, start, press, shift);
                } else {
                    self.end_draft(&id);
                }
            }
            Interaction::TextDraft { id, press, .. } => self.end_text(&id, press),
            Interaction::Linear { id, start, pointer } => self.end_linear(&id, start, pointer),
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
                self.select_caught(ids);
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
                self.select_caught(ids);
            }
            Interaction::LinearPoint { id, handle }
                if self
                    .scene
                    .get(&id)
                    .is_some_and(crate::scene::elbow::is_elbow) =>
            {
                self.end_elbow_handle(&id, handle, released_at);
                self.settle_gesture();
            }
            Interaction::Move { ids, .. } => {
                self.renormalize_elbows_of(&ids);
                self.leave_edited_group_across_a_frame(&ids);
                // Still pending only if the pointer never moved: a click.
                self.settle_gesture();
                if let Some(ids) = self.narrow_on_click.take() {
                    self.set_selection(ids);
                }
                if let Some((id, at)) = self.reopen_text_on_click.take() {
                    self.reopen_text_at(&id, at);
                }
            }
            _ => self.settle_gesture(),
        }
    }

    /// Part of the edited group dragged into a frame, or out of one, leaves the group:
    /// `updateGroupIdsAfterEditingGroup` (`App.tsx@1118751f:11949-12075`), through
    /// [`crate::edit::leave_edited_group`].
    ///
    /// Judged as membership always is here, by where the dragged part lies — the oracle
    /// asks which frame is under the pointer — and only when the part is not a whole
    /// top-level group, which joins or leaves a frame whole with no surgery at all.
    ///
    /// The part goes with its labels. A label carries its shape's groups and is never
    /// what a drag holds, so read without them it stayed in the group its shape left —
    /// keeping that group alive as the rest plus those words — and a whole labelled group
    /// counted its labels as members left behind. The oracle hands its surgery the
    /// selection without bound text (`App.tsx@1118751f:12009`) and shares the first half of that.
    ///
    /// What a peer holds is theirs, as it is to `group_selection`: a member left behind
    /// under their hands keeps the group id rather than be rewritten and restamped, and a
    /// group of one is no group ([`crate::edit::is_live_group`]).
    fn leave_edited_group_across_a_frame(&mut self, moved: &[String]) {
        let Some(editing) = self.editing_group_id.clone() else {
            return;
        };
        let moved =
            crate::edit::with_labels(self.scene.iter_ordered(), &moved.iter().cloned().collect());
        let pending = self.scene.pending_ids();
        let part: Vec<&crate::scene::DrawElement> = moved
            .iter()
            .filter(|id| pending.contains(*id))
            .filter_map(|id| self.scene.get(id))
            .filter(|el| el.container_id.is_none() && crate::edit::is_in_group(el, &editing))
            .collect();
        let Some(top) = part.first().and_then(|el| el.group_ids.last()) else {
            return;
        };
        let whole = self
            .scene
            .iter_ordered()
            .filter(|el| crate::edit::is_in_group(el, top))
            .all(|el| moved.contains(&el.id));
        if whole || part.iter().any(|el| crate::scene::is_frame(el)) {
            return;
        }
        let Some(judged) = part
            .iter()
            .map(|el| crate::scene::element_rotated_bounds(el))
            .reduce(crate::scene::frame::union)
        else {
            return;
        };
        let target =
            crate::scene::frame::FrameOwners::new(self.scene.iter_ordered()).of_bounds(judged);
        if part.iter().all(|el| el.frame_id == target) {
            return;
        }
        let mut left = crate::edit::leave_edited_group(self.scene.iter_ordered(), &moved, &editing);
        left.retain(|el| !self.held.contains_key(&el.id));
        for element in left {
            self.scene.put(element);
        }
        self.editing_group_id = None;
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

    /// Re-derive which frame owns what this commit touched.
    ///
    /// Derived rather than remembered, from where things are. What the commit changed —
    /// and every member of a group it changed, since a group is judged by its whole box
    /// — is judged again; a frame among them (moved, resized, drawn) judges the whole
    /// board, since a frame that grew or went elsewhere can take in or let go of
    /// anything. Nothing else is touched: re-judged by a click somewhere else, stale
    /// membership anywhere on the board was rewritten, stamped and sent as part of an
    /// edit that had nothing to do with it — and undone with it.
    pub(crate) fn refresh_frame_membership(&mut self) {
        let touched = self.scene.pending_ids();
        let everything = touched
            .iter()
            .any(|id| self.scene.get(id).is_some_and(crate::scene::is_frame));
        self.judge_frame_membership(&touched, everything);
    }

    /// What the commit in progress created is judged where it lands — drawn, typed,
    /// pasted, duplicated, dropped in — as the oracle gives a new element the frame it
    /// is created in (`createGenericElementOnPointerDown`, `App.tsx@1118751f:10451-10474`; a
    /// paste, `App.duplicate.ts@1118751f:124-135`). Nothing else judges it: membership is
    /// re-judged only for what a commit touches, so a shape drawn inside a frame stayed
    /// out of it — and was left behind when the frame moved — and a pasted copy kept the
    /// frame of an original it lay far from.
    ///
    /// A frame among them takes in only what was created with it: a pasted or duplicated
    /// frame adopts nothing it lands on, where a frame *drawn* over work does
    /// ([`Self::end_draft`]). Judged as a moved frame is, the whole board over, a frame
    /// duplicated a few units over its original took the original's children.
    ///
    /// Except `given`, whose creator set the frame the oracle gives it
    /// ([`Self::push_history_keeping_frames`]).
    pub(super) fn judge_created_frame_membership(&mut self, given: &[String]) {
        let created: std::collections::HashSet<String> = self
            .scene
            .pending_ids()
            .into_iter()
            .filter(|id| self.scene.created_since_commit(id) && !given.contains(id))
            .collect();
        if !created.is_empty() {
            self.judge_frame_membership(&created, false);
        }
    }

    /// Settles membership for `touched` and the members of their groups — or for every
    /// element, when `everything`.
    fn judge_frame_membership(
        &mut self,
        touched: &std::collections::HashSet<String>,
        everything: bool,
    ) {
        if !self.scene.iter_ordered().any(crate::scene::is_frame) {
            return;
        }
        let groups: std::collections::HashSet<&String> = touched
            .iter()
            .filter_map(|id| self.scene.get(id))
            .flat_map(|el| el.group_ids.iter())
            .collect();
        let owners = crate::scene::frame::FrameOwners::new(self.scene.iter_ordered());
        let mut changes: Vec<(String, Option<String>)> = Vec::new();
        for element in self.scene.iter_ordered() {
            if element.is_deleted || crate::scene::is_frame(element) {
                continue;
            }
            if !everything
                && !touched.contains(&element.id)
                && !element.group_ids.iter().any(|id| groups.contains(id))
            {
                continue;
            }
            let owner = owners.of(element);
            if owner != element.frame_id {
                changes.push((element.id.clone(), owner));
            }
        }
        // Stamped by the commit, as every change is (`stamp.rs`) — so one created here
        // keeps the stamp it was created with.
        let mut joined: Vec<(String, std::collections::HashSet<String>)> = Vec::new();
        for (id, frame_id) in changes {
            if let Some(frame) = &frame_id {
                match joined.iter_mut().find(|(f, _)| f == frame) {
                    Some((_, ids)) => {
                        ids.insert(id.clone());
                    }
                    None => joined.push((frame.clone(), [id.clone()].into())),
                }
            }
            self.scene
                .update(&id, |element| element.frame_id = frame_id);
        }
        // A frame resized puts its children back in one run below it even when none joined:
        // the oracle takes them all out and adds them back on every resize
        // (`App.tsx@1118751f:12097-12117`), which also mends a child left above its frame.
        // Not a frame moved: a drag adds only what it carried (`:12040-12059`).
        let resized: Vec<String> = touched
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| crate::scene::is_frame(el))
            .filter(|el| {
                matches!(self.scene.committed(&el.id), Some(Some(before))
                    if before.width != el.width || before.height != el.height)
            })
            .map(|el| el.id.clone())
            .collect();
        for frame in resized {
            if !joined.iter().any(|(f, _)| *f == frame) {
                joined.push((frame, std::collections::HashSet::new()));
            }
        }
        for (frame, ids) in joined {
            self.stack_under_frame(&frame, &ids, touched);
        }
    }

    /// What joined `frame` goes directly below it, as the oracle keeps a frame's children
    /// in one run under it: a new element is inserted there (`insertNewElements`,
    /// `App.tsx@1118751f:7754-7782`), and one dragged in, pasted in or taken in by a new
    /// frame is moved there (`addElementsToFrame`, `packages/element/src/frame.ts@1118751f:
    /// 538-635`). Drawn, pasted or dragged in, a shape used to stay on top of the board,
    /// above the frame it belonged to.
    ///
    /// The run is what joined, with what the gesture carried of the frame's — a selection
    /// dragged in partly from inside goes together, in its own order, since the oracle
    /// adds the selected elements that are in the frame (`App.tsx@1118751f:12046-12059`)
    /// and reorders whenever they do not all share it already (`getCommonFrameId`,
    /// `frame.ts@1118751f:503-519`). A member the commit only re-routed, an arrow bound to
    /// what was dragged, keeps its place. When the frame itself was drawn, resized or
    /// moved, the run is every member: the children it had, in their order, then what it
    /// took in (`replaceAllElementsInFrame`, `:684-694`, over `getElementsInResizingFrame`,
    /// `:283-377`, which lists loose newcomers before grouped ones where these keep the
    /// stack's order). Each shape takes its label, directly above it (`:578-582`).
    ///
    /// Directly below the frame, or directly above its highest member when one sits above
    /// it (`getFrameChildrenInsertionIndex`, `frame.ts@1118751f:521-536`). A label counts
    /// with its shape's frame, since here only the shape carries it.
    fn stack_under_frame(
        &mut self,
        frame: &str,
        joined: &std::collections::HashSet<String>,
        touched: &std::collections::HashSet<String>,
    ) {
        let whole = touched.contains(frame);
        let carried = if whole {
            std::collections::HashSet::new()
        } else {
            self.moving_selection()
        };
        let members = self
            .scene
            .iter_ordered()
            .filter(|el| el.container_id.is_none() && el.frame_id.as_deref() == Some(frame));
        let run: Vec<&crate::scene::DrawElement> = if whole {
            let (newcomers, had): (Vec<_>, Vec<_>) =
                members.partition(|el| joined.contains(&el.id));
            had.into_iter().chain(newcomers).collect()
        } else {
            members
                .filter(|el| joined.contains(&el.id) || carried.contains(&el.id))
                .collect()
        };
        let mut block: Vec<String> = Vec::new();
        for element in run {
            block.push(element.id.clone());
            let label = element.bound_text_id.as_ref().filter(|label| {
                self.scene.get(label).is_some_and(|text| {
                    !text.is_deleted && text.container_id.as_deref() == Some(&element.id)
                })
            });
            block.extend(label.cloned());
        }
        let anchor = {
            let moving: std::collections::HashSet<&str> =
                block.iter().map(String::as_str).collect();
            self.scene
                .iter_ordered()
                .rev()
                .filter(|el| !moving.contains(el.id.as_str()))
                .find_map(|el| {
                    if el.id == frame {
                        return Some((el.id.clone(), false));
                    }
                    let shape = match &el.container_id {
                        Some(container) => self.scene.get(container),
                        None => Some(el),
                    };
                    shape
                        .is_some_and(|shape| shape.frame_id.as_deref() == Some(frame))
                        .then(|| (el.id.clone(), true))
                })
        };
        match anchor {
            Some((child, true)) => self.scene.place_above(&block, &child),
            Some((frame, false)) => self.scene.place_below(&block, &frame),
            None => {}
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

    /// Settles a new sticky note (`App.tsx@1118751f:11812-11890`). A gesture under the drag
    /// threshold is a click: the default square, centred on the press and then snapped. A
    /// drag keeps its size, grown to the least that holds one line at the next text's size
    /// — or the note would grow on the first keystroke — and squared unless Shift was
    /// held, growing away from the far edge the drag left where it was.
    ///
    /// Then the note is its own step of undo, and the tool, unless it is locked, gives way
    /// to typing into it. Its label is not made here: `edit_label` makes it, and a label
    /// left empty is thrown away with no trace.
    fn end_sticky(&mut self, id: &str, start: Point, press: Point, shift: bool) {
        use crate::scene::sticky::{sticky_min_size, DEFAULT_STICKY_NOTE_SIZE};
        let Some(mut note) = self.scene.get(id).cloned() else {
            return;
        };
        let zoom = self.camera.scale;
        let click = note.width * zoom < super::DRAGGING_THRESHOLD_PX
            && note.height * zoom < super::DRAGGING_THRESHOLD_PX;
        if click {
            let size = DEFAULT_STICKY_NOTE_SIZE;
            let at = self.snap(Point {
                x: press.x - size / 2.0,
                y: press.y - size / 2.0,
            });
            (note.x, note.y, note.width, note.height) = (at.x, at.y, size, size);
        } else {
            let line_height = crate::text::font::family(self.next_font_family)
                .map_or(crate::render::TEXT_LINE_HEIGHT, |family| family.line_height);
            let (min_width, min_height) = sticky_min_size(self.next_font_size, line_height);
            let mut width = note.width.max(min_width);
            let mut height = note.height.max(min_height);
            if !shift {
                width = width.max(height);
                height = width;
            }
            note.x = if note.x < start.x {
                start.x - width
            } else {
                note.x
            };
            note.y = if note.y < start.y {
                start.y - height
            } else {
                note.y
            };
            note.width = width;
            note.height = height;
        }
        note.base_height = Some(note.height);
        self.scene.put(note.clone());
        if self.tool_locked {
            self.clear_selection();
            self.push_history();
            self.request_draw();
            return;
        }
        self.settle_tool();
        self.set_selection(vec![id.to_string()]);
        self.push_history();
        self.edit_label(&note);
        self.request_draw();
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
    fn end_text(&mut self, id: &str, press: Point) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let dragged = element.width.abs() >= TEXT_BOX_MIN_DRAG;
        if dragged {
            element.auto_resize = Some(false);
            element.width = element.width.abs();
            // One line to start with. The height follows the text from here on, because
            // a column's height is a consequence of its width, never a thing you set.
            element.height = crate::text::layout::font_size_of(&element)
                * crate::scene::resolved_line_height(&element);
        } else {
            // Put back the click-sized box `begin_text` could not commit to — its first
            // line centred on the click, or, with the grid on, its top-left grid point
            // (`text_creation_point`). From `press`, the unsnapped position, not `start`:
            // the oracle floors the raw scene point, and `start` is already rounded to
            // the grid by the per-gesture snap every gesture shares.
            let at = self.text_creation_point(&element, press);
            element.x = at.x;
            element.y = at.y;
            element.width = 4.0;
            element.height = crate::text::layout::font_size_of(&element);
        }
        self.scene.put(element.clone());
        self.set_selection(vec![id.to_string()]);
        self.request_text_edit(&element, None);
        self.settle_tool();
    }

    /// The fork between a linear tool's two gestures, decided on release.
    ///
    /// Above [`LINEAR_CLICK_PX`] of travel the gesture was a **drag**: one segment, drawn
    /// and done, which is what this always used to do. Below it, it was a **click**, and a
    /// click starts a path that keeps taking points — so the element it left behind is
    /// handed to `begin_multi_linear` rather than thrown away for being too small.
    ///
    /// Measured in screen pixels from the press to the release, because the threshold is a
    /// statement about the hand rather than about the drawing. Read off the drawing, a
    /// bound arrow — its ends pulled onto two outlines a few pixels apart, or collapsed
    /// between overlapping shapes — turned a real drag into a click, and a drag past the
    /// threshold was thrown away as too small once zoomed in far enough.
    fn end_linear(&mut self, id: &str, start: Point, pointer: Point) {
        if self.scene.get(id).is_none() {
            return;
        }
        let travelled = (pointer.x - start.x).hypot(pointer.y - start.y) * self.camera.scale;
        if travelled < super::LINEAR_CLICK_PX {
            self.begin_multi_linear(id);
            return;
        }
        if self
            .scene
            .get(id)
            .is_some_and(crate::scene::elbow::is_elbow)
        {
            let shape = self.elbow_target_at(pointer);
            self.drop_elbow_end(id, crate::scene::binding::End::End, shape.as_deref());
            if self.discard_invisible_elbow(id) {
                self.settle_tool();
                self.request_draw();
                return;
            }
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
        self.pointer_open = false;
        self.settle_selection();
        self.refresh_live();
    }

    fn cancel_pointer_step(&mut self) {
        self.clear_binding_suggestion();
        // A press Escape interrupted is no click, and what it would have narrowed to was
        // worked out at a level Escape may be about to leave.
        self.narrow_on_click = None;
        self.reopen_text_on_click = None;
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
