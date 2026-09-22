import init, { DrawEngine as WasmDrawEngine } from "../pkg/draw_engine.js";
import type {
  Camera,
  DrawEngineOptions,
  DrawNotice,
  DrawTool,
  TextEditRequest,
} from "./types";

let ready: Promise<void> | null = null;

export function loadDrawEngine(): Promise<void> {
  if (ready === null) ready = init().then(() => undefined);
  return ready;
}

export function parseJson<T>(raw: string, fallback: T): T {
  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function wireCallbacks(
  inner: InstanceType<typeof WasmDrawEngine>,
  options: DrawEngineOptions,
): void {
  if (options.onCameraChange) {
    const cb = options.onCameraChange;
    inner.setOnCameraChange((json: string) => cb(parseJson<Camera>(json, { x: 0, y: 0, scale: 1 })));
  }
  if (options.onToolChange) {
    const cb = options.onToolChange;
    inner.setOnToolChange((tool: string) => cb(tool as DrawTool));
  }
  if (options.onSelectionChange) {
    const cb = options.onSelectionChange;
    inner.setOnSelectionChange((json: string) => cb(parseJson<string[]>(json, [])));
  }
  if (options.onRequestTextEdit) {
    const cb = options.onRequestTextEdit;
    inner.setOnRequestTextEdit((json: string) => {
      const req = parseJson<TextEditRequest | null>(json, null);
      if (req) cb(req);
    });
  }
  if (options.onSceneChange) {
    const cb = options.onSceneChange;
    inner.setOnSceneChange((json: string) => cb(json));
  }
  if (options.onNotice) {
    const cb = options.onNotice;
    inner.setOnNotice((code: string) => cb(code as DrawNotice));
  }
}
