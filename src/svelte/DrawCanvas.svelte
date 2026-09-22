<script lang="ts">
  /**
   * Svelte host for DrawEngine: mounts one <canvas>, tracks size via
   * ResizeObserver, and forwards pointer + keyboard input. Thin adapter — no
   * drawing or tool logic lives here; bindCanvas + the engine own it.
   */
  import { onMount } from "svelte";
  import type { DrawEngine } from "../engine";
  import { LIGHT_THEME } from "../types";
  import { bindCanvasAsync } from "../host/bindCanvas";
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
    onNotice,
    onContextMenu,
    onToolLockChange,
    onPointerDown,
    onPointerMove,
    onPointerUp,
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
    callbacks.onNotice = onNotice;
    callbacks.onContextMenu = onContextMenu;
    callbacks.onToolLockChange = onToolLockChange;
    callbacks.onPointerDown = onPointerDown;
    callbacks.onPointerMove = onPointerMove;
    callbacks.onPointerUp = onPointerUp;
  });

  onMount(() => {
    if (!canvasEl || !containerEl) return;
    let destroy: (() => void) | undefined;
    let cancelled = false;
    void bindCanvasAsync({ canvas: canvasEl, container: containerEl, callbacks, onReady }).then((bound) => {
      if (cancelled) {
        bound.destroy();
        return;
      }
      engine = bound.engine;
      destroy = bound.destroy;
      ready = true;
    });
    return () => {
      cancelled = true;
      ready = false;
      engine = null;
      destroy?.();
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

<!--
  The rule assumes a nonnegative tabindex on a non-widget role is a mistake. Here it is
  the point: this is the drawing surface, it carries `role="application"` precisely
  because it handles its own keys, and every shortcut — tool selection, delete, nudge,
  undo — needs it focusable. Removing the tabindex would make the whole board
  keyboard-inaccessible, which is the opposite of what the rule is for.
-->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  bind:this={containerEl}
  class={className}
  role="application"
  aria-label={ariaLabel}
  tabindex={0}
  style="position: relative; width: 100%; height: 100%; touch-action: none; outline: none;"
>
  <!--
    No tabindex. `tabindex="-1"` is not "unfocusable" — it only removes an element from
    tab order while leaving it focusable by pointer, so a click focused this canvas and
    took focus straight back off the container `onPointerDown` had just focused. Focus
    then sat on an `aria-hidden` node: a screen reader announces nothing, and the
    `role="application"` label never reaches the user. Shortcuts still worked by
    bubbling, which is what hid this.
  -->
  <canvas
    bind:this={canvasEl}
    aria-hidden="true"
    style="display: block; width: 100%; height: 100%;"
  ></canvas>
</div>
