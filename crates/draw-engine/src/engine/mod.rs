use std::collections::{HashMap, HashSet};

use crate::camera::{Camera, Point, IDENTITY};
use crate::history::SnapshotHistory;
use crate::interaction::{DrawTool, SnapGuide};
use crate::render::{light_theme, DrawTheme, GridSettings};
use crate::scene::{
    default_element_style, DrawElement, DrawElementStyle, DrawElementStylePatch, Scene, TextAlign,
    VerticalAlign,
};

mod arrange;
mod autoshape;
mod bucket;
mod clipboard;
mod debug;
mod eraser;
mod frame;
mod hover;
mod image;
mod live;
pub use image::EmbedFrame;
pub use peers::Peer;
mod multi_linear;
mod peers;
mod pointer;
mod pointer_end;
mod pointer_move;
mod radius;
mod stamp;
mod style;
mod text;
mod types;

pub use debug::{DebugInteraction, DebugScene, DebugState, DebugViewport};
pub use frame::{NoopPainter, PaintView, Painter, PeerMark};
pub use hover::HoverCursor;
pub(crate) use types::{default_measure, Interaction};
pub use types::{merge_style_patch, EngineEvents, Notice, TextEditRequest};

const HANDLE_PX: f64 = 8.0;
/// Reach for the handles that are not laid out by [`crate::selection::HandleLayout`] —
/// the point handles on a line or arrow, which sit *on* the geometry and so have no
/// element interior to stay clear of.
const HANDLE_HIT_PX: f64 = 10.0;
/// Excalidraw's `MINIMUM_ARROW_SIZE` (`constants.ts:22`), used as they use it: the fork
/// between the two gestures a linear tool offers.
///
/// Below this the press and release was a **click**, which starts a path taken point by
/// point; above it, a **drag**, which draws one segment and ends. Measured in screen
/// pixels, because it is a statement about the hand rather than about the drawing — the
/// same wobble is the same wobble at every zoom.
const LINEAR_CLICK_PX: f64 = 20.0;
/// Excalidraw's `DRAGGING_THRESHOLD` (`packages/common/src/constants.ts:21`): how far a
/// bound arrow has to be dragged before the drag is allowed to detach it.
///
/// The click that selects an arrow is never perfectly still, and without a floor that
/// click would quietly unbind it. Screen pixels, because it is about the hand.
const DRAGGING_THRESHOLD_PX: f64 = 10.0;
/// How long a segment must be on screen before it gets its own midpoint handle.
/// Two handles a few pixels apart cannot be aimed at deliberately.
const LINEAR_MIDPOINT_MIN_PX: f64 = 28.0;
/// How close, in screen pixels, an endpoint must come to a shape to bind to it.
/// Derived from pixels rather than world units so it does not shrink to nothing when
/// zoomed out.
const BINDING_HOVER_PX: f64 = 32.0;
/// How close, in screen pixels, a dropped arrow end must come to a side midpoint to snap
/// onto it. Excalidraw's reach is its binding distance, 15 scene units at ordinary zoom
/// (`packages/element/src/utils.ts:634-695`); kept in pixels here so it feels the same at
/// every zoom, and capped per shape by `midpoint_snap_radius`.
const MIDPOINT_SNAP_PX: f64 = 16.0;
const ROTATE_GAP_PX: f64 = 26.0;
/// Excalidraw's `DEFAULT_COLLISION_THRESHOLD`: how near a click must be to an element.
const COLLISION_PX: f64 = 10.0;
const DEFAULT_FONT_SIZE: f64 = 20.0;
const SNAP_PX: f64 = 6.0;
const PASTE_OFFSET: f64 = 12.0;
const MOTION_MS: f64 = 140.0;

