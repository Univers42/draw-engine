/**
 * Pure keyboard dispatch for a bound canvas. DOM-free so node:test can pin
 * chords without a window. attachKeyboardInput is the thin listener glue.
 */

import { toolForChord } from "../tools";
import type { DrawTool, FlipAxis, FlowchartDirection, FlowchartShape, ZOrderMode } from "../types";
import type { HostCallbacks } from "./types";

export type KeyResult = "prevent" | "pass";

export interface KeyEvent {
  key: string;
  code: string;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  repeat: boolean;
}

export interface KeyEngine {
  cancelPointer(): void;
  deleteSelection(): void;
  getSelection(): string[];
  nudgeSelection(dx: number, dy: number): void;
  redo(): void;
  undo(): void;
  selectAll(): void;
  duplicateSelection(): void;
  ungroupSelection(): void;
  groupSelection(): void;
  toggleGroupSelection(): void;
  toggleLockSelection(): void;
  reorderSelection(mode: ZOrderMode): void;
  zoomIn(): void;
  zoomOut(): void;
  zoomReset(): void;
  copySelection(): string | null;
  cutSelection(): string | null;
  fit(): void;
  zoomToSelection(): void;
  pageBy(pagesX: number, pagesY: number): void;
  editSelectedText(): boolean;
  flipSelection(axis: FlipAxis): void;
  setToolLocked(locked: boolean): void;
  getToolLocked(): boolean;
  setTool(tool: DrawTool): void;
  activateTool(tool: DrawTool): void;
  linearInProgress(): boolean;
  finishLinear(): void;
  flowchartCreate(direction: FlowchartDirection): void;
  flowchartSetShape(shape: FlowchartShape): void;
  flowchartCommit(): void;
  flowchartCancel(): void;
  isCreatingFlowchart(): boolean;
  flowchartNavigate(direction: FlowchartDirection): string | null;
  flowchartNavigationEnd(): void;
}

export interface KeySession {
  engine: KeyEngine;
  callbacks: Pick<HostCallbacks, "onToolLockChange" | "onFlowchartCreatingChange">;
  spaceHeld: boolean;
}

const BRACKET_KEYS: Partial<Record<string, "]" | "[">> = { BracketRight: "]", BracketLeft: "[" };

const FLOWCHART_ARROWS: Partial<Record<string, FlowchartDirection>> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
};

// While Ctrl/Cmd is held for creation: chooses the pending nodes' shape (an extra the
// oracle does not have). Plain 1/2/3 already select tools (`tools.ts`), so this only
// fires with the modifier down, and only mid-creation — it never steals the plain chord.
const FLOWCHART_SHAPE_KEYS: Partial<Record<string, FlowchartShape>> = {
  "1": "rectangle",
  "2": "diamond",
  "3": "ellipse",
};

