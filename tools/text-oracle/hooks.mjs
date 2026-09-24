// Lets Node run Excalidraw's own TypeScript, unmodified, under type stripping.
//
// Two things stand between `textWrapping.ts` and `node`: the `@excalidraw/common` barrel,
// which drags in packages third_party/ never installs (nanoid, roughjs, tinycolor2), and
// extensionless relative imports (`./textMeasurements`), which bundlers accept and Node's
// ESM resolver does not. The first goes to `common-shim.ts`; the second is retried with
// `.ts`. The oracle's own files are never copied or patched — that would let it lie.

const SHIM = new URL("./common-shim.ts", import.meta.url).href;

export async function resolve(specifier, context, nextResolve) {
  if (specifier === "@excalidraw/common") {
    return { url: SHIM, shortCircuit: true };
  }
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    if (specifier.startsWith(".") && !/\.[cm]?[jt]s$/.test(specifier)) {
      return nextResolve(`${specifier}.ts`, context);
    }
    throw error;
  }
}
