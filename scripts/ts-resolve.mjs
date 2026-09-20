/**
 * Node test loader: resolve extensionless relative specifiers to .ts, matching
 * Vite/tsc. Source stays extensionless so the host app does not change.
 */
export async function resolve(specifier, context, nextResolve) {
  if ((specifier.startsWith(".") || specifier.startsWith("file:")) && !hasKnownExtension(specifier)) {
    for (const suffix of [".ts", "/index.ts"]) {
      try {
        return await nextResolve(`${specifier}${suffix}`, context);
      } catch (error) {
        if (error?.code !== "ERR_MODULE_NOT_FOUND") throw error;
      }
    }
  }
  return nextResolve(specifier, context);
}

function hasKnownExtension(specifier) {
  return [".ts", ".tsx", ".mts", ".js", ".mjs", ".cjs", ".json"].some((ext) => specifier.endsWith(ext));
}
