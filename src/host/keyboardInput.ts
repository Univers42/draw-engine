/**
 * Keyboard + paste wiring. Tool hotkeys, edit chords, and clipboard ride the
 * focused container so the embedding page does not steal them.
 */

import { dispatchKeyDown, dispatchKeyUp } from "./keys";
import type { HostSession } from "./session";

export function attachKeyboardInput(session: HostSession): () => void {
  const { engine, container } = session;

  const onKeyDown = (event: KeyboardEvent) => {
    if (dispatchKeyDown(session, event) === "prevent") event.preventDefault();
  };

  const onKeyUp = (event: KeyboardEvent) => {
    if (event.key === " ") session.spaceHeld = false;
    dispatchKeyUp(session, event);
  };

  const onPaste = (event: ClipboardEvent) => {
    const text = event.clipboardData?.getData("text/plain") ?? "";
    // Centred on the pointer, like the oracle's Ctrl+V (`App.tsx@1118751f:4817-4839`) — not
    // the engine's own offset fallback, which is Ctrl+D's fixed-offset placement and must
    // stay Ctrl+D's alone. Null pointer (paste before the mouse ever entered the canvas)
    // falls back to that offset, same as before.
    const { lastPointer } = session;
    const at = lastPointer ? engine.screenToWorld(lastPointer.x, lastPointer.y) : undefined;
    if (!engine.pasteJson(text || null, at)) engine.pasteJson(null, at);
    event.preventDefault();
  };

  container.addEventListener("keydown", onKeyDown);
  container.addEventListener("keyup", onKeyUp);
  container.addEventListener("paste", onPaste);

  return () => {
    container.removeEventListener("keydown", onKeyDown);
    container.removeEventListener("keyup", onKeyUp);
    container.removeEventListener("paste", onPaste);
  };
}
