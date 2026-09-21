import type { DrawTool } from "./types";

export const BBOX_SHAPE_TOOLS = new Set<DrawTool>(["rectangle", "diamond", "ellipse"]);
export const LINEAR_TOOLS = new Set<DrawTool>(["line", "arrow"]);

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
  h: "hand",
};

export function toolForKey(key: string): DrawTool | null {
  return HOTKEYS[key.toLowerCase()] ?? null;
}

export function isShapeTool(tool: DrawTool): boolean {
  return tool === "rectangle" || tool === "diamond" || tool === "ellipse";
}

export function isLinearTool(tool: DrawTool): boolean {
  return tool === "line" || tool === "arrow";
}