pub struct DrawEngine {
    scene: Scene,
    theme: DrawTheme,
    grid: GridSettings,
    pub camera: Camera,
    width: f64,
    height: f64,
    dpr: f64,
    dirty: bool,
    motion_until: f64,
    disposed: bool,
    now_ms: f64,
    measure_text: fn(&str, f64) -> (f64, f64),
    tool: DrawTool,
    /// Where a toggle tool goes back to. Never a toggle tool itself, so pressing the
    /// eraser key three times enters, leaves, and enters again rather than oscillating.
    tool_before_toggle: DrawTool,
    tool_locked: bool,
    next_style: DrawElementStylePatch,
    next_font_size: f64,
    /// The alignments the next text will be created with, when one was chosen with
    /// nothing selected. `None` means nothing was chosen, so the new element is left
    /// unset too and keeps resolving through its role — picking an alignment must not be
    /// the same as never touching the control.
    next_text_align: Option<TextAlign>,
    next_vertical_align: Option<VerticalAlign>,
    interaction: Option<Interaction>,
    /// The linear element whose individual points are currently on offer.
    ///
    /// A path of more than two points is a shape: it selects to a box that resizes and
    /// turns it. Its corners are still reachable, but behind a double click, so that the
    /// common gesture is the common one. Excalidraw calls this the line editor and enters
    /// it the same way.
    ///
    /// Held by id rather than by index because the scene is reordered underneath it.
    editing_linear: Option<String>,
    /// The group that has been stepped into, if any.
    ///
    /// Session state, not document state: which level you are looking at is a property of
    /// your view, so it is never serialized. Every click and every group operation is
    /// resolved relative to it by `selected_group_for`.
    editing_group_id: Option<String>,
    /// The path being placed point by point, if one is.
    ///
    /// Distinct from [`Self::interaction`] because this is the one gesture that spans
    /// several of them: press, release, move, press, release, and only then — perhaps a
    /// dozen clicks later — an end. Putting it in `Interaction` would have it thrown away
    /// by the release that places its second point.
    multi_linear: Option<types::MultiLinear>,
    selected_ids: HashSet<String>,
    clipboard_buffer: Option<String>,
    snap_guides: Vec<SnapGuide>,
    /// Whether a moving selection snaps to other elements' edges and centres.
    ///
    /// **Off by default**, as Excalidraw's `objectsSnapModeEnabled` is (`appState.ts:129`).
    /// It used to be always on, and the guides pulled every drag a few pixels sideways
    /// toward whatever happened to be near — with no way to turn that off short of
    /// holding a modifier through every drag. Holding Ctrl/Cmd inverts it for one
    /// gesture either way; see `move_selection`.
    objects_snap: bool,
    /// The shape a dragged arrow endpoint would attach to, if released now.
    ///
    /// The painter outlines it, so the attachment is visible before it is committed —
    /// without that, binding is invisible until after the fact and feels like a
    /// coincidence rather than a tool.
    binding_highlight: Option<String>,
    /// Where the arrow end being placed — or the arrow tool hovering — is, for the painter
    /// to mark which side midpoint it would snap to. Set together with
    /// `binding_highlight` and cleared with it.
    binding_point: Option<crate::camera::Point>,
    /// The other end's binding as it was when the drag of one end began, so a drag that
    /// passes over the shape the other end is on and moves on gives that end back.
    /// `(arrow id, the end being dragged, the other end's anchor)`. See `pointer_move.rs`.
    bind_drag_origin: Option<(String, crate::scene::binding::End, Option<crate::scene::binding::Anchor>)>,
    /// Laser strokes on screen, drawn and fading.
    ///
    /// Not part of the scene and never serialized: a laser mark is a gesture, like a
    /// finger pointed at a slide, and putting it in the document would put it in the
    /// undo stack, the autosave and every other participant's board.
    laser: crate::interaction::LaserTrails,
    /// Each peer's laser trails, by peer id — see `peers.rs`. Session state like the
    /// local ones: in no scene, no history, no save.
    peer_lasers: HashMap<String, peers::PeerLaser>,
    /// What the eraser sweep in progress has marked, drawn faded until release deletes
    /// it. Session state, like the selection: nothing in the document changes until the
    /// sweep ends, so Escape can let it all go. See `eraser.rs`.
    erasing: HashSet<String>,
    /// Bumped whenever `erasing` changes, so the painter's cached layer, which holds the
    /// faded picture, is redrawn when the marks move.
    erasing_revision: u64,
    /// Alt, as of the last pointer move. See `set_alt_held`.
    alt_held: bool,
    /// Ctrl/Cmd, as of the last press or move: while held, an arrow end binds to nothing,
    /// as in Excalidraw (`App.tsx:5753-5761`). See `set_ctrl_held`.
    ctrl_held: bool,
    history: SnapshotHistory<stamp::HistoryEntry>,
    history_seq: u64,
    /// A peer's copy of an element with an uncommitted local change, refused because a
    /// gesture in progress wins, as in Excalidraw. The commit stamps above it, or adopts
    /// it if the gesture came to nothing. See `stamp.rs`.
    remote_refused: std::collections::HashMap<String, DrawElement>,
    events: EngineEvents,
    /// The other people in the room, and what they hold. See `peers.rs`.
    peers: Vec<peers::Peer>,
    /// Element id to the index in `peers` of whoever holds it.
    held: HashMap<String, usize>,
}

