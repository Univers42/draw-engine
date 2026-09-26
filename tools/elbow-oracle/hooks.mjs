// Lets Node run Excalidraw's own TypeScript, unmodified, as the oracle's bundler would.
//
// Three things stand between `packages/element/src/elbowArrow.ts` and `node`, and each
// is something Vite does for the oracle at build time — none of them is a change to what
// the oracle computes:
//
// 1. Workspace aliases. `@excalidraw/common`, `/math`, `/element`, … are the monorepo's
//    own packages, resolved to their `src/` here exactly as its `vitest.config.mts` does.
//    Third-party imports resolve to `node_modules/` in this directory, installed at the
//    versions the pin's `packages/*/package.json` name (see package.json).
// 2. Extensionless specifiers (`./binding`, `roughjs/bin/rough`), which bundlers accept
//    and Node's ESM resolver does not: retried with `.ts`, `/index.ts`, then `.js`.
// 3. `import.meta.env`, which Vite defines and Node leaves undefined — `elbowArrow.ts:926`
//    reads `import.meta.env.PROD`. Assigned at the head of each oracle module, on the
//    same line as its first statement so no line number moves: development mode, the
//    oracle's invariants armed, as the text oracle arms `isDevEnv`.
//
// The two TypeScript constructs Node's type stripping cannot erase (`enum`, parameter
// properties — `common/src/constants.ts:58`, `common/src/binary-heap.ts:4`) are emitted
// by Node itself under `--experimental-transform-types`. The oracle's own files are never
// copied or patched on disk.

import { pathToFileURL } from "node:url";
import { join } from "node:path";

const DIR = process.env.EXCALIDRAW_DIR;
if (!DIR) {
  throw new Error("EXCALIDRAW_DIR is not set");
}
const ROOT = pathToFileURL(`${DIR.replace(/\/$/, "")}/`).href;

const WORKSPACE = {
  "@excalidraw/common": "packages/common/src",
  "@excalidraw/math": "packages/math/src",
  "@excalidraw/element": "packages/element/src",
  "@excalidraw/utils": "packages/utils/src",
  "@excalidraw/excalidraw": "packages/excalidraw",
  "@excalidraw/fractional-indexing": "packages/fractional-indexing/src",
  "@excalidraw/laser-pointer": "packages/laser-pointer/src",
};

const ENV = JSON.stringify({
  MODE: "development",
  DEV: true,
  PROD: false,
  SSR: false,
});

const workspaceTarget = (specifier) => {
  for (const [name, dir] of Object.entries(WORKSPACE)) {
    if (specifier === name) {
      return new URL(`${dir}/index.ts`, ROOT).href;
    }
    if (specifier.startsWith(`${name}/`)) {
      return new URL(join(dir, specifier.slice(name.length + 1)), ROOT).href;
    }
  }
  return null;
};

const CANDIDATES = ["", ".ts", "/index.ts", ".js", "/index.js"];

async function resolveFirst(base, context, nextResolve) {
  let lastError;
  for (const suffix of CANDIDATES) {
    try {
      return await nextResolve(`${base}${suffix}`, context);
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError;
}

export async function resolve(specifier, context, nextResolve) {
  const workspace = workspaceTarget(specifier);
  if (workspace) {
    return resolveFirst(workspace, context, nextResolve);
  }
  if (specifier.startsWith(".") || specifier.startsWith("file:")) {
    return resolveFirst(specifier, context, nextResolve);
  }
  // A bare third-party specifier, from wherever it is imported: resolved as if imported
  // from here, so this directory's node_modules — the only place the pinned versions
  // live — is the one searched, with each package's own `exports`/`main`.
  const here = { ...context, parentURL: import.meta.url };
  try {
    return await nextResolve(specifier, here);
  } catch {
    return resolveFirst(specifier, here, nextResolve);
  }
}

export async function load(url, context, nextLoad) {
  const result = await nextLoad(url, context);
  if (url.startsWith(ROOT) && /\.tsx?$/.test(url) && result.source) {
    const source = String(result.source);
    if (source.includes("import.meta.env")) {
      return { ...result, source: `import.meta.env = ${ENV};${source}` };
    }
  }
  return result;
}
