/**
 * @osionos/draw-engine — a runtime-agnostic Canvas2D drawing engine (Excalidraw /
 * Figma / draw.io class), consumed by a host like a plugin. `core/**` is
 * framework-agnostic and DOM-free where possible (node-tested); `host/**` binds a
 * canvas in vanilla DOM. `react/**` is the shipped adapter (backward compatible);
 * `svelte/**` is an additive Svelte 5 adapter. Resolved by the
 * `@osionos/draw-engine` alias — no build step.
 */

// Camera
export {
  type Camera,
  type WorldBounds,
  IDENTITY,
  MAX_ZOOM,
  MIN_ZOOM,
  screenToWorld,
  worldToScreen,
} from "./core/camera/transform";
export { fitBounds, panBy, visibleWorldRect, zoomAt, zoomTo } from "./core/camera/controls";

// Scene model
export {
  ARROWHEADS,
  type Arrowhead,
  bumpVersion,
  createElement,
  DEFAULT_ELEMENT_STYLE,
  type DrawElement,
  type DrawElementStyle,
  type DrawElementType,
  type FillStyle,
  newElementId,
  type StrokeStyle,
} from "./core/scene/element";
export { Scene } from "./core/scene/scene";
export {
  distanceToSegment,
  elementBounds,
  hitTest,
  hitTestElement,
  normalizeRect,
  sceneBounds,
} from "./core/scene/geometry";
export {
  attachPoint,
  bindableAt,
  BINDING_GAP,
  isBindableElement,
  isLinearElement,
  linearEndpoints,
  refreshBindings,
} from "./core/scene/binding";

// Interaction
export { BBOX_SHAPE_TOOLS, type DrawTool, isLinearTool, isShapeTool, LINEAR_TOOLS, toolForKey } from "./core/interaction/tools";
export { isDegenerateRect, type Rect, rectFromDrag } from "./core/interaction/shapeDrag";
export { constrainToAngle, isDegenerateLinear, linearFromDrag } from "./core/interaction/linearDrag";
export { type SnapGuide, type SnapResult, snapMove } from "./core/interaction/snapping";

// Editing (pure cores behind the engine's clipboard/arrange/group ops)
export { expandForCopy, materializeElements, serializeSelection } from "./core/edit/clipboard";
export { reorderElements, type ZOrderMode } from "./core/edit/zorder";
export { type AlignMode, alignElements, distributeElements } from "./core/edit/align";
export { type FlipAxis, flipElements } from "./core/edit/flip";
export { expandToGroups, groupPatches, isSingleGroup, ungroupPatches } from "./core/edit/group";

// Render + engine
export { DARK_THEME, type DrawTheme, LIGHT_THEME } from "./core/render/paint";
export { DrawEngine, type DrawEngineOptions, type TextEditRequest } from "./core/engine";

// Export / import
export { elementsFromJson, type OsidrawFile, sceneToJson } from "./core/export/json";
export { sceneToSvg } from "./core/export/svg";

// Vanilla canvas host (any framework — or none)
export { bindCanvas, type BindCanvasArgs, type BindCanvasResult } from "./host/bindCanvas";
export type { DrawCanvasProps, HostCallbacks } from "./host/types";

// React adapter — the shipped public component. Svelte hosts import from
// "@osionos/draw-engine/svelte" instead, so this barrel never evaluates .svelte.
export { DrawCanvas } from "./react/DrawCanvas";
