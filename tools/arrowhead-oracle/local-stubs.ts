// Stand-in for seven of `bounds.ts`'s own siblings in `packages/element/src`
// (`./shape`, `./linearElementEditor`, `./textElement`, `./typeChecks`, `./utils`,
// `./collision`, `./frame` — see `hooks.mjs`'s redirect table). None of their exports sit
// on `getArrowheadPoints` / `getArrowheadSize` / `getArrowheadAngle`'s call path — they
// exist only so the module graph links — so every one throws if actually called, the same
// reasoning as `common-shim.ts` and `math-shim.ts`.

const unreached = (name: string) => (..._args: any[]) => {
  throw new Error(
    `arrowhead-oracle: ${name} is a stub — getArrowheadPoints was not expected to call it`,
  );
};

// ./shape
export const generateRoughOptions = unreached("generateRoughOptions");
export const getElementShape = unreached("getElementShape");
export class ShapeCache {
  static get(..._args: any[]): any {
    throw new Error("arrowhead-oracle: ShapeCache is a stub — getArrowheadPoints was not expected to call it");
  }
}

// ./linearElementEditor
export class LinearElementEditor {
  static getNormalizeElementPointsAndCoords(..._args: any[]): any {
    throw new Error(
      "arrowhead-oracle: LinearElementEditor is a stub — getArrowheadPoints was not expected to call it",
    );
  }
}

// ./textElement
export const getBoundTextElement = unreached("getBoundTextElement");
export const getContainerElement = unreached("getContainerElement");

// ./typeChecks
export const isArrowElement = unreached("isArrowElement");
export const isBoundToContainer = unreached("isBoundToContainer");
export const isFrameLikeElement = unreached("isFrameLikeElement");
export const isFreeDrawElement = unreached("isFreeDrawElement");
export const isLinearElement = unreached("isLinearElement");
export const isLineElement = unreached("isLineElement");
export const isTextElement = unreached("isTextElement");
export const isExcalidrawElement = unreached("isExcalidrawElement");

// ./utils
export const deconstructDiamondElement = unreached("deconstructDiamondElement");
export const deconstructRectanguloidElement = unreached("deconstructRectanguloidElement");

// ./collision
export const intersectElementWithLineSegment = unreached("intersectElementWithLineSegment");

// ./frame
export const elementOverlapsWithFrame = unreached("elementOverlapsWithFrame");
export const getContainingFrame = unreached("getContainingFrame");
