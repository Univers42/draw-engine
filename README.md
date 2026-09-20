# @osionos/draw-engine

Canvas2D drawing engine (Excalidraw / Figma class): scene model, dirty-driven rAF renderer, camera, tools, bindings.

The core lives in Rust (`crates/draw-engine`), compiled to WASM. TypeScript is host glue only — React/Svelte/vanilla bind a canvas; they do not own the scene. No compile-time or paint speedup is claimed without a measured artifact (see Timings).

## Adapters (hexagonal)

```
react/DrawCanvas.tsx  ──┐
svelte/DrawCanvas.svelte ─┼──► host/bindCanvas ──► WASM DrawEngine
vanilla bindCanvas()  ──┘
```

- `crates/draw-engine` — camera, scene, geometry, edit, history, JSON/SVG, paint loop
- `src/host/**` — one bind primitive: pointer, keyboard (`dispatchKeyDown`), resize
- `src/react/DrawCanvas.tsx` — **shipped** public component (`import { DrawCanvas } from "@osionos/draw-engine"`)
- `src/svelte/DrawCanvas.svelte` — additive (`import { DrawCanvas } from "@osionos/draw-engine/svelte"`)
- `src/index.ts` — public TS contract (types, `DrawEngine`, `DrawCanvas`, `.osidraw`)

`DrawCanvas` calls `loadDrawEngine()` on mount (WASM init is async). Vanilla hosts must `await loadDrawEngine()` before `new DrawEngine` / `bindCanvas`, or use `bindCanvasAsync`.

React stays on the barrel because osionos (and any existing host) imports `DrawCanvas` from `@osionos/draw-engine`. Deleting it is a public-surface break.

## Toolchain

pnpm only (`packageManager`: `pnpm@10.32.1`). npm/yarn are rejected at install. Scripts self-Dockerize; host Node/Rust is not required.

```sh
make help      # targets
make test      # cargo test + host node:test in Docker
make quality   # rustfmt, clippy -D warnings, cargo test, host node:test
make wasm      # wasm-bindgen build into pkg/
make lock      # regenerate pnpm-lock.yaml in Docker
make shell     # container shell
make clean     # containers, volumes, local image
```

Equivalent: `pnpm test` / `pnpm quality` (both call `scripts/docker-run.sh`).

Tests:

- `crates/draw-engine/tests/draw_engine.rs` — camera, scene, geometry, handles, history, json/svg
- `crates/draw-engine/tests/draw_edit.rs` — clipboard, z-order, align, flip, group, snap
- `crates/draw-engine/tests/draw_binding.rs` — connectors, bindings, labels
- `src/host/keys.test.ts` — host keyboard chords

Playwright specs (`drawEditing.spec.mjs`, `drawDiagram.spec.mjs`) stay in osionos: they drive the app UI, not the engine. osionos still vendors a copy at `packages/draw-engine`; switching the consumer alias is a follow-up.

## Timings (measured, not claimed)

Recorded on this machine after the WASM cutover. These are not a speedup claim versus the old TypeScript core (that path had no compile step — `node --experimental-strip-types`).

| Command | Wall time |
| --- | --- |
| `cargo test --workspace` (warm cache) | ~0.4s |
| `cargo test --workspace` (rebuild crate in Docker, crates.io already cached) | ~9.3s compile + test |
| `cargo build --release --target wasm32-unknown-unknown` (warm cache) | ~0.2s |
| `cargo build --release --target wasm32-unknown-unknown` (incremental after WASM API change) | ~3.2s |
| `src/host/**/*.test.ts` (node:test) | ~0.3s |

`quality.sh --with-tests --no-audit` is green (rustfmt, clippy, shellcheck, `make test`). `npm audit` is skipped: this package is pnpm-only and has no `package-lock.json`. Paint vs the deleted TS engine is not measured here: a naive WASM `stroke`/`fill` is a hop into JS, so runtime can be slower until profiled on a canvas. Do not advertise “faster” without that artifact.
