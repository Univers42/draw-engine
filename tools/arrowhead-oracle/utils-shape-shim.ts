// Stand-in for `@excalidraw/utils/shape`, holding only `getCurvePathOps` — the one export
// `bounds.ts` takes from it, and the one `getArrowheadPoints` opens with. Copied verbatim
// from `packages/utils/src/shape.ts@1118751f:199-211`.

export const getCurvePathOps = (shape: any): any[] => {
  if (!shape) {
    return [];
  }
  for (const set of shape.sets) {
    if (set.type === "path") {
      return set.ops;
    }
  }
  return shape.sets[0].ops;
};
