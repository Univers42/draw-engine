/**
 * Bind a <canvas> + focusable container to DrawEngine: create the engine, size it,
 * and attach pointer/keyboard. Framework adapters call this once on mount.
 */

import { DrawEngine } from "../core/engine";
import type { DrawTheme } from "../core/render/paint";
import type { Scene } from "../core/scene/scene";
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
    onSceneChange: (json) => callbacks.onSceneChange?.(json),
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
      observer.disconnect();
      engine.destroy();
      const debugHost = globalThis as unknown as { __osioDrawEngine?: DrawEngine };
      if (debugHost.__osioDrawEngine === engine) delete debugHost.__osioDrawEngine;
    },
  };
}
