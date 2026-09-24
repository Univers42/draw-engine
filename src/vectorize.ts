/**
 * Tracing a picture into vectors, off the main thread.
 *
 * The tracer (`crates/draw-trace`, built to `pkg/draw_trace*`) is its own WASM module and
 * only ever loads inside the Web Worker in `vectorize.worker.ts`, so a board that never
 * vectorizes anything never downloads it, and the engine's own module does not grow by
 * a byte. A trace is seconds of solid work on a large photo; in the worker it cannot
 * freeze the board, and cancelling one is terminating the worker — there is no other
 * way to stop a thread that is busy.
 *
 * The result goes on the board through `DrawEngine.vectorizeImage`, in one commit.
 */

/** The tracer's starting points (`crates/draw-trace/src/config.rs`). */
export type TracePreset = "bw" | "poster" | "photo";

/** How curves are fitted: pixel steps, straight segments, or smooth splines. */
export type TraceMode = "pixel" | "polygon" | "spline";

/** A trace request: a preset, and whichever of its dials were moved. */
export interface TraceConfig {
  preset: TracePreset;
  /** Significant bits per colour channel, 1–8: fewer merges more colours. */
  colorPrecision?: number;
  /** Colour distance between stacked layers, 0–255: larger makes fewer layers. */
  layerDifference?: number;
  /** Patches smaller than this many pixels are dropped as noise. */
  filterSpeckle?: number;
  /** Degrees past which a turn is kept as a corner rather than smoothed, 0–180. */
  cornerThreshold?: number;
  mode?: TraceMode;
}

/** What a trace would cost, for the dialog's stats line (`TraceStats` in draw-trace). */
export interface TraceStats {
  /** Elements an editable insert makes — one per ring. */
  shapes: number;
  /** Colour regions the tracer found. */
  regions: number;
  /** Points across every ring. */
  points: number;
  /** Bytes of the picture, before base64. */
  svgBytes: number;
  width: number;
  height: number;
}

export type TracePhase = "segment" | "compose" | "optimize";

export interface TraceProgress {
  phase: TracePhase;
  /** How far through `phase`, 0–1. */
  fraction: number;
}

/** A finished trace: its cost, and the picture the preview shows. */
export interface TraceResult {
  stats: TraceStats;
  svg: string;
}

/** What the board accepts, from `packages/contract/src/limits.ts`. */
export interface VectorizeLimits {
  maxElements: number;
  maxPointsPerElement: number;
  maxTraceShapes: number;
  maxDataUrlLength: number;
  maxGroupDepth: number;
}

export interface VectorizeOptions {
  /** Leave the traced image under its trace. */
  keepOriginal: boolean;
  limits: VectorizeLimits;
}

/**
 * A trace's editable rings, flat (`TraceRings` in `engine/vectorize.rs`): ring `i` is
 * `lengths[i]` points in colour `colours[i]` (`0xRRGGBB`), taken in turn from `coords` as
 * `x, y` pairs, each a fraction of the traced picture.
 */
export interface TraceRings {
  colours: Uint32Array;
  lengths: Uint32Array;
  coords: Float64Array;
}

/** What goes on the board: the editable rings, or the picture. */
export type VectorizeInsert =
  | { as: "shapes"; rings: TraceRings }
  | { as: "picture"; dataUrl: string };

/** Why the engine inserted nothing (`VectorizeRefusal` in `engine/vectorize.rs`). */
export type VectorizeRefusal =
  | "not-an-image"
  | "locked"
  | "held"
  | "empty"
  | "malformed"
  | "too-many-shapes"
  | "board-full"
  | "not-a-picture"
  | "too-large";

export type VectorizeOutcome = { ids: string[] } | { refused: VectorizeRefusal };

/** The SVG as an image `data:` URL, which is how a picture element carries it. */
export function svgDataUrl(svg: string): string {
  const bytes = new TextEncoder().encode(svg);
  let binary = "";
  // In slices: `String.fromCharCode(...bytes)` on megabytes overflows the call stack.
  for (let at = 0; at < bytes.length; at += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  }
  return `data:image/svg+xml;base64,${btoa(binary)}`;
}