function handleModChords(session: KeySession, event: KeyEvent, key: string): boolean {
  const { engine } = session;
  if (key === "z") {
    if (event.shiftKey) engine.redo();
    else engine.undo();
    return true;
  }
  if (key === "y") {
    engine.redo();
    return true;
  }
  if (key === "a") {
    engine.selectAll();
    return true;
  }
  if (key === "d") {
    engine.duplicateSelection();
    return true;
  }
  if (key === "g") {
    // Ctrl+G toggles rather than only grouping. Excalidraw's is a no-op on a selection
    // that is already exactly one group — observed, not assumed — which leaves the key
    // with no inverse and no way out of a group using the key you reached for. Ctrl+
    // Shift+G is left alone and still matches the oracle exactly.
    if (event.shiftKey) engine.ungroupSelection();
    else engine.toggleGroupSelection();
    return true;
  }
  // Lock and unlock, as the context menu's toggle, with something selected
  // (`actionToggleElementLock`'s `keyTest`, `actionElementLock.ts@1118751f:151-160`).
  if (key === "l" && event.shiftKey && engine.getSelection().length > 0) {
    engine.toggleLockSelection();
    return true;
  }
  // Zoom by the printed key, and before the brackets' physical-key fallback below: on a
  // layout whose + sits on a bracket key (Dvorak, QWERTZ), Ctrl++ is still a zoom.
  if (event.key === "=" || event.key === "+") {
    engine.zoomIn();
    return true;
  }
  if (event.key === "-" || event.key === "_") {
    engine.zoomOut();
    return true;
  }
  if (event.key === "0") {
    engine.zoomReset();
    return true;
  }
  // Excalidraw sends to the end with Shift on Windows and Linux, with Alt on macOS
  // (`actionZindex.tsx`); both work here on every platform. Either modifier changes what
  // the key prints, so held, the physical key decides — as the oracle's `event.code` does.
  const bracket =
    event.key === "]" || event.key === "["
      ? event.key
      : event.shiftKey || event.altKey
        ? BRACKET_KEYS[event.code]
        : undefined;
  if (bracket) {
    const toEnd = event.shiftKey || event.altKey;
    engine.reorderSelection(bracket === "]" ? (toEnd ? "front" : "forward") : toEnd ? "back" : "backward");
    return true;
  }
  if (key === "c") {
    const json = engine.copySelection();
    if (json) navigator.clipboard?.writeText(json).catch(() => undefined);
    return true;
  }
  if (key === "x") {
    const json = engine.cutSelection();
    if (json) navigator.clipboard?.writeText(json).catch(() => undefined);
    return true;
  }
  return false;
}

function handlePlainKeys(session: KeySession, event: KeyEvent): boolean {
  const { engine } = session;
  if (event.key === "Delete" || event.key === "Backspace") {
    engine.deleteSelection();
    return true;
  }
  if (event.key.startsWith("Arrow") && !(event.metaKey || event.ctrlKey)) {
    if (engine.getSelection().length === 0) return false;
    const step = event.shiftKey ? 10 : 1;
    const dx = event.key === "ArrowLeft" ? -step : event.key === "ArrowRight" ? step : 0;
    const dy = event.key === "ArrowUp" ? -step : event.key === "ArrowDown" ? step : 0;
    engine.nudgeSelection(dx, dy);
    return true;
  }
  if (event.key === " " && !(event.metaKey || event.ctrlKey)) {
    if (!event.repeat) session.spaceHeld = true;
    return true;
  }
  if (event.shiftKey && !(event.metaKey || event.ctrlKey) && event.code === "Digit1") {
    engine.fit();
    return true;
  }
  if (event.shiftKey && !(event.metaKey || event.ctrlKey) && event.code === "Digit2") {
    // Frame the selection rather than the board. `fit` cannot stand in for it: a fit has
    // to hold everything, so the shape you are working on ends up as small as the
    // furthest stray one allows.
    engine.zoomToSelection();
    return true;
  }
  if (event.key === "PageUp" || event.key === "PageDown") {
    // Shift pages sideways, matching the wheel: shift turns vertical scrolling into
    // horizontal everywhere else on the board, so it would be odd here alone.
    const forward = event.key === "PageDown" ? 1 : -1;
    if (event.shiftKey) engine.pageBy(forward, 0);
    else engine.pageBy(0, forward);
    return true;
  }
  if (event.key === "Enter") {
    return engine.editSelectedText();
  }
  // By the physical key, as Excalidraw matches them (`actionFlip.ts:51`, `:76-77`), so
  // the chord is where it is on every layout. Ctrl+Shift+V never gets here: a modifier
  // chord is handled or passed on before the plain keys.
  const flip = event.code === "KeyH" ? "horizontal" : event.code === "KeyV" ? "vertical" : null;
  if (flip && event.shiftKey && engine.getSelection().length > 0) {
    engine.flipSelection(flip);
    return true;
  }
  if (event.key.toLowerCase() === "q") {
    engine.setToolLocked(!engine.getToolLocked());
    session.callbacks.onToolLockChange?.(engine.getToolLocked());
    return true;
  }
  // Shift is part of the chord for exactly one tool — Shift+X is autoshape where X is
  // freedraw — and `activateTool` rather than `setTool` because the hand and the eraser
  // go back to the tool they interrupted when their key is pressed a second time.
  const tool = toolForChord(event.key, event.shiftKey);
  if (tool) {
    engine.activateTool(tool);
    return true;
  }
  return false;
}

