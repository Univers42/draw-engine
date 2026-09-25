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
        // Sharp, as Excalidraw inserts images (`App.tsx@1118751f:10092`, `roundness: null`). The
        // default style would make it Round, and the panel would describe a corner the
        // square bitmap does not have.
        element.roundness = None;

        let id = element.id.clone();
        self.scene.add(element);
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
        self.set_selection(vec![id.clone()]);
        self.push_history();
        self.request_draw();
        Some(id)
    }
}

impl DrawEngine {
    /// Point an embed at another link, resolved exactly as a pasted one is.
    ///
    /// The box keeps its place and size: it is the person's, sized where they wanted it.
    /// Returns whether anything changed — `false` for a link the rules refuse, an element
    /// that is not an embed, or one that is locked or held by a peer.
    pub fn set_embed_url(&mut self, id: &str, raw_url: &str) -> bool {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return false;
        };
        if element.kind != DrawElementType::Embed
            || element.is_deleted
            || self.untouchable(&element)
        {
            return false;
        }
        let Some(resolved) = crate::scene::embed_link(raw_url) else {
            return false;
        };
        if element.embed_url.as_deref() == Some(resolved.url.as_str()) {
            return false;
        }
        element.embed_url = Some(resolved.url);
        self.scene.put(element);
        self.push_history();
        self.request_draw();
        true
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
    /// The document to frame instead of `url`, for a provider with no address that can
    /// be framed — see [`crate::scene::EmbedLink::document`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub srcdoc: Option<String>,
    /// `"video"` or `"generic"`: a player's viewport is kept legible when zoomed out.
    pub kind: &'static str,
    /// Whether the frame may keep its origin. Decided by the engine, per provider, so a
    /// host cannot quietly grant it to everything.
    pub allow_same_origin: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Radians. A turned embed needs a CSS rotation about its own centre.
    pub angle: f64,
    /// The camera's zoom. The host lays the page out at the embed's size on the board
    /// and scales it by this, so the page zooms with the board instead of reflowing into
    /// a box that grows and shrinks around it.
    pub scale: f64,
    /// Whether any of it is on screen. The host loads a frame only once it has been
    /// seen, and keeps it after — see [`DrawEngine::embed_frames`].
    pub visible: bool,
}

impl DrawEngine {
    /// Every live embed, with its box in screen pixels.
    ///
    /// Not culled: a frame the host stops hearing about is an `<iframe>` it unmounts,
    /// which stopped a video the moment it was panned out of view. Instead each says
    /// whether it is on screen, so a board with thirty videos still loads only the
    /// players someone has looked at — Excalidraw's `initializedEmbeds`.
    pub fn embed_frames(&self) -> Vec<EmbedFrame> {
        let visible = crate::camera::visible_world_rect(self.camera, self.width, self.height);
        self.scene
            .iter_ordered()
            .filter(|element| element.kind == DrawElementType::Embed && !element.is_deleted)
            .filter_map(|element| {
                // Resolved again, not read back as stored: a board saved under older
                // rules — a tweet framed at its own page, which refuses — gets the rules
                // as they are now. A link the rules no longer accept gets no frame.
                let resolved = crate::scene::embed_link(element.embed_url.as_deref()?)?;
                let rect = crate::scene::normalize_rect(
                    element.x,
                    element.y,
                    element.width,
                    element.height,
                );
                let top_left = crate::world_to_screen(self.camera, rect.x, rect.y);
                Some(EmbedFrame {
                    id: element.id.clone(),
                    url: resolved.url,
                    srcdoc: resolved.document,
                    kind: match resolved.kind {
                        crate::scene::EmbedKind::Video => "video",
                        crate::scene::EmbedKind::Generic => "generic",
                    },
                    allow_same_origin: resolved.allow_same_origin,
                    x: top_left.x,
                    y: top_left.y,
                    width: rect.width * self.camera.scale,
                    height: rect.height * self.camera.scale,
                    angle: element.angle,
                    scale: self.camera.scale,
                    visible: crate::render::bounds::intersects_viewport(element, &visible),
                })
            })
            .collect()
    }
}
