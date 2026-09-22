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
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
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
  const { deltaX, deltaY } = event;
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
