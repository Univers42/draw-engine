import { DrawEngine as WasmDrawEngine } from "../pkg/draw_engine.js";
import { DEFAULT_ELEMENT_STYLE, Scene } from "./types";
import { parseJson, wireCallbacks } from "./wasmLoad";
import type {
  AlignMode,
  Arrowhead,
  Camera,
  DrawElement,
  DrawElementStyle,
  DrawEngineOptions,
  DrawTheme,
  DrawTool,
  FlipAxis,
  ZOrderMode,
} from "./types";

export { loadDrawEngine } from "./wasmLoad";

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

  panBy(dx: number, dy: number): void {
    this.inner.panBy(dx, dy);
  }

  fit(padding?: number): void {
    this.inner.fit(padding);
  }

  screenToWorld(sx: number, sy: number): { x: number; y: number } {
    const camera = this.camera;
    return { x: (sx - camera.x) / camera.scale, y: (sy - camera.y) / camera.scale };
  }

  hitTest(sx: number, sy: number, tolerance = 0): DrawElement | null {
    const json = this.inner.hitTest(sx, sy, tolerance);
    return json ? parseJson<DrawElement | null>(json, null) : null;
  }

  setTool(tool: DrawTool): void {
    this.inner.setTool(tool);
  }

  getTool(): DrawTool {
    return this.inner.getTool() as DrawTool;
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
