/** Public types for @osionos/draw-engine. Values live in the WASM core. */

export interface Camera {
  x: number;
  y: number;
  scale: number;
}

export interface WorldBounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 30;
export const IDENTITY: Camera = { x: 0, y: 0, scale: 1 };

export type DrawElementType =
  | "rectangle"
  | "diamond"
  | "ellipse"
  | "line"
  | "arrow"
  | "freedraw"
  | "text"
  | "image"
  | "frame"
  | "embed";

export type FillStyle = "hachure" | "cross-hatch" | "solid" | "zigzag";
export type StrokeStyle = "solid" | "dashed" | "dotted";
export type Arrowhead = "none" | "arrow" | "triangle" | "dot" | "diamond" | "bar";
export const ARROWHEADS: Arrowhead[] = ["none", "arrow", "triangle", "dot", "diamond", "bar"];

export type DrawTool =
  | "select"
  /** Free-form selection: draw a loop, take what it encloses. */
  | "lasso"
  | "hand"
  | "rectangle"
  | "diamond"
  | "ellipse"
  | "line"
  | "arrow"
  | "freedraw"
  | "text"
  | "eraser"
  /** A trail that follows the cursor and fades. Draws nothing into the scene. */
  | "laser"
  /** A named region that owns whatever is drawn inside it. */
  | "frame";

export type ZOrderMode = "front" | "back" | "forward" | "backward";
export type AlignMode = "left" | "centerX" | "right" | "top" | "centerY" | "bottom";
export type FlipAxis = "horizontal" | "vertical";

export interface DrawElementStyle {
  strokeColor: string;
  backgroundColor: string;
  fillStyle: FillStyle;
  strokeWidth: number;
  strokeStyle: StrokeStyle;
  roughness: number;
  opacity: number;
  roundness: number | null;
}

export const DEFAULT_ELEMENT_STYLE: DrawElementStyle = {
  strokeColor: "#1e1e1e",
  backgroundColor: "transparent",
  fillStyle: "hachure",
  strokeWidth: 2,
  strokeStyle: "solid",
  roughness: 1,
  opacity: 100,
  roundness: 8,
};

export interface DrawElement extends DrawElementStyle {
  id: string;
  type: DrawElementType;
  x: number;
  y: number;
  width: number;
  height: number;
  angle: number;
  seed: number;
  points?: Array<[number, number]>;
  startBinding?: string | null;
  endBinding?: string | null;
  startArrowhead?: Arrowhead;
  endArrowhead?: Arrowhead;
  text?: string;
  fontSize?: number;
  containerId?: string | null;
  boundTextId?: string | null;
  groupId?: string | null;
  locked?: boolean;
  version: number;
  versionNonce: number;
  updated: number;
  isDeleted: boolean;
}

/**
 * The canvas grid: whether it is drawn, how coarse, how often a line is emphasised, and
 * whether gestures land on it.
 *
 * Mirrors the engine's `GridSettings`. Defaults are Excalidraw's — off, 20 units, every
 * 5th line major.
 */
export interface GridSettings {
  enabled: boolean;
  size: number;
  step: number;
  snap: boolean;
}

export const DEFAULT_GRID: GridSettings = {
  enabled: false,
  size: 20,
  step: 5,
  snap: true,
};

export interface DrawTheme {
  background: string;
  grid: string;
  accent: string;
  /**
   * The colour a bindable element's outline is traced with while an arrow endpoint
   * hovers it. Excalidraw's BINDING_HIGHLIGHT_RGB — taken from their source and
   * confirmed by sampling excalidraw.com's interactive canvas, which returned exactly
   * rgb(106, 189, 252).
   *
   * Optional so a theme stored before this field existed still loads; the engine
   * substitutes the light value.
   */
  bindingHighlight?: string;
}

// The accent is Excalidraw's primary, and the same one the chrome uses. The engine
// previously had its own (#4c6ef5), so the selection frame was a different violet from
// the panels drawn around it.
export const LIGHT_THEME: DrawTheme = {
  background: "#ffffff",
  grid: "rgba(17, 17, 17, 0.06)",
  accent: "#6965db",
  bindingHighlight: "rgb(106, 189, 252)",
};
export const DARK_THEME: DrawTheme = {
  background: "#191919",
  grid: "rgba(255, 255, 255, 0.06)",
  accent: "#a8a5ff",
  bindingHighlight: "rgb(104, 182, 240)",
};

export interface TextEditRequest {
  id: string;
  x: number;
  y: number;
  fontSize: number;
  color: string;
  text: string;
  width?: number;
  containerId?: string | null;
}

export interface DrawEngineOptions {
  canvas: HTMLCanvasElement;
  onCameraChange?: (camera: Camera) => void;
  onToolChange?: (tool: DrawTool) => void;
  onSelectionChange?: (ids: string[]) => void;
  onRequestTextEdit?: (request: TextEditRequest) => void;
  onSceneChange?: (json: string) => void;
}

export class Scene {
  constructor(private elements: DrawElement[] = []) {}
  ordered(): DrawElement[] {
    return this.elements.filter((element) => !element.isDeleted);
  }
  toArray(): DrawElement[] {
    return this.elements;
  }
}

export interface OsidrawFile {
  type: "osidraw";
  version: number;
  elements: DrawElement[];
}
