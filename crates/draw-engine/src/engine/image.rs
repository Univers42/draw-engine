use crate::engine::DrawEngine;
use crate::scene::{create_element, default_element_style, fit_image, DrawElementType, Geometry};

impl DrawEngine {
    /// Put an image on the board, sized to the viewport and centred on a point.
    ///
    /// The host decodes the file, because only a browser can — and having decoded it,
    /// it already knows the natural size. Everything after that is arithmetic and
    /// happens here, so two frontends dropping the same file at the same place get the
    /// same element rather than two different ideas of "a sensible size".
    ///
    /// `sx`/`sy` are **screen** coordinates, like every other pointer-driven entry
    /// point: a drop reports where the pointer was, and a toolbar insert can report the
    /// middle of the viewport.
    ///
    /// Returns the new element's id, or `None` when the image could not be measured.
    pub fn insert_image(
        &mut self,
        data_url: &str,
        natural_width: f64,
        natural_height: f64,
        sx: f64,
        sy: f64,
    ) -> Option<String> {
        let centre = self.screen_to_world(sx, sy);
        let fit = fit_image(
            natural_width,
            natural_height,
            self.height,
            self.camera.scale,
            centre,
        );
        if fit.width <= 0.0 || fit.height <= 0.0 {
            return None;
        }

        // An image brings its own appearance. Merging the current element style would
        // give it a stroke colour and a fill it cannot use, and a sloppiness that would
        // make the *frame* round it wobble.
        let mut element = create_element(
            DrawElementType::Image,
            Geometry {
                x: fit.x,
                y: fit.y,
                width: fit.width,
                height: fit.height,
            },
            default_element_style(),
            self.now_ms,
        );
        element.data_url = Some(data_url.to_string());
        element.roughness = 0.0;
        element.background_color = "transparent".into();

        let id = element.id.clone();
        self.scene.add(element);
        // Membership follows position, and an image dropped inside a frame belongs to it
        // exactly as a shape drawn there would.
        self.refresh_frame_membership();
        self.set_selection(vec![id.clone()]);
        self.push_history();
        self.request_draw();
        Some(id)
    }
}
