// Lets Node run the oracle's own `bounds.ts` — unmodified — far enough to call
// `getArrowheadPoints` / `getArrowheadSize` / `getArrowheadAngle`.
//
// `bounds.ts` is not a leaf module: it pulls in `@excalidraw/common`, `@excalidraw/math`,
// `@excalidraw/utils/shape`, and seven of `packages/element`'s own siblings (`./shape`,
// `./linearElementEditor`, `./textElement`, `./typeChecks`, `./utils`, `./collision`,
// `./frame`) — none of which this checkout has installed (`third_party/excalidraw` is a
// bare clone, no `yarn install`; see `make oracle`). Three of those are shimmed with the
// real, small, pure functions the three target exports actually call, copied from the
// pinned SHA exactly as `text-oracle/common-shim.ts` already does for its own two files;
// the rest are stand-ins that throw if anything calls them — nothing the three target
// exports do should reach past the shim layer, and a throw means that assumption broke
// louder than a silently-wrong number would.
//
// `roughjs` and `points-on-curve` are real, installed dependencies (`npm install`, this
// tool's own `package.json`) — not shimmed, because the whole point of this oracle is to
// feed `getArrowheadPoints` a `Drawable` the *real* `RoughGenerator` produced, exactly as
// `generateElementShape`/`getArrowheadShapes` do. Neither ships an "exports" map, so a
// deep subpath (`roughjs/bin/rough`, or roughjs's own internal `./scan-line-hachure`)
// fails Node's stricter ESM resolution the same way `text-oracle`'s and `rough-oracle`'s
// targets do; retried here with `.js` then `.ts`, matching both those hooks combined.

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const url = (name) => `file://${join(HERE, name)}`;

const REDIRECTS = {
  "@excalidraw/common": url("common-shim.ts"),
  "@excalidraw/math": url("math-shim.ts"),
  "@excalidraw/utils/shape": url("utils-shape-shim.ts"),
  "./shape": url("local-stubs.ts"),
  "./linearElementEditor": url("local-stubs.ts"),
  "./textElement": url("local-stubs.ts"),
  "./typeChecks": url("local-stubs.ts"),
  "./utils": url("local-stubs.ts"),
  "./collision": url("local-stubs.ts"),
  "./frame": url("local-stubs.ts"),
};

// `roughjs` and `points-on-curve` are real dependencies of *this tool*
// (`arrowhead-oracle/package.json`), not of `third_party/excalidraw` (a bare clone with no
// `node_modules` at all — see `make oracle`). Node resolves a bare specifier from the
// importing file's own location, and `bounds.ts` lives under `/excalidraw`, which has no
// `node_modules` up its whole tree. Rebasing `parentURL` to this hook's own file — inside
// `arrowhead-oracle/`, which does — is what lets `bounds.ts`'s `import rough from
// "roughjs/bin/rough"` resolve at all, unmodified.
const HERE_URL = import.meta.url;
const isRoughOrPointsOnCurve = (specifier) =>
  specifier === "roughjs" || specifier.startsWith("roughjs/") ||
  specifier === "points-on-curve" || specifier.startsWith("points-on-curve/");

export async function resolve(specifier, context, nextResolve) {
  const redirect = REDIRECTS[specifier];
  if (redirect) {
    return { url: redirect, shortCircuit: true };
  }
  const rebased = isRoughOrPointsOnCurve(specifier) ? { ...context, parentURL: HERE_URL } : context;
  try {
    return await nextResolve(specifier, rebased);
  } catch (error) {
    for (const ext of [".js", ".ts"]) {
      if (specifier.endsWith(ext)) continue;
      try {
        return await nextResolve(`${specifier}${ext}`, rebased);
      } catch {
        // try the next extension
      }
    }
    throw error;
  }
}
