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
  // duplicates, and one key doing both meant a duplicate could never snap. It also keeps
  // an arrow end from binding.
  session.engine.setCtrlHeld(event.ctrlKey || event.metaKey);
  session.engine.movePointer(x, y, event.shiftKey, event.ctrlKey || event.metaKey);
}

function clearPendingHover(session: HostSession): void {
  if (session.hoverRaf) cancelAnimationFrame(session.hoverRaf);
  session.hoverRaf = 0;
  session.pendingHover = null;
}

/**
 * A move with no button held, coalesced to one engine step per frame like any other. The
 * arrow tool lights the shape an arrow started here would attach to; every other tool
 * returns at once inside the engine.
 */
function queueHover(session: HostSession, event: PointerEvent): void {
  session.pendingHover = event;
  if (session.hoverRaf) return;
  session.hoverRaf = requestAnimationFrame(() => {
    const queued = session.pendingHover;
    session.hoverRaf = 0;
    session.pendingHover = null;
    if (!queued || session.down) return;
    const { x, y } = localPoint(session.canvas, queued);
    session.engine.setAltHeld(queued.altKey);
    session.engine.setCtrlHeld(queued.ctrlKey || queued.metaKey);
    session.engine.hoverPointer(x, y);
  });
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
    clearPendingHover(session);
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
    engine.setCtrlHeld(event.ctrlKey || event.metaKey);
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
    // Tracked unconditionally, like Excalidraw's `updateCurrentCursorPosition`
    // (`App.tsx@1118751f:5477-5482`): a keyboard paste needs the pointer's last position
    // whether or not a button is held or a gesture is in progress.
    session.lastPointer = localPoint(canvas, event);
    if (!wantsMove()) {
      queueHover(session, event);
      return;
    }
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

  const onPointerLeave = () => {
    clearPendingHover(session);
    if (!session.down) engine.endHover();
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
    const onBox = engine.hitsSelectionBox(x, y);
    // A selected element keeps the selection it is in, so the menu can act on all of
    // it — binding a text to a shape takes both (`App.tsx@1118751f:13299-13319`). Nothing
    // under the point but still inside the padded box around a multi-selection is a
    // right-click on that selection too — the frame between shapes opens the element menu,
    // not the board's (`isHittingCommonBoundingBoxOfSelectedElements`,
    // `App.tsx@1118751f:13276-13279`); the selection is left exactly as it was, as the
    // oracle leaves `state` untouched for that case.
    //
    // Nothing under the point at all, inside the box or out, never clears the selection
    // either: a right-button press runs no selection-clearing path in the oracle at all
    // (`openContextMenu`, `App.tsx@1118751f:13296-13326` only ever adds to `state`, never
    // clears it). The menu kind is still the hit's, independent of what stays selected —
    // "element" for the point or the box, "canvas" otherwise (`:13299`,
    // `const type = element || isHittingCommonBoundBox ? "element" : "canvas"`).
    if (hit && !engine.getSelection().includes(hit.id)) engine.select([hit.id]);
    callbacks.onContextMenu?.({ x, y }, hit || onBox ? "element" : "canvas");
  };

  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("pointerdown", onPointerDown);
  canvas.addEventListener("pointermove", onPointerMove);
  canvas.addEventListener("pointerup", onPointerUp);
  canvas.addEventListener("pointerleave", onPointerLeave);
  canvas.addEventListener("dblclick", onDoubleClick);
  canvas.addEventListener("contextmenu", onContext);

  return () => {
    clearPendingMove(session);
    clearPendingHover(session);
    canvas.removeEventListener("wheel", onWheel);
    canvas.removeEventListener("pointerdown", onPointerDown);
    canvas.removeEventListener("pointermove", onPointerMove);
    canvas.removeEventListener("pointerup", onPointerUp);
    canvas.removeEventListener("pointerleave", onPointerLeave);
    canvas.removeEventListener("dblclick", onDoubleClick);
    canvas.removeEventListener("contextmenu", onContext);
  };
}
