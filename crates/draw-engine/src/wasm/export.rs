//! Whole-scene PNG export: the rasterising half.
//!
//! [`crate::export::ExportFrame`] decides what the picture is and how big; this draws it.
//! The split is the point — the numbers are testable without a browser, and everything here
//! needs one.
//!
//! The target is an offscreen canvas of the frame's own device pixels, never the one on
//! screen: an export is framed by the scene, not by wherever the person happens to have
//! scrolled to. The oracle does the same with a canvas it creates per export
//! (`export.ts@1118751f:198-203`).

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::engine::DrawEngine;

/// Paints the whole scene into a canvas of the frame's own size, and hands back the PNG.
///
/// `transparent` is the oracle's `exportBackground`, negated: it passes
/// `viewBackgroundColor: null` when the background is not wanted
/// (`export.ts@1118751f:263`) and `bootstrapCanvas` then paints no background at all
/// (`helpers.ts@1118751f:96-107`) — a fresh canvas is already fully transparent, so
/// leaving it alone is what "no background" means here. Not a white fill, and not a
/// transparent-coloured fill: nothing.
pub fn export_png(
    engine: &DrawEngine,
    frame: &crate::export::ExportFrame,
    transparent: bool,
) -> Option<js_sys::Promise> {
    let (canvas, ctx) = super::paint::make_layer(frame.pixel_width(), frame.pixel_height())?;
    super::paint::paint_static(&ctx, &engine.export_view(frame), None, !transparent);
    Some(png_blob(canvas))
}

/// The canvas as a PNG, once the browser has encoded it.
///
/// `toBlob` is asynchronous, so this is a promise. The callback holds only the canvas and
/// the resolve function, both owned, so nothing borrowed from the engine outlives the call
/// that asked for the picture.
///
/// `null` where the browser refuses: a canvas too large to encode answers `null`, which is
/// the oracle's `CANVAS_POSSIBLY_TOO_BIG` (`data/blob.ts@1118751f:245-252`) without its
/// message. The web saves what it is given and nothing otherwise.
///
/// **`toBlob` throwing rejects rather than being swallowed**, which is the difference
/// between a caller that learns the export failed and a caller that waits forever. The
/// oracle's `canvasToBlob` rejects for the same reason (`blob.ts:240-256`).
fn png_blob(canvas: web_sys::HtmlCanvasElement) -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, reject| {
        let on_encoded = Closure::once_into_js(move |blob: Option<web_sys::Blob>| {
            let value = match blob {
                Some(blob) => JsValue::from(blob),
                None => JsValue::NULL,
            };
            let _ = resolve.call1(&JsValue::NULL, &value);
        });
        if canvas
            .to_blob(on_encoded.as_ref().unchecked_ref::<js_sys::Function>())
            .is_err()
        {
            let _ = reject.call1(
                &JsValue::NULL,
                &JsValue::from_str("the export canvas could not be encoded"),
            );
        }
    })
}
