use crate::camera::{Camera, WorldBounds};
use crate::engine::DrawEngine;
use crate::interaction::SnapGuide;
use crate::render::DrawTheme;
use crate::scene::DrawElement;
use crate::selection::marquee_rect;

pub struct PaintView {
    pub camera: Camera,
    pub theme: DrawTheme,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub in_motion: bool,
    pub elements: Vec<DrawElement>,
    pub selected: Vec<DrawElement>,
    pub marquee: Option<WorldBounds>,
    pub snap_guides: Vec<SnapGuide>,
    pub rotate_gap: f64,
    pub handle_px: f64,
}

pub trait Painter {
    fn paint(&mut self, view: &PaintView);
}

pub struct NoopPainter;

impl Painter for NoopPainter {
    fn paint(&mut self, _view: &PaintView) {}
}

impl DrawEngine {
    pub fn take_dirty(&mut self) -> bool {
        let dirty = self.dirty;
        self.dirty = false;
        dirty
    }

    pub fn paint_view(&self) -> PaintView {
        let selected: Vec<DrawElement> = self.get_selected_elements();
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
        PaintView {
            camera: self.camera,
            theme: self.theme.clone(),
            width: self.width,
            height: self.height,
            dpr,
            in_motion: self.in_motion(),
            elements: self.scene.ordered_cloned(),
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
