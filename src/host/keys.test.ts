/**
 * Host keyboard dispatch tests. Written against the React adapter's original
 * chords so a Svelte (or vanilla) bind cannot drift. node:test — the canonical
 * zero-dep runner for this package (no vitest/jest configured; see facts.sh).
 */
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { dispatchKeyDown, dispatchKeyUp, type KeyEngine, type KeyEvent, type KeySession } from "./keys.ts";
import { localPoint } from "./session.ts";

function event(partial: Partial<KeyEvent> & Pick<KeyEvent, "key">): KeyEvent {
  return {
    code: "",
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ctrlKey: false,
    repeat: false,
    ...partial,
  };
}

function session(engine: KeyEngine, extras: Partial<KeySession> = {}): KeySession {
  return { engine, callbacks: {}, spaceHeld: false, ...extras };
}

function recording(
  selection: string[] = [],
  placingPath = false,
  creatingFlowchart = false,
  navigateTarget: string | null = null,
): { engine: KeyEngine; calls: string[] } {
  const calls: string[] = [];
  let locked = false;
  const engine = {
    flowchartCreate: (direction: string) => calls.push(`flowchartCreate:${direction}`),
    flowchartSetShape: (shape: string) => calls.push(`flowchartSetShape:${shape}`),
    flowchartCommit: () => calls.push("flowchartCommit"),
    flowchartCancel: () => calls.push("flowchartCancel"),
    isCreatingFlowchart: () => creatingFlowchart,
    flowchartNavigate: (direction: string) => {
      calls.push(`flowchartNavigate:${direction}`);
      return navigateTarget;
    },
    flowchartNavigationEnd: () => calls.push("flowchartNavigationEnd"),
    cancelPointer: () => calls.push("cancelPointer"),
    deleteSelection: () => calls.push("deleteSelection"),
    getSelection: () => selection,
    nudgeSelection: (dx: number, dy: number) => calls.push(`nudge:${dx},${dy}`),
    redo: () => calls.push("redo"),
    undo: () => calls.push("undo"),
    selectAll: () => calls.push("selectAll"),
    duplicateSelection: () => calls.push("duplicateSelection"),
    ungroupSelection: () => calls.push("ungroupSelection"),
    groupSelection: () => calls.push("groupSelection"),
    toggleGroupSelection: () => calls.push("toggleGroupSelection"),
    toggleLockSelection: () => calls.push("toggleLockSelection"),
    reorderSelection: (mode: string) => calls.push(`reorder:${mode}`),
    zoomIn: () => calls.push("zoomIn"),
    zoomOut: () => calls.push("zoomOut"),
    zoomReset: () => calls.push("zoomReset"),
    copySelection: () => {
      calls.push("copySelection");
      return null;
    },
    cutSelection: () => {
      calls.push("cutSelection");
      return null;
    },
    fit: () => calls.push("fit"),
    zoomToSelection: () => calls.push("zoomToSelection"),
    pageBy: (x: number, y: number) => calls.push(`pageBy:${x},${y}`),
    editSelectedText: () => {
      calls.push("editSelectedText");
      return false;
    },
    flipSelection: (axis: string) => calls.push(`flip:${axis}`),
    setToolLocked: (next: boolean) => {
      locked = next;
      calls.push(`setToolLocked:${next}`);
    },
    getToolLocked: () => locked,
    setTool: (tool: string) => calls.push(`setTool:${tool}`),
    activateTool: (tool: string) => calls.push(`activateTool:${tool}`),
    linearInProgress: () => placingPath,
    finishLinear: () => calls.push("finishLinear"),
  } as unknown as KeyEngine;
  return { engine, calls };
}

