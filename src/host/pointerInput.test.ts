/**
 * When a right-button press opens the menu, and when a right-button drag does not.
 *
 * The point of these tests is the **ordering**, not the outcome. The platform fires
 * `contextmenu` with the press on macOS and Linux and after the release on Windows, and
 * the host cannot tell which it is on until it is already too late: a menu opened on the
 * press appears under the pointer and the drag can never happen. So the engine is asked
 * whether an event belongs to a live session (`consumesContextMenu`), and this file pins
 * that the host *asks before it opens anything* — on the press, on the release, and
 * after a drag.
 *
 * The engine stub here is deliberately thin, and deliberately not the oracle: it records
 * that a session is live, and answers as a session must. Whether the real session latches
 * at 5px, swallows the right events and opens the menu on the right release is
 * `tests/ci_secondary_pan.rs`, and this file would not notice if that changed — it is
 * about the wiring around it.
 *
 * node:test — the canonical zero-dep runner for this package.
 */
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { attachPointerInput } from "./pointerInput.ts";
import type { HostSession } from "./session.ts";

/** The engine surface this file uses, and nothing more. */
interface PanEngine {
  beginSecondaryPan(sx: number, sy: number, pointersDown: number): string;
  endSecondaryPan(): string;
  consumesContextMenu(): boolean;
  wantsPointerMoves(): boolean;
  hitTest(x: number, y: number, tolerance: number): { id: string } | null;
  hitsSelectionBox(x: number, y: number): boolean;
  getSelection(): string[];
  selectElement(id: string): void;
  beginPointer(x: number, y: number, shift: boolean, alt: boolean): void;
  endPointer(): void;
  hoverPointer(x: number, y: number): void;
  endHover(): void;
  movePointer(x: number, y: number, shift: boolean, invert: boolean): void;
  setAltHeld(held: boolean): void;
  setCtrlHeld(held: boolean): void;
  setTool(tool: string): void;
  getTool(): string;
  linearInProgress(): boolean;
  panBy(dx: number, dy: number): void;
  wheelZoom(x: number, y: number, delta: number): void;
}

/** What a session answers, so a test can place the platform's event on either side. */
interface Knobs {
  /** What `beginSecondaryPan` answers for a press that qualifies. */
  start?: string;
  /** What the release answers: the three `SecondaryPanEnd` values. */
  release?: string;
}

/** One live session, in one variable, and nothing else. */
function stubEngine(
  hit: { id: string } | null = null,
  knobs: Knobs = {},
): PanEngine & { calls: string[] } {
  const calls: string[] = [];
  let session = false;
  let suppressed = false;
  return {
    calls,
    beginSecondaryPan(_sx, _sy, pointersDown) {
      calls.push(`beginSecondaryPan:${pointersDown}`);
      if (pointersDown > 1) return "declined";
      session = true;
      return knobs.start ?? "started";
    },
    endSecondaryPan() {
      calls.push("endSecondaryPan");
      if (!session) return "none";
      session = false;
      if (knobs.release === "drag") {
        suppressed = true;
        return "drag";
      }
      return knobs.release ?? "menu";
    },
    consumesContextMenu() {
      if (session) return true;
      if (suppressed) {
        suppressed = false;
        return true;
      }
      return false;
    },
    wantsPointerMoves: () => session,
    hitTest: () => hit,
    hitsSelectionBox: () => false,
    getSelection: () => [],
    selectElement: (id) => calls.push(`selectElement:${id}`),
    beginPointer: () => {},
    endPointer: () => {},
    hoverPointer: () => {},
    endHover: () => {},
    movePointer: () => {},
    setAltHeld: () => {},
    setCtrlHeld: () => {},
    setTool: () => {},
    getTool: () => "select",
    linearInProgress: () => false,
    panBy: () => {},
    wheelZoom: () => {},
  };
}

/** A canvas that records the listeners it is given, and nothing else. */
function stubCanvas() {
  const listeners = new Map<string, (event: never) => void>();
  const captured: number[] = [];
  const canvas = {
    addEventListener(type: string, fn: (event: never) => void) {
      listeners.set(type, fn);
    },
    removeEventListener(type: string) {
      listeners.delete(type);
    },
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
    setPointerCapture: (id: number) => captured.push(id),
    hasPointerCapture: (id: number) => captured.includes(id),
    releasePointerCapture: (id: number) => {
      captured.splice(captured.indexOf(id), 1);
    },
    // The paste listeners live on the document and the window, not the canvas.
    ownerDocument: { defaultView: stubView() },
  };
  return { canvas, listeners, captured };
}

/** Just enough window for the paste guard, which this file does not exercise. */
function stubView() {
  return {
    addEventListener: () => {},
    removeEventListener: () => {},
    setTimeout: () => 0,
    clearTimeout: () => {},
  };
}

function pointer(partial: Record<string, unknown> = {}): PointerEvent {
  return {
    pointerId: 1,
    button: 2,
    buttons: 1,
    clientX: 400,
    clientY: 300,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    stopPropagation() {},
    ...partial,
  } as unknown as PointerEvent;
}

function mouse(partial: Record<string, unknown> = {}): MouseEvent {
  return {
    button: 2,
    clientX: 400,
    clientY: 300,
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    stopPropagation() {},
    ...partial,
  } as unknown as MouseEvent;
}

/** A bound canvas, and the menu openings it reported. */
function bound(engine: PanEngine) {
  const { canvas, listeners, captured } = stubCanvas();
  const menus: { x: number; y: number; kind: string }[] = [];
  const session = {
    canvas,
    container: { focus: () => {} },
    engine,
    callbacks: {
      onContextMenu: (point: { x: number; y: number }, kind: string) =>
        menus.push({ ...point, kind }),
    },
  } as unknown as HostSession;
  const detach = attachPointerInput(session);
  const fire = (type: string, event: never) => listeners.get(type)?.(event);
  return { fire, detach, menus, captured };
}

