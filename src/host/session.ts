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
  /**
   * The tool in hand before a pen's eraser end took over, to go back to on release.
   * Null the rest of the time.
   */
  toolBeforePenEraser: DrawTool | null;
}

export function localPoint(canvas: HTMLCanvasElement, event: { clientX: number; clientY: number }): { x: number; y: number } {
  const rect = canvas.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}
