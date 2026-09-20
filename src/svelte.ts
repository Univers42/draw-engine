/**
 * Svelte 5 adapter. Hosts with `@sveltejs/vite-plugin-svelte` import from
 * `@osionos/draw-engine/svelte`. Core/types stay on the main barrel.
 */

export { default as DrawCanvas } from "./svelte/DrawCanvas.svelte";
export type { DrawCanvasProps } from "./host/types";
