/**
 * What a wheel event means: zoom, or pan, and along which axis.
 *
 * Only the *meaning* — none of these tests say how far. How far a wheel delta is worth
 * is arithmetic and lives in the engine (`wheel_zoom_scale`, pinned by
 * `tests/ci_zoom_wheel.rs`), so this file asserts that the raw delta is handed over
 * untouched and never pre-chewed here, where only one of the hosts would get it.
 *
 * node:test — the canonical zero-dep runner for this package.
 */
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { wheelIntent, type WheelInput } from "./wheel.ts";

function wheel(partial: Partial<WheelInput> = {}): WheelInput {
  return { deltaX: 0, deltaY: 0, ctrlKey: false, metaKey: false, shiftKey: false, ...partial };
}

describe("wheelIntent", () => {
  it("zooms on ctrl+wheel and on cmd+wheel", () => {
    // ctrl+wheel is also how a browser delivers a trackpad pinch, so this is the pinch
    // path as much as it is the keyboard one.
    assert.deepEqual(wheelIntent(wheel({ deltaY: -100, ctrlKey: true })), {
      kind: "zoom",
      deltaY: -100,
    });
    assert.deepEqual(wheelIntent(wheel({ deltaY: 100, metaKey: true })), {
      kind: "zoom",
      deltaY: 100,
    });
  });

  it("hands the raw delta to the engine rather than a factor", () => {
    // The whole point of the split. A host that converted the delta to a factor would be
    // deciding the step itself, and the next host would decide a different one — which is
    // how the zoom came to jump by 2.7× per notch in the first place.
    for (const deltaY of [-240, -100, -53, -3, 3, 53, 100, 240]) {
      assert.deepEqual(wheelIntent(wheel({ deltaY, ctrlKey: true })), { kind: "zoom", deltaY });
    }
  });

  it("pans on a plain wheel, in the direction the content should move", () => {
    // Negated: scrolling the wheel down means the content goes up, so the camera origin
    // moves the other way from the delta.
    assert.deepEqual(wheelIntent(wheel({ deltaX: 30, deltaY: 40 })), {
      kind: "pan",
      dx: -30,
      dy: -40,
    });
  });

  it("pans horizontally on shift+wheel", () => {
    assert.deepEqual(wheelIntent(wheel({ deltaY: 40, shiftKey: true })), {
      kind: "pan",
      dx: -40,
      dy: 0,
    });
  });

  it("takes shift+wheel from deltaX when that is where the platform puts it", () => {
    // macOS turns shift+wheel into a horizontal delta before the page ever sees it, so
    // the same gesture arrives on the other axis. Reading only deltaY would make
    // shift+wheel do nothing there.
    assert.deepEqual(wheelIntent(wheel({ deltaX: 40, shiftKey: true })), {
      kind: "pan",
      dx: -40,
      dy: 0,
    });
  });

  it("pans vertically on ctrl+shift+wheel instead of zooming", () => {
    // Shift claims the event for panning; the modifier that would otherwise zoom only
    // chooses the axis.
    assert.deepEqual(wheelIntent(wheel({ deltaY: 40, shiftKey: true, ctrlKey: true })), {
      kind: "pan",
      dx: 0,
      dy: -40,
    });
    assert.deepEqual(wheelIntent(wheel({ deltaY: 40, shiftKey: true, metaKey: true })), {
      kind: "pan",
      dx: 0,
      dy: -40,
    });
  });

  it("pans sideways on a horizontal-only wheel even with the zoom modifier down", () => {
    // A tilt wheel, or a sideways two-finger scroll with ctrl still held from the last
    // pinch. There is no vertical delta to zoom by, and zooming by the horizontal one
    // would zoom the board while you are plainly scrolling across it.
    assert.deepEqual(wheelIntent(wheel({ deltaX: 30, ctrlKey: true })), {
      kind: "pan",
      dx: -30,
      dy: 0,
    });
  });

  it("does nothing for an event with no delta at all", () => {
    // Browsers do emit these — at the end of a momentum scroll, and when a wheel listener
    // is attached to an element that never scrolls. Panning by zero would still cost a
    // WASM hop and a repaint per event.
    assert.deepEqual(wheelIntent(wheel()), { kind: "none" });
    assert.deepEqual(wheelIntent(wheel({ ctrlKey: true })), { kind: "none" });
    assert.deepEqual(wheelIntent(wheel({ shiftKey: true })), { kind: "none" });
  });
});
