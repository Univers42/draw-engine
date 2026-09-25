import { DrawEngine as WasmDrawEngine } from "../pkg/draw_engine.js";
import { readProbe, resetProbe, timeHitTest } from "./host/probe";
import { DEFAULT_ELEMENT_STYLE, DEFAULT_GRID, EMPTY_SELECTION_STYLE, Scene } from "./types";
import { parseJson, wireCallbacks } from "./wasmLoad";
import type {
  AlignMode,
  Arrowhead,
  Camera,
  DebugSnapshot,
  DrawElement,
  DrawElementStyle,
  DrawEngineOptions,
  DrawPeer,
  DrawTheme,
  DrawTool,
  FlipAxis,
  GridSettings,
  SelectionStyle,
  TextAlign,
  TextEditLayout,
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
   * Who else is in the room, what they hold and what they are doing right now. Replaces
   * what the engine knew; `[]` when everyone has left. What this engine had selected and
   * a peer now holds is let go — see `engine/peers.rs`.
   */
  setPeers(peers: readonly DrawPeer[]): void {
    this.inner.setPeers(JSON.stringify(peers));
  }

  /**
   * Where a peer's laser pointer is, in world units, and whether they are pressing it —
   * their trail is drawn in their colour and fades as it does on their screen. See
   * `engine/peers.rs`.
   */
  peerLaser(id: string, color: string, x: number, y: number, down: boolean): void {
    this.inner.peerLaser(id, color, x, y, down);
  }

  /** The id of the peer holding what is under the pointer, if someone does. */
  peerAt(sx: number, sy: number): string | null {
    return this.inner.peerAt(sx, sy) ?? null;
  }

  /**
   * The elements the gesture in progress is changing, as they are this instant — empty
   * between gestures. What a host streams to peers while something is being drawn,
   * moved or resized, so it moves on their screens too.
   */
  gestureElements(): DrawElement[] {
    return parseJson<DrawElement[]>(this.inner.gestureElementsJson(), []);
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
  measureText(
    text: string,
    fontSize: number,
    family?: number,
  ): { width: number; height: number } {
    // A `Float64Array` from WASM, so indexing is `number | undefined` under
    // `noUncheckedIndexedAccess`. The engine always returns both, but the fallback keeps
    // a truncated array from becoming `NaN` widths downstream.
    const measured = this.inner.measureText(text, fontSize, family);
    return { width: measured[0] ?? 4, height: measured[1] ?? fontSize };
  }

  /**
   * The CSS font stack the canvas draws text with — for Excalidraw family `id`
   * (`TextEditRequest.fontFamily`), or the system stack legacy text uses when omitted or 0.
   */
  fontFamily(id?: number): string {
    return this.inner.fontFamily(id);
  }

  /**
   * Tell the engine web fonts finished loading: it re-measures and re-lays every text in a
   * named family without stamping it, so a late font never reads as an edit.
   */
  fontsLoaded(): void {
    this.inner.fontsLoaded();
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

  /** Snapping to other elements while moving. Off by default, as in Excalidraw. */
  setObjectsSnap(on: boolean): void {
    this.inner.setObjectsSnap(on);
  }

  getObjectsSnap(): boolean {
    return this.inner.getObjectsSnap();
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
   * Point embed `id` at another link, resolved by the same rules as `insertEmbed`.
   * Returns whether it changed: `false` for a refused link, or a locked or held embed.
   */
  setEmbedUrl(id: string, rawUrl: string): boolean {
    return this.inner.setEmbedUrl(id, rawUrl);
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
    /** Framed as a document of its own rather than at an address — a gist. */
    document: boolean;
  } | null {
    const json = this.inner.resolveEmbed(rawUrl);
    return json ? JSON.parse(json) : null;
  }

  /** Every live embed, where its frame goes in screen pixels, and whether it is on screen. */
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

  /**
   * Whether align would move anything: at least two units — a group counts as one, a
   * lone group as what it holds — and no frame in the selection. Offer the control on
   * this rather than on the element count.
   */
  canAlign(): boolean {
    return this.inner.canAlign();
  }

  /** Whether distribute would move anything: at least three units, and no frame. */
  canDistribute(): boolean {
    return this.inner.canDistribute();
  }

  flipSelection(axis: FlipAxis): void {
    this.inner.flipSelection(axis);
  }

  groupSelection(): void {
    this.inner.groupSelection();
  }

  /**
   * Ctrl+G: groups what is loose, ungroups what is already exactly one group.
   *
   * Deliberately not the oracle's behaviour — theirs is a no-op on a grouped selection,
   * which leaves no way out of a group with the key you reached for. On a nested
   * selection this peels one level, never all of them.
   */
  toggleGroupSelection(): void {
    this.inner.toggleGroupSelection();
  }

  /** The group that has been stepped into, if any. */
  editingGroupId(): string | null {
    return this.inner.editingGroupId() ?? null;
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

  /**
   * Set the Excalidraw font family (1 Virgil, 2 Helvetica, 3 Cascadia, 5 Excalifont,
   * 6 Nunito, 7 Lilita One, 8 Comic Shanns, 9 Liberation Sans) of the selected texts and
   * labels, and of the next text drawn. Unknown ids are ignored.
   *
   * The text is laid out, and its shape grown, at once and in whatever face the browser
   * has: load the face first (`document.fonts.load` with the stack `fontFamily(id)` gives),
   * as Excalidraw's `changeFontFamily` waits for it (`actionProperties.tsx@1118751f:1302-1356`).
   * Laid out in a fallback wider than the face, a shape grows and keeps the growth.
   */
  setFontFamily(id: number): void {
    this.inner.setFontFamily(id);
  }

  /** The family of the first selected text, else the one the next text is drawn in. */
  getFontFamily(): number {
    return this.inner.getFontFamily();
  }

  /** Switch the selected free texts between growing with their text and a fixed width. */
  setTextAutoResize(autoResize: boolean): void {
    this.inner.setTextAutoResize(autoResize);
  }

  /** Whether the selected containers' labels wrap (the container grows taller) or not (wider). */
  setLabelWrap(wrap: boolean): void {
    this.inner.setLabelWrap(wrap);
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

  /** Alt, as held for the move about to be reported. The eraser un-marks with it. */
  setAltHeld(held: boolean): void {
    this.inner.setAltHeld(held);
  }

  /** Ctrl/Cmd, as held for the pointer event about to be reported: an arrow binds to nothing while it is down. */
  setCtrlHeld(held: boolean): void {
    this.inner.setCtrlHeld(held);
  }

  /** A move with no button held: the arrow tool lights the shape it would attach to. */
  hoverPointer(sx: number, sy: number): void {
    this.inner.hoverPointer(sx, sy);
  }

  /** The pointer left the canvas; whatever a hover lit goes out. */
  endHover(): void {
    this.inner.endHover();
  }

  /** `invertSnap` flips object snapping for this move — the host's Ctrl/Cmd. */
  movePointer(sx: number, sy: number, square = false, invertSnap = false): void {
    this.inner.movePointer(sx, sy, square, invertSnap);
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

  /**
   * Writes `text` into the text `id` and commits it, in one call, ending a session open
   * on it; emptied, the text goes as `commitTextEdit` removes it. See `updateTextEdit`.
   */
  setElementText(id: string, text: string): void {
    this.inner.setElementText(id, text);
  }

  /**
   * The open text with `text` typed into it — laid out, its shape grown or shrunk —
   * with no step and no stamp: peers see it through `gestureElements`. `false` when no
   * text is open any more, and the editor should close.
   */
  updateTextEdit(text: string): boolean {
    return this.inner.updateTextEdit(text);
  }

  /**
   * Ends the edit with `text`, as one step; emptied, the text goes. `viaKeyboard`
   * (Escape, Ctrl/Cmd+Enter) leaves it — or a label's shape — selected.
   */
  commitTextEdit(text: string, viaKeyboard: boolean): void {
    this.inner.commitTextEdit(text, viaKeyboard);
  }

  /** Where the open text is and how it looks; null when none is open. */
  textEditLayout(): TextEditLayout | null {
    return parseJson<TextEditLayout | null>(this.inner.textEditLayoutJson(), null);
  }

  /**
   * The text element `id` as it would be with `text` in it, without committing it —
   * for a host writing text whole (`setElementText`) to show peers while someone types.
   * A host on the typing session streams `gestureElements()`, which carries the shape the
   * text grows too. Null when `id` is not a text.
   */
  textPreview(id: string, text: string): DrawElement | null {
    return parseJson<DrawElement | null>(this.inner.textPreviewJson(id, text), null);
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

  /**
   * Replaces the image `imageId` with its trace, in one undoable step: the traced regions
   * as a group of editable shapes, or one SVG picture in the image's box. Tracing itself
   * happens off the main thread — see `TraceWorker` in `./vectorize`.
   */
  vectorizeImage(
    imageId: string,
    insert: import("./vectorize").VectorizeInsert,
    options: import("./vectorize").VectorizeOptions,
  ): import("./vectorize").VectorizeOutcome {
    const json = JSON.stringify(options);
    const raw =
      insert.as === "shapes"
        ? this.inner.vectorizeToShapes(
            imageId,
            insert.rings.colours,
            insert.rings.lengths,
            insert.rings.coords,
            json,
          )
        : this.inner.vectorizeToPicture(imageId, insert.dataUrl, json);
    const outcome = parseJson<{ ids?: string[]; id?: string; refused?: import("./vectorize").VectorizeRefusal }>(
      raw,
      { refused: "malformed" },
    );
    if (outcome.refused) return { refused: outcome.refused };
    return { ids: outcome.ids ?? (outcome.id ? [outcome.id] : []) };
  }

  /** Everything the properties panel shows. Ask again when `styleRevision()` moves. */
  selectionStyle(): SelectionStyle {
    return parseJson<SelectionStyle>(this.inner.selectionStyleJson(), EMPTY_SELECTION_STYLE);
  }

  /** Moves whenever `selectionStyle()` may have: selection, style, undo, a peer's edit. */
  styleRevision(): number {
    return this.inner.styleRevision();
  }

  /** Shows a style on the canvas without committing it; `applyStyle` commits. */
  previewStyle(patch: Partial<DrawElementStyle>): void {
    this.inner.previewStyleJson(JSON.stringify(patch));
  }

  /** Remembers the first selected element's style. False when nothing is selected. */
  copyStyles(): boolean {
    return this.inner.copyStyles();
  }

  pasteStyles(): void {
    this.inner.pasteStyles();
  }

  /** How many live elements use each stroke or background colour. */
  colorCounts(key: "strokeColor" | "backgroundColor"): [string, number][] {
    return parseJson<[string, number][]>(
      this.inner.colorCountsJson(key === "backgroundColor"),
      [],
    );
  }
}
