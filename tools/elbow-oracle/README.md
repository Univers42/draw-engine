# elbow-oracle

Runs Excalidraw's own elbow-arrow router (`elbowArrow.ts`) and everything it imports,
unmodified, under Node. The router reaches most of `packages/element`, so this cannot
work like `text-oracle`, which shims one package. Instead, `hooks.mjs` resolves the
workspace aliases the way the oracle's `vitest.config.mts` does. It also accepts
extensionless specifiers, as a bundler does, and defines `import.meta.env` as Vite does.
Eight third-party packages are installed at the versions the pin's `package.json` names:
`package.json` lists them, and `package-lock.json` pins them.

```sh
# from the host repo; runs in mcr.microsoft.com/playwright:v1.63.0-noble
make oracle-fixtures
# or by hand, from this directory
npm ci --ignore-scripts
EXCALIDRAW_DIR=/path/to/excalidraw ORACLE_SHA=<pin> \
  node --experimental-transform-types --import ./register.mjs generate.mjs [seed]
```

`--experimental-transform-types` is needed because the oracle uses two TypeScript
constructs that type stripping cannot erase: an `enum` (`common/src/constants.ts:58`) and
parameter properties (`common/src/binary-heap.ts:4`). Node emits them itself.

The checkout must be at the pin. Before writing anything, the script replays the oracle's
own "can properly generate orthogonal arrow points" test and exits 2 if it fails. It
writes `crates/draw-engine/tests/fixtures/elbow.oracle.json`, which
`tests/ci_elbow_oracle.rs` replays within 1e-9.

The sweep has 2340 cases and 11176 steps: free ends, one end bound, both ends bound with
every heading pair, nested shapes, raw anchors, routes while dragging, fixed segments,
and shape moves. A few steps where the oracle throws on its own invariants are logged and
left out.

Output is deterministic. Regenerate only when the pin moves, never to turn a red test
green.
