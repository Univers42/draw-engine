/**
 * @osionos/draw-engine — Canvas2D drawing engine. Rust/WASM owns the scene,
 * paint loop, and tools. TypeScript is the host glue (bindCanvas, React, Svelte)
 * plus the public type barrel. Call `loadDrawEngine()` before constructing
 * DrawEngine (DrawCanvas does this on mount).
 */

export {
  type Camera,
  type WorldBounds,
  IDENTITY,
  MAX_ZOOM,
  MIN_ZOOM,
} from "./types";
export { fitBounds, panBy, screenToWorld, visibleWorldRect, worldToScreen, zoomAt, zoomTo } from "./camera";

export {
  ARROWHEADS,
  type Arrowhead,
  DEFAULT_ELEMENT_STYLE,
  type DrawElement,
  type DrawElementStyle,
  type DrawElementType,
  type FillStyle,
  type StrokeStyle,
  Scene,
  DARK_THEME,
  type DrawTheme,
  LIGHT_THEME,
  type DrawEngineOptions,
  type TextEditRequest,
  type DrawTool,
  type ZOrderMode,
  type AlignMode,
  type FlipAxis,
  type OsidrawFile,
} from "./types";
export { bumpVersion, createElement, elementsFromJson, isBindableElement, isLinearElement, newElementId, sceneToJson } from "./json";
export { BBOX_SHAPE_TOOLS, isLinearTool, isShapeTool, LINEAR_TOOLS, toolForKey } from "./tools";

export { DrawEngine, loadDrawEngine } from "./engine";

export { bindCanvas, bindCanvasAsync, type BindCanvasArgs, type BindCanvasResult } from "./host/bindCanvas";
export type { DrawCanvasProps, HostCallbacks } from "./host/types";

export { DrawCanvas } from "./react/DrawCanvas";
