import { DrawEngine as WasmDrawEngine } from "../pkg/draw_engine.js";
import { screenToWorld } from "./cameraMath";
import { readProbe, resetProbe, timeHitTest } from "./host/probe";
import {
  DEFAULT_ELEMENT_STYLE,
  DEFAULT_FIGURE_PARAMS,
  DEFAULT_GRID,
  EMPTY_SELECTION_STYLE,
  Scene,
} from "./types";
import { parseJson, wireCallbacks } from "./wasmLoad";
import type {
  AlignMode,
  Arrowhead,
  ArrowType,
  Camera,
  ClipboardCopy,
  ClipboardFormatName,
  ClipboardSupport,
  DebugSnapshot,
  DrawElement,
  DrawElementStyle,
  ConversionType,
  DrawEngineOptions,
  DrawPeer,
  DrawTheme,
  DrawTool,
  FigureParams,
  FlipAxis,
  FlowchartDirection,
  FlowchartShape,
  GridSettings,
  PngExport,
  SelectionStyle,
  SecondaryPanEnd,
  SecondaryPanStart,
  SvgExport,
  StylePatch,
  TextAlign,
  TextEditLayout,
  VerticalAlign,
  ZOrderMode,
} from "./types";

export { loadDrawEngine } from "./wasmLoad";

/** The seven names `ConvertTo::name` writes — the boundary check on `sharedConversionType`. */
const CONVERSION_TYPES: readonly string[] = [
  "rectangle",
  "diamond",
  "ellipse",
  "line",
  "sharpArrow",
  "curvedArrow",
  "elbowArrow",
];

const isConversionType = (name: string): name is ConversionType =>
  CONVERSION_TYPES.includes(name);

/**
 * CSS cursors, indexed by the engine's `HoverCursor` discriminant.
 *
 * The order is the contract with `engine/hover.rs` and is append-only — inserting in the
 * middle silently reassigns every cursor after it. `ci_cursor_parity.rs` asserts the two agree;
 * it replaced a sentence here that named a `ci_ts_parity` which has never existed anywhere in
 * this repository, so the risk was real and the guarantee was not.
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

/**
 * A right-button press, indexed by the engine's `SecondaryPanStart` discriminant.
 *
 * The order is the contract with `pan.rs` and is append-only, like the cursor table
 * above. `editing-text` is last because it is the same session with one difference, and
 * a host that read index 1 as it would prevent the default of a press that must keep it.
 */
const SECONDARY_PAN_START = ["declined", "started", "editing-text"] as const;

/** A right-button release, indexed by the engine's `SecondaryPanEnd` discriminant. */
const SECONDARY_PAN_END = ["none", "drag", "menu"] as const;

/**
 * The blob the browser encoded, or `null`.
 *
 * The binding hands back whatever `toBlob` resolved with, typed no more precisely than
 * `any` because the value crosses the WASM boundary as a JS object — so this is where it
 * becomes a `Blob` or is found not to be one. `null` is also the answer for a canvas too
 * large to encode (`data/blob.ts@1118751f:245-252`), which the oracle reports as a
 * `CanvasError` and we leave to the host to notice.
 */
function encodedBlob(value: unknown): Blob | null {
  return value instanceof Blob ? value : null;
}

/**
 * The engine's clipboard answer, as a value the host can trust.
 *
 * **Every field is read defensively, and a malformed answer is a declined one.** The
 * binding is a `JsValue` because a promise cannot cross into a plain object, so nothing
 * here is checked by the type system, and a host that read a missing `mime` as `""` and
 * wrote it would produce a clipboard nothing can paste — the exact failure this feature
 * exists to prevent. A shape that does not hold up is reported as `supported: false` with
 * no payload, which is what the engine says when it declines, so there is one "no" and not
 * two.
 */
