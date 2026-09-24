//! The browser binding: one [`Tracer`] per image, driven from a Web Worker.

use wasm_bindgen::prelude::*;

use crate::{TraceConfig, Tracer};

#[wasm_bindgen]
pub struct TraceSession {
    tracer: Tracer,
}

#[wasm_bindgen]
impl TraceSession {
    /// A session over an RGBA image — `ImageData.data`, row by row.
    #[wasm_bindgen(constructor)]
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Result<TraceSession, JsError> {
        Tracer::from_rgba(rgba, width as usize, height as usize)
            .map(|tracer| Self { tracer })
            .map_err(|message| JsError::new(&message))
    }

    /// Traces with `config` (a [`TraceConfig`] as JSON) and returns the stats as JSON.
    ///
    /// `on_progress(phase, fraction)` is called as clustering and compositing advance —
    /// `phase` is `"segment"`, `"compose"` or `"optimize"`, `fraction` within it.
    pub fn render(
        &mut self,
        config: &str,
        on_progress: &js_sys::Function,
    ) -> Result<String, JsError> {
        let config: TraceConfig =
            serde_json::from_str(config).map_err(|error| JsError::new(&error.to_string()))?;
        let mut report = |progress: vtracer::Progress| {
            let phase = match progress.phase {
                vtracer::Phase::Segment => "segment",
                vtracer::Phase::Compose => "compose",
                vtracer::Phase::Optimize => "optimize",
            };
            let _ = on_progress.call2(
                &JsValue::NULL,
                &JsValue::from_str(phase),
                &JsValue::from_f64(f64::from(progress.fraction)),
            );
        };
        let rendered = self
            .tracer
            .render(&config, &mut report)
            .map_err(|error| JsError::new(&error.to_string()))?;
        serde_json::to_string(&rendered.stats).map_err(|error| JsError::new(&error.to_string()))
    }

    /// The last trace as an SVG document, or `undefined` before the first.
    pub fn svg(&self) -> Option<String> {
        self.tracer.rendered().map(|rendered| rendered.svg.clone())
    }

    /// The last trace's editable rings, flat — what `DrawEngine.vectorizeImage` takes —
    /// or `undefined` before the first.
    pub fn rings(&self) -> Option<TraceRings> {
        self.tracer.rendered().map(|rendered| {
            let flat = rendered.shapes.flat();
            TraceRings {
                colours: flat.colours,
                lengths: flat.lengths,
                coords: flat.coords,
            }
        })
    }
}

/// [`crate::rings::FlatRings`] for JavaScript: each field read once becomes a typed array.
#[wasm_bindgen(getter_with_clone)]
pub struct TraceRings {
    /// Per ring, its colour as `0xRRGGBB`.
    pub colours: Vec<u32>,
    /// Per ring, how many points it has.
    pub lengths: Vec<u32>,
    /// Every ring's points in turn, `x, y`, as fractions of the picture.
    pub coords: Vec<f64>,
}
