import type { DrawElement, DrawElementStyle, DrawElementType, OsidrawFile } from "./types";
import { DEFAULT_ELEMENT_STYLE } from "./types";

let nonceCounter = 1;

function randInt(): number {
  return Math.floor(Math.random() * 0x7fffffff);
}

export function newElementId(): string {
  return typeof crypto !== "undefined" && crypto.randomUUID ? crypto.randomUUID() : `el-${randInt()}-${nonceCounter++}`;
}

export function createElement(
  type: DrawElementType,
  geometry: { x: number; y: number; width: number; height: number },
  style: Partial<DrawElementStyle> = {},
  now = 0,
): DrawElement {
  return {
    id: newElementId(),
    type,
    x: geometry.x,
    y: geometry.y,
    width: geometry.width,
    height: geometry.height,
    angle: 0,
    ...DEFAULT_ELEMENT_STYLE,
    ...style,
    seed: randInt(),
    version: 1,
    versionNonce: randInt(),
    updated: now,
    isDeleted: false,
  };
}

export function bumpVersion(element: DrawElement, patch: Partial<DrawElement>, now = 0): DrawElement {
  return {
    ...element,
    ...patch,
    version: element.version + 1,
    versionNonce: randInt(),
    updated: now,
  };
}

export function isLinearElement(element: DrawElement): boolean {
  return element.type === "line" || element.type === "arrow";
}

export function isBindableElement(element: DrawElement): boolean {
  return element.type === "rectangle" || element.type === "diamond" || element.type === "ellipse";
}

export function sceneToJson(elements: readonly DrawElement[]): string {
  const file: OsidrawFile = {
    type: "osidraw",
    version: 1,
    elements: elements.filter((element) => !element.isDeleted),
  };
  return JSON.stringify(file, null, 2);
}

export function elementsFromJson(json: string): DrawElement[] | null {
  try {
    const data = JSON.parse(json) as Partial<OsidrawFile>;
    if (data?.type === "osidraw" && Array.isArray(data.elements)) return data.elements as DrawElement[];
  } catch {
    /* not an osidraw document */
  }
  return null;
}