describe("a right-button press and the context menu", () => {
  it("swallows a contextmenu that arrives with the press, and opens on the release", () => {
    // macOS and Linux: the event comes with the mousedown, before the gesture can be
    // known. Opening the menu here is the bug — it appears under the pointer and the
    // right-drag to pan never happens.
    const engine = stubEngine();
    const { fire, menus } = bound(engine);
    const down = pointer();
    fire("pointerdown", down);
    fire("contextmenu", mouse());
    assert.deepEqual(menus, [], "no menu on the press");
    fire("pointerup", pointer({ buttons: 0 }));
    assert.equal(menus.length, 1, "the release opens it instead");
    assert.equal(menus[0].kind, "canvas");
  });

  it("prevents the press's default, so the page neither scrolls nor steals the focus", () => {
    const engine = stubEngine();
    const { fire } = bound(engine);
    const down = pointer();
    fire("pointerdown", down);
    assert.equal(down.defaultPrevented, true);
  });

  it("leaves the press's default alone while a text is open", () => {
    // Issue #4489: preventing it while text is being typed breaks the caret and focus.
    // The engine is the one that knows a text is open, and it says so in its answer.
    const engine = stubEngine(null, { start: "editing-text" });
    const { fire } = bound(engine);
    const down = pointer();
    fire("pointerdown", down);
    assert.equal(down.defaultPrevented, false, "the caret needs the default");
  });

  it("swallows a contextmenu that follows a drag's release", () => {
    // Windows ordering: the platform's event comes after the release, and a release that
    // turned out to be a drag is not a click.
    const engine = stubEngine(null, { release: "drag" });
    const { fire, menus } = bound(engine);
    fire("pointerdown", pointer());
    fire("pointerup", pointer({ buttons: 0 }));
    fire("contextmenu", mouse());
    assert.deepEqual(menus, [], "a drag is not a click");
  });

  it("opens for a click whose contextmenu follows the release", () => {
    // The other Windows case, and the reason the two platforms can share one code path:
    // a release that never engaged is an ordinary right-click, and the event the platform
    // sends afterwards opens the menu exactly as it always has.
    const engine = stubEngine(null, { release: "none" });
    const { fire, menus } = bound(engine);
    fire("pointerdown", pointer());
    fire("pointerup", pointer({ buttons: 0 }));
    fire("contextmenu", mouse());
    assert.equal(menus.length, 1);
  });

  it("opens the menu for a right-click with nothing under the point, and selects nothing", () => {
    const engine = stubEngine(null, { release: "none" });
    const { fire, menus } = bound(engine);
    fire("pointerdown", pointer());
    fire("pointerup", pointer({ buttons: 0 }));
    fire("contextmenu", mouse());
    assert.equal(menus.length, 1);
    assert.equal(menus[0].kind, "canvas");
    assert.deepEqual(
      engine.calls.filter((call) => call.startsWith("selectElement")),
      [],
      "a right-click over bare canvas clears no selection and picks nothing",
    );
  });

  it("selects what is under the point for a right-click, as a press does", () => {
    const engine = stubEngine({ id: "shape-1" }, { release: "none" });
    const { fire, menus } = bound(engine);
    fire("pointerdown", pointer());
    fire("pointerup", pointer({ buttons: 0 }));
    fire("contextmenu", mouse());
    assert.deepEqual(engine.calls, ["beginSecondaryPan:1", "endSecondaryPan", "selectElement:shape-1"]);
    assert.equal(menus[0].kind, "element");
  });
});

describe("a right-button press as a pan", () => {
  it("asks the engine for moves with no button of the host's own down", () => {
    // The cost gate in `wantsMove` drops moves when no button is held. A session has to be
    // visible to it or the moves that decide click-versus-drag are dropped and every
    // right-drag is a right-click.
    const engine = stubEngine();
    const { fire } = bound(engine);
    fire("pointerdown", pointer());
    assert.equal(engine.wantsPointerMoves(), true);
    fire("pointerup", pointer({ buttons: 0 }));
    assert.equal(engine.wantsPointerMoves(), false);
  });

  it("counts the pointer that is down, so a second finger is not a pan", () => {
    const engine = stubEngine();
    const { fire, menus } = bound(engine);
    fire("pointerdown", pointer({ pointerId: 7, button: 0, buttons: 1 }));
    fire("pointerdown", pointer({ pointerId: 9 }));
    assert.ok(
      engine.calls.includes("beginSecondaryPan:2"),
      "the engine is told two pointers are down and declines",
    );
    fire("pointerup", pointer({ pointerId: 9, buttons: 0 }));
    // And the second pointer's release is the one that ends the session, not a menu.
    assert.deepEqual(menus, []);
  });

  it("captures the pointer, so a drag that leaves the canvas keeps panning", () => {
    const engine = stubEngine();
    const { fire, captured } = bound(engine);
    fire("pointerdown", pointer());
    assert.deepEqual(captured, [1]);
    fire("pointerup", pointer({ buttons: 0 }));
    assert.deepEqual(captured, []);
  });

  it("forgets a pointer the platform cancelled, so the count cannot stay raised", () => {
    // A touch claimed by a browser gesture of its own never sends a pointerup. Left in the
    // count, it declines every right press for the rest of the session.
    const engine = stubEngine();
    const { fire } = bound(engine);
    fire("pointerdown", pointer({ pointerId: 3, button: 0 }));
    fire("pointercancel", pointer({ pointerId: 3 }));
    fire("pointerdown", pointer({ pointerId: 4 }));
    assert.ok(engine.calls.includes("beginSecondaryPan:1"), "one pointer down, so a pan");
  });
});
