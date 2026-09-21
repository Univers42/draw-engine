/**
 * Pointer, wheel, double-click, and context-menu wiring for a bound canvas.
 * Coalesces pointermove to one engine step per animation frame.
 */

import { localPoint, type HostSession } from "./session";

function processMove(session: HostSession, event: PointerEvent): void {
  const { x, y } = localPoint(session.canvas, event);
  session.engine.movePointer(x, y, event.shiftKey, event.altKey);
}

function clearPendingMove(session: HostSession): void {
  if (session.moveRaf) cancelAnimationFrame(session.moveRaf);
  session.moveRaf = 0;
  session.pendingMove = null;
}

function flushPendingMove(session: HostSession): void {
  const queued = session.pendingMove;
  clearPendingMove(session);
  if (queued) processMove(session, queued);
}

export function attachPointerInput(session: HostSession): () => void {
  const { canvas, engine, callbacks } = session;
  let intercepted = false;

  const onWheel = (event: WheelEvent) => {
    event.preventDefault();
    const { x: sx, y: sy } = localPoint(canvas, event);
    if (event.ctrlKey || event.metaKey) engine.zoomAt(sx, sy, Math.exp(-event.deltaY * 0.01));
    else engine.panBy(-event.deltaX, -event.deltaY);
  };

  const onPointerDown = (event: PointerEvent) => {
    if (event.button === 2) return;
    clearPendingMove(session);
    session.down = true;
    session.container.focus();
    const { x, y } = localPoint(canvas, event);
    canvas.setPointerCapture(event.pointerId);
    if (callbacks.onPointerDown?.({ x, y }, event)) {
      intercepted = true;
      return;
    }
    intercepted = false;
    if (event.button === 1 || session.spaceHeld) {
      event.preventDefault();
      engine.beginPan(x, y);
      return;
    }
    engine.beginPointer(x, y, event.shiftKey, event.altKey);
  };

  const onPointerMove = (event: PointerEvent) => {
    if (!session.down) return;
    const { x, y } = localPoint(canvas, event);
    callbacks.onPointerMove?.({ x, y }, event);
    if (intercepted) return;
    session.pendingMove = event;
    if (session.moveRaf) return;
    session.moveRaf = requestAnimationFrame(() => {
      session.moveRaf = 0;
      const queued = session.pendingMove;
      session.pendingMove = null;
      if (queued && session.down) processMove(session, queued);
    });
  };

  const onPointerUp = (event: PointerEvent) => {
    const { x, y } = localPoint(canvas, event);
    callbacks.onPointerUp?.({ x, y }, event);
    if (intercepted) {
      intercepted = false;
      session.down = false;
      if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
      return;
    }
    flushPendingMove(session);
    session.down = false;
    engine.endPointer();
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
  };

  const onDoubleClick = (event: MouseEvent) => {
    const { x, y } = localPoint(canvas, event);
    engine.handleDoubleClick(x, y);
    event.preventDefault();
  };

  const onContext = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    const { x, y } = localPoint(canvas, event);
    const hit = engine.hitTest(x, y, 4);
    if (hit) engine.select([hit.id]);
    else engine.clearSelection();
    callbacks.onContextMenu?.({ x, y });
  };

  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("pointerdown", onPointerDown);
  canvas.addEventListener("pointermove", onPointerMove);
  canvas.addEventListener("pointerup", onPointerUp);
  canvas.addEventListener("dblclick", onDoubleClick);
  canvas.addEventListener("contextmenu", onContext);

  return () => {
    clearPendingMove(session);
    canvas.removeEventListener("wheel", onWheel);
    canvas.removeEventListener("pointerdown", onPointerDown);
    canvas.removeEventListener("pointermove", onPointerMove);
    canvas.removeEventListener("pointerup", onPointerUp);
    canvas.removeEventListener("dblclick", onDoubleClick);
    canvas.removeEventListener("contextmenu", onContext);
  };
}
