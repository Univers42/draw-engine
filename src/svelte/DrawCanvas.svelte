<script lang="ts">
  /**
   * Svelte host for DrawEngine: mounts one <canvas>, tracks size via
   * ResizeObserver, and forwards pointer + keyboard input. Thin adapter — no
   * drawing or tool logic lives here; bindCanvas + the engine own it.
   */
  import { onMount } from "svelte";
  import type { DrawEngine } from "../core/engine";
  import { LIGHT_THEME } from "../core/render/paint";
  import { bindCanvas } from "../host/bindCanvas";
  import type { DrawCanvasProps, HostCallbacks } from "../host/types";

  let {
    scene,
    theme = LIGHT_THEME,
    defaultStroke,
    className = "",
    ariaLabel = "Drawing canvas",
    onReady,
    onCameraChange,
    onToolChange,
    onSelectionChange,
    onRequestTextEdit,
    onSceneChange,
    onContextMenu,
    onToolLockChange,
  }: DrawCanvasProps = $props();

  let containerEl: HTMLDivElement | undefined;
  let canvasEl: HTMLCanvasElement | undefined;
  let engine: DrawEngine | null = null;
  let ready = $state(false);

  const callbacks: HostCallbacks = {};

  $effect(() => {
    callbacks.onCameraChange = onCameraChange;
    callbacks.onToolChange = onToolChange;
    callbacks.onSelectionChange = onSelectionChange;
    callbacks.onRequestTextEdit = onRequestTextEdit;
    callbacks.onSceneChange = onSceneChange;
    callbacks.onContextMenu = onContextMenu;
    callbacks.onToolLockChange = onToolLockChange;
  });

  onMount(() => {
    if (!canvasEl || !containerEl) return;
    const bound = bindCanvas({ canvas: canvasEl, container: containerEl, callbacks, onReady });
    engine = bound.engine;
    ready = true;
    return () => {
      ready = false;
      engine = null;
      bound.destroy();
    };
  });

  $effect(() => {
    if (!ready) return;
    if (scene) engine?.setScene(scene);
  });

  $effect(() => {
    if (!ready) return;
    engine?.setTheme(theme);
  });

  $effect(() => {
    if (!ready) return;
    if (defaultStroke) engine?.setNextStyle({ strokeColor: defaultStroke });
  });
</script>

<div
  bind:this={containerEl}
  class={className}
  role="application"
  aria-label={ariaLabel}
  tabindex={0}
  style="position: relative; width: 100%; height: 100%; touch-action: none; outline: none;"
>
  <canvas
    bind:this={canvasEl}
    aria-hidden="true"
    tabindex={-1}
    style="display: block; width: 100%; height: 100%;"
  ></canvas>
</div>