export function dispatchKeyDown(session: KeySession, event: KeyEvent): KeyResult {
  const { engine } = session;
  const mod = event.metaKey || event.ctrlKey;
  if (event.key === "Escape") {
    // A cluster being previewed is dropped and the key goes no further — the oracle's
    // flowchart handler returns ahead of its other Escape handling
    // (`App.tsx@1118751f:5574`), so the selection it grew from stays selected.
    if (engine.isCreatingFlowchart()) {
      engine.flowchartCancel();
      session.callbacks.onFlowchartCreatingChange?.(false);
      return "prevent";
    }
    engine.cancelPointer();
    return "pass";
  }
  // Enter ends a path being placed, the way Excalidraw's does — both keys run their
  // `actionFinalize`. Guarded on there actually being one, so Enter keeps meaning
  // nothing here the rest of the time rather than becoming a key that swallows itself.
  if (event.key === "Enter" && session.engine.linearInProgress()) {
    session.engine.finishLinear();
    return "prevent";
  }
  const flowchartDirection = FLOWCHART_ARROWS[event.key];
  if (flowchartDirection) {
    // Ctrl/Cmd+Arrow grows the flowchart; Alt+Arrow walks it — the oracle's
    // `App.flowchart.ts@1118751f:106-150`, checked ahead of the other chords because it
    // must also fire held-and-repeated. Both reveal what they reach in the engine.
    if (mod && !event.shiftKey) {
      // Taken even with nothing to grow from, as the oracle's is: Ctrl+Arrow never nudges.
      engine.flowchartCreate(flowchartDirection);
      session.callbacks.onFlowchartCreatingChange?.(engine.isCreatingFlowchart());
      return "prevent";
    }
    if (event.altKey && engine.getSelection().length === 1) {
      engine.flowchartNavigate(flowchartDirection);
      return "prevent";
    }
    // With anything but one element selected, Alt+Arrow is a plain nudge in the oracle.
    if (event.altKey && !mod) return handlePlainKeys(session, event) ? "prevent" : "pass";
  }
  const flowchartShape = FLOWCHART_SHAPE_KEYS[event.key];
  if (mod && flowchartShape && engine.isCreatingFlowchart()) {
    engine.flowchartSetShape(flowchartShape);
    return "prevent";
  }
  if (mod && handleModChords(session, event, event.key.toLowerCase())) return "prevent";
  if (mod || event.altKey) return "pass";
  if (handlePlainKeys(session, event)) return "prevent";
  return "pass";
}

/**
 * Keyup has exactly one job today: finalizing the flowchart gesture by looking at which
 * modifiers are *still* down, not which key was released — the oracle's own approach
 * (`App.flowchart.ts@1118751f:handleKeyEvent`, the keyup half), so a fast Ctrl-then-Arrow-
 * release ordering still commits. Both calls are no-ops when nothing is pending/exploring.
 */
export function dispatchKeyUp(session: KeySession, event: KeyEvent): void {
  const { engine } = session;
  if (!event.altKey) engine.flowchartNavigationEnd();
  if (!(event.metaKey || event.ctrlKey)) {
    engine.flowchartCommit();
    // Unconditional, like the commit call above: a no-op commit leaves isCreatingFlowchart
    // already false, so telling the host "not creating" again is harmless.
    session.callbacks.onFlowchartCreatingChange?.(false);
  }
}
