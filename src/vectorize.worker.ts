/**
 * The tracer's thread. Holds one picture's pixels in a `TraceSession` and answers the
 * requests `TraceWorker` sends (`./vectorize.ts`), one at a time, in order.
 */
import init, { TraceSession } from "../pkg/draw_trace.js";
import type { TracePhase, TraceReply, TraceRequest } from "./vectorize";

/** The worker's global, typed without pulling the WebWorker lib into a DOM program. */
const scope = self as unknown as {
  postMessage(message: TraceReply, transfer?: Transferable[]): void;
  onmessage: ((event: MessageEvent<TraceRequest>) => void) | null;
};

let ready: Promise<unknown> | null = null;
let session: TraceSession | null = null;
let queue: Promise<void> = Promise.resolve();

/** The picture's pixels as RGBA bytes, row by row — `ImageData`, drawn at its own size. */
function pixels(bitmap: ImageBitmap): Uint8Array {
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) throw new Error("no 2D context in this worker");
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
  return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
}

/**
 * Progress, but not every tick of it: a message per call would be thousands on a large
 * picture, all queued behind each other on their way to a thread that paints once a frame.
 */
function reporter(id: number): (phase: TracePhase, fraction: number) => void {
  let last: { phase: TracePhase; fraction: number } | null = null;
  return (phase, fraction) => {
    if (last && last.phase === phase && fraction - last.fraction < 0.02 && fraction < 1) return;
    last = { phase, fraction };
    scope.postMessage({ id, type: "progress", progress: { phase, fraction } });
  };
}

/** The answer to `request`, and the buffers to move to the other side rather than copy. */
function answer(request: TraceRequest): [unknown, Transferable[]] {
  if (request.type === "open") {
    const { width, height } = request.bitmap;
    const rgba = pixels(request.bitmap);
    session?.free();
    session = new TraceSession(rgba, width, height);
    return [{ width, height }, []];
  }
  if (!session) throw new Error("no picture to trace");
  if (request.type === "render") {
    const config = JSON.stringify(request.config);
    const stats: unknown = JSON.parse(session.render(config, reporter(request.id)));
    return [{ stats, svg: session.svg() }, []];
  }
  const rings = session.rings();
  if (!rings) return [null, []];
  // Each getter copies out of WASM memory once; the copies are ours to hand over.
  const flat = { colours: rings.colours, lengths: rings.lengths, coords: rings.coords };
  rings.free();
  return [flat, [flat.colours.buffer, flat.lengths.buffer, flat.coords.buffer]];
}

async function handle(request: TraceRequest): Promise<void> {
  try {
    ready ??= init();
    await ready;
    const [value, transfer] = answer(request);
    scope.postMessage({ id: request.id, type: "done", value }, transfer);
  } catch (error) {
    scope.postMessage({
      id: request.id,
      type: "error",
      message: error instanceof Error ? error.message : String(error),
    });
  }
}

scope.onmessage = (event) => {
  queue = queue.then(() => handle(event.data));
};
