/**
 * The React host for DrawEngine: mounts one <canvas> and forwards it to
 * bindCanvas after WASM init. Thin adapter — drawing and tool chords live in host/.
 */

import { useEffect, useRef } from "react";
import { bindCanvasAsync } from "../host/bindCanvas";
import type { DrawCanvasProps, HostCallbacks } from "../host/types";
import { LIGHT_THEME } from "../types";
import type { DrawEngine } from "../engine";

export type { DrawCanvasProps } from "../host/types";

export function DrawCanvas({
  scene,
  theme = LIGHT_THEME,
  defaultStroke,
  className,
  ariaLabel = "Drawing canvas",
  onReady,
  onCameraChange,
  onToolChange,
  onSelectionChange,
  onRequestTextEdit,
  onSceneChange,
  onContextMenu,
  onToolLockChange,
  onPointerDown,
  onPointerMove,
  onPointerUp,
}: DrawCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<DrawEngine | null>(null);
  const sceneRef = useRef(scene);
  const themeRef = useRef(theme);
  const strokeRef = useRef(defaultStroke);
  sceneRef.current = scene;
  themeRef.current = theme;
  strokeRef.current = defaultStroke;
  const callbacksRef = useRef<HostCallbacks>({});
  const callbacks = callbacksRef.current;
  callbacks.onCameraChange = onCameraChange;
  callbacks.onToolChange = onToolChange;
  callbacks.onSelectionChange = onSelectionChange;
  callbacks.onRequestTextEdit = onRequestTextEdit;
  callbacks.onSceneChange = onSceneChange;
  callbacks.onContextMenu = onContextMenu;
  callbacks.onToolLockChange = onToolLockChange;
  callbacks.onPointerDown = onPointerDown;
  callbacks.onPointerMove = onPointerMove;
  callbacks.onPointerUp = onPointerUp;

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;
    let cancelled = false;
    let destroy: (() => void) | undefined;
    void bindCanvasAsync({ canvas, container, callbacks, onReady }).then((bound) => {
      if (cancelled) {
        bound.destroy();
        return;
      }
      engineRef.current = bound.engine;
      destroy = bound.destroy;
      if (sceneRef.current) bound.engine.setScene(sceneRef.current);
      bound.engine.setTheme(themeRef.current);
      if (strokeRef.current) bound.engine.setNextStyle({ strokeColor: strokeRef.current });
    });
    return () => {
      cancelled = true;
      destroy?.();
      engineRef.current = null;
    };
    // Engine is created once for the lifetime of the mount; prop syncs live in
    // the effects below.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount-once bind; callbacks bag is mutated in render
  }, []);

  useEffect(() => {
    if (scene) engineRef.current?.setScene(scene);
  }, [scene]);

  useEffect(() => {
    engineRef.current?.setTheme(theme);
  }, [theme]);

  useEffect(() => {
    if (defaultStroke) engineRef.current?.setNextStyle({ strokeColor: defaultStroke });
  }, [defaultStroke]);

  return (
    <div
      ref={containerRef}
      className={className}
      role="application"
      aria-label={ariaLabel}
      tabIndex={0}
      style={{ position: "relative", width: "100%", height: "100%", touchAction: "none", outline: "none" }}
    >
      {/* No tabIndex: `-1` still leaves an element focusable by pointer, so a click
          moved focus off the container onto this `aria-hidden` canvas. See the Svelte
          adapter for the full note. */}
      <canvas ref={canvasRef} aria-hidden="true" style={{ display: "block", width: "100%", height: "100%" }} />
    </div>
  );
}
