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
  | "embed"
  /** A note: a filled pad with a shadow, its creation date, and a label it fits. */
  | "stickynote"
  /** A parametric shape — a polygon, a star, a parallelogram, a trapezoid, a cylinder or
   *  a document — generated from [`FigureParams`]. See `docs/reference/figure.md`. */
  | "figure";

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
  | "bucketfill"
  /** Places a sticky note — a click, or a drag for its size — and opens its label. */
  | "stickynote"
  /** Draws a figure — the Shapes picker's kind, sides and ratio (`nextFigure`). */
  | "figure";

export type ZOrderMode = "front" | "back" | "forward" | "backward";
export type AlignMode = "left" | "centerX" | "right" | "top" | "centerY" | "bottom";
export type FlipAxis = "horizontal" | "vertical";

/** Which way Ctrl/Cmd+Arrow grows the flowchart, or Alt+Arrow walks it. */
export type FlowchartDirection = "up" | "down" | "left" | "right";

/** The three shapes 1/2/3 chooses while a flowchart cluster is being created. */
export type FlowchartShape = "rectangle" | "diamond" | "ellipse";

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

/**
 * What `applyStyle`/`previewStyle`/`setNextStyle` take: the 8 `DrawElementStyle` fields,
 * plus a font — family, size, horizontal alignment — the engine's `DrawElementStylePatch`
 * also carries. The font fields restyle every selected text and the label of every
 * selected shape as one step with the rest of the patch, and fall back to the next text's
 * style with nothing selected; see `engine/selection_style.rs`.
 */
export interface StylePatch extends Partial<DrawElementStyle> {
  fontFamily?: number;
  fontSize?: number;
  textAlign?: TextAlign;
  /** A figure's own two controls — the inspector's sides stepper and ratio slider.
   *  Ignored on anything that is not a figure, and on a kind with no such control. */
  figureSides?: number;
  figureRatio?: number;
}

export interface DrawElement extends DrawElementStyle {
  /**
   * An explicit corner radius, set by dragging a corner-radius handle. Absent means the
   * adaptive corner every rounded shape has always had — and absent is what every board
   * saved before this carries, so none of them change. Only applies while `roundness` is
   * set: Sharp wins, and the radius is remembered for when it is Round again.
   */
  cornerRadius?: number;
  id: string;
  type: DrawElementType;
  x: number;
  y: number;
  width: number;
  height: number;
  angle: number;
  seed: number;
  points?: Array<[number, number]>;
  /**
   * Only a line's: closed on its first point and painted as a filled shape rather than
   * an open stroke (Excalidraw's `polygon`). Absent — every line saved before this field
   * existed — reads as `false`, the open path it always was.
   */
  polygon?: boolean;
  startBinding?: string | null;
  endBinding?: string | null;
  /**
   * Where on its shape each bound end is anchored, as a ratio of the shape's unrotated
   * width and height, and whether the end sits there (`inside`) or stops a gap clear of
   * the outline (`orbit`). Absent on an arrow bound before anchors existed: the centre,
   * in orbit.
   */
  startFixedPoint?: [number, number];
  endFixedPoint?: [number, number];
  startBindMode?: "inside" | "orbit";
  endBindMode?: "inside" | "orbit";
  startArrowhead?: Arrowhead;
  endArrowhead?: Arrowhead;
  /** What is drawn: the source with its soft line breaks baked in. */
  text?: string;
  /**
   * The text as it was typed, before wrapping (Excalidraw's `originalText`). Absent on
   * every text saved before it existed, where `text` is the source.
   */
  originalText?: string;
  fontSize?: number;
  /**
   * Excalidraw's numeric font family id (1 Virgil, 2 Helvetica, 3 Cascadia, 5 Excalifont,
   * 6 Nunito, 7 Lilita One, 8 Comic Shanns, 9 Liberation Sans). Absent is the system
   * stack every text saved before families existed was drawn with; an id the engine does
   * not know is kept, and drawn with that stack. One outside 1..=64 is dropped as it
   * comes in, as a `lineHeight` outside 0.5..=4 is: the server refuses both.
   */
  fontFamily?: number;
  /** Unitless, a multiple of the font size. Absent is the family's own. */
  lineHeight?: number;
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
  /**
   * Only a label's: whether it wraps inside its shape (absent or `true`) or keeps its
   * hard lines while the shape grows wide enough for them (`false`).
   */
  wrap?: boolean;
  containerId?: string | null;
  boundTextId?: string | null;
  /**
   * The groups this element is in, **innermost first**.
   *
   * The array *is* the nesting — there is no group entity and no parent pointer, just
   * ids that several elements share, so an element two levels down carries both. See
   * `docs/reference/groups.md`.
   */
  groupIds?: string[];
  locked?: boolean;
  /**
   * A sticky note's: the height it was given, which it grows above to fit its label and
   * never shrinks below. Absent on a note from before the field: its height is its base.
   */
  baseHeight?: number;
  /**
   * When a sticky note was drawn, in epoch ms: the date its footer shows. Absent or
   * `null` when unknown, and then the footer is not drawn.
   */
  created?: number | null;
  /**
   * A sticky note label's: the size it was given, the ceiling its fit shrinks below.
   * Absent means `fontSize` is the ceiling. Meaningless on a text no note holds.
   */
  baseFontSize?: number | null;
  /**
   * A figure's parameters — its kind, and the sides/ratio that shape it. Absent on every
   * element that is not a figure. Mirrors the engine's `FigureParams` (`scene/figure.rs`).
   */
  figure?: FigureParams;
  version: number;
  versionNonce: number;
  updated: number;
  isDeleted: boolean;
}

