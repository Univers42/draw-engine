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
    if (!engine.pasteJson(text || null)) engine.pasteJson(null);
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