describe("dispatchKeyDown", () => {
  it("cancels the pointer on Escape without preventDefault", () => {
    const { engine, calls } = recording();
    const notified: boolean[] = [];
    const result = dispatchKeyDown(
      session(engine, { callbacks: { onFlowchartCreatingChange: (creating) => notified.push(creating) } }),
      event({ key: "Escape" }),
    );
    assert.equal(result, "pass");
    assert.deepEqual(calls, ["cancelPointer"]);
    assert.deepEqual(notified, []);
  });

  /**
   * The oracle's flowchart handler takes Escape ahead of everything else while a cluster
   * is previewed (`App.tsx@1118751f:5574`): the cluster goes and nothing else happens —
   * the node it grew from stays selected.
   */
  it("drops the previewed flowchart on Escape and takes the key", () => {
    const { engine, calls } = recording(["node"], false, true);
    const notified: boolean[] = [];
    const result = dispatchKeyDown(
      session(engine, { callbacks: { onFlowchartCreatingChange: (creating) => notified.push(creating) } }),
      event({ key: "Escape" }),
    );
    assert.equal(result, "prevent");
    assert.deepEqual(calls, ["flowchartCancel"]);
    assert.deepEqual(notified, [false]);
  });

  /**
   * Enter ends a path being placed, as Excalidraw's does — both it and Escape run their
   * `actionFinalize`. Escape reaches the same place through `cancelPointer`, which the
   * engine routes to the same finish, so it needs no branch of its own here.
   */
  it("finishes a path being placed on Enter", () => {
    const { engine, calls } = recording([], true);
    const result = dispatchKeyDown(session(engine), event({ key: "Enter" }));
    assert.equal(result, "prevent");
    assert.deepEqual(calls, ["finishLinear"]);
  });

  /**
   * The other half, and the reason the branch above is guarded rather than
   * unconditional: Enter already means "edit the selected text". Claiming the key
   * outright would take that away, so a path being placed borrows it and nothing else
   * changes.
   */
  it("leaves Enter to the text editor when no path is being placed", () => {
    const { engine, calls } = recording([], false);
    const result = dispatchKeyDown(session(engine), event({ key: "Enter" }));
    assert.equal(result, "pass");
    assert.deepEqual(calls, ["editSelectedText"]);
  });

  /**
   * Ctrl+G toggles; Ctrl+Shift+G is still the explicit ungroup. The oracle's Ctrl+G is a
   * no-op once a selection is grouped, which leaves the key with no inverse.
   */
  it("toggles grouping on Ctrl+G and ungroups on Ctrl+Shift+G", () => {
    const toggled = recording(["a", "b"]);
    dispatchKeyDown(session(toggled.engine), event({ key: "g", ctrlKey: true }));
    assert.deepEqual(toggled.calls, ["toggleGroupSelection"]);

    const ungrouped = recording(["a", "b"]);
    dispatchKeyDown(
      session(ungrouped.engine),
      event({ key: "g", ctrlKey: true, shiftKey: true }),
    );
    assert.deepEqual(ungrouped.calls, ["ungroupSelection"]);
  });

  /**
   * Ctrl/Cmd+Shift+L locks or unlocks the selection — `actionToggleElementLock`'s
   * `keyTest` (`actionElementLock.ts@1118751f:151-160`) — and only with something
   * selected: with nothing, the chord is left to the browser.
   */
  it("toggles the lock on Ctrl/Cmd+Shift+L with a selection", () => {
    for (const mod of [{ ctrlKey: true }, { metaKey: true }]) {
      const held = recording(["a"]);
      const result = dispatchKeyDown(session(held.engine), event({ key: "L", shiftKey: true, ...mod }));
      assert.equal(result, "prevent");
      assert.deepEqual(held.calls, ["toggleLockSelection"]);
    }

    const empty = recording([]);
    const result = dispatchKeyDown(session(empty.engine), event({ key: "L", ctrlKey: true, shiftKey: true }));
    assert.equal(result, "pass");
    assert.deepEqual(empty.calls, []);
  });

  it("deletes the selection on Delete and prevents default", () => {
    const { engine, calls } = recording(["a"]);
    const result = dispatchKeyDown(session(engine), event({ key: "Delete" }));
    assert.equal(result, "prevent");
    assert.deepEqual(calls, ["deleteSelection"]);
  });

  it("does not nudge when nothing is selected", () => {
    const { engine, calls } = recording([]);
    const result = dispatchKeyDown(session(engine), event({ key: "ArrowLeft" }));
    assert.equal(result, "pass");
    assert.deepEqual(calls, []);
  });

  it("nudges one pixel, or ten with Shift", () => {
    const a = recording(["id"]);
    assert.equal(dispatchKeyDown(session(a.engine), event({ key: "ArrowRight" })), "prevent");
    assert.deepEqual(a.calls, ["nudge:1,0"]);
    const b = recording(["id"]);
    assert.equal(dispatchKeyDown(session(b.engine), event({ key: "ArrowUp", shiftKey: true })), "prevent");
    assert.deepEqual(b.calls, ["nudge:0,-10"]);
  });

  it("undoes on Ctrl+Z and redoes on Ctrl+Shift+Z / Ctrl+Y", () => {
    const z = recording();
    assert.equal(dispatchKeyDown(session(z.engine), event({ key: "z", ctrlKey: true })), "prevent");
    assert.deepEqual(z.calls, ["undo"]);
    const zs = recording();
    assert.equal(dispatchKeyDown(session(zs.engine), event({ key: "Z", ctrlKey: true, shiftKey: true })), "prevent");
    assert.deepEqual(zs.calls, ["redo"]);
    const y = recording();
    assert.equal(dispatchKeyDown(session(y.engine), event({ key: "y", metaKey: true })), "prevent");
    assert.deepEqual(y.calls, ["redo"]);
  });

  it("does not steal Ctrl+V — paste rides the native event", () => {
    const { engine, calls } = recording();
    const result = dispatchKeyDown(session(engine), event({ key: "v", ctrlKey: true }));
    assert.equal(result, "pass");
    assert.deepEqual(calls, []);
  });

  it("fits on Shift+1 and leaves Ctrl+chords to the page", () => {
    const fit = recording();
    assert.equal(dispatchKeyDown(session(fit.engine), event({ key: "!", shiftKey: true, code: "Digit1" })), "prevent");
    assert.deepEqual(fit.calls, ["fit"]);
    const chord = recording();
    assert.equal(dispatchKeyDown(session(chord.engine), event({ key: "k", altKey: true })), "pass");
    assert.deepEqual(chord.calls, []);
  });

  it("zooms to the selection on Shift+2", () => {
    // Matched on `code`, like Shift+1: `key` for a shifted digit is a punctuation mark
    // that differs per layout — "@" on a US keyboard, "é" on a French one.
    const zoom = recording(["id"]);
    assert.equal(
      dispatchKeyDown(session(zoom.engine), event({ key: "@", shiftKey: true, code: "Digit2" })),
      "prevent",
    );
    assert.deepEqual(zoom.calls, ["zoomToSelection"]);
  });

  it("pages the canvas on Page Up and Page Down, sideways with shift", () => {
    // Shift turns vertical into horizontal here the same way it does for the wheel, so
    // the modifier means one thing across the whole board.
    const down = recording();
    assert.equal(dispatchKeyDown(session(down.engine), event({ key: "PageDown" })), "prevent");
    assert.deepEqual(down.calls, ["pageBy:0,1"]);

    const up = recording();
    assert.equal(dispatchKeyDown(session(up.engine), event({ key: "PageUp" })), "prevent");
    assert.deepEqual(up.calls, ["pageBy:0,-1"]);

    const sideways = recording();
    assert.equal(
      dispatchKeyDown(session(sideways.engine), event({ key: "PageDown", shiftKey: true })),
      "prevent",
    );
    assert.deepEqual(sideways.calls, ["pageBy:1,0"]);
  });

  it("flips a selection with Shift+H / Shift+V, else maps H to the hand tool", () => {
    // `activateTool`, not `setTool`: the hand is a toggle tool, so pressing H while
    // already panning goes back to the tool it interrupted. A toolbar click stays on
    // `setTool`, which never toggles.
    const flip = recording(["id"]);
    assert.equal(dispatchKeyDown(session(flip.engine), event({ key: "H", code: "KeyH", shiftKey: true })), "prevent");
    assert.deepEqual(flip.calls, ["flip:horizontal"]);
    const hand = recording([]);
    assert.equal(dispatchKeyDown(session(hand.engine), event({ key: "h" })), "prevent");
    assert.deepEqual(hand.calls, ["activateTool:hand"]);
  });

  it("flips by the physical key, whatever the layout prints on it (actionFlip.ts:51, :76-77)", () => {
    // A Russian layout prints Р on the H key and М on the V key.
    const horizontal = recording(["id"]);
    assert.equal(dispatchKeyDown(session(horizontal.engine), event({ key: "Р", code: "KeyH", shiftKey: true })), "prevent");
    assert.deepEqual(horizontal.calls, ["flip:horizontal"]);
    const vertical = recording(["id"]);
    assert.equal(dispatchKeyDown(session(vertical.engine), event({ key: "М", code: "KeyV", shiftKey: true })), "prevent");
    assert.deepEqual(vertical.calls, ["flip:vertical"]);
    // Ctrl+Shift+V is the browser's paste as plain text, never a flip.
    const paste = recording(["id"]);
    dispatchKeyDown(session(paste.engine), event({ key: "V", code: "KeyV", shiftKey: true, ctrlKey: true }));
    assert.deepEqual(paste.calls, []);
  });

  it("reorders on Ctrl+[ / Ctrl+], to the end with Shift or Alt (actionZindex.tsx)", () => {
    const chords: [Partial<KeyEvent> & Pick<KeyEvent, "key">, string][] = [
      [{ key: "]", code: "BracketRight", ctrlKey: true }, "reorder:forward"],
      [{ key: "[", code: "BracketLeft", ctrlKey: true }, "reorder:backward"],
      // Windows and Linux. Shift prints a brace on a US layout: the physical key decides.
      [{ key: "}", code: "BracketRight", ctrlKey: true, shiftKey: true }, "reorder:front"],
      [{ key: "{", code: "BracketLeft", ctrlKey: true, shiftKey: true }, "reorder:back"],
      // macOS, where Option prints a quote on the bracket keys.
      [{ key: "‘", code: "BracketRight", metaKey: true, altKey: true }, "reorder:front"],
      [{ key: "“", code: "BracketLeft", metaKey: true, altKey: true }, "reorder:back"],
      // Unshifted, a layout that prints + on that key keeps it for zoom.
      [{ key: "+", code: "BracketRight", ctrlKey: true }, "zoomIn"],
      // Shifted too: Ctrl++ on US Dvorak, whose = / + key is BracketRight.
      [{ key: "+", code: "BracketRight", ctrlKey: true, shiftKey: true }, "zoomIn"],
    ];
    for (const [chord, call] of chords) {
      const { engine, calls } = recording(["id"]);
      assert.equal(dispatchKeyDown(session(engine), event(chord)), "prevent", JSON.stringify(chord));
      assert.deepEqual(calls, [call], JSON.stringify(chord));
    }
  });

  it("toggles the tool lock on Q", () => {
    const { engine, calls } = recording();
    const lockCalls: boolean[] = [];
    const result = dispatchKeyDown(session(engine, { callbacks: { onToolLockChange: (locked) => lockCalls.push(locked) } }), event({ key: "q" }));
    assert.equal(result, "prevent");
    assert.deepEqual(calls, ["setToolLocked:true"]);
    assert.deepEqual(lockCalls, [true]);
  });

  it("sets spaceHeld on Space so middle-mouse-or-space pan can see it", () => {
    const { engine } = recording();
    const state = session(engine);
    assert.equal(dispatchKeyDown(state, event({ key: " " })), "prevent");
    assert.equal(state.spaceHeld, true);
  });

  it("creates a flowchart node on Ctrl/Cmd+Arrow, in any of the four directions", () => {
    const cases: [string, string][] = [
      ["ArrowRight", "right"],
      ["ArrowLeft", "left"],
      ["ArrowUp", "up"],
      ["ArrowDown", "down"],
    ];
    for (const [key, direction] of cases) {
      for (const chord of [{ ctrlKey: true }, { metaKey: true }]) {
        const { engine, calls } = recording(["node"], false, true);
        const notified: boolean[] = [];
        const result = dispatchKeyDown(
          session(engine, { callbacks: { onFlowchartCreatingChange: (creating) => notified.push(creating) } }),
          event({ key, ...chord }),
        );
        assert.equal(result, "prevent");
        assert.deepEqual(calls, [`flowchartCreate:${direction}`]);
        assert.deepEqual(notified, [true]);
      }
    }
  });

  it("takes Ctrl/Cmd+Arrow even when there is nothing to grow from, and says nothing is pending", () => {
    const { engine, calls } = recording([], false, false);
    const notified: boolean[] = [];
    const result = dispatchKeyDown(
      session(engine, { callbacks: { onFlowchartCreatingChange: (creating) => notified.push(creating) } }),
      event({ key: "ArrowRight", ctrlKey: true }),
    );
    assert.equal(result, "prevent");
    assert.deepEqual(calls, ["flowchartCreate:right"]);
    assert.deepEqual(notified, [false]);
  });

  it("leaves Ctrl/Cmd+Shift+Arrow alone: it is not creation", () => {
    const { engine, calls } = recording(["node"]);
    assert.equal(dispatchKeyDown(session(engine), event({ key: "ArrowRight", ctrlKey: true, shiftKey: true })), "pass");
    assert.deepEqual(calls, []);
  });

  it("navigates the flowchart on Alt+Arrow with one element selected", () => {
    const { engine, calls } = recording(["id"], false, false, "sibling-id");
    assert.equal(dispatchKeyDown(session(engine), event({ key: "ArrowRight", altKey: true })), "prevent");
    assert.deepEqual(calls, ["flowchartNavigate:right"]);

    const nowhere = recording(["id"], false, false, null);
    assert.equal(dispatchKeyDown(session(nowhere.engine), event({ key: "ArrowUp", altKey: true })), "prevent");
    assert.deepEqual(nowhere.calls, ["flowchartNavigate:up"]);
  });

  it("nudges on Alt+Arrow with anything but one element selected, as the oracle does", () => {
    const two = recording(["a", "b"]);
    assert.equal(dispatchKeyDown(session(two.engine), event({ key: "ArrowLeft", altKey: true })), "prevent");
    assert.deepEqual(two.calls, ["nudge:-1,0"]);

    const none = recording([]);
    assert.equal(dispatchKeyDown(session(none.engine), event({ key: "ArrowLeft", altKey: true })), "pass");
    assert.deepEqual(none.calls, []);
  });

  it("picks the pending shape on Ctrl+1/2/3 only while a cluster is being created", () => {
    const idle = recording([], false, false);
    assert.equal(dispatchKeyDown(session(idle.engine), event({ key: "2", ctrlKey: true })), "pass");
    assert.deepEqual(idle.calls, []);

    const creating = recording([], false, true);
    assert.equal(dispatchKeyDown(session(creating.engine), event({ key: "2", ctrlKey: true })), "prevent");
    assert.deepEqual(creating.calls, ["flowchartSetShape:diamond"]);
  });

  it("leaves plain 1/2/3 to the tool shortcuts", () => {
    const { engine, calls } = recording([], false, true);
    const result = dispatchKeyDown(session(engine), event({ key: "2" }));
    assert.equal(result, "prevent");
    assert.deepEqual(calls, ["activateTool:rectangle"]);
  });
});

