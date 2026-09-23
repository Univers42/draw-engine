import { DrawEngine as WasmDrawEngine } from "../pkg/draw_engine.js";
import { readProbe, resetProbe, timeHitTest } from "./host/probe";
import { DEFAULT_ELEMENT_STYLE, DEFAULT_GRID, Scene } from "./types";
import { parseJson, wireCallbacks } from "./wasmLoad";
import type {
  AlignMode,
  Arrowhead,
  Camera,
  DebugSnapshot,
  DrawElement,
  DrawElementStyle,
  DrawEngineOptions,
  DrawTheme,
  DrawTool,
  FlipAxis,
  GridSettings,
  TextAlign,
  VerticalAlign,
  ZOrderMode,
} from "./types";

export { loadDrawEngine } from "./wasmLoad";

/**
 * CSS cursors, indexed by the engine's `HoverCursor` discriminant.
 *
 * The order is the contract with `engine/hover.rs` and is append-only — inserting in the
 * middle silently reassigns every cursor after it. `ci_ts_parity` asserts the two agree.
 */
const HOVER_CURSORS = [
  "default", // Default
  "move", // Move
  "ns-resize", // ResizeNs
  "ew-resize", // ResizeEw
  "nesw-resize", // ResizeNesw
  "nwse-resize", // ResizeNwse
  "grab", // Grab
  "grabbing", // Grabbing
  "pointer", // PointHandle
  "crosshair", // Crosshair
  "text", // Text
] as const;

/** Public DrawEngine: same method names as the old TS class, backed by WASM. */
export class DrawEngine {
  private readonly inner: InstanceType<typeof WasmDrawEngine>;
  private readonly canvas: HTMLCanvasElement;

  constructor(options: DrawEngineOptions) {
    this.canvas = options.canvas;
    this.inner = new WasmDrawEngine(options.canvas);
    wireCallbacks(this.inner, options);
  }

  get camera(): Camera {
    return parseJson<Camera>(this.inner.cameraJson(), { x: 0, y: 0, scale: 1 });
  }

  setScene(scene: Scene): void {
    this.inner.setSceneJson(JSON.stringify({ type: "osidraw", version: 1, elements: scene.toArray() }));
  }

  getScene(): Scene {
    const elements = parseJson<{ elements?: DrawElement[] }>(this.inner.exportJson(), {}).elements ?? [];
    return new Scene(elements);
  }

  setTheme(theme: DrawTheme): void {
    this.inner.setTheme(JSON.stringify(theme));
  }

  setViewport(width: number, height: number, dpr: number): void {
    this.inner.setViewport(width, height, dpr);
  }

  zoomAt(sx: number, sy: number, factor: number): void {
    this.inner.zoomAt(sx, sy, factor);
  }

  /**
   * One wheel event's worth of zoom, anchored at `(sx, sy)`.
   *
   * Pass `WheelEvent.deltaY` straight through. The engine owns the step — how much a
   * wheel delta is worth is arithmetic, and it is the part that has to be bounded or the
   * zoom teleports instead of moving.
   */
  wheelZoom(sx: number, sy: number, deltaY: number): void {
    this.inner.wheelZoom(sx, sy, deltaY);
  }

  panBy(dx: number, dy: number): void {
    this.inner.panBy(dx, dy);
  }

  fit(padding?: number): void {
    this.inner.fit(padding);
  }

  /** Frame the selection. Does nothing when nothing is selected. */
  zoomToSelection(padding?: number): void {
    this.inner.zoomToSelection(padding);
  }

  /**
   * How the last frames were served: redraws, scrolls and reuses of the static layer,
   * then reset. A diagnostic — the layer is either being reused or it is not, and from
   * outside those look identical until something is measured against the wrong guess.
   */
  paintStats(): { redraws: number; scrolls: number; reuses: number } {
    return parseJson(this.inner.paintStatsJson(), { redraws: 0, scrolls: 0, reuses: 0 });
  }

