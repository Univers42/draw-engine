import type { DrawTool } from "./types";

export const BBOX_SHAPE_TOOLS = new Set<DrawTool>(["rectangle", "diamond", "ellipse"]);
export const LINEAR_TOOLS = new Set<DrawTool>(["line", "arrow"]);

const HOTKEYS: Record<string, DrawTool> = {
  "1": "select",
  v: "select",
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
  "0": "eraser",
  e: "eraser",
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
