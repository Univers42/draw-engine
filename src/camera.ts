import type { Camera, WorldBounds } from "./types";
import { MAX_ZOOM, MIN_ZOOM } from "./types";

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function worldToScreen(camera: Camera, wx: number, wy: number): { x: number; y: number } {
  return { x: wx * camera.scale + camera.x, y: wy * camera.scale + camera.y };
}

export function screenToWorld(camera: Camera, sx: number, sy: number): { x: number; y: number } {
  return { x: (sx - camera.x) / camera.scale, y: (sy - camera.y) / camera.scale };
}

export function zoomAt(camera: Camera, sx: number, sy: number, factor: number): Camera {
  const scale = clamp(camera.scale * factor, MIN_ZOOM, MAX_ZOOM);
  const ratio = scale / camera.scale;
  return { scale, x: sx - (sx - camera.x) * ratio, y: sy - (sy - camera.y) * ratio };
}

export function zoomTo(camera: Camera, sx: number, sy: number, nextScale: number): Camera {
  const scale = clamp(nextScale, MIN_ZOOM, MAX_ZOOM);
  const ratio = scale / camera.scale;
  return { scale, x: sx - (sx - camera.x) * ratio, y: sy - (sy - camera.y) * ratio };
}

export function panBy(camera: Camera, dxScreen: number, dyScreen: number): Camera {
  return { ...camera, x: camera.x + dxScreen, y: camera.y + dyScreen };
}

export function fitBounds(bounds: WorldBounds, width: number, height: number, padding = 96): Camera {
  const worldW = Math.max(1, bounds.maxX - bounds.minX);
  const worldH = Math.max(1, bounds.maxY - bounds.minY);
  const scale = clamp(Math.min((width - padding * 2) / worldW, (height - padding * 2) / worldH), MIN_ZOOM, MAX_ZOOM);
  const centerX = (bounds.minX + bounds.maxX) / 2;
  const centerY = (bounds.minY + bounds.maxY) / 2;
  return { scale, x: width / 2 - centerX * scale, y: height / 2 - centerY * scale };
}

export function visibleWorldRect(camera: Camera, width: number, height: number): WorldBounds {
  const topLeft = screenToWorld(camera, 0, 0);
  const bottomRight = screenToWorld(camera, width, height);
  return { minX: topLeft.x, minY: topLeft.y, maxX: bottomRight.x, maxY: bottomRight.y };
}
