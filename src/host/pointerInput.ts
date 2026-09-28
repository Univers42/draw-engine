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

/** How long after a middle-button pan's release its paste is still dropped (`App.pan.ts@1118751f:183-193`). */
const PASTE_AFTER_PAN_MS = 100;

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

  // Pointers down on the canvas, by id. A platform fact handed to the engine, which is
  // where the reading of it lives: more than one pointer is a multi-finger gesture, and a
  // right-button press inside one is not a pan (`getPointerCount() <= 1`,
  // `App.pan.ts@1118751f:95`).
  const pointersDown = new Set<number>();

  // Linux pastes its primary selection on a middle release, and the one ending a
  // middle-button pan is no exception — every pan pasted the last copied shapes. Once
  // such a pan moves, the next paste is dropped until shortly after the release, as the
  // oracle does (`App.pan.ts@1118751f:157-197`). On the window's capture phase, because
  // the paste listeners here sit on the container and the host's above it.
  const view = canvas.ownerDocument.defaultView ?? window;
  let middleFrom: { x: number; y: number } | null = null;
  let pasteTimer = 0;
  const allowPaste = () => {
    view.clearTimeout(pasteTimer);
    view.removeEventListener("paste", dropPaste, true);
  };
  const dropPaste = (event: Event) => {
    event.stopImmediatePropagation();
    event.preventDefault();
    allowPaste();
  };

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
    pointersDown.add(event.pointerId);
    clearPendingMove(session);
    clearPendingHover(session);
    if (event.button === 2) {
      // A right press is a pan session or a right-click, and the engine says which: it is
      // a pan only once the pointer has travelled past the threshold, so nothing may be
      // claimed for it here or it would have been decided twice.
      // (`App.pan.ts@1118751f:88-125`, and `App.tsx:8698-8700`, where the oracle returns
      // from the pointer down before the press becomes a scene gesture at all.)
      const { x, y } = localPoint(canvas, event);
      const start = engine.beginSecondaryPan(x, y, pointersDown.size);
      if (start === "declined") return;
      // The default is what scrolls the page and takes the focus (#4489) — but not while a
      // text is open, where preventing it breaks the caret. The engine reads that state,
      // so this host does not have a rule of its own about it.
      if (start === "started") event.preventDefault();
      // Capture, as the middle-button pan below does: the moves that decide a pan keep
      // arriving after the pointer has left the canvas.
      canvas.setPointerCapture(event.pointerId);
      return;
    }
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
      if (event.button === 1) middleFrom = { x: event.clientX, y: event.clientY };
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
   * the hundreds of hover moves a minute costs a WASM call each for nothing. The engine
   * names the two exceptions, which are the gestures that follow the cursor with no
   * button of the host's own down: a line or arrow placed point by point, whose segment
   * being aimed only appears if the moves between its clicks get through, and a
   * right-button session, which cannot tell a click from a drag without them.
   */
  const wantsMove = (): boolean => session.down || engine.wantsPointerMoves();

  const onPointerMove = (event: PointerEvent) => {
    // Counted before the gate, so the ratio of events to engine steps is honest about
    // what the device actually sent rather than about what we chose to forward.
    countPointerEvent();
    // Tracked unconditionally, like Excalidraw's `updateCurrentCursorPosition`
    // (`App.tsx@1118751f:5477-5482`): a keyboard paste needs the pointer's last position
    // whether or not a button is held or a gesture is in progress.
    session.lastPointer = localPoint(canvas, event);
    if (
      middleFrom &&
      (Math.abs(event.clientX - middleFrom.x) > 1 || Math.abs(event.clientY - middleFrom.y) > 1)
    ) {
      middleFrom = null;
      allowPaste();
      view.addEventListener("paste", dropPaste, true);
    }
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
    pointersDown.delete(event.pointerId);
    if (event.button === 2) {
      if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
      // The move the frame was still holding is part of this gesture, so it is applied
      // before the session ends. Then the release, which either is a right-click whose
      // menu this host has to open — on macOS and Linux the platform's own event came with
      // the press and was swallowed — or is nothing at all, because the platform's event
      // is still to come (`App.pan.ts@1118751f:247-264`).
      flushPendingMove(session);
      if (engine.endSecondaryPan() === "menu") openContextMenuAt(event);
      return;
    }
    if (event.button === 1) {
      middleFrom = null;
      pasteTimer = view.setTimeout(allowPaste, PASTE_AFTER_PAN_MS);
    }
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

  /**
   * A pointer the platform took away — a touch the browser claims for a gesture of its
   * own. Forgets it, so the count it belongs to cannot stay raised and decline every
   * right press after it.
   */
  const onPointerCancel = (event: PointerEvent) => {
    pointersDown.delete(event.pointerId);
  };

  const onDoubleClick = (event: MouseEvent) => {
    const { x, y } = localPoint(canvas, event);
    engine.handleDoubleClick(x, y);
    event.preventDefault();
  };

  /**
   * The menu for one point, whether the platform's `contextmenu` led here or a
   * right-button release did.
   */
  const openContextMenuAt = (event: MouseEvent | PointerEvent) => {
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
    //
    // What is not selected yet is selected as a press selects it, with its group
    // (`selectGroupsForSelectedElements`, `:13301-13306`) — the element alone left the
    // rest of its group out of the menu's Unlock, Delete and Copy.
    if (hit && !engine.getSelection().includes(hit.id)) engine.selectElement(hit.id);
    callbacks.onContextMenu?.({ x, y }, hit || onBox ? "element" : "canvas");
  };

  const onContext = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    // A right-button session answers for its own menu, and this is the half of it that
    // matters on the platforms where the event comes with the press — macOS and Linux
    // fire `contextmenu` on mousedown, before anything can know whether the gesture is a
    // click or a drag. Opening the menu here would put it under the pointer on the press,
    // and no right-drag to pan would ever happen. The session opens the menu on the
    // release instead (`App.pan.ts@1118751f:74-84`, `App.tsx:13230-13236`).
    //
    // It also covers the other platform: an event following a release that turned out to
    // be a drag is not a click, whatever the platform believes about its own timing.
    if (engine.consumesContextMenu()) return;
    openContextMenuAt(event);
  };

  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("pointerdown", onPointerDown);
  canvas.addEventListener("pointermove", onPointerMove);
  canvas.addEventListener("pointerup", onPointerUp);
  canvas.addEventListener("pointerleave", onPointerLeave);
  canvas.addEventListener("pointercancel", onPointerCancel);
  canvas.addEventListener("dblclick", onDoubleClick);
  canvas.addEventListener("contextmenu", onContext);

  return () => {
    clearPendingMove(session);
    clearPendingHover(session);
    allowPaste();
    pointersDown.clear();
    canvas.removeEventListener("wheel", onWheel);
    canvas.removeEventListener("pointerdown", onPointerDown);
    canvas.removeEventListener("pointermove", onPointerMove);
    canvas.removeEventListener("pointerup", onPointerUp);
    canvas.removeEventListener("pointerleave", onPointerLeave);
    canvas.removeEventListener("pointercancel", onPointerCancel);
    canvas.removeEventListener("dblclick", onDoubleClick);
    canvas.removeEventListener("contextmenu", onContext);
  };
}
