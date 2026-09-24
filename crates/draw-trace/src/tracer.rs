//! One image, traced as many times as the dialog's dials move.

use serde::Serialize;
use vtracer::svg::SvgWriter;
use vtracer::{CancelToken, ColorImage, Progress, Session, VectorDoc};

use crate::config::TraceConfig;
use crate::rings::{shapes_payload, ShapesPayload};
use crate::{FLATTEN_TOLERANCE, MAX_RING_POINTS};

/// What the dialog shows under the preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceStats {
    /// Elements an editable insert makes — one per ring.
    pub shapes: usize,
    /// Distinct regions the tracer found, before holes and islands are counted apart.
    pub regions: usize,
    /// Points across every ring, which is what an editable insert costs the board.
    pub points: usize,
    /// Bytes of the picture, before it is base64-encoded into a `data:` URL.
    pub svg_bytes: usize,
    pub width: u32,
    pub height: u32,
}

/// The last trace, in both forms.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub doc: VectorDoc,
    pub svg: String,
    pub shapes: ShapesPayload,
    pub stats: TraceStats,
}

/// A vtracer [`Session`] over one image, and its last result.
///
/// The session keeps the clustering — the expensive stage — and redoes it only when a
/// dial that feeds it moves; the corner threshold or the fit mode re-render from it.
pub struct Tracer {
    session: Session,
    rendered: Option<Rendered>,
}

impl Tracer {
    /// Takes the image as RGBA bytes, row by row.
    pub fn from_rgba(rgba: Vec<u8>, width: usize, height: usize) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("the image is empty".into());
        }
        if Some(rgba.len()) != width.checked_mul(height).and_then(|n| n.checked_mul(4)) {
            return Err(format!(
                "{} bytes is not a {width}x{height} RGBA image",
                rgba.len()
            ));
        }
        let mut image = ColorImage::new_w_h(width, height);
        image.pixels = rgba;
        Ok(Self {
            session: Session::new(image),
            rendered: None,
        })
    }

    /// Traces with `config`, reporting each phase as it goes.
    ///
    /// Nothing cancels a trace from inside: in a worker the one thread is busy with it,
    /// so no message could arrive to say stop. Cancelling is terminating the worker.
    pub fn render(
        &mut self,
        config: &TraceConfig,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<&Rendered, vtracer::Error> {
        let doc = self.session.render_with_progress(
            &config.to_vtracer(),
            &CancelToken::new(),
            on_progress,
        )?;
        let svg = picture(&doc);
        let shapes = shapes_payload(&doc, FLATTEN_TOLERANCE, MAX_RING_POINTS);
        let rings = shapes.shapes.iter().flat_map(|shape| &shape.rings);
        let stats = TraceStats {
            shapes: rings.clone().count(),
            regions: doc.shapes.len(),
            points: rings.map(|ring| ring.len() / 2).sum(),
            svg_bytes: svg.len(),
            width: doc.width,
            height: doc.height,
        };
        Ok(self.rendered.insert(Rendered {
            doc,
            svg,
            shapes,
            stats,
        }))
    }

    pub fn rendered(&self) -> Option<&Rendered> {
        self.rendered.as_ref()
    }
}

/// The trace as an SVG that fills whatever box it is drawn into.
///
/// vtracer writes `width` and `height` but no `viewBox`, and an SVG without one is not
/// scaled when drawn at another size — it is cropped, or left in a corner. An image on
/// the board is drawn at its element's size, which is anything once it is resized, and
/// stretched there like the raster was, hence `preserveAspectRatio="none"`.
fn picture(doc: &VectorDoc) -> String {
    SvgWriter::default().write(doc).replacen(
        "<svg ",
        &format!(
            r#"<svg viewBox="0 0 {} {}" preserveAspectRatio="none" "#,
            doc.width, doc.height
        ),
        1,
    )
}
