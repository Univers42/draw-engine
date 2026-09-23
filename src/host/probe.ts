/**
 * Host-side counters for the debug snapshot.
 *
 * Three of the things worth measuring cannot be seen from inside the engine, so they are
 * measured here instead:
 *
 * - **Pointer frequency.** The engine only ever sees one move per animation frame,
 *   because `pointerInput` coalesces them. So the engine cannot tell a device sending 60
 *   events a second from one sending 1000 — and the difference is the whole cost of a
 *   high-rate mouse.
 * - **Hit-test duration.** The engine is runtime-agnostic and has no clock; `now_ms` is
 *   handed to it. Timing its calls from the outside keeps it that way.
 * - **The coalescing ratio itself.** `pointerEvents` against `engineSteps` says whether
 *   the frame-coalescing is working, which is invisible from either side alone.
 *
 * Page-wide rather than per-engine, deliberately: this is a debugging aid, and a page
 * with two canvases on it is not a case worth carrying a registry for. A snapshot taken
 * with two engines mounted sums them, which is worth knowing before reading one.
 *
 * Costs one increment on the pointer path and one `performance.now()` pair per hit test.
 * Both are cheap enough to leave on, and the whole surface is behind the DEV gate that
 * already hides `window.__drawEngine`.
 */

export interface HostProbe {
  /** Raw `pointermove` events the canvas received. */
  pointerEvents: number;
  /** Moves actually forwarded to the engine, after per-frame coalescing. */
  engineSteps: number;
  hitTests: number;
  /** Cumulative, so two reads either side of a gesture give that gesture's cost. */
  hitTestMs: number;
  /** Longest single hit test seen. A linear scan over a large board lives here. */
  maxHitTestMs: number;
}

const probe: HostProbe = {
  pointerEvents: 0,
  engineSteps: 0,
  hitTests: 0,
  hitTestMs: 0,
  maxHitTestMs: 0,
};

export function countPointerEvent(): void {
  probe.pointerEvents += 1;
}

export function countEngineStep(): void {
  probe.engineSteps += 1;
}

/**
 * Times `run` as a hit test and returns its result.
 *
 * A wrapper rather than paired start/stop calls so a caller cannot forget the stop and
 * leave the counter permanently wrong — which is the usual way instrumentation starts
 * lying.
 */
export function timeHitTest<T>(run: () => T): T {
  const started = performance.now();
  try {
    return run();
  } finally {
    const took = performance.now() - started;
    probe.hitTests += 1;
    probe.hitTestMs += took;
    if (took > probe.maxHitTestMs) probe.maxHitTestMs = took;
  }
}

/** A copy, so a caller cannot mutate the counters by holding onto the result. */
export function readProbe(): HostProbe {
  return { ...probe };
}

/** Zeroes the counters, for measuring one gesture rather than a whole session. */
export function resetProbe(): void {
  probe.pointerEvents = 0;
  probe.engineSteps = 0;
  probe.hitTests = 0;
  probe.hitTestMs = 0;
  probe.maxHitTestMs = 0;
}
