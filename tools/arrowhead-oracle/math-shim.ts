// Stand-in for `@excalidraw/math`, holding only the value imports `bounds.ts` takes from
// it (its `type` imports — `Curve`, `Degrees`, `GlobalPoint`, ... — are erased by Node's
// type stripping and need no runtime export at all).
//
// `pointFrom`, `pointFromArray`, `pointRotateRads` and `degreesToRadians` are copied
// verbatim from the pinned SHA (`packages/math/src/point.ts@1118751f:22-42,50-56,125-139`,
// `packages/math/src/angle.ts@1118751f:29-31`) — every one of them is on
// `getArrowheadPoints`'s own call path. `lineSegment` is not; it throws instead, the same
// reasoning as `common-shim.ts`.

export function pointFrom(xOrCoords: any, y?: number): any {
  return typeof xOrCoords === "object"
    ? [xOrCoords.x, xOrCoords.y]
    : [xOrCoords, y];
}

export function pointFromArray(numberArray: number[]): any {
  return numberArray.length === 2 ? pointFrom(numberArray[0], numberArray[1]) : undefined;
}

export function pointRotateRads(point: any, center: any, angle: number): any {
  if (!angle) {
    return point;
  }
  const [x, y] = point;
  const [cx, cy] = center;
  return pointFrom(
    (x - cx) * Math.cos(angle) - (y - cy) * Math.sin(angle) + cx,
    (x - cx) * Math.sin(angle) + (y - cy) * Math.cos(angle) + cy,
  );
}

export function degreesToRadians(degrees: number): number {
  return (degrees * Math.PI) / 180;
}

export function lineSegment(..._args: any[]): any {
  throw new Error(
    "arrowhead-oracle: @excalidraw/math's lineSegment is a stub — getArrowheadPoints was not expected to call it",
  );
}
