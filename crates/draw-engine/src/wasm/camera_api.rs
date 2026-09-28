//! The camera's world↔screen transform and its zoom limits, over the WASM boundary.
//!
//! The host used to keep a TypeScript mirror of these four (`engine/src/camera.ts`) and
//! the front called that mirror, so the numbers on screen were a hand-kept copy of
//! [`crate::camera`] rather than the engine's own. They are free functions rather than
//! [`WasmEngine`] methods for a reason: the callers are chrome that already holds a
//! [`Camera`] and often has no engine at all — a peer's cursor, the shape-switch panel,
//! a presentation path badge. Reading the camera through a method would mean a
//! `cameraJson` round trip per point, on a path that runs per frame.
//!
//! Nothing here computes anything. Each function is the binding and the arithmetic
//! stays in [`crate::camera::world_to_screen`], [`crate::camera::screen_to_world`] and
//! the two limit constants, so there is one copy of each and this file cannot become a
//! second one. `apps/web/src/lib/draw-chrome/cameraParity.test.ts` is what holds that.
//!
//! The camera arrives as three numbers and the answer leaves as a `Float64Array` of two,
//! rather than as a `Camera`/`Point` pair: neither struct is a `#[wasm_bindgen]` type, and
//! making them one would put a copy-to-JS of a third type on a per-frame path to save an
//! allocation this does not make in the first place.

use wasm_bindgen::prelude::*;

use crate::camera::Camera;

/// [`Camera`], from the three numbers the host already holds.
fn camera(x: f64, y: f64, scale: f64) -> Camera {
    Camera { x, y, scale }
}

#[wasm_bindgen(js_name = worldToScreen)]
pub fn world_to_screen_js(
    camera_x: f64,
    camera_y: f64,
    camera_scale: f64,
    wx: f64,
    wy: f64,
) -> Box<[f64]> {
    let at = crate::camera::world_to_screen(camera(camera_x, camera_y, camera_scale), wx, wy);
    Box::new([at.x, at.y])
}

#[wasm_bindgen(js_name = screenToWorld)]
pub fn screen_to_world_js(
    camera_x: f64,
    camera_y: f64,
    camera_scale: f64,
    sx: f64,
    sy: f64,
) -> Box<[f64]> {
    let at = crate::camera::screen_to_world(camera(camera_x, camera_y, camera_scale), sx, sy);
    Box::new([at.x, at.y])
}

/// [`crate::camera::MIN_ZOOM`]. A call rather than a constant because `wasm_bindgen` has
/// no way to export one, and a TypeScript copy of the pair is what this file exists to
/// end.
#[wasm_bindgen(js_name = minZoom)]
pub fn min_zoom_js() -> f64 {
    crate::camera::MIN_ZOOM
}

/// [`crate::camera::MAX_ZOOM`]. See [`min_zoom_js`] on why this is a call.
#[wasm_bindgen(js_name = maxZoom)]
pub fn max_zoom_js() -> f64 {
    crate::camera::MAX_ZOOM
}
