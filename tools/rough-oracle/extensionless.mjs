// roughjs 4.6.4 ships `bin/` as ESM but with extensionless relative imports
// (`./scan-line-hachure`, `./hachure-filler`), which Node's ESM resolver rejects.
// Bundlers tolerate it, Node does not.
//
// Rather than patch files inside node_modules — which would silently un-pin the oracle
// and make it lie — this hook retries a failed relative resolve with `.js` appended.
// The module graph stays exactly the published one.

export async function resolve(specifier, context, nextResolve) {
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    if (specifier.startsWith(".") && !specifier.endsWith(".js")) {
      return nextResolve(`${specifier}.js`, context);
    }
    throw error;
  }
}