function readClipboardCopy(raw: unknown, format: ClipboardFormatName): ClipboardCopy {
  const declined: ClipboardCopy = { supported: false, mime: "", scope: "scene" };
  if (typeof raw !== "object" || raw === null) return declined;
  const answer = raw as Record<string, unknown>;
  if (answer["supported"] !== true) {
    const refusal = answer["refusal"];
    // The two reasons the engine declines are carried through, so the host can say the
    // oracle's own sentence for each instead of one "no" for both.
    if (refusal === "browser-cannot-take" || refusal === "nothing-to-copy") {
      declined.refusal = refusal;
    }
    return declined;
  }
  const mime = answer["mime"];
  const scope = answer["scope"];
  if (typeof mime !== "string" || mime === "") return declined;
  if (scope !== "selection" && scope !== "scene") return declined;
  const payload = format === "png" ? answer["blob"] : answer["text"];
  if (payload === undefined || payload === null) return declined;
  const copy: ClipboardCopy = { supported: true, mime, scope };
  if (format === "png") {
    copy.blob = Promise.resolve(payload).then(encodedBlob);
  } else if (typeof payload === "string") {
    copy.text = payload;
  } else {
    return declined;
  }
  return copy;
}

/** Public DrawEngine: same method names as the old TS class, backed by WASM. */
export class DrawEngine {
  private readonly inner: InstanceType<typeof WasmDrawEngine>;

  constructor(options: DrawEngineOptions) {
    // The canvas itself belongs to WASM, which paints into it — this side no longer holds
    // a reference, because the only thing that used one was encoding the visible canvas as
    // the export, and the export is now the engine's own offscreen target.
    this.inner = new WasmDrawEngine(options.canvas);
    wireCallbacks(this.inner, options);
  }

  get camera(): Camera {
    return parseJson<Camera>(this.inner.cameraJson(), { x: 0, y: 0, scale: 1 });
  }

