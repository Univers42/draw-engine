use crate::camera::{Camera, WorldBounds};
use crate::engine::DrawEngine;
use crate::interaction::SnapGuide;
use crate::render::{DrawTheme, GridSettings};
use crate::scene::DrawElement;
use crate::selection::marquee_rect;

/// Everything a painter needs for one frame.
///
/// Elements are **borrowed from the scene**, not cloned. This used to own
/// `Vec<DrawElement>`, so every frame deep-copied the entire scene — three `String`s and
/// a point vector per element — whether anything had changed or not. Borrowing makes a
/// frame's setup cost proportional to the number of *visible* elements rather than to
/// the size of the document.
pub struct PaintView<'a> {
    pub camera: Camera,
    pub theme: DrawTheme,
    pub grid: GridSettings,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub in_motion: bool,
    /// Visible elements in z-order. Already culled to the viewport.
    pub elements: Vec<&'a DrawElement>,
    /// A number that changes whenever the scene does.
    ///
    /// The painter reuses its offscreen layer between frames and needs to know whether
    /// the picture is still the one in it. Deliberately taken from the whole scene rather
    /// than from the culled list above: panning changes which elements are visible, and a
    /// signal that moved with the camera would throw the layer away exactly when it was
    /// most worth keeping.
    pub scene_revision: u64,
    pub selected: Vec<&'a DrawElement>,
    pub marquee: Option<WorldBounds>,
    /// The lasso loop in progress, in world space. Empty unless one is being drawn.
    pub lasso: Vec<crate::camera::Point>,
    /// Clip boxes for children that stick out of the frame that owns them, by element id.
    ///
    /// Only the ones that need it. A child sitting wholly inside its frame has nothing to
    /// clip, and setting a clip path for it anyway costs a path per element every frame
    /// for no visible difference.
    pub frame_clips: std::collections::HashMap<String, WorldBounds>,
    /// Where each frame's name sits, and what it says.
    pub frame_names: Vec<(crate::camera::Point, String)>,
    /// Laser strokes to fill, oldest first, in world space.
    ///
    /// Already shaped: each is a closed outline whose width varies along its length, not
    /// a centre line to be stroked. The engine computes them so that every host draws the
    /// same beam, and so that none of them has to own the fade.
    pub laser: Vec<Vec<crate::interaction::LaserPoint>>,
    /// The colour laser strokes are filled with.
    pub laser_color: String,
    pub snap_guides: Vec<SnapGuide>,
    pub rotate_gap: f64,
    pub handle_px: f64,
    /// Where the frame and handles sit. Shared with the hit test so the painter can only
    /// ever draw handles that are actually grabbable, and vice versa.
    pub handle_layout: crate::selection::HandleLayout,
    /// The shape a dragged arrow endpoint would bind to. Painted as a halo on its
    /// outline, so the attachment is visible before it is committed.
    pub binding_highlight: Option<&'a DrawElement>,
    /// Point handles for a selected line or arrow.
    ///
    /// Non-empty only when exactly one linear element is selected. When it is, the
    /// selection frame and box handles are suppressed entirely — a linear element is
    /// edited by its points, and Excalidraw shows no bounding box for one either.
    pub linear_handles: Vec<crate::selection::LinearHandlePoint>,
}

pub trait Painter {
    fn paint(&mut self, view: &PaintView<'_>);
}

pub struct NoopPainter;

impl Painter for NoopPainter {
    fn paint(&mut self, _view: &PaintView<'_>) {}
}

impl DrawEngine {
    pub fn take_dirty(&mut self) -> bool {
        let dirty = self.dirty;
        self.dirty = false;
        dirty
    }

    pub fn paint_view(&self) -> PaintView<'_> {
        let selected: Vec<&DrawElement> = self
            .selected_ids
            .iter()
            .filter_map(|id| self.scene.get(id))
            .filter(|el| !el.is_deleted)
            .collect();
        let lasso = match &self.interaction {
            Some(super::Interaction::Lasso { path, .. }) => path.clone(),
            _ => Vec::new(),
        };
        let marquee = match &self.interaction {
            Some(super::Interaction::Marquee { start, current, .. }) => {
                Some(marquee_rect(start.x, start.y, current.x, current.y))
            }
            _ => None,
        };
        let dpr = if self.in_motion() {
            self.interactive_dpr()
        } else {
            self.quality_dpr()
        };
        let visible = crate::camera::visible_world_rect(self.camera, self.width, self.height);
        let scene_revision = self.scene.revision();

        // Computed before the struct literal takes ownership of `selected`.
        let linear_handles = match selected.as_slice() {
            [single] if crate::selection::linear::is_point_edited(single) => {
                crate::selection::linear::handle_points(
                    single,
                    super::LINEAR_MIDPOINT_MIN_PX / self.camera.scale,
                )
            }
            _ => Vec::new(),
        };
        // Frame chrome, decided here so every host paints the same boundaries and clips
        // the same children. A host is handed boxes and labels, not rules.
        let mut frame_clips = std::collections::HashMap::new();
        let mut frame_names = Vec::new();
        for frame in self.scene.iter_ordered().filter(|el| {
            crate::scene::is_frame(el)
                && !el.is_deleted
                && crate::render::bounds::intersects_viewport(el, &visible)
        }) {
            if let Some(name) = frame.name.clone() {
                frame_names.push((crate::scene::frame_name_anchor(frame), name));
            }
            let clip = crate::scene::frame_clip_bounds(frame);
            for child_id in crate::scene::frame_children(self.scene.iter_ordered(), &frame.id) {
                if let Some(child) = self.scene.get(&child_id) {
                    if crate::scene::needs_frame_clip(child, frame) {
                        frame_clips.insert(child_id, clip);
                    }
                }
            }
        }

        PaintView {
            camera: self.camera,
            theme: self.theme.clone(),
            grid: self.grid,
            width: self.width,
            height: self.height,
            dpr,
            in_motion: self.in_motion(),
            // Culled here rather than in the painter: an element off-screen costs a
            // bounds check instead of a full path replay, which is what keeps a large
            // document responsive when you are zoomed in on one corner of it.
            elements: self
                .scene
                .iter_ordered()
                .filter(|element| crate::render::bounds::intersects_viewport(element, &visible))
                .collect(),
            scene_revision,
            selected,
            marquee,
            lasso,
            frame_clips,
            frame_names,
            laser: self.laser.outlines(self.now_ms, self.camera.scale),
            laser_color: crate::interaction::DEFAULT_LASER_COLOR.to_string(),
            snap_guides: self.snap_guides.clone(),
            rotate_gap: super::ROTATE_GAP_PX / self.camera.scale,
            handle_px: super::HANDLE_PX,
            handle_layout: self.handle_layout(),
            binding_highlight: self
                .binding_highlight
                .as_deref()
                .and_then(|id| self.scene.get(id)),
            linear_handles,
        }
    }

    pub fn paint_if_dirty(&mut self, painter: &mut impl Painter) {
        if self.disposed || !self.dirty {
            return;
        }
        self.dirty = false;
        painter.paint(&self.paint_view());
        if self.needs_frame() {
            self.dirty = true;
        }
    }
}