/** Raised by a request the worker was terminated under. */
export class TraceCancelled extends Error {
  constructor() {
    super("the trace was cancelled");
    this.name = "TraceCancelled";
  }
}

/** Messages to the worker. */
export type TraceRequest =
  | { id: number; type: "open"; bitmap: ImageBitmap }
  | { id: number; type: "render"; config: TraceConfig }
  | { id: number; type: "rings" };

/** Messages from the worker. */
export type TraceReply =
  | { id: number; type: "progress"; progress: TraceProgress }
  | { id: number; type: "done"; value: unknown }
  | { id: number; type: "error"; message: string };

interface Waiting {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

/**
 * One picture in one worker, traced as often as the dials move.
 *
 * Renders run one at a time, and a render asked for while one is running waits — and is
 * replaced by any newer one, whose settings are the only ones anybody still wants. A
 * replaced render resolves `null`.
 */
export class TraceWorker {
  private readonly worker: Worker;
  private readonly waiting = new Map<number, Waiting>();
  private nextId = 0;
  private busy = false;
  private queued: {
    config: TraceConfig;
    resolve: (result: TraceResult | null) => void;
    reject: (error: Error) => void;
  } | null = null;
  private terminated = false;

  private constructor(onProgress: (progress: TraceProgress) => void) {
    this.worker = new Worker(new URL("./vectorize.worker.ts", import.meta.url), {
      type: "module",
    });
    this.worker.onmessage = (event: MessageEvent<TraceReply>) => {
      const reply = event.data;
      if (reply.type === "progress") {
        onProgress(reply.progress);
        return;
      }
      const waiting = this.waiting.get(reply.id);
      if (!waiting) return;
      this.waiting.delete(reply.id);
      if (reply.type === "done") waiting.resolve(reply.value);
      else waiting.reject(new Error(reply.message));
    };
    this.worker.onerror = (event) => {
      event.preventDefault();
      this.fail(new Error(event.message || "the tracer failed to load"));
    };
  }

  /**
   * A worker holding `bitmap`'s pixels. The bitmap is copied, not transferred, so the
   * caller can open another worker over it after a cancel.
   */
  static async open(
    bitmap: ImageBitmap,
    onProgress: (progress: TraceProgress) => void,
  ): Promise<TraceWorker> {
    const tracer = new TraceWorker(onProgress);
    await tracer.request({ type: "open", bitmap });
    return tracer;
  }

  /** Traces with `config`; `null` when a newer render replaced this one first. */
  render(config: TraceConfig): Promise<TraceResult | null> {
    if (!this.busy) return this.run(config);
    this.queued?.resolve(null);
    return new Promise((resolve, reject) => {
      this.queued = { config, resolve, reject };
    });
  }

  /** The last trace's rings, as `DrawEngine.vectorizeImage` takes them. */
  async rings(): Promise<TraceRings> {
    const rings = (await this.request({ type: "rings" })) as TraceRings | null;
    if (!rings) throw new Error("nothing has been traced yet");
    return rings;
  }

  /** Stops whatever is running, for good. Every pending request rejects. */
  terminate(): void {
    if (this.terminated) return;
    this.terminated = true;
    this.worker.terminate();
    this.fail(new TraceCancelled());
  }

  private async run(config: TraceConfig): Promise<TraceResult | null> {
    this.busy = true;
    try {
      return (await this.request({ type: "render", config })) as TraceResult;
    } finally {
      this.busy = false;
      const next = this.queued;
      this.queued = null;
      if (next) void this.run(next.config).then(next.resolve, next.reject);
    }
  }

  private request(body: DistributiveOmit<TraceRequest, "id">): Promise<unknown> {
    if (this.terminated) return Promise.reject(new TraceCancelled());
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { resolve, reject });
      this.worker.postMessage({ ...body, id });
    });
  }

  private fail(error: Error): void {
    for (const waiting of this.waiting.values()) waiting.reject(error);
    this.waiting.clear();
    this.queued?.reject(error);
    this.queued = null;
  }
}

type DistributiveOmit<T, K extends keyof T> = T extends unknown ? Omit<T, K> : never;
