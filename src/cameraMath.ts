/**
 * The camera's world↔screen transform and its zoom limits — **called, not written**.
 *
 * Every function here is a binding on `engine/crates/draw-engine/src/camera.rs`:
 * `world_to_screen` (`camera.rs:147`), `screen_to_world` (`:154`) and the two limit
 * constants (`:5-6`), exported over WASM from `src/wasm/camera_api.rs`. This file used
 * to be a hand-kept TypeScript mirror of the same four, and the front called the mirror,
 * so every number the chrome placed was a copy of the engine's arithmetic that nothing
 * compared to it.
 *
 * **Do not add arithmetic here.** If a host needs a number the engine does not produce,
 * the fix is a Rust function in `camera.rs` and a `#[wasm_bindgen]` beside the ones
 * already here — not a second expression. `apps/web/src/lib/draw-chrome/cameraParity.test.ts`
 * reads both sides and fails if one of these expressions reappears in TypeScript.
 */
import {
  maxZoom as engineMaxZoom,
  minZoom as engineMinZoom,
  screenToWorld as engineScreenToWorld,
  worldToScreen as engineWorldToScreen,
} from "../pkg/draw_engine.js";
import type { Camera } from "./types";

export interface ScreenPoint {
  x: number;
  y: number;
}

/**
 * Where a world point lands on screen: `wx * scale + camera.x`, from the engine.
 *
 * Takes the camera rather than reading it, so a caller that already holds one — the
 * peer-cursor layer, a presentation path badge — pays no `cameraJson` round trip. That
 * is the only reason this is a free function and not a `DrawEngine` method, and it is
 * why the method is not what a per-frame path should reach for.
 */
export function worldToScreen(camera: Camera, wx: number, wy: number): ScreenPoint {
  const at = engineWorldToScreen(camera.x, camera.y, camera.scale, wx, wy);
  // The Rust side always answers with two numbers. `NaN` rather than a non-null
  // assertion, because `noUncheckedIndexedAccess` reads the index as optional and a
  // missing coordinate is `NaN` — which every consumer here already handles as "no
  // position" — not a crash.
  return { x: at[0] ?? Number.NaN, y: at[1] ?? Number.NaN };
}

/** Where a screen point is in the world: `(sx - camera.x) / scale`, from the engine. */
export function screenToWorld(camera: Camera, sx: number, sy: number): ScreenPoint {
  const at = engineScreenToWorld(camera.x, camera.y, camera.scale, sx, sy);
  return { x: at[0] ?? Number.NaN, y: at[1] ?? Number.NaN };
}

/** [`camera.rs:5`]: the furthest out a camera may be pulled. */
export function minZoom(): number {
  return engineMinZoom();
}

/** [`camera.rs:6`]: the closest in a camera may be pushed. */
export function maxZoom(): number {
  return engineMaxZoom();
}