  /** Move by a screenful, in page counts: `pageBy(0, -1)` is one page up. */
  pageBy(pagesX: number, pagesY: number): void {
    this.inner.pageBy(pagesX, pagesY);
  }

  screenToWorld(sx: number, sy: number): { x: number; y: number } {
    const camera = this.camera;
    return { x: (sx - camera.x) / camera.scale, y: (sy - camera.y) / camera.scale };
  }

  hitTest(sx: number, sy: number, tolerance = 0): DrawElement | null {
    // Timed here rather than in the engine: hit testing is a linear scan with no spatial
    // index, so its cost is the first thing to look at on a large board — and the engine
    // is runtime-agnostic and has no clock of its own.
    const json = timeHitTest(() => this.inner.hitTest(sx, sy, tolerance));
    return json ? parseJson<DrawElement | null>(json, null) : null;
  }

  /**
   * Everything the engine and the host know about the current state, in one object.
   *
   * `scene` / `viewport` / `interaction` come from the engine; `rendering` from the WASM
   * frame loop, which is the only place a frame is visible; `host` from the browser
   * side, which is the only place a raw pointer event or a clock is.
   *
   * Read by `tools/editor-inspector`. Safe to poll — nothing here is computed for the
   * call, it is all state that already exists.
   */
  debugSnapshot(): DebugSnapshot {
    const engine = parseJson<Omit<DebugSnapshot, "host">>(
      this.inner.debugSnapshotJson(),
      {} as Omit<DebugSnapshot, "host">,
    );
    return { ...engine, host: readProbe() };
  }

  /** Zeroes the host counters, so one gesture can be measured rather than a session. */
  resetDebugCounters(): void {
    resetProbe();
  }

  /**
   * The CSS cursor for the pointer at this position.
   *
   * The engine decides, not the host: what the pointer is over — a resize handle, a
   * rotation handle, a movable element — is the engine's own hit-test state, and a host
   * that guessed at it from the tool alone is how a shape ends up silently resizing when
   * you meant to move it.
   *
   * Crosses the boundary as a small integer and is mapped here, so a hover costs no
   * allocation on either side.
   */
  hoverCursor(sx: number, sy: number): string {
    return HOVER_CURSORS[this.inner.hoverCursor(sx, sy)] ?? "default";
  }

  /**
   * The size a run of text will occupy, in world units.
   *
   * Measured against the font the canvas actually draws with, so anything sizing itself
   * to text — the editing overlay, a label's box — agrees with what appears on screen.
   */
  measureText(text: string, fontSize: number): { width: number; height: number } {
    // A `Float64Array` from WASM, so indexing is `number | undefined` under
    // `noUncheckedIndexedAccess`. The engine always returns both, but the fallback keeps
    // a truncated array from becoming `NaN` widths downstream.
    const measured = this.inner.measureText(text, fontSize);
    return { width: measured[0] ?? 4, height: measured[1] ?? fontSize };
  }

  /** The CSS font family the canvas draws text with. */
  fontFamily(): string {
    return this.inner.fontFamily();
  }

  /**
   * Show, size and snap to the canvas grid.
   *
   * `enabled` and `snap` are separate: a grid can be a visual reference you draw freely
   * over, which is a different request from being held to it.
   */
  setGrid(grid: Partial<GridSettings>): void {
    this.inner.setGridJson(JSON.stringify({ ...this.getGrid(), ...grid }));
  }

  getGrid(): GridSettings {
    return parseJson<GridSettings>(this.inner.getGridJson(), DEFAULT_GRID);
  }

  setTool(tool: DrawTool): void {
    this.inner.setTool(tool);
  }

  /**
   * Choose a tool the way a keyboard shortcut does.
   *
   * The difference from `setTool` is the return trip: pressing the hand or eraser key
   * while that tool is already active goes back to the tool it interrupted. Toolbar
   * buttons stay on `setTool`, because a button that shows a tool as active must not
   * switch away when clicked again.
   */
  activateTool(tool: DrawTool): void {
    this.inner.activateTool(tool);
  }

  getTool(): DrawTool {
    return this.inner.getTool() as DrawTool;
  }

