# flowchart-oracle

Runs Excalidraw's own `flowchart.ts`, `App.flowchart.ts` and `viewport.ts`, unmodified,
under Node's type transforms (`--experimental-transform-types`: the oracle's classes use
parameter properties, which plain stripping refuses). `hooks.mjs` resolves the
`@excalidraw/*` workspace packages to their sources as the oracle's bundler does, and
defines `import.meta.env` as vitest does, so element ids come out `id0`, `id1`, ….
`package.json` pins the third-party packages at the versions the oracle pins; they are
installed here, outside the pnpm workspace.

```sh
# from the host repo; runs in mcr.microsoft.com/playwright:v1.63.0-noble
make oracle-fixtures
# or by hand, from this directory
npm ci --ignore-scripts
EXCALIDRAW_DIR=/path/to/excalidraw ORACLE_SHA=<pin> \
  node --experimental-transform-types --import ./register.mjs generate.mjs
```

`App` is a React component and cannot run here. `generate.mjs` stands in for the members
`AppFlowchart` calls; the two holding logic, `revealIfHidden` and `insertNewElements`, are
transcribed from `App.tsx` at the pin and call the oracle's own helpers. Nothing else is
shimmed.

The checkout must be at the pin. Before writing anything, the script replays the oracle's
own flowchart tests against itself and exits 2 if any fails. It writes
`crates/draw-engine/tests/fixtures/flowchart.oracle.json`: a sweep of Ctrl/Cmd+Arrow and
Alt+Arrow key sequences — shapes, sizes, styles, turned nodes, frames, zooms, chrome
offsets — with what the oracle holds after every key: the pending cluster, the scene, the
selection and where the camera lands. `tests/ci_flowchart_oracle.rs` replays it.

The oracle's flowchart arrows are elbow arrows, which this engine does not route yet, so
the replay leaves out each arrow's points, and its x/y and anchors where an end is on a
diamond or a turned node (the elbow-only snapping). Everything else is compared, bindings
included. The fixture's `note` says the same.

Output is deterministic. Regenerate only when the pin moves, never to turn a red test
green.
