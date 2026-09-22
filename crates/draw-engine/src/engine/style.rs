use crate::engine::DrawEngine;
use crate::scene::{apply_style_patch, bump_version, is_linear_element, DrawElementStylePatch};

impl DrawEngine {
    pub fn apply_style(&mut self, patch: DrawElementStylePatch) {
        let selected = self.get_selected_elements();
        if selected.is_empty() {
            self.set_next_style(patch);
            return;
        }
        let now = self.now_ms;
        for mut element in selected {
            apply_style_patch(&mut element, &patch);
            self.scene.put(bump_version(element, now));
        }
        self.push_history();
        self.request_draw();
    }

    pub fn set_arrowheads(
        &mut self,
        start: Option<crate::scene::Arrowhead>,
        end: Option<crate::scene::Arrowhead>,
    ) {
        let linears: Vec<_> = self
            .get_selected_elements()
            .into_iter()
            .filter(is_linear_element)
            .collect();
        if linears.is_empty() {
            return;
        }
        let now = self.now_ms;
        for mut element in linears {
            if let Some(kind) = start {
                element.start_arrowhead = Some(kind);
            }
            if let Some(kind) = end {
                element.end_arrowhead = Some(kind);
            }
            self.scene.put(bump_version(element, now));
        }
        self.push_history();
        self.request_draw();
    }

    pub fn set_font_size(&mut self, size: f64) {
        self.next_font_size = size;
        let texts: Vec<_> = self
            .get_selected_elements()
            .into_iter()
            .filter(|el| el.kind == crate::scene::DrawElementType::Text)
            .collect();
        if texts.is_empty() {
            return;
        }
        let now = self.now_ms;
        let measure = self.measure_text;
        for mut element in texts {
            let (width, height) = measure(element.text.as_deref().unwrap_or(""), size);
            element.font_size = Some(size);
            element.width = width;
            element.height = height;
            self.scene.put(bump_version(element, now));
        }
        self.apply_bindings();
        self.push_history();
        self.request_draw();
    }

    pub fn get_font_size(&self) -> f64 {
        self.get_selected_elements()
            .into_iter()
            .find(|el| el.kind == crate::scene::DrawElementType::Text)
            .and_then(|el| el.font_size)
            .unwrap_or(self.next_font_size)
    }

    pub fn zoom_at(&mut self, sx: f64, sy: f64, factor: f64) {
        self.set_camera(crate::zoom_at(self.camera, sx, sy, factor));
        self.bump_motion();
    }

    /// One wheel event, anchored at the cursor.
    ///
    /// The host passes the raw `deltaY` and nothing else: how far a wheel event is worth
    /// is arithmetic, it differs per browser and per device, and every platform this
    /// engine is embedded in would otherwise have to get it right independently.
    ///
    /// Returns early when the step changes nothing, which is the common case at either
    /// zoom limit — a trackpad goes on sending ticks for as long as the fingers move, and
    /// publishing a camera event for each would repaint the whole scene to produce the
    /// identical picture.
    pub fn wheel_zoom(&mut self, sx: f64, sy: f64, delta_y: f64) {
        let next = crate::camera::wheel_zoom_scale(self.camera.scale, delta_y);
        if next == self.camera.scale {
            return;
        }
        self.set_camera(crate::zoom_to(self.camera, sx, sy, next));
        self.bump_motion();
    }

    pub fn set_camera(&mut self, camera: crate::camera::Camera) {
        self.camera = camera;
        self.events.camera = Some(camera);
        self.request_draw();
    }

    pub fn pan_by(&mut self, dx_screen: f64, dy_screen: f64) {
        self.set_camera(crate::pan_by(self.camera, dx_screen, dy_screen));
        self.bump_motion();
    }

    pub fn fit(&mut self, padding: f64) {
        if let Some(bounds) = self.scene.bounds() {
            self.set_camera(crate::fit_bounds(bounds, self.width, self.height, padding));
        }
    }

    /// Frame what is selected, rather than the whole board.
    ///
    /// The command `fit` cannot stand in for: a fit has to hold everything, so the thing
    /// you are working on ends up as small as the furthest stray shape allows. Does
    /// nothing when the selection is empty — framing "nothing" has no meaning, and
    /// jumping somewhere arbitrary is the worst of the available answers.
    pub fn zoom_to_selection(&mut self, padding: f64) {
        let selected = self.get_selected_elements();
        if selected.is_empty() {
            return;
        }
        let Some(bounds) = crate::scene_bounds(selected.iter()) else {
            return;
        };
        self.set_camera(crate::fit_bounds(bounds, self.width, self.height, padding));
    }

    /// Move by a screenful, in units of pages.
    ///
    /// `PAGE_OVERLAP` is the point: moving exactly one screen leaves nothing in common
    /// between the two views, so you lose your place at every press. Keeping a strip of
    /// the old view is what makes paging better than dragging.
    ///
    /// Screen pixels, not world units, so a page is a screenful at every zoom level.
    /// Scaling it by the zoom would make paging useless exactly when it is most needed.
    pub fn page_by(&mut self, pages_x: f64, pages_y: f64) {
        /// How much of the outgoing view is still visible after a page.
        const PAGE_OVERLAP: f64 = 0.15;
        let step_x = self.width * (1.0 - PAGE_OVERLAP);
        let step_y = self.height * (1.0 - PAGE_OVERLAP);
        self.pan_by(-pages_x * step_x, -pages_y * step_y);
    }

    pub fn zoom_in(&mut self) {
        self.zoom_at(self.width / 2.0, self.height / 2.0, 1.2);
    }

    pub fn zoom_out(&mut self) {
        self.zoom_at(self.width / 2.0, self.height / 2.0, 1.0 / 1.2);
    }

    pub fn zoom_reset(&mut self) {
        self.zoom_at(self.width / 2.0, self.height / 2.0, 1.0 / self.camera.scale);
    }

    pub fn content_in_view(&self) -> bool {
        let Some(bounds) = self.scene.bounds() else {
            return true;
        };
        let view = crate::visible_world_rect(self.camera, self.width, self.height);
        bounds.min_x <= view.max_x
            && bounds.max_x >= view.min_x
            && bounds.min_y <= view.max_y
            && bounds.max_y >= view.min_y
    }
}