impl Default for DrawEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawEngine {
    pub fn new() -> Self {
        Self {
            scene: Scene::new([]),
            theme: light_theme(),
            grid: GridSettings::default(),
            camera: IDENTITY,
            width: 0.0,
            height: 0.0,
            dpr: 1.0,
            dirty: true,
            motion_until: 0.0,
            disposed: false,
            now_ms: 0.0,
            measure_text: default_measure,
            tool: DrawTool::Select,
            tool_before_toggle: DrawTool::Select,
            tool_locked: false,
            next_style: DrawElementStylePatch::default(),
            next_font_size: DEFAULT_FONT_SIZE,
            next_text_align: None,
            next_vertical_align: None,
            interaction: None,
            editing_linear: None,
            editing_group_id: None,
            multi_linear: None,
            selected_ids: HashSet::new(),
            clipboard_buffer: None,
            snap_guides: Vec::new(),
            objects_snap: false,
            binding_highlight: None,
            binding_point: None,
            bind_drag_origin: None,
            laser: crate::interaction::LaserTrails::default(),
            peer_lasers: HashMap::new(),
            erasing: HashSet::new(),
            erasing_revision: 0,
            alt_held: false,
            ctrl_held: false,
            history: SnapshotHistory::new(stamp::HistoryEntry::default(), |entry| entry.seq, 200),
            history_seq: 0,
            remote_refused: std::collections::HashMap::new(),
            events: EngineEvents::default(),
            peers: Vec::new(),
            held: HashMap::new(),
        }
    }

    pub fn set_now(&mut self, now_ms: f64) {
        self.now_ms = now_ms;
        // Faded laser strokes are dropped here rather than while painting, so the paint
        // view can stay borrow-only. Without it a long presentation keeps one dead stroke
        // per flick and walks all of them every frame to draw nothing.
        self.laser.prune(now_ms);
        self.prune_peer_lasers(now_ms);
    }

    pub fn set_measure_text(&mut self, measure: fn(&str, f64) -> (f64, f64)) {
        self.measure_text = measure;
    }

    pub fn drain_events(&mut self) -> EngineEvents {
        std::mem::take(&mut self.events)
    }

    pub fn set_scene(&mut self, scene: Scene) {
        self.scene = scene;
        // Whatever the painter holds is a picture of the scene this replaced.
        self.scene.start_journal();
        // Built by adding every element, so every element was pending for the host —
        // and the first edit after loading a board sent the whole board as its delta:
        // five megabytes of JSON for one stroke on a board of 2,000, serialised here and
        // parsed twice more on the other side. The host supplied this scene; it has it.
        self.scene.forget_pending();
        self.reset_history();
        self.request_draw();
    }

    pub fn get_scene(&self) -> Vec<DrawElement> {
        self.scene.to_array()
    }

    pub fn set_theme(&mut self, theme: DrawTheme) {
        self.theme = theme;
        self.request_draw();
    }

    pub fn theme(&self) -> &DrawTheme {
        &self.theme
    }

    pub fn set_grid(&mut self, grid: GridSettings) {
        self.grid = grid;
        self.request_draw();
    }

    pub fn grid(&self) -> GridSettings {
        self.grid
    }

    /// Turns snapping to other elements on or off.
    ///
    /// A preference, like the grid: not part of the drawing, and not undoable.
    pub fn set_objects_snap(&mut self, on: bool) {
        self.objects_snap = on;
        if !on {
            self.snap_guides.clear();
        }
        self.request_draw();
    }

