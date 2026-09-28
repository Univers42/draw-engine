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
///
/// The payload is built here, from **this scope's** elements, and is why the answer is a
/// promise: a `toBlob` result is an immutable `Blob`, and the scene has to be *inside* the
/// file rather than beside it. The oracle has the same shape and reaches it the same way —
/// `blob.then(blob => encodePngMetadata({blob, metadata}))`
/// (`data/index.ts@1118751f:176-185`), whose return is
/// `new Blob([encodePng(chunks)], { type: MIME_TYPES.png })` (`data/image.ts:111`… `:46`).
pub fn export_png(
    engine: &DrawEngine,
    scope: &crate::export::ExportScope<'_>,
    options: &crate::export::ExportOptions,
) -> Option<js_sys::Promise> {
    let frame = &scope.frame;
    let (canvas, ctx) = super::paint::make_layer(frame.pixel_width(), frame.pixel_height())?;
    let view = engine.export_view_of(frame, &scope.elements);
    super::paint::paint_static(&ctx, &view, None, options.background);
    let scene = options
        .embed_scene
        .then(|| crate::export::scene_payload(&scope.elements));
    Some(png_blob(canvas, scene))
}

/// The canvas as a PNG, once the browser has encoded it, carrying `scene` when there is one.
///
/// `toBlob` is asynchronous, so this is a promise. The callback holds only the canvas and
/// the resolve function, both owned, so nothing borrowed from the engine outlives the call
/// that asked for the picture.
///
/// With no scene this is the promise 4.1 made. With one it is a second link: the browser's
/// `Blob` has to be read back as bytes before a chunk can be put in it, because a `Blob` is
/// immutable and `data/image.ts:111`… `:32` rewrites exactly that — `decodePng(new
/// Uint8Array(await blobToArrayBuffer(blob)))` — which is the oracle's own admission that
/// the bytes have to come back out.
///
/// `null` where the browser refuses: a canvas too large to encode answers `null`, which is
/// the oracle's `CANVAS_POSSIBLY_TOO_BIG` (`data/blob.ts@1118751f:245-252`) without its
/// message. The web saves what it is given and nothing otherwise.
///
/// **`toBlob` throwing rejects rather than being swallowed**, which is the difference
/// between a caller that learns the export failed and a caller that waits forever. The
/// oracle's `canvasToBlob` rejects for the same reason (`blob.ts:240-256`).
fn png_blob(canvas: web_sys::HtmlCanvasElement, scene: Option<String>) -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, reject| {
        let resolve: JsValue = resolve.into();
        // A local the inner `move` closure can take: the outer one is `FnMut`, and moving a
        // captured `Option<String>` out of it is not a thing it can do.
        let scene = scene.clone();
        let on_encoded = Closure::once_into_js(move |blob: Option<web_sys::Blob>| {
            match (blob, scene) {
                // The scene is spliced into the browser's own bytes and the answer is the
                // new `Blob` — the oracle's `data/image.ts:111`… `:46`. Resolved from
                // inside the splice, because until the bytes are back there is no file.
                (Some(blob), Some(payload)) => with_scene(blob, payload, &resolve),
                (Some(blob), None) => call(&resolve, JsValue::from(blob)),
                (None, _) => call(&resolve, JsValue::NULL),
            };
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

/// Resolve a promise with `value`, ignoring the outcome — there is nothing left to report
/// a failure *to*, and the export's answer is the value either way.
fn call(resolve: &JsValue, value: JsValue) {
    let _ = resolve
        .unchecked_ref::<js_sys::Function>()
        .call1(&JsValue::NULL, &value);
}

/// The browser's PNG with the scene spliced into it, resolved as a new `Blob`.
///
/// Two awaits, in order and not at the same time: `Blob.arrayBuffer` gives the bytes, and
/// the rewrite is Rust's. **A failure resolves `null` rather than rejecting**, and that is
/// the same answer the browser's own `null` is for — a picture that came out with no scene
/// in it is a picture somebody can still open, and a rejected promise would leave the save
/// button spinning on a file that is never coming. `restore` is where a person finds out.
fn with_scene(blob: web_sys::Blob, payload: String, resolve: &JsValue) {
    // One clone for the callback and one for the failure below: the callback is a `move`
    // closure and takes it, and `then` not being callable is a failure the export has to be
    // able to answer as well.
    let answered = resolve.clone();
    let resolve = resolve.clone();
    // `Promise::then` takes a `ScopedClosure<dyn FnMut(JsValue)>` in this wasm-bindgen
    // (0.2.128, pinned), so the value arrives untyped and is read as the `ArrayBuffer` the
    // `array_buffer()` contract says it is. The same shape as the `toBlob` callback above,
    // which is handed a `Closure` for the same reason.
    let on_bytes = Closure::once_into_js(move |value: JsValue| {
        let buffer: &js_sys::ArrayBuffer = value.unchecked_ref();
        let bytes: Vec<u8> = js_sys::Uint8Array::new(buffer).to_vec();
        let spliced = crate::export::png_chunk::insert_text_chunk(
            &bytes,
            crate::export::SCENE_FORMAT,
            &payload,
        );
        let spliced = match spliced.ok().and_then(png_blob_of_bytes) {
            Some(blob) => JsValue::from(blob),
            None => JsValue::NULL,
        };
        call(&answered, spliced);
    });
    // `Promise::then` in this wasm-bindgen (0.2.128, pinned) wants a `&ScopedClosure`, which
    // borrows and so cannot outlive this function — and the callback is meant to run after
    // it has returned. `Reflect::apply` over the same `then` is what `then` compiles to, and
    // it takes the callback as a plain function object, which `Closure::once_into_js` owns.
    // The same trick the `toBlob` callback above already uses, one call earlier.
    let pending = blob.array_buffer();
    let Ok(then) = js_sys::Reflect::get(&pending, &JsValue::from_str("then")) else {
        return call(&resolve, JsValue::NULL);
    };
    let then: js_sys::Function = then.unchecked_ref::<js_sys::Function>().clone();
    let callback: js_sys::Function = on_bytes.unchecked_ref::<js_sys::Function>().clone();
    let args = js_sys::Array::new();
    args.push(&callback.into());
    let _ = js_sys::Reflect::apply(&then, &pending, &args);
}

/// A PNG `Blob` of type `image/png` from its bytes — `MIME_TYPES.png`
/// (`data/image.ts@1118751f:46`).
///
/// The type is the engine's, not the host's, for the reason it is in the oracle: a host
/// that wrote `new Blob([bytes])` would be deciding what file it just saved, and
/// `apps/web` already builds a `Blob` of its own for the SVG (`DrawExportModal.svelte`).
/// The raster is the engine's because the raster is the engine's.
fn png_blob_of_bytes(bytes: Vec<u8>) -> Option<web_sys::Blob> {
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let parts = js_sys::Array::new();
    parts.push(&array.into());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("image/png");
    web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).ok()
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
    set_optional(
        &out,
        "refusal",
        copy.refusal.map(|why| {
            let name = match why {
                crate::export::ClipboardRefusal::BrowserCannotTake => "browser-cannot-take",
                crate::export::ClipboardRefusal::NothingToCopy => "nothing-to-copy",
            };
            JsValue::from_str(name)
        }),
    );
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
