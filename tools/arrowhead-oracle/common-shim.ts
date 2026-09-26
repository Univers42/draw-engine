// Stand-in for `@excalidraw/common`, holding only what `bounds.ts` imports from it.
// `invariant` is copied verbatim from `packages/common/src/utils.ts@1118751f:859-863` —
// `getArrowheadPoints` actually calls it (the `data.length === 6` assertion), so it has to
// behave exactly like the real one. The other three are never reached by
// `getArrowheadPoints` / `getArrowheadSize` / `getArrowheadAngle`; each throws instead of
// silently returning something plausible, so a wrong assumption fails loudly here rather
// than producing a quietly-wrong fixture.

export function invariant(condition: any, message: string): asserts condition {
  if (!condition) {
    throw new Error(message);
  }
}

const unreached = (name: string) => (..._args: any[]) => {
  throw new Error(
    `arrowhead-oracle: @excalidraw/common's ${name} is a stub — getArrowheadPoints was not expected to call it`,
  );
};

export const arrayToMap = unreached("arrayToMap");
export const rescalePoints = unreached("rescalePoints");
export const sizeOf = unreached("sizeOf");
