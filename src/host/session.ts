/**
 * Shared mutable state for a bound canvas: engine + pointer-gesture bookkeeping.
 */

import type { DrawEngine } from "../engine";
import type { DrawTool } from "../types";
import type { HostCallbacks } from "./types";

export interface HostSession {
  engine: DrawEngine;
  canvas: HTMLCanvasElement;
  container: HTMLElement;
  callbacks: HostCallbacks;
  down: boolean;
  spaceHeld: boolean;
  moveRaf: number;
  pendingMove: PointerEvent | null;
  /** The same coalescing for moves with no button held. */
  hoverRaf: number;
  pendingHover: PointerEvent | null;
  /**
   * The tool in hand before a pen's eraser end took over, to go back to on release.
   * Null the rest of the time.
   */
  toolBeforePenEraser: DrawTool | null;
  /**
   * Where the pointer last was over the canvas, canvas-relative — Excalidraw's
   * `viewport.lastPosition` (`App.tsx@1118751f:5477-5482`), which a keyboard paste centres
   * the pasted selection on (`App.tsx@1118751f:4776-4839`). Null until the pointer has
   * entered the canvas once.
   */
  lastPointer: { x: number; y: number } | null;
}

export function localPoint(canvas: HTMLCanvasElement, event: { clientX: number; clientY: number }): { x: number; y: number } {
  const rect = canvas.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}
