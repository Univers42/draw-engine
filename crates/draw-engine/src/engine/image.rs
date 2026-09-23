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
        // Sharp, as Excalidraw inserts images (`App.tsx:10084`, `roundness: null`). The
        // default style would make it Round, and the panel would describe a corner the
        // square bitmap does not have.
        element.roundness = None;

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

impl DrawEngine {
    /// Put an embed on the board, sized to the provider's own shape.
    ///
    /// `raw_url` is whatever was pasted. Resolving it — deciding whether the host may be
    /// framed at all, and rewriting a watch page into a player — happens in the engine,
    /// so a host cannot accidentally frame something the rules would have refused.
    ///
    /// Returns the new element's id, or `None` when the link is not embeddable.
    pub fn insert_embed(&mut self, raw_url: &str, sx: f64, sy: f64) -> Option<String> {
        let resolved = crate::scene::embed_link(raw_url)?;
        let centre = self.screen_to_world(sx, sy);
        // Sized exactly as an image is: an embed arriving several screens tall is the
        // same problem, and the answer should not depend on which one you inserted.
        let fit = crate::scene::fit_image(
            resolved.intrinsic_width,
            resolved.intrinsic_height,
            self.height,
            self.camera.scale,
            centre,
        );
        if fit.width <= 0.0 || fit.height <= 0.0 {
            return None;
        }

        let mut element = create_element(
            DrawElementType::Embed,
            Geometry {
                x: fit.x,
                y: fit.y,
                width: fit.width,
                height: fit.height,
            },
            default_element_style(),
            self.now_ms,
        );
        element.embed_url = Some(resolved.url);
        // A frame round a live page, not a drawing of one: a hand-drawn border would
        // never line up with the rectangle the browser actually clips the page to.
        element.roughness = 0.0;
        element.background_color = "transparent".into();

        let id = element.id.clone();
        self.scene.add(element);
        self.refresh_frame_membership();
        self.set_selection(vec![id.clone()]);
        self.push_history();
        self.request_draw();
        Some(id)
    }
}

/// Where a live frame has to be put, in screen pixels.
///
/// An embed is the one element a canvas cannot draw: a page inside a board is a real
/// `<iframe>`, positioned over the canvas by the host. What the host must not do is work
/// out *where* — that is the camera, and getting it slightly wrong makes the frame drift
/// away from the rectangle drawn under it while you pan.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedFrame {
    pub id: String,
    pub url: String,
    /// Whether the frame may keep its origin. Decided by the engine, per provider, so a
    /// host cannot quietly grant it to everything.
    pub allow_same_origin: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Radians. A turned embed needs a CSS rotation about its own centre.
    pub angle: f64,
}

impl DrawEngine {
    /// The embeds currently on screen, with their boxes in screen pixels.
    ///
    /// Culled, because an off-screen `<iframe>` is a page still running: a board with
    /// thirty videos on it should not have thirty players loaded because one is visible.
    pub fn embed_frames(&self) -> Vec<EmbedFrame> {
        let visible = crate::camera::visible_world_rect(self.camera, self.width, self.height);
        self.scene
            .iter_ordered()
            .filter(|element| {
                element.kind == DrawElementType::Embed
                    && !element.is_deleted
                    && element.embed_url.is_some()
                    && crate::render::bounds::intersects_viewport(element, &visible)
            })
            .map(|element| {
                let rect = crate::scene::normalize_rect(
                    element.x,
                    element.y,
                    element.width,
                    element.height,
                );
                let top_left = crate::world_to_screen(self.camera, rect.x, rect.y);
                EmbedFrame {
                    id: element.id.clone(),
                    url: element.embed_url.clone().unwrap_or_default(),
                    allow_same_origin: element
                        .embed_url
                        .as_deref()
                        .and_then(crate::scene::embed_link)
                        .map(|resolved| resolved.allow_same_origin)
                        .unwrap_or(false),
                    x: top_left.x,
                    y: top_left.y,
                    width: rect.width * self.camera.scale,
                    height: rect.height * self.camera.scale,
                    angle: element.angle,
                }
            })
            .collect()
    }
}