describe("dispatchKeyUp", () => {
  it("commits the pending flowchart once every modifier that could still be creating is up", () => {
    const { engine, calls } = recording();
    const notified: boolean[] = [];
    dispatchKeyUp(
      session(engine, { callbacks: { onFlowchartCreatingChange: (creating) => notified.push(creating) } }),
      event({ key: "Control" }),
    );
    assert.deepEqual(calls, ["flowchartNavigationEnd", "flowchartCommit"]);
    assert.deepEqual(notified, [false]);
  });

  it("does not commit while Ctrl/Cmd is still held", () => {
    const { engine, calls } = recording();
    dispatchKeyUp(session(engine), event({ key: "Shift", ctrlKey: true }));
    assert.deepEqual(calls, ["flowchartNavigationEnd"]);
  });

  it("ends navigation once Alt is up, independently of the commit", () => {
    const { engine, calls } = recording();
    dispatchKeyUp(session(engine), event({ key: "ArrowRight", altKey: false, ctrlKey: true }));
    assert.deepEqual(calls, ["flowchartNavigationEnd"]);
  });
});

describe("localPoint", () => {
  it("subtracts the canvas screen origin", () => {
    const canvas = { getBoundingClientRect: () => ({ left: 10, top: 20 }) } as HTMLCanvasElement;
    assert.deepEqual(localPoint(canvas, { clientX: 15, clientY: 30 }), { x: 5, y: 10 });
  });
});