  /**
   * Place a decoded image, centred on a screen point.
   *
   * The caller decodes the file, because only a browser can, and having done so already
   * knows the natural size. Everything after that — how large the image should appear,
   * where it lands, which frame it joins — is the engine's, so every frontend produces
   * the same element. Returns the new element's id, or `null` if the image could not be
   * measured.
   */
  /**
   * Place an embed, resolving the pasted link first.
   *
   * Whether the host may be framed at all, and what the page rewrites to, are the
   * engine's: a caller that resolved links itself could frame something the rules would
   * have refused. Returns the new element's id, or `null` if the link is not embeddable.
   */
  insertEmbed(rawUrl: string, screenX: number, screenY: number): string | null {
    return this.inner.insertEmbed(rawUrl, screenX, screenY) ?? null;
  }

  /**
   * What a pasted link resolves to, or `null` if it cannot be embedded.
   *
   * Lets a caller say so *before* putting an empty box on the board.
   */
  resolveEmbed(rawUrl: string): {
    url: string;
    intrinsicWidth: number;
    intrinsicHeight: number;
    kind: "video" | "generic";
    allowSameOrigin: boolean;
  } | null {
    const json = this.inner.resolveEmbed(rawUrl);
    return json ? JSON.parse(json) : null;
  }

  /** The embeds on screen and where their frames go, in screen pixels. */
  embedFramesJson(): string {
    return this.inner.embedFrames();
  }

  insertImage(
    dataUrl: string,
    naturalWidth: number,
    naturalHeight: number,
    screenX: number,
    screenY: number,
  ): string | null {
    return this.inner.insertImage(dataUrl, naturalWidth, naturalHeight, screenX, screenY) ?? null;
  }

  setNextStyle(style: Partial<DrawElementStyle>): void {
    this.inner.setNextStyleJson(JSON.stringify(style));
  }

  getNextStyle(): DrawElementStyle {
    return parseJson<DrawElementStyle>(this.inner.getNextStyleJson(), DEFAULT_ELEMENT_STYLE);
  }

  getSelection(): string[] {
    return parseJson<string[]>(this.inner.getSelectionJson(), []);
  }

  getSelectedElements(): DrawElement[] {
    return parseJson<DrawElement[]>(this.inner.getSelectedElementsJson(), []);
  }

  applyStyle(patch: Partial<DrawElementStyle>): void {
    this.inner.applyStyleJson(JSON.stringify(patch));
  }

  clearSelection(): void {
    this.inner.clearSelection();
  }

  select(ids: string[]): void {
    this.inner.selectJson(JSON.stringify(ids));
  }

  selectAll(): void {
    this.inner.selectAll();
  }

  deleteSelection(): void {
    this.inner.deleteSelection();
  }

  copySelection(): string | null {
    return this.inner.copySelection() ?? null;
  }

  cutSelection(): string | null {
    return this.inner.cutSelection() ?? null;
  }

  /**
   * Merges a peer's elements into the scene by id, last-writer-wins.
   *
   * Use this for anything arriving over the wire. `pasteJson` mints a new id for every
   * element — correct for a paste, catastrophic for a merge: it turns each incoming
   * edit into a duplicate, and the resulting change is broadcast back, so two clients
   * grow the board without bound.
   *
   * Optional `order` (array of live ids) in the JSON rewrites z-order after the merge.
   *
   * Returns whether anything actually changed, so an echo costs nothing.
   */
  applyRemotePatch(json: string): boolean {
    return this.inner.applyRemotePatch(json);
  }

  pasteJson(json?: string | null, at?: { x: number; y: number }): boolean {
    return this.inner.pasteJson(json ?? undefined, at?.x, at?.y);
  }

  duplicateSelection(): void {
    this.inner.duplicateSelection();
  }

  nudgeSelection(dx: number, dy: number): void {
    this.inner.nudgeSelection(dx, dy);
  }

  reorderSelection(mode: ZOrderMode): void {
    this.inner.reorderSelection(mode);
  }

