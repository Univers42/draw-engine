/**
 * Bind a <canvas> + focusable container to DrawEngine: create the engine, size it,
 * and attach pointer/keyboard. Framework adapters call this once on mount.
 * WASM must already be initialized (`loadDrawEngine()`); use `bindCanvasAsync`.
 */

import { DrawEngine, loadDrawEngine } from "../engine";
import type { DrawTheme, Scene } from "../types";
import { attachKeyboardInput } from "./keyboardInput";
import { attachPointerInput } from "./pointerInput";
import type { HostSession } from "./session";
import type { HostCallbacks } from "./types";

export interface BindCanvasArgs {
  canvas: HTMLCanvasElement;
  container: HTMLElement;
  callbacks: HostCallbacks;
  onReady?: (engine: DrawEngine) => void;
  scene?: Scene;
  theme?: DrawTheme;
  defaultStroke?: string;
}

export interface BindCanvasResult {
  engine: DrawEngine;
  destroy: () => void;
}

export function bindCanvas(args: BindCanvasArgs): BindCanvasResult {
  const { canvas, container, callbacks } = args;
  const engine = new DrawEngine({
    canvas,
    onCameraChange: (camera) => callbacks.onCameraChange?.(camera),
    onToolChange: (tool) => callbacks.onToolChange?.(tool),
    onSelectionChange: (ids) => callbacks.onSelectionChange?.(ids),
    onRequestTextEdit: (request) => callbacks.onRequestTextEdit?.(request),
    onRequestFrameRename: (request) => callbacks.onRequestFrameRename?.(request),
    onSceneChange: (json) => callbacks.onSceneChange?.(json),
    onNotice: (notice) => callbacks.onNotice?.(notice),
  });

  const session: HostSession = {
    engine,
    canvas,
    container,
    callbacks,
    down: false,
    spaceHeld: false,
    moveRaf: 0,
    pendingMove: null,
    hoverRaf: 0,
    pendingHover: null,
    toolBeforePenEraser: null,
  };

  const applySize = () => {
    const rect = container.getBoundingClientRect();
    engine.setViewport(rect.width, rect.height, window.devicePixelRatio || 1);
  };
  applySize();
  const observer = new ResizeObserver(applySize);
  observer.observe(container);

  const detachPointer = attachPointerInput(session);
  const detachKeyboard = attachKeyboardInput(session);

  // `prefers-reduced-motion`: WASM has no `matchMedia`, so the host reads it and keeps the
  // engine current — an eased camera move (fit, zoom to selection, zoom in/out/reset) then
  // lands at once rather than animating. A standing OS setting, not something that changes
  // mid-session for most people, but `change` is cheap to listen for and a jump straight to
  // "off by default, matched live" beats a value read once at load and never revisited.
  const reducedMotionQuery =
    typeof window !== "undefined" ? window.matchMedia("(prefers-reduced-motion: reduce)") : null;
  const applyReducedMotion = () => engine.setReducedMotion(reducedMotionQuery?.matches ?? false);
  applyReducedMotion();
  reducedMotionQuery?.addEventListener("change", applyReducedMotion);

  if (args.scene) engine.setScene(args.scene);
  if (args.theme) engine.setTheme(args.theme);
  if (args.defaultStroke) engine.setNextStyle({ strokeColor: args.defaultStroke });

  (globalThis as unknown as { __osioDrawEngine?: DrawEngine }).__osioDrawEngine = engine;
  args.onReady?.(engine);

  return {
    engine,
    destroy: () => {
      detachPointer();
      detachKeyboard();
      reducedMotionQuery?.removeEventListener("change", applyReducedMotion);
      observer.disconnect();
      engine.destroy();
      const debugHost = globalThis as unknown as { __osioDrawEngine?: DrawEngine };
      if (debugHost.__osioDrawEngine === engine) delete debugHost.__osioDrawEngine;
    },
  };
}

export async function bindCanvasAsync(args: BindCanvasArgs): Promise<BindCanvasResult> {
  await loadDrawEngine();
  return bindCanvas(args);
}
