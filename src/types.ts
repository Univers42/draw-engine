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

/** Where a line of text sits across the width of its own box. */
export type TextAlign = "left" | "center" | "right";
export const TEXT_ALIGNS: TextAlign[] = ["left", "center", "right"];

/** Where a label sits down the height of the shape holding it. */
export type VerticalAlign = "top" | "middle" | "bottom";
export const VERTICAL_ALIGNS: VerticalAlign[] = ["top", "middle", "bottom"];

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
  | "frame"
  /** Picks a file and places it. Never a drag: the picker decides, not the pointer. */
  | "image"
  /** Frames a live web page. Like the image tool, it asks rather than draws. */
  | "embed"
  /** Draws freehand and converts the stroke into the shape it was meant to be. */
  | "autoshape"
  /** Fills the region under the pointer. The click is the whole gesture. */
  | "bucketfill";

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
  /**
   * Absent means "nobody has chosen", which is not the same as any of the three values:
   * a text with no alignment falls back to left, a label with none falls back to centre.
   * Writing a value here where none was chosen re-aligns every board saved before the
   * field existed.
   */
  textAlign?: TextAlign;
  verticalAlign?: VerticalAlign;
  /**
   * `false` for a column dragged out with the text tool, which keeps the width it was
   * given and wraps inside it. Absent means auto — every text saved before this field
   * existed sized itself to its glyphs, so only `false` is ever written.
   */
  autoResize?: boolean;
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
  /**
   * Already resolved, so the overlay never has to work out what an unset element means.
   * The overlay must lay the text out the way the canvas will, or it jumps the moment
   * the edit is committed.
   */
  textAlign: TextAlign;
  containerId?: string | null;
}

/**
 * Something the person should be told, as a stable code rather than a sentence.
 *
 * The motor knows *what* happened; the wording, the language and the room it has to fit
 * in belong to whatever is hosting it. A headless host is free to ignore these entirely.
 */
export type DrawNotice = "fill-region-not-closed" | "fill-region-too-complex";

export interface DrawEngineOptions {
  canvas: HTMLCanvasElement;
  onCameraChange?: (camera: Camera) => void;
  onToolChange?: (tool: DrawTool) => void;
  onSelectionChange?: (ids: string[]) => void;
  onRequestTextEdit?: (request: TextEditRequest) => void;
  onSceneChange?: (json: string) => void;
  onNotice?: (notice: DrawNotice) => void;
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

/**
 * A structured view of what the editor is doing, for an inspector or a human.
 *
 * Four sections, each from the only place that can see it: `scene`/`viewport`/
 * `interaction` from the engine, `rendering` from the WASM frame loop, `host` from the
 * browser. Produced by `DrawEngine.debugSnapshot()` and read by
 * `tools/editor-inspector`; DEV builds only, like `window.__drawEngine` itself.
 */
export interface DebugSnapshot {
  scene: {
    /** Live elements. Tombstones are counted separately, not here. */
    elementCount: number;
    /** Tombstones. They stay in the store because a deletion has to be *sent*. */
    deletedCount: number;
    selectedCount: number;
    /** Sorted, so two snapshots of one selection compare equal. */
    selectedIds: string[];
    /** Moves when the scene does — the nearest analogue to a state-update count. */
    revision: number;
    canUndo: boolean;
    canRedo: boolean;
  };
  viewport: {
    x: number;
    y: number;
    /** Camera scale; 1 is 100%. */
    zoom: number;
    /** CSS pixels. `rendering.canvasWidth` over this is the dpr in force. */
    width: number;
    height: number;
    dpr: number;
    /** `[minX, minY, maxX, maxY]` of the world on screen — what culling keeps. */
    visibleWorld: [number, number, number, number];
  };
  interaction: {
    tool: string;
    toolLocked: boolean;
    /** The gesture in progress, by name, or null between gestures. */
    kind: string | null;
    dragging: boolean;
    resizing: boolean;
    placingLinear: string | null;
    editingLinear: string | null;
  };
  rendering: {
    frames: number;
    /** Wall time between frames: display cadence plus everything else on the page. */
    medianFrameMs: number;
    p95FrameMs: number;
    /** Our own CPU, kept apart from the interval above. */
    medianBuildMs: number;
    medianPaintMs: number;
    p95PaintMs: number;
    lastBuildMs: number;
    lastPaintMs: number;
    /** After culling. Against `scene.elementCount`, this says whether culling works. */
    elementsRendered: number;
    /** Rough geometry reused vs regenerated. Misses during a pan mean trouble. */
    shapeCacheHits: number;
    shapeCacheMisses: number;
    shapeCacheLen: number;
    /** Device pixels. */
    canvasWidth: number;
    canvasHeight: number;
    /** Whether a frame is owed. There are no dirty *regions*; the canvas repaints whole. */
    dirty: boolean;
  };
  host: {
    /** Raw pointermove events seen. */
    pointerEvents: number;
    /** Moves forwarded after per-frame coalescing. The ratio is the coalescing. */
    engineSteps: number;
    hitTests: number;
    hitTestMs: number;
    maxHitTestMs: number;
  };
}