  alignSelection(mode: AlignMode): void {
    this.inner.alignSelection(mode);
  }

  distributeSelection(axis: "x" | "y"): void {
    this.inner.distributeSelection(axis);
  }

  flipSelection(axis: FlipAxis): void {
    this.inner.flipSelection(axis);
  }

  groupSelection(): void {
    this.inner.groupSelection();
  }

  ungroupSelection(): void {
    this.inner.ungroupSelection();
  }

  selectionIsGroup(): boolean {
    return this.inner.selectionIsGroup();
  }

  toggleLockSelection(): void {
    this.inner.toggleLockSelection();
  }

  selectionLocked(): boolean {
    return this.inner.selectionLocked();
  }

  setFontSize(size: number): void {
    this.inner.setFontSize(size);
  }

  getFontSize(): number {
    return this.inner.getFontSize();
  }

  /** Re-widths a dragged-out text column and re-wraps it. Auto-sizing text is ignored. */
  setTextBoxWidth(id: string, width: number): void {
    this.inner.setTextBoxWidth(id, width);
  }

  setTextAlign(align: TextAlign): void {
    this.inner.setTextAlign(align);
  }

  getTextAlign(): TextAlign {
    return this.inner.getTextAlign() as TextAlign;
  }

  setVerticalAlign(align: VerticalAlign): void {
    this.inner.setVerticalAlign(align);
  }

  getVerticalAlign(): VerticalAlign {
    return this.inner.getVerticalAlign() as VerticalAlign;
  }

  zoomIn(): void {
    this.inner.zoomIn();
  }

  zoomOut(): void {
    this.inner.zoomOut();
  }

  zoomReset(): void {
    this.inner.zoomReset();
  }

  contentInView(): boolean {
    return this.inner.contentInView();
  }

  setToolLocked(locked: boolean): void {
    this.inner.setToolLocked(locked);
  }

  getToolLocked(): boolean {
    return this.inner.getToolLocked();
  }

  editSelectedText(): boolean {
    return this.inner.editSelectedText();
  }

  beginPointer(sx: number, sy: number, additive = false, duplicate = false): void {
    this.inner.beginPointer(sx, sy, additive, duplicate);
  }

  beginPan(sx: number, sy: number): void {
    this.inner.beginPan(sx, sy);
  }

  movePointer(sx: number, sy: number, square = false, bypassSnap = false): void {
    this.inner.movePointer(sx, sy, square, bypassSnap);
  }

  endPointer(): void {
    this.inner.endPointer();
  }

  cancelPointer(): void {
    this.inner.cancelPointer();
  }

  handleDoubleClick(sx: number, sy: number): void {
    this.inner.handleDoubleClick(sx, sy);
  }

  /**
   * Whether a line or arrow is being placed point by point.
   *
   * The canvas asks on every pointer move, because a path is the one thing that tracks
   * the cursor with no button held.
   */
  linearInProgress(): boolean {
    return this.inner.linearInProgress();
  }

  /** Ends a path being placed, keeping the points already put down. */
  finishLinear(): void {
    this.inner.finishLinear();
  }

  setElementText(id: string, text: string): void {
    this.inner.setElementText(id, text);
  }

  setArrowheads(patch: { start?: Arrowhead; end?: Arrowhead }): void {
    this.inner.setArrowheadsJson(JSON.stringify(patch));
  }

  requestDraw(): void {
    /* WASM schedules its own rAF. */
  }

  exportJson(): string {
    return this.inner.exportJson();
  }

  exportSvg(padding = 16): string | null {
    return this.inner.exportSvg(padding) ?? null;
  }

  async exportPng(): Promise<Blob | null> {
    return new Promise((resolve) => this.canvas.toBlob((blob) => resolve(blob), "image/png"));
  }

  loadScene(json: string): boolean {
    return this.inner.loadScene(json);
  }

  clear(): void {
    this.inner.clear();
  }

  undo(): void {
    this.inner.undo();
  }

  redo(): void {
    this.inner.redo();
  }

  destroy(): void {
    this.inner.destroy();
  }
}
