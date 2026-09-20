/**
 * Pure keyboard dispatch for a bound canvas. DOM-free so node:test can pin
 * chords without a window. attachKeyboardInput is the thin listener glue.
 */

import { toolForKey } from "../tools";
import type { DrawTool, FlipAxis, ZOrderMode } from "../types";
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
  reorderSelection(mode: ZOrderMode): void;
  zoomIn(): void;
  zoomOut(): void;
  zoomReset(): void;
  copySelection(): string | null;
  cutSelection(): string | null;
  fit(): void;
  editSelectedText(): boolean;
  flipSelection(axis: FlipAxis): void;
  setToolLocked(locked: boolean): void;
  getToolLocked(): boolean;
  setTool(tool: DrawTool): void;
}

export interface KeySession {
  engine: KeyEngine;
  callbacks: Pick<HostCallbacks, "onToolLockChange">;
  spaceHeld: boolean;
}

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
    if (event.shiftKey) engine.ungroupSelection();
    else engine.groupSelection();
    return true;
  }
  if (event.key === "]" || event.key === "[") {
    const toEnd = event.altKey;
    engine.reorderSelection(event.key === "]" ? (toEnd ? "front" : "forward") : toEnd ? "back" : "backward");
    return true;
  }
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
  if (event.key === "Enter") {
    return engine.editSelectedText();
  }
  if (event.shiftKey && engine.getSelection().length > 0) {
    const flip = event.key.toLowerCase();
    if (flip === "h" || flip === "v") {
      engine.flipSelection(flip === "h" ? "horizontal" : "vertical");
      return true;
    }
  }
  if (event.key.toLowerCase() === "q") {
    engine.setToolLocked(!engine.getToolLocked());
    session.callbacks.onToolLockChange?.(engine.getToolLocked());
    return true;
  }
  const tool = toolForKey(event.key);
  if (tool) {
    engine.setTool(tool);
    return true;
  }
  return false;
}

export function dispatchKeyDown(session: KeySession, event: KeyEvent): KeyResult {
  const mod = event.metaKey || event.ctrlKey;
  if (event.key === "Escape") {
    session.engine.cancelPointer();
    return "pass";
  }
  if (mod && handleModChords(session, event, event.key.toLowerCase())) return "prevent";
  if (mod || event.altKey) return "pass";
  if (handlePlainKeys(session, event)) return "prevent";
  return "pass";
}
