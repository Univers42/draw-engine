// Lets Node run Excalidraw's own TypeScript, unmodified, under type stripping.
//
// Three things stand between `flowchart.ts` and `node`, each answered here the way the
// oracle's own bundler (Vite) answers it — never by copying or patching an oracle file,
// which would let it lie:
//
// 1. The `@excalidraw/*` workspace packages, which the oracle's tsconfig/vite aliases map
//    to their `src/`. Resolved to the same files here.
// 2. Extensionless relative imports (`./binding`), which bundlers accept and Node's ESM
//    resolver does not — retried with `.ts`, then `/index.ts`; and roughjs's `bin/`, whose
//    own relative imports lack `.js` (see ../rough-oracle/extensionless.mjs).
// 3. `import.meta.env`, which Vite defines at build time and Node never does. Supplied as
//    vitest sets it — MODE "test", DEV, not PROD — the environment the oracle's own
//    flowchart tests run in: element ids come out as `id0`, `id1`, …, and every dev-only
//    invariant (`heading.ts`, `elbowArrow.ts`, `Scene.ts`) stays armed. It is prepended on
//    line 1 without a newline, so every line number in a stack trace is still the file's.
//
// Third-party packages resolve from this directory's node_modules, installed at the exact
// versions the oracle pins (package.json).

import { readFile } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";

const DIR = process.env.EXCALIDRAW_DIR;
if (!DIR) throw new Error("[flowchart-oracle] EXCALIDRAW_DIR is not set");
const ROOT = pathToFileURL(`${DIR.replace(/\/$/, "")}/`).href;
const HERE = new URL("./", import.meta.url).href;

const WORKSPACE = {
  "@excalidraw/common": "packages/common/src/",
  "@excalidraw/math": "packages/math/src/",
  "@excalidraw/element": "packages/element/src/",
  "@excalidraw/utils": "packages/utils/src/",
  "@excalidraw/excalidraw": "packages/excalidraw/",
  "@excalidraw/fractional-indexing": "packages/fractional-indexing/src/",
  "@excalidraw/laser-pointer": "packages/laser-pointer/src/",
};

const ENV = 'import.meta.env = { MODE: "test", DEV: true, PROD: false };';

async function withSuffixes(base, context, nextResolve, suffixes) {
  let lastError;
  for (const suffix of suffixes) {
    try {
      return await nextResolve(`${base}${suffix}`, context);
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError;
}

export async function resolve(specifier, context, nextResolve) {
  const pkg = Object.keys(WORKSPACE).find(
    (name) => specifier === name || specifier.startsWith(`${name}/`),
  );
  if (pkg) {
    const rest = specifier.slice(pkg.length + 1);
    const base = new URL(`${WORKSPACE[pkg]}${rest}`, ROOT).href.replace(/\/$/, "");
    return withSuffixes(base, context, nextResolve, rest ? ["", ".ts", "/index.ts"] : ["/index.ts"]);
  }
  if (specifier.startsWith(".")) {
    try {
      return await nextResolve(specifier, context);
    } catch (error) {
      if (/\.[cm]?[jt]sx?$/.test(specifier)) throw error;
      return withSuffixes(specifier, context, nextResolve, [".ts", "/index.ts", ".js"]);
    }
  }
  if (/^[a-z@]/i.test(specifier) && !specifier.startsWith("node:")) {
    // A bare package imported from the oracle's tree: from this tool's node_modules.
    try {
      return await nextResolve(specifier, context);
    } catch {
      return withSuffixes(specifier, { ...context, parentURL: HERE }, nextResolve, ["", ".js"]);
    }
  }
  return nextResolve(specifier, context);
}

export async function load(url, context, nextLoad) {
  if (url.startsWith(ROOT) && url.endsWith(".ts")) {
    const source = await readFile(fileURLToPath(url), "utf8");
    if (source.includes("import.meta.env")) {
      return { format: "module-typescript", source: ENV + source, shortCircuit: true };
    }
  }
  return nextLoad(url, context);
}