/** A figure's shape. Mirrors the engine's `FigureKind` (`scene/figure.rs`). */
export type FigureKind =
  | "polygon"
  | "star"
  | "parallelogram"
  | "trapezoid"
  | "cylinder"
  | "document";

/**
 * A figure's own two controls — the inspector's sides stepper and ratio slider.
 * `sides`/`ratio` absent falls back to `kind`'s own default (`resolvedSides`/
 * `resolvedRatio` on the Rust side); a kind with no such control ignores the one it
 * has none of.
 */
export interface FigureParams {
  kind: FigureKind;
  sides?: number;
  ratio?: number;
}

/** A hexagon — a shape with a visible `sides` and no `ratio`. Mirrors the engine's
 *  `FigureParams::default()`. */
export const DEFAULT_FIGURE_PARAMS: FigureParams = { kind: "polygon", sides: 6 };

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
  /** A side midpoint an arrow end is near but would not snap to yet. Optional as above. */
  bindingMidpoint?: string;
}

// The accent is Excalidraw's primary, and the same one the chrome uses. The engine
// previously had its own (#4c6ef5), so the selection frame was a different violet from
// the panels drawn around it.
export const LIGHT_THEME: DrawTheme = {
  background: "#ffffff",
  grid: "rgba(17, 17, 17, 0.06)",
  accent: "#6965db",
  bindingHighlight: "rgb(106, 189, 252)",
  bindingMidpoint: "rgba(65, 65, 65, 0.5)",
};
export const DARK_THEME: DrawTheme = {
  background: "#191919",
  grid: "rgba(255, 255, 255, 0.06)",
  accent: "#a8a5ff",
  bindingHighlight: "rgb(104, 182, 240)",
  bindingMidpoint: "rgba(237, 237, 237, 0.8)",
};

export interface TextEditRequest {
  id: string;
  x: number;
  y: number;
  fontSize: number;
  color: string;
  /** What was typed (the element's `originalText`, else its `text`), not what is drawn. */
  text: string;
  /**
   * The width the canvas wraps the lines at, with `x` its left edge. Absent for
   * auto-sizing text, whose glyphs decide.
   */
  width?: number;
  /**
   * Already resolved, so the overlay never has to work out what an unset element means.
   * The overlay must lay the text out the way the canvas will, or it jumps the moment
   * the edit is committed.
   */
  textAlign: TextAlign;
  containerId?: string | null;
  /**
   * The family the text is drawn in — an Excalidraw id, `0` for the system stack of a
   * text with none. `engine.fontFamily(id)` gives its CSS; measure with it too.
   */
  fontFamily: number;
  /** Unitless: the overlay's lines must be as far apart as the canvas's. */
  lineHeight: number;
  /**
   * Where the caret goes on open, a UTF-16 code unit offset into `text` — a click on a
   * text that was already the sole selection, at the click. Absent selects the whole
   * text instead, as every other entry point opens it.
   */
  caret?: number;
}