    pub fn objects_snap(&self) -> bool {
        self.objects_snap
    }

    /// A world point rounded onto the grid, or unchanged when the grid is not snapping.
    ///
    /// Every gesture that positions something goes through this, so turning the grid on
    /// changes drawing, dragging and resizing together rather than only one of them.
    pub(crate) fn snap(&self, point: Point) -> Point {
        let (x, y) = self.grid.snap_point(point.x, point.y);
        Point { x, y }
    }

    pub fn set_viewport(&mut self, width: f64, height: f64, dpr: f64) {
        self.width = width;
        self.height = height;
        self.dpr = dpr;
        self.request_draw();
    }

    pub fn viewport(&self) -> (f64, f64, f64) {
        (self.width, self.height, self.dpr)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn in_motion(&self) -> bool {
        self.now_ms < self.motion_until
    }

    /// Whether another frame is owed, for any reason.
    ///
    /// The single question a host's frame loop should ask. `is_dirty` alone is not it:
    /// dirtiness means "something changed", and a fading laser changes with no input at
    /// all, so a loop that stopped at `dirty || in_motion` would freeze a trail
    /// part-faded until something unrelated happened to repaint.
    ///
    /// Answering here rather than in each host is the point — otherwise every frontend
    /// has to learn, separately, every reason the engine might still have work to do.
    pub fn needs_frame(&self) -> bool {
        !self.disposed
            && (self.dirty
                || self.in_motion()
                || self.laser.is_active(self.now_ms)
                || self.peer_laser_active())
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed
    }

    pub fn destroy(&mut self) {
        self.disposed = true;
        self.dirty = false;
    }

    pub(crate) fn request_draw(&mut self) {
        if !self.disposed {
            self.dirty = true;
        }
    }

    fn bump_motion(&mut self) {
        self.motion_until = self.now_ms + MOTION_MS;
    }

    pub fn screen_to_world(&self, sx: f64, sy: f64) -> Point {
        crate::screen_to_world(self.camera, sx, sy)
    }

    /// Topmost element under the pointer, locked ones included.
    ///
    /// By reference, like [`Self::selectable_hit`]: this is called from JS on hover and
    /// on every click, and cloning the document to answer one question about one element
    /// made the cost of a click scale with the size of the board.
    pub fn hit_test(&self, sx: f64, sy: f64, tolerance: f64) -> Option<DrawElement> {
        let world = self.screen_to_world(sx, sy);
        self.scene
            .iter_ordered()
            .rev()
            .find(|el| crate::hit_test_element(el, world.x, world.y, tolerance))
            .cloned()
    }

    fn selectable(&self) -> Vec<DrawElement> {
        self.scene
            .iter_ordered()
            .filter(|el| !self.untouchable(el))
            .cloned()
            .collect()
    }

    /// Topmost unlocked element under the pointer.
    ///
    /// Walks the scene by reference. This used to clone every element in the document —
    /// three `String`s and a point vector each — on every click and on every pointer move
    /// while erasing, which made a single click cost more the larger the board got.
    fn selectable_hit(&self, sx: f64, sy: f64, tolerance: f64) -> Option<DrawElement> {
        let world = self.screen_to_world(sx, sy);
        // Reversed: the topmost element in z-order wins.
        self.scene
            .iter_ordered()
            .rev()
            .find(|el| {
                !self.untouchable(el) && crate::hit_test_element(el, world.x, world.y, tolerance)
            })
            .cloned()
    }

    /// How close a click has to be to count as landing on an element.
    ///
    /// Excalidraw's `DEFAULT_COLLISION_THRESHOLD`, in screen pixels so it does not shrink
    /// to nothing when zoomed out. It matters more than it used to: a transparent shape
    /// is hit only on its outline, and a two-pixel band around a hand-drawn stroke is not
    /// something anyone can aim at.
    pub(crate) fn collision_tolerance(&self) -> f64 {
        COLLISION_PX / self.camera.scale
    }

    /// Where the selection frame and handles sit, for both painting and hit testing.
    pub(crate) fn handle_layout(&self) -> crate::selection::HandleLayout {
        crate::selection::HandleLayout::screen(HANDLE_PX, ROTATE_GAP_PX, self.camera.scale)
    }

    pub fn set_tool(&mut self, tool: DrawTool) {
        self.set_tool_step(tool);
        self.refresh_live();
    }

    fn set_tool_step(&mut self, tool: DrawTool) {
        if tool == self.tool {
            return;
        }
        // Reaching for another tool is an answer to "is this path finished?" too, and
        // leaving it open would strand it: nothing else would ever end it, and the next
        // click of the line tool would carry on from wherever it was abandoned. Not
        // `finish_linear`, which settles the tool — the caller is already choosing one.
        self.finish_multi_linear();
        // Leaving the eraser mid-sweep lets its marks go, as Excalidraw's `endPath` on a
        // tool change does: with the eraser gone, nothing would ever delete them or
        // clear them, and they would stay faded.
        if self.tool == DrawTool::Eraser {
            self.abandon_sweep();
        }
        // Remembered before the move, and never the tool being left if that tool is
        // itself a toggle — otherwise pressing E twice would bounce the eraser against
        // itself instead of returning you to what you were drawing.
        if !crate::is_toggle_tool(self.tool) {
            self.tool_before_toggle = self.tool;
        }
        self.tool = tool;
        self.events.tool = Some(tool);
        // Picking any tool but select puts down whatever was being held.
        // `setActiveTool` does the same (`App.tsx:6211-6226`), and the reason is the
        // style panel: it offers the union of what the active tool can style and what the
        // selection can, and a swatch applies to the selection whenever there is one. So
        // a shape left selected from a moment ago silently captures the colour meant for
        // the next thing drawn — choosing a green for the bucket recoloured the rectangle
        // behind it, and the fill still came out the fallback shade.
        //
        // Going back to select is how a person picks up what they have just made, so that
        // one direction keeps it.
        if tool != DrawTool::Select {
            self.clear_selection();
        }
    }

    /// Choose a tool the way a keyboard shortcut does.
    ///
    /// The difference from [`set_tool`](Self::set_tool) is the return trip: pressing the
    /// hand or the eraser key while that tool is already active goes back to the tool it
    /// interrupted. A toolbar button must *not* do this — the button shows the tool as
    /// active, so switching away on a second click would contradict what is on screen —
    /// which is why the two doors are separate.
    pub fn activate_tool(&mut self, tool: DrawTool) {
        self.activate_tool_step(tool);
        self.refresh_live();
    }

    fn activate_tool_step(&mut self, tool: DrawTool) {
        if tool == self.tool && crate::is_toggle_tool(tool) {
            // Toggling back is leaving the tool too, and skips `set_tool`: pressing E
            // again mid-sweep switched to select while the sweep went on marking, and
            // the release deleted what it had marked.
            if tool == DrawTool::Eraser {
                self.abandon_sweep();
            }
            let back = self.tool_before_toggle;
            self.tool = back;
            self.events.tool = Some(back);
            return;
        }
        self.set_tool(tool);
    }

    pub fn get_tool(&self) -> DrawTool {
        self.tool
    }

    pub fn set_next_style(&mut self, patch: DrawElementStylePatch) {
        self.next_style = merge_style_patch(&self.next_style, &patch);
    }

    pub fn get_next_style(&self) -> DrawElementStyle {
        crate::merge_style(&default_element_style(), &self.next_style)
    }

    pub fn get_selection(&self) -> Vec<String> {
        self.selected_ids.iter().cloned().collect()
    }

    pub fn get_selected_elements(&self) -> Vec<DrawElement> {
        self.selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id).cloned())
            .filter(|el| !el.is_deleted)
            .collect()
    }

    fn set_selection(&mut self, ids: impl IntoIterator<Item = String>) {
        // The one door every selection goes through — a click, a marquee, a lasso, select
        // all, a group expanding — so what someone else holds can never be let in, and
        // nothing that acts on the selection can touch it. See `peers.rs`.
        self.selected_ids = ids
            .into_iter()
            .filter(|id| !self.held.contains_key(id))
            .collect();
        // The point editor belongs to one element, and closes the moment that element
        // stops being the only thing held. Without this it survives onto whatever is
        // picked up next, which shows a stranger's corners over the new selection.
        if let Some(editing) = self.editing_linear.as_deref() {
            if self.selected_ids.len() != 1 || !self.selected_ids.contains(editing) {
                self.editing_linear = None;
            }
        }
        self.events.selection = Some(self.get_selection());
        self.request_draw();
    }

    /// Whether this element offers its individual points rather than a bounding box.
    ///
    /// The shape of the element decides it — two points or fewer — plus the one element
    /// the person has explicitly opened by double clicking it.
    pub(crate) fn shows_point_handles(&self, element: &DrawElement) -> bool {
        crate::selection::linear::is_point_edited(element)
            || self.editing_linear.as_deref() == Some(element.id.as_str())
    }

    /// Open a longer path for point editing, if this element is one.
    ///
    /// Returns whether it did, so the double-click handler knows the gesture was spent.
    pub(crate) fn open_linear_points(&mut self, element: &DrawElement) -> bool {
        let is_linear = matches!(
            element.kind,
            crate::scene::DrawElementType::Line | crate::scene::DrawElementType::Arrow
        );
        let has_points = element.points.as_deref().is_some_and(|p| p.len() > 2);
        if !is_linear || !has_points {
            return false;
        }
        self.set_selection(vec![element.id.clone()]);
        self.editing_linear = Some(element.id.clone());
        self.request_draw();
        true
    }

    /// The point handles currently on offer, if any.
    ///
    /// The same list the painter draws and the pointer tests, so a host — or a test —
    /// can ask what is actually grabbable rather than infer it.
    pub fn linear_points(&self) -> Vec<crate::selection::linear::LinearHandlePoint> {
        match self.get_selected_elements().as_slice() {
            [single] if self.shows_point_handles(single) => {
                crate::selection::linear::handle_points(
                    single,
                    LINEAR_MIDPOINT_MIN_PX / self.camera.scale,
                )
            }
            _ => Vec::new(),
        }
    }

    pub fn clear_selection(&mut self) {
        if !self.selected_ids.is_empty() {
            self.set_selection(Vec::new());
        }
    }

    pub fn select(&mut self, ids: Vec<String>) {
        self.set_selection(ids);
    }

    pub fn select_all(&mut self) {
        let ids = self.scene.ordered_cloned().into_iter().map(|el| el.id);
        self.set_selection(ids);
    }

    fn single_selected(&self) -> Option<DrawElement> {
        if self.selected_ids.len() != 1 {
            return None;
        }
        let id = self.selected_ids.iter().next()?;
        self.scene.get(id).cloned()
    }

    fn apply_bindings(&mut self) {
        crate::scene::binding::refresh_bindings_in_place(&mut self.scene);
    }

    fn settle_tool(&mut self) {
        if !self.tool_locked {
            self.set_tool(DrawTool::Select);
        }
    }

    pub fn set_tool_locked(&mut self, locked: bool) {
        self.tool_locked = locked;
    }

    pub fn get_tool_locked(&self) -> bool {
        self.tool_locked
    }

    /// The group currently stepped into, if any.
    pub fn editing_group_id(&self) -> Option<String> {
        self.editing_group_id.clone()
    }

    /// Steps back out to the top level.
    ///
    /// Not a selection change on its own: leaving a group keeps what is held, so the
    /// next click behaves normally rather than the selection vanishing under you.
    pub(crate) fn leave_group(&mut self) -> bool {
        if self.editing_group_id.take().is_none() {
            return false;
        }
        // The selection is re-derived at the new level, so stepping out leaves you
        // holding the group you stepped out of rather than the pieces you were looking
        // at inside it. Without this the old inner selection survives, and the next
        // click on one of its members reads as "grab what is already selected" and never
        // re-expands — so the group could be entered but never properly left.
        if !self.selected_ids.is_empty() {
            let ids = crate::edit::expand_within(
                self.scene.iter_ordered(),
                self.selected_ids.iter().cloned(),
                None,
            );
            self.set_selection(ids);
        }
        self.request_draw();
        true
    }

    pub fn snap_guides(&self) -> &[SnapGuide] {
        &self.snap_guides
    }

    pub fn quality_dpr(&self) -> f64 {
        self.dpr.min(2.0)
    }

    pub fn interactive_dpr(&self) -> f64 {
        1.0_f64.max(self.quality_dpr() * 0.5)
    }
}
