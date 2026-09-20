# @osionos/draw-engine

Canvas2D drawing engine (Excalidraw / Figma class): scene model, dirty-driven rAF renderer, camera, tools, bindings.

The paint loop is vanilla TypeScript in `src/core/**`. A UI framework does not draw shapes. No speed claim for Svelte vs React is measured here (`agents/benchmarker.md`: no baseline, no number).

## Adapters (hexagonal)

```
react/DrawCanvas.tsx  ──┐
svelte/DrawCanvas.svelte ─┼──► host/bindCanvas ──► core/DrawEngine
vanilla bindCanvas()  ──┘
```

- `src/core/**` — engine, scene, render, interaction (framework-free)
- `src/host/**` — one bind primitive: pointer, keyboard (`dispatchKeyDown`), resize
- `src/react/DrawCanvas.tsx` — **shipped** public component (`import { DrawCanvas } from "@osionos/draw-engine"`)
- `src/svelte/DrawCanvas.svelte` — additive (`import { DrawCanvas } from "@osionos/draw-engine/svelte"`)

React stays on the barrel because osionos (and any existing host) imports `DrawCanvas` from `@osionos/draw-engine`. Deleting it is a public-surface break.

## Toolchain

pnpm only (`packageManager`: `pnpm@10.32.1`). npm/yarn are rejected at install. Scripts self-Dockerize; host Node is not used.

```sh
make help      # targets
make test      # node:test suite in Docker
make quality   # same gate, via pnpm
make lock      # regenerate pnpm-lock.yaml in Docker
make shell     # container shell
make clean     # containers, volumes, local image
```

Equivalent: `pnpm test` / `pnpm quality` (both call `scripts/docker-run.sh`).

`node:test` covers `src/**/*.test.ts` and `tests/**/*.test.ts`:

- `tests/draw-engine.test.ts` — camera, scene, geometry, handles, history, json/svg
- `tests/draw-edit.test.ts` — clipboard, z-order, align, flip, group, snap
- `tests/draw-binding.test.ts` — connectors, bindings, labels
- `src/host/keys.test.ts` — host keyboard chords

Playwright specs (`drawEditing.spec.mjs`, `drawDiagram.spec.mjs`) stay in osionos: they drive the app UI, not the engine.