/**
 * The text open in the host's editor, where it is and how it looks — what the editor
 * reads to sit exactly over it (`engine/text_session.rs`). Re-read after every
 * keystroke, camera change and style change.
 */
export interface TextEditLayout {
  id: string;
  /** Screen position of the text's unrotated top-left, canvas-relative. */
  x: number;
  y: number;
  /** The text box in world units: the editor is scaled by `zoom`, not sized by it. */
  width: number;
  height: number;
  fontSize: number;
  /** Unitless. */
  lineHeight: number;
  /** An Excalidraw family id, `0` for the system stack of a text with none. */
  fontFamily: number;
  textAlign: TextAlign;
  verticalAlign: VerticalAlign;
  /** Radians. */
  angle: number;
  /** The camera scale. */
  zoom: number;
  /** As the painter fills the glyphs. */
  color: string;
  /** `0..1`. */
  opacity: number;
  /** Whether the lines wrap at `width` (`pre-wrap`) or keep their hard breaks (`pre`). */
  wrap: boolean;
  containerId?: string;
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
  /**
   * A text is to be typed: the request opens a session on it (`updateTextEdit`), which
   * the host ends with `commitTextEdit` — or `setElementText` on the same id. Until then
   * undo and redo do nothing. Deleting the text, replacing the scene or a peer taking it
   * ends the session too.
   */
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

/**
 * Someone else in the room, as the host last heard of them. Mirrors the engine's `Peer`
 * (`engine/peers.rs`).
 *
 * What they `hold` nobody else can touch — it cannot be selected, moved, resized, edited,
 * deleted or erased — and it is outlined in their `color` with their `name` on it.
 * `preview` is their gesture in progress, painted in place of the committed elements
 * until the commit arrives: not in the scene, not in its history, not saved.
 */
export interface DrawPeer {
  id: string;
  name: string;
  /** A CSS colour. */
  color: string;
  holds?: string[];
  preview?: DrawElement[];
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
    /** The group stepped into, if any. Never serialized. */
    editingGroupId: string | null;
    /** Whether a move snaps to other elements. Off by default. */
    objectsSnap: boolean;
    /** What the eraser sweep in progress has marked, sorted. Empty between sweeps. */
    markedForErasure: string[];
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
    /**
     * How each frame was served. **Read these first.** Three caches are stacked here and
     * this is the outermost: on a `reuse` frame the static layer's bitmap is kept and no
     * element is replayed, so neither cache below is consulted. High `reuses` against
     * `frames` is the renderer working — and is why the counters below look frozen.
     */
    redraws: number;
    scrolls: number;
    reuses: number;
    /**
     * **The cache that decides per-frame work** *when a redraw happens.* One Path2D per
     * piece of geometry,
     * stroked with a single canvas call thereafter. Misses climbing during a pan or a
     * drag mean the geometry fingerprint covers something it should not.
     */
    pathCacheHits: number;
    pathCacheMisses: number;
    pathCacheLen: number;
    /**
     * Rough geometry, the layer *behind* the path cache — consulted only when that one
     * misses. So `shapeCacheHits` is normally zero however well things are going, and
     * reading it as "the cache is broken" is exactly backwards. Use `pathCache*`.
     */
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
    /**
     * Calls to the host's `hitTest()` wrapper only. Clicking to select does not go
     * through it — the engine hit-tests internally on the pointer path — so ordinary
     * use reads zero, which is correct rather than broken.
     */
    hitTests: number;
    hitTestMs: number;
    maxHitTestMs: number;
  };
}

/**
 * The properties panel's one read of the selection — see the engine's
 * `selection_style.rs`. Every value is `null` when the selection disagrees on it (the
 * panel then marks nothing as current) or when nothing selected has it.
 */
