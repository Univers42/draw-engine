/**
 * What a wheel event means, before anything is done about it.
 *
 * Only the meaning. How far a wheel delta moves the camera is arithmetic and lives in
 * the engine — `wheelZoom` takes the raw `deltaY` and works the step out there, so every
 * host that mounts this engine zooms by the same amount. A host that converted the delta
 * to a zoom factor itself would be choosing that step in private, and the next host would
 * choose a different one.
 *
 * The mappings follow Excalidraw (`App.wheel.ts`): ctrl/cmd zooms, which is also how a
 * trackpad pinch arrives; shift pans horizontally; ctrl/cmd+shift pans vertically.
 */

/** The fields of a `WheelEvent` any of this depends on. */
export interface WheelInput {
  deltaX: number;
  deltaY: number;
  /**
   * `WheelEvent.deltaMode` — the unit the two deltas are counted in, not a quantity.
   * 0 pixels, 1 lines, 2 pages.
   */
  deltaMode: number;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

/** `WheelEvent.DOM_DELTA_LINE` / `DOM_DELTA_PAGE`, named so the branch below reads. */
const DOM_DELTA_LINE = 1;
const DOM_DELTA_PAGE = 2;

/**
 * What one line and one page are worth in pixels.
 *
 * The values from Facebook's `normalizeWheel`, which is where most of the web got them
 * and what the numbers below are chosen to agree with. 40 is the useful one: Firefox
 * reports a mouse notch as 3 lines, so a normalised notch is 120px — exactly what a
 * Windows mouse already sends in pixel mode. The alternative, 33.3, would match Chrome's
 * 100 to the pixel but invent a unit no device produces.
 */
const PIXELS_PER_LINE = 40;
const PIXELS_PER_PAGE = 800;

/**
 * Both deltas in pixels, whatever unit the browser counted them in.
 *
 * `deltaMode` is the half of a wheel event everyone forgets, including Excalidraw, which
 * reads `deltaX`/`deltaY` raw (`App.wheel.ts`). It costs Chromium nothing — it always
 * reports pixels — and it costs Firefox on Windows and Linux almost everything: a mouse
 * notch there is `deltaY: 3, deltaMode: 1`, which read as pixels zoomed the board by 3%
 * instead of 10% and panned it three pixels instead of a hundred.
 *
 * An unrecognised mode is passed through rather than scaled. The spec defines three, and
 * treating a fourth as pixels is wrong by at most the unit, where guessing a multiplier
 * for it is wrong by forty.
 */
function toPixels(delta: number, mode: number): number {
  if (mode === DOM_DELTA_LINE) return delta * PIXELS_PER_LINE;
  if (mode === DOM_DELTA_PAGE) return delta * PIXELS_PER_PAGE;
  return delta;
}

/**
 * `zoom` carries the raw `deltaY`; `pan` carries screen-pixel offsets ready for
 * `panBy`, already negated so the content moves against the wheel.
 */
export type WheelIntent =
  | { kind: "zoom"; deltaY: number }
  | { kind: "pan"; dx: number; dy: number }
  | { kind: "none" };

/**
 * The offset the camera moves by for a wheel delta: the other way, and never `-0`.
 *
 * Plain `-delta` turns a zero delta into `-0`, which is `=== 0` but not `Object.is` it —
 * so it compares unequal in tests and in any later memoisation of the last pan, for a
 * value that means nothing happened.
 */
function away(delta: number): number {
  return delta ? -delta : 0;
}

export function wheelIntent(event: WheelInput): WheelIntent {
  // Normalised before anything reads them, so every branch below — zoom, both pan axes,
  // and the macOS shift+wheel fallback from deltaY to deltaX — is in one unit. Doing it
  // per branch is how one axis ends up fixed and the other left slow.
  const deltaX = toPixels(event.deltaX, event.deltaMode);
  const deltaY = toPixels(event.deltaY, event.deltaMode);
  if (!deltaX && !deltaY) return { kind: "none" };

  const hasZoomModifier = event.ctrlKey || event.metaKey;

  // Shift claims the event for panning outright; the zoom modifier alongside it only
  // chooses which axis. Checked before the zoom so the two cannot both fire.
  if (event.shiftKey) {
    // macOS turns shift+wheel into a horizontal delta before the page sees it, so the
    // same gesture arrives on whichever axis the platform felt like using.
    const delta = deltaY || deltaX;
    return hasZoomModifier
      ? { kind: "pan", dx: 0, dy: away(delta) }
      : { kind: "pan", dx: away(delta), dy: 0 };
  }

  // A horizontal-only wheel — a tilt wheel, or a sideways two-finger scroll with ctrl
  // still held from the last pinch — has nothing to zoom by, and zooming by the sideways
  // delta would zoom the board while you are plainly scrolling across it.
  if (hasZoomModifier && deltaY !== 0) return { kind: "zoom", deltaY };

  return { kind: "pan", dx: away(deltaX), dy: away(deltaY) };
}
