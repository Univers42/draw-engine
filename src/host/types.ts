/**
 * Host-facing canvas contract. Framework adapters bind a <canvas>
 * and forward these callbacks; the engine owns drawing and tools.
 */

import type { DrawEngine } from "../engine";
import type { Camera, DrawTheme, DrawTool, Scene, TextEditRequest } from "../types";

export interface DrawCanvasProps {
  scene?: Scene;
  theme?: DrawTheme;
  /** Stroke color new shapes adopt (the shell resolves a theme token so ink is
   *  visible in both light and dark). */
  defaultStroke?: string;
  className?: string;
  ariaLabel?: string;
  onReady?: (engine: DrawEngine) => void;
  onCameraChange?: (camera: Camera) => void;
  onToolChange?: (tool: DrawTool) => void;
  onSelectionChange?: (ids: string[]) => void;
  onRequestTextEdit?: (request: TextEditRequest) => void;
  /** `.osidraw` JSON after every scene mutation — persist it. */
  onSceneChange?: (json: string) => void;
  /** Right-click at a canvas-local point, after the engine has selected whatever
   *  sits under it — the host opens its context menu (e.g. arrowhead choice). */
  onContextMenu?: (point: { x: number; y: number }) => void;
  /** The keep-tool padlock was toggled (Q) — hosts mirror it in the toolbar. */
  onToolLockChange?: (locked: boolean) => void;
}

/** Mutable callback bag so a one-shot bind still sees the latest host handlers. */
export type HostCallbacks = Pick<
  DrawCanvasProps,
  | "onCameraChange"
  | "onToolChange"
  | "onSelectionChange"
  | "onRequestTextEdit"
  | "onSceneChange"
  | "onContextMenu"
  | "onToolLockChange"
>;
