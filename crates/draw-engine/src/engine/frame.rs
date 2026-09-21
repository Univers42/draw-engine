use crate::camera::{Camera, WorldBounds};
use crate::engine::DrawEngine;
use crate::interaction::SnapGuide;
use crate::render::DrawTheme;
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
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub in_motion: bool,
    /// Visible elements in z-order. Already culled to the viewport.
    pub elements: Vec<&'a DrawElement>,
    pub selected: Vec<&'a DrawElement>,
    pub marquee: Option<WorldBounds>,
    pub snap_guides: Vec<SnapGuide>,
    pub rotate_gap: f64,
    pub handle_px: f64,
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
        PaintView {
            camera: self.camera,
            theme: self.theme.clone(),
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
            selected,
            marquee,
            snap_guides: self.snap_guides.clone(),
            rotate_gap: super::ROTATE_GAP_PX / self.camera.scale,
            handle_px: super::HANDLE_PX,
        }
    }

    pub fn paint_if_dirty(&mut self, painter: &mut impl Painter) {
        if self.disposed || !self.dirty {
            return;
        }
        self.dirty = false;
        painter.paint(&self.paint_view());
        if self.in_motion() {
            self.dirty = true;
        }
    }
}
