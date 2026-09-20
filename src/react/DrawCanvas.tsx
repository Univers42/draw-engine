/**
 * The React host for DrawEngine: mounts one <canvas> and forwards it to
 * bindCanvas. Thin adapter — drawing and tool chords live in host/ + core/.
 */

import { useEffect, useRef } from "react";
import { bindCanvas } from "../host/bindCanvas";
import type { DrawCanvasProps, HostCallbacks } from "../host/types";
import { LIGHT_THEME } from "../core/render/paint";
import type { DrawEngine } from "../core/engine";

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
}: DrawCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<DrawEngine | null>(null);
  const callbacksRef = useRef<HostCallbacks>({});
  const callbacks = callbacksRef.current;
  callbacks.onCameraChange = onCameraChange;
  callbacks.onToolChange = onToolChange;
  callbacks.onSelectionChange = onSelectionChange;
  callbacks.onRequestTextEdit = onRequestTextEdit;
  callbacks.onSceneChange = onSceneChange;
  callbacks.onContextMenu = onContextMenu;
  callbacks.onToolLockChange = onToolLockChange;

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;
    const bound = bindCanvas({ canvas, container, callbacks, onReady });
    engineRef.current = bound.engine;
    return () => {
      bound.destroy();
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
      <canvas ref={canvasRef} aria-hidden="true" tabIndex={-1} style={{ display: "block", width: "100%", height: "100%" }} />
    </div>
  );
}
