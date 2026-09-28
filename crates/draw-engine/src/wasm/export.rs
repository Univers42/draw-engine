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
/// `options.background` is the oracle's `exportBackground`, which it passes as
/// `viewBackgroundColor: null` when the background is not wanted
/// (`export.ts@1118751f:263`) and `bootstrapCanvas` then paints no background at all
/// (`helpers.ts@1118751f:96-107`) — a fresh canvas is already fully transparent, so
/// leaving it alone is what "no background" means here. Not a white fill, and not a
/// transparent-coloured fill: nothing.
pub fn export_png(
    engine: &DrawEngine,
    scope: &crate::export::ExportScope<'_>,
    options: &crate::export::ExportOptions,
) -> Option<js_sys::Promise> {
    let frame = &scope.frame;
    let (canvas, ctx) = super::paint::make_layer(frame.pixel_width(), frame.pixel_height())?;
    let view = engine.export_view_of(frame, &scope.elements);
    super::paint::paint_static(&ctx, &view, None, options.background);
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

/// The clipboard copy as the host reads it: `{supported, mime, scope, text, blob}`.
///
/// Four fields, and each one is a thing the host cannot work out for itself (BUNNY.md §2):
/// whether to write at all, under what type, of what, and the payload. `blob` is the
/// promise [`export_png`] made — passed through, not awaited, so the host is writing the
/// picture the browser has already encoded and this binding never holds a borrow across it.
///
/// `text` and `blob` are `undefined` rather than `null` for "not this format": the host
/// checks one payload per call, and `undefined` is what a missing property is in JS. A
/// declined copy has **both** absent and `mime` empty, so a host that ignored `supported`
/// would have nothing to write.
pub fn clipboard_json(
    format: crate::export::ClipboardFormat,
    copy: &crate::export::ClipboardCopy<'_>,
    blob: Option<js_sys::Promise>,
) -> JsValue {
    let out = js_sys::Object::new();
    let scope = match copy.scope.kind {
        crate::export::ExportScopeKind::Selection => "selection",
        crate::export::ExportScopeKind::Scene => "scene",
    };
    let text = match format {
        crate::export::ClipboardFormat::Svg => copy.text.clone(),
        crate::export::ClipboardFormat::Png => None,
    };
    let _ = js_sys::Reflect::set(&out, &"supported".into(), &copy.supported.into());
    let _ = js_sys::Reflect::set(&out, &"mime".into(), &JsValue::from_str(copy.mime));
    let _ = js_sys::Reflect::set(&out, &"scope".into(), &JsValue::from_str(scope));
    set_optional(&out, "text", text.map(JsValue::from));
    if let Some(blob) = blob {
        let _ = js_sys::Reflect::set(&out, &"blob".into(), &blob.into());
    }
    out.into()
}

/// Sets a property only when there is a value, so "absent" is one shape rather than two.
fn set_optional(out: &js_sys::Object, name: &str, value: Option<JsValue>) {
    if let Some(value) = value {
        let _ = js_sys::Reflect::set(out, &JsValue::from_str(name), &value);
    }
}
