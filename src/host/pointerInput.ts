/**
 * Pointer, wheel, double-click, and context-menu wiring for a bound canvas.
 * Coalesces pointermove to one engine step per animation frame.
 */

import { countEngineStep, countPointerEvent } from "./probe";
import { localPoint, type HostSession } from "./session";
import { wheelIntent } from "./wheel";

/**
 * `PointerEvent.button` for a pen's eraser end. Excalidraw's `POINTER_BUTTON.ERASER`.
 */
const PEN_ERASER_BUTTON = 5;

function processMove(session: HostSession, event: PointerEvent): void {
  const { x, y } = localPoint(session.canvas, event);
  countEngineStep();
  // Alt first: the eraser reads it to un-mark what it passes back over.
  session.engine.setAltHeld(event.altKey);
  // Ctrl/Cmd inverts object snapping for the move, as in Excalidraw. Not Alt: Alt+drag
  // duplicates, and one key doing both meant a duplicate could never snap.
  session.engine.movePointer(x, y, event.shiftKey, event.ctrlKey || event.metaKey);
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
    const intent = wheelIntent(event);
    if (intent.kind === "none") return;
    if (intent.kind === "pan") {
      engine.panBy(intent.dx, intent.dy);
      return;
    }
    // Applied per event rather than coalesced per frame: the engine derives each step
    // from the scale the one before it produced, so a burst of ticks walks the zoom.
    // Summing them into one frame's delta would instead collapse the burst into a single
    // clamped step, and a trackpad fling would barely move the board.
    const { x: sx, y: sy } = localPoint(canvas, event);
    engine.wheelZoom(sx, sy, intent.deltaY);
  };

  const onPointerDown = (event: PointerEvent) => {
    if (event.button === 2) return;
    clearPendingMove(session);
    session.down = true;
    session.container.focus();
    const { x, y } = localPoint(canvas, event);
    canvas.setPointerCapture(event.pointerId);
    // Turning the pen round erases, whatever tool is in hand, and releasing puts that
    // tool back — Excalidraw's handling of the eraser button (`App.tsx:8700-8730`).
    // Switched before the host is told of the press, so the host sees the eraser too.
    if (event.button === PEN_ERASER_BUTTON && engine.getTool() !== "eraser") {
      session.toolBeforePenEraser = engine.getTool();
      engine.setTool("eraser");
    }
    // A pan is decided before the host sees the press. It is never a host gesture, and
    // asking the host first let one start: the eraser drew its trail across a
    // space-drag, and the sticky-note tool claimed the press instead of panning.
    if (event.button === 1 || session.spaceHeld) {
      intercepted = false;
      event.preventDefault();
      engine.beginPan(x, y);
      return;
    }
    if (callbacks.onPointerDown?.({ x, y }, event)) {
      intercepted = true;
      return;
    }
    intercepted = false;
    engine.beginPointer(x, y, event.shiftKey, event.altKey);
  };

  /**
   * Whether the engine wants this move.
   *
   * Normally only while a button is held — every other gesture is a drag, and forwarding
   * the hundreds of hover moves a minute costs a WASM call each for nothing. The
   * exception is a line or arrow being placed point by point: it follows the cursor
   * *between* its clicks, so the segment being aimed only appears if the moves with no
   * button held get through. It is the one gesture in the engine that works that way.
   */
  const wantsMove = (): boolean => session.down || engine.linearInProgress();

  const onPointerMove = (event: PointerEvent) => {
    // Counted before the gate, so the ratio of events to engine steps is honest about
    // what the device actually sent rather than about what we chose to forward.
    countPointerEvent();
    if (!wantsMove()) return;
    const { x, y } = localPoint(canvas, event);
    // Still only while a button is held. Hosts read this callback as "a drag is
    // happening", and firing it on hover would make every one of them wrong.
    if (session.down) callbacks.onPointerMove?.({ x, y }, event);
    if (intercepted) return;
    session.pendingMove = event;
    if (session.moveRaf) return;
    session.moveRaf = requestAnimationFrame(() => {
      session.moveRaf = 0;
      const queued = session.pendingMove;
      session.pendingMove = null;
      if (queued && wantsMove()) processMove(session, queued);
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
    if (session.toolBeforePenEraser !== null) {
      engine.setTool(session.toolBeforePenEraser);
      session.toolBeforePenEraser = null;
    }
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