export interface SelectionStyle {
  /** Live selected elements. Zero means the values are the next element's style. */
  count: number;
  /** The selection's types plus the labels its shapes carry, distinct. */
  kinds: DrawElementType[];
  /** The subset of `kinds` whose background paints something. */
  filledKinds: DrawElementType[];
  textAlignable: boolean;
  verticalAlignable: boolean;
  canAlign: boolean;
  canDistribute: boolean;
  /**
   * Whether the polygon toggle would do anything: every selected element a line with
   * four points or more, and at least one selected.
   */
  canTogglePolygon: boolean;
  /** Only asked for a single element: from two up the group row shows regardless. */
  isGroup: boolean;
  strokeColor: string | null;
  backgroundColor: string | null;
  fillStyle: FillStyle | null;
  strokeWidth: number | null;
  strokeStyle: StrokeStyle | null;
  roughness: number | null;
  opacity: number | null;
  edges: "sharp" | "round" | null;
  startArrowhead: Arrowhead | null;
  endArrowhead: Arrowhead | null;
  /** Read off arrows alone; `edges` reads everything else. */
  arrowType: ArrowType | null;
  /**
   * Whether the selected lines are already closed polygons — `null` for a mixed
   * selection, or one holding nothing the polygon toggle applies to. Read off lines
   * alone, the same way `arrowType` reads off arrows alone.
   */
  isPolygon: boolean | null;
  fontSize: number | null;
  /** The family the texts share — 0 for the system stack a text with none is drawn in. */
  fontFamily: number | null;
  textAlign: TextAlign | null;
  verticalAlign: VerticalAlign | null;
  /** A free text is selected: auto-resize means something, and "Wrap text in a container" is offered. */
  hasFreeText: boolean;
  /** Whether the free texts size themselves to their text. */
  autoResize: boolean | null;
  /** A label is reached — selected, or carried by a selected shape. */
  hasLabel: boolean;
  /** Whether the labels wrap inside their shapes. */
  labelWrap: boolean | null;
  /** "Bind text to the container" is offered: a free text and one empty shape selected. */
  canBindText: boolean;
  /** "Unbind text" is offered: a selected shape carries a label. */
  canUnbindText: boolean;
  /** The figure kind shared by the selection, or the next figure's with nothing
   *  selected. `null` for a mixed selection, or one holding no figure at all. */
  figureKind: FigureKind | null;
  /** Resolved — a figure whose own `sides`/`ratio` is absent reads back as the kind's
   *  own default, the way it is actually drawn. */
  figureSides: number | null;
  figureRatio: number | null;
  /** Whether `figureKind` — resolved to one kind, not mixed or absent — has a sides
   *  stepper or a ratio slider at all. */
  figureHasSides: boolean;
  figureHasRatio: boolean;
  /**
   * Whose colours a stroke pick sets: sticky notes' (their own palette, no transparent,
   * the row reads "Text color"), the other elements', or both.
   */
  strokeDomain: ColorDomain;
  /** As `strokeDomain`, for a background pick. */
  backgroundDomain: ColorDomain;
}

/** A colour pick's domain: sticky notes keep colours of their own. */
export type ColorDomain = "regular" | "sticky" | "mixed";

/** The arrow types the engine draws: Excalidraw's less `elbow`. */
export type ArrowType = "sharp" | "round";

/** What a panel shows before an engine exists: the default style, nothing selected. */
export const EMPTY_SELECTION_STYLE: SelectionStyle = {
  count: 0,
  kinds: [],
  filledKinds: [],
  textAlignable: false,
  verticalAlignable: false,
  canAlign: false,
  canDistribute: false,
  canTogglePolygon: false,
  isGroup: false,
  strokeColor: DEFAULT_ELEMENT_STYLE.strokeColor,
  backgroundColor: DEFAULT_ELEMENT_STYLE.backgroundColor,
  fillStyle: DEFAULT_ELEMENT_STYLE.fillStyle,
  strokeWidth: DEFAULT_ELEMENT_STYLE.strokeWidth,
  strokeStyle: DEFAULT_ELEMENT_STYLE.strokeStyle,
  roughness: DEFAULT_ELEMENT_STYLE.roughness,
  opacity: DEFAULT_ELEMENT_STYLE.opacity,
  edges: DEFAULT_ELEMENT_STYLE.roundness == null ? "sharp" : "round",
  startArrowhead: "none",
  endArrowhead: "arrow",
  arrowType: "round",
  isPolygon: null,
  fontSize: 20,
  fontFamily: 5,
  textAlign: "left",
  verticalAlign: "middle",
  hasFreeText: false,
  autoResize: null,
  hasLabel: false,
  labelWrap: null,
  canBindText: false,
  canUnbindText: false,
  figureKind: null,
  figureSides: null,
  figureRatio: null,
  figureHasSides: false,
  figureHasRatio: false,
  strokeDomain: "regular",
  backgroundDomain: "regular",
};
