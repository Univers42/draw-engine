/**
 * Host-facing canvas contract. Framework adapters bind a <canvas>
 * and forward these callbacks; the engine owns drawing and tools.
 */

import type { DrawEngine } from "../engine";
import type {
  Camera,
  DrawNotice,
  DrawTheme,
  DrawTool,
  Scene,
  TextEditRequest,
} from "../types";

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
  /**
   * A text is to be typed: the request opens a session on it (`updateTextEdit`), which
   * the host ends with `commitTextEdit` — or `setElementText` on the same id. Until then
   * undo and redo do nothing. Deleting the text, replacing the scene or a peer taking it
   * ends the session too.
   */
  onRequestTextEdit?: (request: TextEditRequest) => void;
  /** `.osidraw` JSON after every scene mutation — persist it. */
  onSceneChange?: (json: string) => void;
  /** Something the person should be told, as a stable code. The host picks the words. */
  onNotice?: (notice: DrawNotice) => void;
  /** Right-click at a canvas-local point, after the engine has selected whatever
   *  sits under it — the host opens its context menu (e.g. arrowhead choice). `kind` is
   *  "element" when the point or the current selection's own box was hit, "canvas"
   *  otherwise — decided by the hit alone, independent of what stays selected, since a
   *  right-click never clears the selection (`App.tsx@1118751f:13296-13326`). */
  onContextMenu?: (point: { x: number; y: number }, kind: "element" | "canvas") => void;
  /** The keep-tool padlock was toggled (Q) — hosts mirror it in the toolbar. */
  onToolLockChange?: (locked: boolean) => void;
  /**
   * A flowchart cluster still being previewed (Ctrl/Cmd held) may have grown off-screen —
   * the host scrolls it into view itself (a short ease, not a jump). Not fired for
   * Alt+Arrow navigation or for the commit: `flowchart_navigate`/`flowchart_commit` ease
   * the camera in the engine itself (`DrawEngine::reveal`), and a host-driven pan on top
   * of that would fight its animation rather than cooperate with it. No payload: the host
   * already holds the engine and reads `pendingFlowchartElements()` itself.
   */
  onFlowchartReveal?: () => void;
  /** A flowchart cluster started or stopped being previewed (Ctrl/Cmd down vs. released,
   *  or Escape) — hosts use it to show/hide the shape-chooser strip. */
  onFlowchartCreatingChange?: (creating: boolean) => void;
  /** Pointer down on canvas. Return true to intercept and cancel engine pointer handling. */
  onPointerDown?: (point: { x: number; y: number }, event: PointerEvent) => boolean | void;
  /** Pointer move on canvas. */
  onPointerMove?: (point: { x: number; y: number }, event: PointerEvent) => void;
  /** Pointer up on canvas. */
  onPointerUp?: (point: { x: number; y: number }, event: PointerEvent) => void;
}

/** Mutable callback bag so a one-shot bind still sees the latest host handlers. */
export type HostCallbacks = Pick<
  DrawCanvasProps,
  | "onCameraChange"
  | "onToolChange"
  | "onSelectionChange"
  | "onRequestTextEdit"
  | "onSceneChange"
  | "onNotice"
  | "onContextMenu"
  | "onToolLockChange"
  | "onFlowchartReveal"
  | "onFlowchartCreatingChange"
  | "onPointerDown"
  | "onPointerMove"
  | "onPointerUp"
>;
