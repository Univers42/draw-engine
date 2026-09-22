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
  return {
    deltaX: 0,
    deltaY: 0,
    // DOM_DELTA_PIXEL. The default because it is what Chromium and WebKit always
    // report, so every test that is not about deltaMode reads as it did before.
    deltaMode: 0,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...partial,
  };
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

  it("normalises a line-mode delta to pixels, so Firefox moves like Chrome", () => {
    // The bug this pins. `deltaMode` says what unit `deltaY` is in, and browsers do not
    // agree: Chromium always reports PIXEL, Firefox reports LINE for a mouse wheel on
    // Windows and Linux — one notch is `deltaY: 3, deltaMode: 1` there against Chrome's
    // `deltaY: 100, deltaMode: 0`.
    //
    // Read raw, that notch was worth three pixels. For the zoom that is under
    // `MAX_WHEEL_DELTA` and came out as 3% instead of 10%; for the pan it moved the board
    // three pixels instead of a hundred, which is a board that does not move.
    assert.deepEqual(wheelIntent(wheel({ deltaY: -3, deltaMode: 1, ctrlKey: true })), {
      kind: "zoom",
      deltaY: -120,
    });
    assert.deepEqual(wheelIntent(wheel({ deltaY: 3, deltaMode: 1 })), {
      kind: "pan",
      dx: 0,
      dy: -120,
    });
  });

  it("normalises a page-mode delta to pixels", () => {
    // DOM_DELTA_PAGE. Rare — some Windows configurations and a few assistive devices —
    // but it is the same failure an order of magnitude further on: one page read as one
    // pixel is a board that is frozen rather than merely slow.
    assert.deepEqual(wheelIntent(wheel({ deltaY: -1, deltaMode: 2, ctrlKey: true })), {
      kind: "zoom",
      deltaY: -800,
    });
    assert.deepEqual(wheelIntent(wheel({ deltaY: 1, deltaMode: 2 })), {
      kind: "pan",
      dx: 0,
      dy: -800,
    });
  });

  it("normalises both axes, not just the one being read", () => {
    // deltaX carries the pan on a tilt wheel and on macOS shift+wheel. Normalising only
    // deltaY would leave sideways scrolling slow on exactly the browser this fixes.
    assert.deepEqual(wheelIntent(wheel({ deltaX: 3, deltaY: 3, deltaMode: 1 })), {
      kind: "pan",
      dx: -120,
      dy: -120,
    });
    assert.deepEqual(wheelIntent(wheel({ deltaX: 2, deltaMode: 1, shiftKey: true })), {
      kind: "pan",
      dx: -80,
      dy: 0,
    });
  });

  it("lands a line-mode notch on a delta some mouse already sends in pixels", () => {
    // Why 40 and not 33.3: a Windows mouse in pixel mode sends 120 for one notch, so a
    // normalised Firefox notch is a value the stack already handles rather than a new
    // one invented for it. Both clamp to a single step in `wheel_zoom_scale`, so the
    // zoom is identical; the pan differs by the 20px that separates the two real
    // browsers anyway.
    const firefoxNotch = wheelIntent(wheel({ deltaY: -3, deltaMode: 1, ctrlKey: true }));
    const windowsMouseNotch = wheelIntent(wheel({ deltaY: -120, deltaMode: 0, ctrlKey: true }));
    assert.deepEqual(firefoxNotch, windowsMouseNotch);
  });

  it("leaves a pixel-mode delta exactly as it arrived", () => {
    // The regression guard for the fix itself: every browser that already worked must
    // keep its arithmetic untouched, to the bit.
    for (const deltaY of [-240, -100, -53, -3, 3, 53, 100, 240]) {
      assert.deepEqual(wheelIntent(wheel({ deltaY, deltaMode: 0, ctrlKey: true })), {
        kind: "zoom",
        deltaY,
      });
    }
  });

  it("treats an unknown deltaMode as pixels rather than scaling by a guess", () => {
    // The spec defines 0, 1 and 2. A future or vendor value must not be multiplied by a
    // constant chosen for a different unit — passing it through is wrong by at most the
    // unit, where guessing is wrong by 40×.
    assert.deepEqual(wheelIntent(wheel({ deltaY: -100, deltaMode: 7, ctrlKey: true })), {
      kind: "zoom",
      deltaY: -100,
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
