use std::collections::HashSet;

use crate::camera::{Camera, Point, IDENTITY};
use crate::history::SnapshotHistory;
use crate::interaction::{DrawTool, SnapGuide};
use crate::render::{light_theme, DrawTheme};
use crate::scene::{
    default_element_style, DrawElement, DrawElementStyle, DrawElementStylePatch, Scene,
};

mod arrange;
mod clipboard;
mod frame;
mod hover;
mod pointer;
mod pointer_end;
mod pointer_move;
mod style;
mod text;
mod types;

pub use frame::{NoopPainter, PaintView, Painter};
pub use hover::HoverCursor;
pub(crate) use types::{default_measure, history_signature, Interaction};
pub use types::{merge_style_patch, EngineEvents, TextEditRequest};

const HANDLE_PX: f64 = 8.0;
/// Reach for the handles that are not laid out by [`crate::selection::HandleLayout`] —
/// the point handles on a line or arrow, which sit *on* the geometry and so have no
/// element interior to stay clear of.
const HANDLE_HIT_PX: f64 = 10.0;
/// How long a segment must be on screen before it gets its own midpoint handle.
/// Two handles a few pixels apart cannot be aimed at deliberately.
const LINEAR_MIDPOINT_MIN_PX: f64 = 28.0;
/// How close, in screen pixels, an endpoint must come to a shape to bind to it.
/// Derived from pixels rather than world units so it does not shrink to nothing when
/// zoomed out.
const BINDING_HOVER_PX: f64 = 32.0;
const ROTATE_GAP_PX: f64 = 26.0;
const DEFAULT_FONT_SIZE: f64 = 20.0;
const SNAP_PX: f64 = 6.0;
const PASTE_OFFSET: f64 = 12.0;
const MOTION_MS: f64 = 140.0;

pub struct DrawEngine {
    scene: Scene,
    theme: DrawTheme,
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
    tool_locked: bool,
    next_style: DrawElementStylePatch,
    next_font_size: f64,
    interaction: Option<Interaction>,
    selected_ids: HashSet<String>,
    clipboard_buffer: Option<String>,
    snap_guides: Vec<SnapGuide>,
    /// The shape a dragged arrow endpoint would attach to, if released now.
    ///
    /// The painter outlines it, so the attachment is visible before it is committed —
    /// without that, binding is invisible until after the fact and feels like a
    /// coincidence rather than a tool.
    binding_highlight: Option<String>,
    history: SnapshotHistory<Vec<std::rc::Rc<DrawElement>>>,
    events: EngineEvents,
}

impl Default for DrawEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawEngine {
    pub fn new() -> Self {
        Self {
            scene: Scene::default(),
            theme: light_theme(),
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
            tool_locked: false,
            next_style: DrawElementStylePatch::default(),
            next_font_size: DEFAULT_FONT_SIZE,
            interaction: None,
            selected_ids: HashSet::new(),
            clipboard_buffer: None,
            snap_guides: Vec::new(),
            binding_highlight: None,
            history: SnapshotHistory::new(Vec::new(), |els| history_signature(els), 200),
            events: EngineEvents::default(),
        }
    }

    pub fn set_now(&mut self, now_ms: f64) {
        self.now_ms = now_ms;
    }

    pub fn set_measure_text(&mut self, measure: fn(&str, f64) -> (f64, f64)) {
        self.measure_text = measure;
    }

    pub fn drain_events(&mut self) -> EngineEvents {
        std::mem::take(&mut self.events)
    }

    pub fn set_scene(&mut self, scene: Scene) {
        self.scene = scene;
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
            .filter(|el| !el.locked())
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
            .find(|el| !el.locked() && crate::hit_test_element(el, world.x, world.y, tolerance))
            .cloned()
    }

    /// Where the selection frame and handles sit, for both painting and hit testing.
    pub(crate) fn handle_layout(&self) -> crate::selection::HandleLayout {
        crate::selection::HandleLayout::screen(HANDLE_PX, ROTATE_GAP_PX, self.camera.scale)
    }

    pub fn set_tool(&mut self, tool: DrawTool) {
        if tool == self.tool {
            return;
        }
        self.tool = tool;
        self.events.tool = Some(tool);
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
        self.selected_ids = ids.into_iter().collect();
        self.events.selection = Some(self.get_selection());
        self.request_draw();
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
