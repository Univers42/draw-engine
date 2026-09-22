import type { DrawTool } from "./types";

export const BBOX_SHAPE_TOOLS = new Set<DrawTool>(["rectangle", "diamond", "ellipse"]);
export const LINEAR_TOOLS = new Set<DrawTool>(["line", "arrow"]);

/**
 * Excalidraw's tool shortcuts, keyed by chord.
 *
 * A second copy of the engine's `tool_for_chord`, because the host dispatches keys before
 * the engine sees them. Two copies of a mapping drift, and this one already had — so
 * `engine/crates/draw-engine/tests/ci_shortcuts.rs` reads this file back as text and
 * holds it to the Rust table. Keep the literal shape below: that test matches on it.
 *
 * `shift+x` is autoshape while `x` is freedraw, which is why the key of this table is a
 * chord rather than a key.
 */
const HOTKEYS: Record<string, DrawTool> = {
  "1": "select",
  v: "select",
  // Excalidraw exposes the lasso as a *mode* of the selection tool, sharing its
  // shortcut, with Alt+Ctrl toggling it mid-drag. Ours is its own toolbar entry, so it
  // needs a key of its own; "l" is already the line.
  s: "lasso",
  "2": "rectangle",
  r: "rectangle",
  "3": "diamond",
  d: "diamond",
  "4": "ellipse",
  o: "ellipse",
  "5": "arrow",
  a: "arrow",
  "6": "line",
  l: "line",
  "7": "freedraw",
  p: "freedraw",
  // Freedraw is the only tool with two letters, and X is the one people arriving from
  // other editors reach for.
  x: "freedraw",
  "8": "text",
  t: "text",
  // Excalidraw binds the eraser to both E and 0; the toolbar badge has always shown 0,
  // so without the digit the badge promised a key that did nothing.
  "0": "eraser",
  e: "eraser",
  // Excalidraw's laser key, and the same reasoning as the lasso: "l" is the line.
  k: "laser",
  f: "frame",
  "9": "image",
  w: "embed",
  // The one chord in the table: the shifted freedraw key.
  "shift+x": "autoshape",
  // Excalidraw's own key for the bucket.
  b: "bucketfill",
  h: "hand",
};

/** Tools that switch back to the previous tool when their own key is pressed again. */
const TOGGLE_TOOLS = new Set<DrawTool>(["hand", "eraser"]);

export function isToggleTool(tool: DrawTool): boolean {
  return TOGGLE_TOOLS.has(tool);
}

/**
 * The tool a key press selects.
 *
 * `shift` matters for exactly one chord and is inert everywhere else: Excalidraw dropped
 * Shift+letter as a separate binding and made tool keys case-insensitive.
 */
export function toolForChord(key: string, shift: boolean): DrawTool | null {
  const lower = key.toLowerCase();
  if (shift) {
    const chord = HOTKEYS[`shift+${lower}`];
    if (chord) return chord;
  }
  return HOTKEYS[lower] ?? null;
}

export function toolForKey(key: string): DrawTool | null {
  return toolForChord(key, false);
}

export function isShapeTool(tool: DrawTool): boolean {
  return tool === "rectangle" || tool === "diamond" || tool === "ellipse";
}

export function isLinearTool(tool: DrawTool): boolean {
  return tool === "line" || tool === "arrow";
}
