//! The binding for `engine/vectorize.rs`: typed arrays and JSON in, JSON out, one commit
//! per call.

use wasm_bindgen::prelude::*;

use super::WasmEngine;
use crate::engine::vectorize::{TraceRings, VectorizeOptions, VectorizeRefusal};

/// `{"ids":[…]}`, `{"id":"…"}` or `{"refused":"…"}` — a refusal is an answer the dialog
/// shows, not an exception.
///
/// Written out by hand: `serde_json::json!` brought its whole `Value` serialiser into the
/// engine's module for these two lines.
fn outcome<T: serde::Serialize>(key: &str, result: Result<T, VectorizeRefusal>) -> String {
    match result.map(|value| serde_json::to_string(&value)) {
        Ok(Ok(value)) => format!(r#"{{"{key}":{value}}}"#),
        Ok(Err(_)) => format!(
            r#"{{"refused":"{}"}}"#,
            VectorizeRefusal::Malformed.as_str()
        ),
        Err(refusal) => format!(r#"{{"refused":"{}"}}"#, refusal.as_str()),
    }
}

#[wasm_bindgen(js_class = DrawEngine)]
impl WasmEngine {
    /// Replaces the image `image_id` with its traced rings — draw-trace's
    /// `TraceSession.rings()`, as three typed arrays — as a group of filled lines.
    /// `options_json` is a `VectorizeOptions`. Returns `{"ids":[…]}` or `{"refused":"…"}`.
    #[wasm_bindgen(js_name = vectorizeToShapes)]
    pub fn vectorize_to_shapes(
        &self,
        image_id: &str,
        colours: Vec<u32>,
        lengths: Vec<u32>,
        coords: Vec<f64>,
        options_json: &str,
    ) -> String {
        let result = match serde_json::from_str::<VectorizeOptions>(options_json) {
            Ok(options) => {
                let trace = TraceRings {
                    colours,
                    lengths,
                    coords,
                };
                let result = self
                    .cell
                    .borrow_mut()
                    .engine
                    .vectorize_to_shapes(image_id, &trace, options);
                self.flush();
                result
            }
            Err(_) => Err(VectorizeRefusal::Malformed),
        };
        outcome("ids", result)
    }

    /// Replaces the image `image_id` with one picture, the SVG `data:` URL `data_url`, in
    /// the same box. Returns `{"id":"…"}` or `{"refused":"…"}`.
    #[wasm_bindgen(js_name = vectorizeToPicture)]
    pub fn vectorize_to_picture(
        &self,
        image_id: &str,
        data_url: &str,
        options_json: &str,
    ) -> String {
        let result = match serde_json::from_str::<VectorizeOptions>(options_json) {
            Ok(options) => {
                let result = self
                    .cell
                    .borrow_mut()
                    .engine
                    .vectorize_to_picture(image_id, data_url, options);
                self.flush();
                result
            }
            Err(_) => Err(VectorizeRefusal::Malformed),
        };
        outcome("id", result)
    }
}