  /** Where the camera is headed: an eased move's end (a fit, a reveal) while one runs, else `camera`. */
  get cameraTarget(): Camera {
    return parseJson<Camera>(this.inner.cameraTargetJson(), this.camera);
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

  /**
   * How far the host's own UI reaches over each side of the canvas, in CSS pixels —
   * Excalidraw's measured `[data-viewport-ui]` offsets. A reveal (Ctrl/Cmd+Arrow,
   * Alt+Arrow) brings what it shows into the room they leave.
   */
  setViewportOffsets(offsets: { top: number; right: number; bottom: number; left: number }): void {
    this.inner.setViewportOffsets(offsets.top, offsets.right, offsets.bottom, offsets.left);
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
   * Shift+1: everything, zoomed out to hold it or in, up to 100%, clear of the chrome
   * given to {@link setViewportOffsets}. Excalidraw's `zoomToFit`.
   */
  zoomToFit(): void {
    this.inner.zoomToFit();
  }

  /** Shift+2: the selection — everything when nothing is — no closer than 100%. */
  zoomToFitSelectionInViewport(): void {
    this.inner.zoomToFitSelectionInViewport();
  }

  /** Shift+3: the selection — everything when nothing is — filling the view. */
  zoomToFitSelection(): void {
    this.inner.zoomToFitSelection();
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

  /**
   * Where a screen point is in the world, from the engine's `screen_to_world`.
   *
   * The expression used to be written out here — the same one `engine/src/camera.ts`
   * mirrored and the same one the front called — so this was the third copy. It is now
   * the engine's own, reached through the wrapper beside it.
   *
   * The method and the free function differ only in where the camera comes from: this
   * one reads it through the `camera` getter, which is a `cameraJson` call and a
   * `JSON.parse`. A caller that already holds the camera should call the free
   * `screenToWorld` from `./cameraMath` instead.
   */
  screenToWorld(sx: number, sy: number): { x: number; y: number } {
    return screenToWorld(this.camera, sx, sy);
  }

  hitTest(sx: number, sy: number, tolerance = 0): DrawElement | null {
    // Timed here rather than in the engine: hit testing is a linear scan with no spatial
    // index, so its cost is the first thing to look at on a large board — and the engine
    // is runtime-agnostic and has no clock of its own.
    const json = timeHitTest(() => this.inner.hitTest(sx, sy, tolerance));
    return json ? parseJson<DrawElement | null>(json, null) : null;
  }

  /**
   * Whether `(sx, sy)` lands inside the padded common bounding box of the current
   * multi-selection, with no element itself under it — a right-click there is still a
   * right-click on the selection, not on the board (`hits_selection_box`).
   */
  hitsSelectionBox(sx: number, sy: number): boolean {
    return this.inner.hitsSelectionBox(sx, sy);
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

  /** `prefers-reduced-motion`: an eased camera move (fit, zoom to selection, zoom
   *  in/out/reset) lands at once rather than animating while this is set. Kept current by
   *  `bindCanvas` — WASM has no `matchMedia` of its own. */
  setReducedMotion(on: boolean): void {
    this.inner.setReducedMotion(on);
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

  setNextStyle(style: StylePatch): void {
    this.inner.setNextStyleJson(JSON.stringify(style));
  }

  getNextStyle(): DrawElementStyle {
    return parseJson<DrawElementStyle>(this.inner.getNextStyleJson(), DEFAULT_ELEMENT_STYLE);
  }

  /** What the Shapes picker sets: which figure the tool draws next. Never restyles a
   *  selected figure — the inspector's own sides stepper and ratio slider do that. */
  setNextFigure(figure: FigureParams): void {
    this.inner.setNextFigureJson(JSON.stringify(figure));
  }

  getNextFigure(): FigureParams {
    return parseJson<FigureParams>(this.inner.getNextFigureJson(), DEFAULT_FIGURE_PARAMS);
  }

  getSelection(): string[] {
    return parseJson<string[]>(this.inner.getSelectionJson(), []);
  }

  getSelectedElements(): DrawElement[] {
    return parseJson<DrawElement[]>(this.inner.getSelectedElementsJson(), []);
  }

  applyStyle(patch: StylePatch): void {
    this.inner.applyStyleJson(JSON.stringify(patch));
  }

  clearSelection(): void {
    this.inner.clearSelection();
  }

  select(ids: string[]): void {
    this.inner.selectJson(JSON.stringify(ids));
  }

  /** Selects `id` as a press on it does — with its group — locked or not: a right-click. */
  selectElement(id: string): void {
    this.inner.selectElement(id);
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

  /**
   * Plain text pasted as text elements — one per line, each wrapped to half the visible
   * width and centred on `at`, the **world** point the pointer is at (the same convention
   * `pasteJson` takes). One step of undo, the new texts selected; the clipboard is
   * untouched, nothing was copied.
   *
   * This is the other branch of a paste: `pasteJson` for this app's own element JSON, this
   * for everything else. `false` for text that would make no element at all, so a caller
   * can fall through to whatever it had.
   */
  pasteText(text: string, at?: { x: number; y: number }): boolean {
    return this.inner.pasteText(text, at?.x, at?.y);
  }

  /**
   * A scene made elsewhere — the Mermaid import — placed as `pasteJson` places it, centred
   * on the **world** point `at`, but with every text laid out in this engine's fonts and
   * every shape grown to hold its label. One step of undo; the clipboard is untouched.
   */
  insertJson(json: string, at?: { x: number; y: number }): boolean {
    return this.inner.insertJson(json, at?.x, at?.y);
  }

  /**
   * The command palette's "Add rectangle / diamond / ellipse / figure": a default-sized
   * shape centred on the **screen** point `(screenX, screenY)`, selected, as one step of
   * undo — `insertImage`/`insertEmbed`'s own convention. `"figure"` takes whatever
   * `setNextFigure` last set (`nextFigure` if nothing was). `null` for any other kind.
   */
  insertDefaultShape(
    kind: FlowchartShape | "figure",
    screenX: number,
    screenY: number,
  ): string | null {
    return this.inner.insertDefaultShape(kind, screenX, screenY) ?? null;
  }

  duplicateSelection(): void {
    this.inner.duplicateSelection();
  }

  nudgeSelection(dx: number, dy: number): void {
    this.inner.nudgeSelection(dx, dy);
  }

  /** How far one arrow key moves the selection: 1, or 5 with Shift; by the grid when held to it. */
  nudgeStep(shift: boolean): number {
    return this.inner.nudgeStep(shift);
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

  /** Whether the lock toggle would unlock: something it acts on is locked. */
  selectionLocked(): boolean {
    return this.inner.selectionLocked();
  }

  /** The board menu's "Unlock all": every locked element unlocked and selected. */
  unlockAll(): void {
    this.inner.unlockAll();
  }

  /** Whether "Unlock all" is on offer: nothing selected, and something locked. */
  canUnlockAll(): boolean {
    return this.inner.canUnlockAll();
  }

  /**
   * Closes the selected line(s) into filled polygons, or opens them back up
   * (`actionTogglePolygon`). Whether this would do anything, and whether the panel's
   * toggle should show pressed, are read off `selectionStyle()` — `canTogglePolygon` and
   * `isPolygon` — not asked for here.
   */
  togglePolygon(): void {
    this.inner.togglePolygon();
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

  /**
   * `setTextAutoResize` and `setLabelWrap` together, as one step of undo — the wrap
   * row's own sense: `true` keeps a free text at its width and wraps a label in its
   * shape, `false` the reverse.
   */
  setTextWrap(wrap: boolean): void {
    this.inner.setTextWrap(wrap);
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

  /**
   * Starts a right-button pan session, or declines it: `"declined"` when more than one
   * pointer is down, and otherwise a session that is a pan only once the pointer has
   * travelled past the threshold — see `pan.rs` and `App.pan.ts@1118751f:88-125`.
   *
   * `"started"` also answers whether the press may prevent its own default: it may not
   * while a text is open (`App.pan.ts:1118751f:118-125`, issue #4489), which is the
   * `"editing-text"` case. That rule is here rather than in the host so two hosts cannot
   * answer it differently.
   *
   * `pointersDown` is the host's count of pointers on the canvas — the platform's count,
   * which the engine cannot read for itself.
   */
  beginSecondaryPan(sx: number, sy: number, pointersDown: number): SecondaryPanStart {
    return SECONDARY_PAN_START[this.inner.beginSecondaryPan(sx, sy, pointersDown)] ?? "declined";
  }

  /** Ends a right-button session. See `beginSecondaryPan`. */
  endSecondaryPan(): SecondaryPanEnd {
    return SECONDARY_PAN_END[this.inner.endSecondaryPan()] ?? "none";
  }

  /**
   * Whether a `contextmenu` event belongs to a right-button session and must not open the
   * menu: it came with the press, or it follows a release that turned out to be a drag.
   *
   * Platforms disagree about which of those it is — macOS and Linux fire it on mousedown,
   * Windows on mouseup — and swallowing the wrong one is how a right-drag to pan ends up
   * with a menu open over it (`App.pan.ts@1118751f:74-84`, and the doc comment at `:12-25`
   * that is the design).
   */
  consumesContextMenu(): boolean {
    return this.inner.consumesContextMenu();
  }

  /**
   * Whether pointer moves arriving with no button of the host's own down are wanted: a path
   * placed point by point, or a right-button session deciding whether it is a click or a pan.
   */
  wantsPointerMoves(): boolean {
    return this.inner.wantsPointerMoves();
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
   * Commits a frame's name, trimmed, as one step — the host's input over the name label
   * (opened by `onRequestFrameRename`) calls this once, on Enter, blur or Escape alike.
   * Emptied, the name falls back to the generic default.
   */
  renameFrame(id: string, name: string): void {
    this.inner.renameFrame(id, name);
  }

  /**
   * Numbers the frames `ids` lists 0, 1, 2… on the presentation path, in that order, as
   * one step. Ids that are not live frames are skipped; frames not listed keep their step.
   */
  setPresentationPath(ids: readonly string[]): void {
    this.inner.setPresentationPath(JSON.stringify(ids));
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

  /** The heads of the selected lines and arrows, and always of the next arrow drawn. */
  setArrowheads(patch: { start?: Arrowhead; end?: Arrowhead }): void {
    this.inner.setArrowheadsJson(JSON.stringify(patch));
  }

  /** Sharp, curved or elbow, for the selected arrows and the next one. */
  setArrowType(type: ArrowType): void {
    this.inner.setArrowType(type);
  }

  /** Ctrl/Cmd+Shift+> and <: the selected texts a tenth bigger or smaller, each from its own size. */
  stepFontSize(increase: boolean): void {
    this.inner.stepFontSize(increase);
  }

  /**
   * The font picker's hover: `id` shown on the selected texts uncommitted, or — with none —
   * what the last hover changed given back. `setFontFamily` commits; load the face first,
   * as for it.
   */
  previewFontFamily(id?: number): void {
    this.inner.previewFontFamily(id);
  }

  /** The families the board's texts use, each once: the font picker's "In this scene". */
  sceneFontFamilies(): number[] {
    return Array.from(this.inner.sceneFontFamilies());
  }

  /** "Bind text to the container": a free text and one empty shape selected (`selectionStyle().canBindText`). */
  bindText(): void {
    this.inner.bindText();
  }

  /** "Unbind text": each selected shape's label becomes free text (`selectionStyle().canUnbindText`). */
  unbindText(): void {
    this.inner.unbindText();
  }

  /** "Wrap text in a container": a rectangle around each selected free text (`selectionStyle().hasFreeText`). */
  wrapTextInContainer(): void {
    this.inner.wrapTextInContainer();
  }

  /** Whether Tab has a closed shape, or a line or an unbound arrow, selected to switch. */
  canConvertSelection(): boolean {
    return this.inner.canConvertSelection();
  }

  /**
   * The type the panel shows pressed, or `null` when the selection disagrees on it or
   * holds nothing to switch.
   *
   * The engine's to answer, not the host's: for a line or an arrow it is a reading of
   * `roundness` and `elbowed` (`LinearType::of`, `engine/convert.rs`), and a host that
   * worked it out from the scene JSON would be running an engine formula in TypeScript.
   */
  sharedConversionType(): ConversionType | null {
    // wasm-bindgen hands an `Option<String>` back as a plain string, so the union is a
    // claim to check rather than a type it can carry. The engine writes only these seven
    // names (`ConvertTo::name`), and anything else is a binding that has drifted.
    const name = this.inner.sharedConversionType();
    return name !== undefined && isConversionType(name) ? name : null;
  }

  /** The shape switch opened: until `endConversion`, a label shrunk by a switch grows back. */
  beginConversion(): void {
    this.inner.beginConversion();
  }

  /** The shape switch closed. */
  endConversion(): void {
    this.inner.endConversion();
  }

  /**
   * Switches the selection to `to`, or a step round from the type it is at — forward for
   * Tab, back for Shift+Tab. One step of undo. Whether anything changed.
   *
   * A closed shape, a line or an unbound arrow, whichever the selection holds first: the
   * generic branch has preference (`ConvertElementTypePopup.tsx@1118751f:648-653`). Asking
   * for a type from the other family changes nothing (`isValidConversion`, `:929-950`).
   */
  convertSelection(to: ConversionType | null, forward = true): boolean {
    return this.inner.convertSelection(to ?? undefined, forward);
  }

  requestDraw(): void {
    /* WASM schedules its own rAF. */
  }

  exportJson(): string {
    return this.inner.exportJson();
  }

  /**
   * The drawing as an SVG, of the selection when `selectionOnly` says so.
   *
   * The margin is the engine's own now — `ExportOptions`'s 10, the same one the PNG uses —
   * and it used to be a `16` this signature took. §2 does not allow the front to pick a
   * margin: it made the two formats disagree by 6px per side, and there was no way to tell
   * from here which of the two was meant. `selectionOnly` is the whole of the front's
   * contribution; whether that means the scene, a selection or one frame is decided in Rust,
   * because an empty selection is the scene (`data/index.ts@1118751f:48-96`).
   */
  exportSvg(options: SvgExport = {}): string | null {
    return this.inner.exportSvg(options.selectionOnly) ?? null;
  }

  /**
   * The drawing as a PNG, of the selection when `selectionOnly` says so.
   *
   * The framing, the size and the background are the engine's — `ExportFrame` in
   * `crates/draw-engine/src/export/png.rs`, ported from the oracle's `exportToCanvas`
   * (`packages/excalidraw/scene/export.ts@1118751f:180-284`). Nothing is computed here and
   * the defaults are the binding's, so this is a forward and nothing else.
   *
   * The on-screen canvas is not what gets encoded. It never was meant to be: an export is
   * the scene, not wherever the camera happens to be, and a scene scrolled half off screen
   * used to export half a picture.
   *
   * `selectionOnly` is the dialog's checkbox and the whole of what the front chooses. It
   * does not say *which* thing to export: an empty selection is still the scene, and
   * selecting exactly one frame is a frame export, both decided in Rust.
   */
  exportPng(options: PngExport = {}): Promise<Blob | null> {
    const pending = this.inner.exportPng(
      options.scale,
      options.transparent,
      options.selectionOnly,
    );
    return pending === undefined ? Promise.resolve(null) : pending.then(encodedBlob);
  }

  /**
   * One clipboard copy: whether to write it, under what type, of what, and the payload.
   *
   * The engine decides all four (BUNNY.md §2) — the MIME type is `image/png` or
   * `text/plain` and never the host's to pick, and the scope is the export scope, so an
   * empty selection is still the whole scene and a lone selected frame is still a frame
   * export. The two booleans are the only thing the host contributes, and they are facts
   * about the browser rather than decisions: `supportsClipboardBlob` and
   * `supportsClipboardWriteText` in the oracle's own spelling (`clipboard.ts@1118751f:65-72`).
   *
   * `blob` is a promise for the raster and `text` a string for the vector; the format's
   * other payload is absent. The promise is passed through rather than awaited here, so
   * this stays a forward — and a host that writes it and finds the promise resolved to
   * `null` has the oracle's `CANVAS_POSSIBLY_TOO_BIG` (`data/blob.ts@1118751f:245-252`),
   * a canvas too large to encode, which is a browser's answer and not one to retry.
   */
  clipboardCopy(format: ClipboardFormatName, options: ClipboardSupport = {}): ClipboardCopy {
    const raw = this.inner.clipboardCopy(
      format,
      options.canWriteBlob ?? true,
      options.canWriteText ?? true,
    );
    return readClipboardCopy(raw, format);
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
  previewStyle(patch: StylePatch): void {
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

  /** Ctrl/Cmd+Arrow: preview a cluster of new nodes off the selected flowchart node. */
  flowchartCreate(direction: FlowchartDirection): void {
    this.inner.flowchartCreate(direction);
  }

  /** While Ctrl/Cmd is held, 1/2/3 picks the pending nodes' shape. */
  flowchartSetShape(shape: FlowchartShape): void {
    this.inner.flowchartSetShape(shape);
  }

  /** Releasing Ctrl/Cmd: commits the pending cluster as one step of history. */
  flowchartCommit(): void {
    this.inner.flowchartCommit();
  }

  /** Escape while creating: drops the pending cluster without a trace. */
  flowchartCancel(): void {
    this.inner.flowchartCancel();
  }

  isCreatingFlowchart(): boolean {
    return this.inner.isCreatingFlowchart();
  }

  /** The pending cluster, for host UI that needs it outside the canvas painter. */
  pendingFlowchartElements(): DrawElement[] {
    return parseJson<DrawElement[]>(this.inner.pendingFlowchartElementsJson(), []);
  }

  /** Alt+Arrow: selects the connected node in that direction. Returns its id, if any. */
  flowchartNavigate(direction: FlowchartDirection): string | null {
    return this.inner.flowchartNavigate(direction) ?? null;
  }

  /** Alt released: ends the exploration. */
  flowchartNavigationEnd(): void {
    this.inner.flowchartNavigationEnd();
  }
}
