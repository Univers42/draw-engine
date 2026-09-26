// Parity fixture for elbow arrows, produced by Excalidraw's OWN elbowArrow.ts.
//
//   EXCALIDRAW_DIR=/path/to/excalidraw [ORACLE_SHA=<sha>] \
//     node --experimental-transform-types --import ./register.mjs generate.mjs [seed]
//
// Runs the oracle's router and everything it imports unmodified (see hooks.mjs), through
// the oracle's own entry points — `mutateElement`, `calculateFixedPointForElbowArrowBinding`,
// `LinearElementEditor.moveFixedSegment` / `deleteFixedSegment` and `updateBoundElements` —
// against a minimal scene object. Writes, relative to this directory:
//
//   ../../crates/draw-engine/tests/fixtures/elbow.oracle.json   replayed by
//       tests/ci_elbow_oracle.rs within 1e-9.
//
// Every case is a board (shapes in z-order, one elbow arrow) and a sequence of steps. Each
// step records the arrow after it (a shape move carries the shape's new box), and the replay
// starts every step from the oracle's own state before it, so a step is one exact unit of
// the router and a drift in one cannot hide in or cascade into the next.
//
// The sweep: free ends; one end bound (either end); both bound, with each end bound on each
// of the four sides — every heading pair — across every relative placement, overlapping
// and nested shapes and both ends on one shape included; the three shape kinds, sharp and
// rounded, turned, thin strokes and thick; with and without arrowheads (the gap is 6× or
// 2×); routes while dragging (`isDragging`, which snaps to the outline and re-detects the
// shape under each end); fixed segments moved, then carried through an endpoint drag and a
// shape move, then released; and renormalisation.
//
// Before anything is written, a self-check replays the oracle's own expectation for a free
// arrow (packages/element/tests/elbowArrow.test.tsx, "can properly generate orthogonal
// arrow points") and exits 2 on a mismatch.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(HERE, "..", "..", "crates", "draw-engine", "tests", "fixtures", "elbow.oracle.json");

const fail = (message, code = 1) => {
  console.error(`[elbow-oracle] ${message}`);
  process.exit(code);
};

// ---------------------------------------------------------------- the pinned oracle
const DIR = process.env.EXCALIDRAW_DIR || fail("EXCALIDRAW_DIR is not set");
const previousPin = existsSync(FIXTURE)
  ? JSON.parse(readFileSync(FIXTURE, "utf8")).oracle?.excalidraw
  : undefined;
// A moved pin is a deliberate act: say so with ORACLE_SHA, or regenerate at the old one.
const PIN = process.env.ORACLE_SHA || previousPin || fail("no pin: set ORACLE_SHA");
const HEAD = execFileSync("git", ["-c", "safe.directory=*", "-C", DIR, "rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
if (HEAD !== PIN) fail(`${DIR} is at ${HEAD}, the pin is ${PIN}`);

const oracle = (file) => import(pathToFileURL(join(DIR, "packages/element/src", file)).href);
const { mutateElement } = await oracle("mutateElement.ts");
const { newElement, newArrowElement } = await oracle("newElement.ts");
const { calculateFixedPointForElbowArrowBinding, updateBoundElements } = await oracle("binding.ts");
const { LinearElementEditor } = await oracle("linearElementEditor.ts");

const SEED = Number(process.argv[2] ?? 7);
const ZOOM = { value: 1 };

// ---------------------------------------------------------------- deterministic draws
let state = SEED >>> 0;
const random = () => {
  // mulberry32
  state = (state + 0x6d2b79f5) >>> 0;
  let t = state;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (list) => list[Math.floor(random() * list.length)];
const between = (lo, hi) => lo + random() * (hi - lo);
// Half the coordinates are whole, as a pointer on an unzoomed board gives; half carry
// fractions, as a zoomed or panned one does — the arithmetic must hold for both.
const coordinate = (lo, hi) => (random() < 0.5 ? Math.round(between(lo, hi)) : between(lo, hi));

// ---------------------------------------------------------------- a board
const ROUNDNESS = { rectangle: 3, diamond: 2, ellipse: 2 };
const KINDS = ["rectangle", "rectangle", "diamond", "ellipse"];
const ARROWHEADS = [null, "arrow", "triangle"];

let nextId = 0;
const shape = (over = {}) => {
  const type = over.type ?? pick(KINDS);
  const rounded = over.rounded ?? random() < 0.5;
  const turned = over.angle ?? (random() < 0.15 ? pick([0.3, 0.7853981633974483, 1.2, Math.PI / 2, 2.5]) : 0);
  const element = newElement({
    type,
    x: over.x ?? coordinate(-400, 400),
    y: over.y ?? coordinate(-400, 400),
    width: over.width ?? coordinate(30, 260),
    height: over.height ?? coordinate(30, 260),
    angle: turned,
    strokeWidth: over.strokeWidth ?? pick([1, 2, 2, 4]),
    roundness: rounded ? { type: ROUNDNESS[type] } : null,
  });
  return { ...element, id: `s${nextId++}`, boundElements: [] };
};

const arrow = (x, y, heads) =>
  ({
    ...newArrowElement({
      type: "arrow",
      x,
      y,
      elbowed: true,
      points: [
        [0, 0],
        [0, 0],
      ],
      startArrowhead: heads?.[0] ?? pick(ARROWHEADS),
      endArrowhead: heads?.[1] ?? pick([...ARROWHEADS, "arrow"]),
      roundness: null,
    }),
    id: "arrow",
  });

const board = (shapes, a) => {
  const map = new Map();
  for (const s of shapes) map.set(s.id, s);
  map.set(a.id, a);
  return {
    map,
    scene: {
      getNonDeletedElementsMap: () => map,
      getNonDeletedElements: () => [...map.values()],
      mutateElement: (element, updates, options) => mutateElement(element, map, updates, options),
    },
  };
};

// ---------------------------------------------------------------- recording
const shapeRecord = (s) => ({
  id: s.id,
  type: s.type,
  x: s.x,
  y: s.y,
  width: s.width,
  height: s.height,
  angle: s.angle,
  strokeWidth: s.strokeWidth,
  rounded: s.roundness !== null,
});
const binding = (b) => (b ? { elementId: b.elementId, fixedPoint: [...b.fixedPoint] } : null);
const segments = (list) =>
  list == null ? null : list.map((s) => ({ index: s.index, start: [...s.start], end: [...s.end] }));
/** What a step can change; the replay holds everything else as the case began. */
const routeRecord = (a) => ({
  x: a.x,
  y: a.y,
  width: a.width,
  height: a.height,
  angle: a.angle,
  points: a.points.map((p) => [...p]),
  fixedSegments: segments(a.fixedSegments),
  startIsSpecial: a.startIsSpecial ?? null,
  endIsSpecial: a.endIsSpecial ?? null,
});
const arrowRecord = (a) => ({
  x: a.x,
  y: a.y,
  width: a.width,
  height: a.height,
  angle: a.angle,
  points: a.points.map((p) => [...p]),
  startBinding: binding(a.startBinding),
  endBinding: binding(a.endBinding),
  startArrowhead: a.startArrowhead,
  endArrowhead: a.endArrowhead,
  fixedSegments: segments(a.fixedSegments),
  startIsSpecial: a.startIsSpecial ?? null,
  endIsSpecial: a.endIsSpecial ?? null,
});

const cases = [];
const thrown = [];

/** Runs `steps` against a fresh board and keeps the case when the oracle took them all. */
const record = (name, shapes, a, steps) => {
  const { map, scene } = board(shapes, a);
  const initial = { shapes: shapes.map(shapeRecord), arrow: arrowRecord(a) };
  const done = [];
  try {
    for (const step of steps) {
      const op = step(map, scene, a);
      if (op === null) break;
      done.push({ ...op, arrow: routeRecord(a) });
    }
  } catch (error) {
    // The oracle throws on a handful of degenerate inputs (its own invariants); those
    // steps are not something to be at parity with.
    thrown.push(`${name}: ${error.message.split("\n")[0]}`);
    if (done.length === 0) return;
  }
  if (done.length > 0) cases.push({ name, ...initial, steps: done });
};

// ---------------------------------------------------------------- steps
const local = (a, [x, y]) => [x - a.x, y - a.y];

/** Moves the ends (either may be left out) as `movePoints` does: only the two ends. */
const moveEnds = (start, end, isDragging = false) => (map, scene, a) => {
  const points = [start ? local(a, start) : a.points[0], end ? local(a, end) : a.points.at(-1)];
  mutateElement(a, map, { points }, { isDragging });
  return { op: "mutate", updates: { points }, isDragging };
};

/** Binds one end where it is, as `bindBindingElement` does for an elbow arrow. */
const bind = (end, target) => (map, scene, a) => {
  const { fixedPoint } = calculateFixedPointForElbowArrowBinding(a, target, end, map, ZOOM);
  const key = end === "start" ? "startBinding" : "endBinding";
  mutateElement(a, map, { [key]: { elementId: target.id, fixedPoint, mode: "orbit" } });
  if (!target.boundElements.some((b) => b.id === a.id)) {
    target.boundElements = [...target.boundElements, { id: a.id, type: "arrow" }];
  }
  return { op: "bind", end, shape: target.id, fixedPoint: [...fixedPoint] };
};

/** Binds one end to a given anchor, then routes with `points` as the ends. */
const anchor = (end, target, fixedPoint) => (map, scene, a) => {
  const key = end === "start" ? "startBinding" : "endBinding";
  const value = { elementId: target.id, fixedPoint, mode: "orbit" };
  const points = [a.points[0], a.points.at(-1)];
  mutateElement(a, map, { [key]: value, points });
  target.boundElements = [...target.boundElements, { id: a.id, type: "arrow" }];
  return { op: "mutate", updates: { points, [key]: binding(value) }, isDragging: false };
};

const route = () => (map, scene, a) => {
  const points = [a.points[0], a.points.at(-1)];
  mutateElement(a, map, { points });
  return { op: "mutate", updates: { points }, isDragging: false };
};

const renormalize = () => (map, scene, a) => {
  mutateElement(a, map, {});
  return { op: "mutate", updates: {}, isDragging: false };
};

/** Drags segment `index` (counted from 1) to `(x, y)`, as the editor does. */
const moveSegment = (pickIndex, offset) => (map, scene, a) => {
  const index = pickIndex(a);
  if (index === null) return null;
  const [p, q] = [a.points[index - 1], a.points[index]];
  const x = a.x + (p[0] + q[0]) / 2 + offset[0];
  const y = a.y + (p[1] + q[1]) / 2 + offset[1];
  LinearElementEditor.moveFixedSegment({ elementId: a.id }, index, x, y, scene);
  return { op: "moveSegment", index, x, y };
};

const releaseSegment = () => (map, scene, a) => {
  if (!a.fixedSegments?.length) return null;
  const index = pick(a.fixedSegments).index;
  LinearElementEditor.deleteFixedSegment(a, scene, index);
  return { op: "releaseSegment", index };
};

/** Moves and resizes a shape, then lets the oracle carry every arrow bound to it. */
const moveShape = (target, dx, dy, dw = 0, dh = 0) => (map, scene) => {
  const next = { x: target.x + dx, y: target.y + dy, width: target.width + dw, height: target.height + dh };
  mutateElement(target, map, next);
  updateBoundElements(target, scene);
  return { op: "moveShape", shape: target.id, ...next };
};

const middleSegment = (a) => (a.points.length > 3 ? 1 + Math.floor(random() * (a.points.length - 3)) + 1 : null);
const anySegment = (a) => (a.points.length > 1 ? 1 + Math.floor(random() * (a.points.length - 1)) : null);

// ---------------------------------------------------------------- the sweep
const SIDES = {
  top: (s) => [s.x + s.width * between(0.2, 0.8), s.y - between(0, 12)],
  right: (s) => [s.x + s.width + between(0, 12), s.y + s.height * between(0.2, 0.8)],
  bottom: (s) => [s.x + s.width * between(0.2, 0.8), s.y + s.height + between(0, 12)],
  left: (s) => [s.x - between(0, 12), s.y + s.height * between(0.2, 0.8)],
  // The side midpoints, where the snap pulls an end onto the axis.
  "top-mid": (s) => [s.x + s.width / 2 + between(-2, 2), s.y - 3],
  "right-mid": (s) => [s.x + s.width + 3, s.y + s.height / 2 + between(-2, 2)],
  "inside": (s) => [s.x + s.width * between(0.3, 0.7), s.y + s.height * between(0.3, 0.7)],
};
const SIDE_NAMES = ["top", "right", "bottom", "left"];
/** A point near a side of `s`, turned with it. */
const near = (s, side) => {
  const [x, y] = SIDES[side](s);
  if (!s.angle) return [x, y];
  const [cx, cy] = [s.x + s.width / 2, s.y + s.height / 2];
  const [c, n] = [Math.cos(s.angle), Math.sin(s.angle)];
  return [cx + (x - cx) * c - (y - cy) * n, cy + (x - cx) * n + (y - cy) * c];
};

// 1. Free ends: every direction, aligned and not, short and long.
for (let i = 0; i < 160; i++) {
  const a = arrow(coordinate(-300, 300), coordinate(-300, 300));
  const d = [
    i % 7 === 0 ? 0 : coordinate(-500, 500),
    i % 11 === 0 ? 0 : coordinate(-500, 500),
  ];
  record(`free ${i}`, [], a, [moveEnds(null, [a.x + d[0], a.y + d[1]]), renormalize()]);
}

// 2. One end bound, the other free — each end in turn, every side, turned shapes too.
for (let i = 0; i < 360; i++) {
  const s = shape();
  const side = pick([...SIDE_NAMES, "top-mid", "right-mid"]);
  const [bx, by] = near(s, side);
  const free = [coordinate(-600, 600), coordinate(-600, 600)];
  const startBound = i % 2 === 0;
  const a = startBound ? arrow(bx, by) : arrow(free[0], free[1]);
  const steps = startBound
    ? [bind("start", s), moveEnds(null, free), route()]
    : [moveEnds(null, [bx, by]), bind("end", s), route()];
  record(`one bound ${i} ${s.type} ${side}`, [s], a, steps);
}

// 3. Both bound: every heading pair across every placement.
const PLACEMENTS = [
  [1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, 1], [1, -1], [-1, -1],
];
for (let i = 0; i < 640; i++) {
  const s = shape();
  const [px, py] = PLACEMENTS[i % PLACEMENTS.length];
  const gap = pick([-60, -10, 20, 80, 240]); // negative: the two overlap
  const t = shape({
    x: s.x + (px > 0 ? s.width + gap : px < 0 ? -gap - 150 : between(-80, 80)),
    y: s.y + (py > 0 ? s.height + gap : py < 0 ? -gap - 150 : between(-80, 80)),
  });
  const pair = Math.floor(i / PLACEMENTS.length) % 16;
  const [from, to] = [near(s, SIDE_NAMES[pair % 4]), near(t, SIDE_NAMES[Math.floor(pair / 4)])];
  const a = arrow(from[0], from[1]);
  record(`both bound ${i} ${SIDE_NAMES[pair % 4]}→${SIDE_NAMES[Math.floor(pair / 4)]}`, [s, t], a, [
    bind("start", s),
    moveEnds(null, to),
    bind("end", t),
    route(),
    renormalize(),
  ]);
}

// 4. Nested shapes, and both ends on the same shape.
for (let i = 0; i < 80; i++) {
  const outer = shape({ width: coordinate(200, 400), height: coordinate(200, 400) });
  const inner = shape({
    x: outer.x + outer.width * between(0.2, 0.4),
    y: outer.y + outer.height * between(0.2, 0.4),
    width: outer.width * 0.3,
    height: outer.height * 0.3,
  });
  const same = i % 2 === 0;
  const [from, to] = [near(outer, pick(SIDE_NAMES)), near(same ? outer : inner, pick(SIDE_NAMES))];
  const a = arrow(from[0], from[1]);
  record(`${same ? "same shape" : "nested"} ${i}`, [outer, inner], a, [
    bind("start", outer),
    moveEnds(null, to),
    bind("end", same ? outer : inner),
    route(),
  ]);
}

// 5. Anchors given outright — inside, outside, on the 0.5 the oracle nudges to 0.5001.
for (let i = 0; i < 160; i++) {
  const [s, t] = [shape(), shape()];
  const ratio = () => pick([0.5, 0, 1, between(-0.2, 1.2), between(0, 1)]);
  const a = arrow(coordinate(-300, 300), coordinate(-300, 300));
  record(`anchored ${i}`, [s, t], a, [
    anchor("start", s, [ratio(), ratio()]),
    anchor("end", t, [ratio(), ratio()]),
    route(),
  ]);
}

// 6. Dragging: each end snapped to the outline under it, the shape re-detected per move.
for (let i = 0; i < 320; i++) {
  const [s, t] = [shape(), shape()];
  const from = near(s, pick([...SIDE_NAMES, "top-mid", "right-mid", "inside"]));
  const a = arrow(from[0], from[1]);
  const to = random() < 0.8 ? near(t, pick([...SIDE_NAMES, "top-mid", "right-mid", "inside"])) : [coordinate(-600, 600), coordinate(-600, 600)];
  const halfway = [(from[0] + to[0]) / 2 + coordinate(-40, 40), (from[1] + to[1]) / 2 + coordinate(-40, 40)];
  const steps = [bind("start", s), moveEnds(null, halfway, true), moveEnds(null, to, true)];
  if (random() < 0.6) steps.push(bind("end", t), route());
  record(`dragging ${i}`, [s, t], a, steps);
}

// 7. Fixed segments: moved, carried through an end drag and a shape move, released.
for (let i = 0; i < 420; i++) {
  const [s, t] = [shape(), shape()];
  const from = near(s, pick(SIDE_NAMES));
  const to = near(t, pick(SIDE_NAMES));
  const a = arrow(from[0], from[1]);
  const bindEnd = random() < 0.7;
  const steps = [bind("start", s), moveEnds(null, to)];
  if (bindEnd) steps.push(bind("end", t));
  steps.push(route());
  const offset = () => [coordinate(-120, 120), coordinate(-120, 120)];
  steps.push(moveSegment(random() < 0.7 ? middleSegment : anySegment, offset()));
  if (random() < 0.5) steps.push(moveSegment(anySegment, offset()));
  switch (i % 4) {
    case 0:
      steps.push(moveShape(s, coordinate(-80, 80), coordinate(-80, 80)));
      break;
    case 1:
      steps.push(moveShape(t, coordinate(-80, 80), coordinate(-80, 80), coordinate(-20, 40), coordinate(-20, 40)));
      break;
    case 2:
      if (!bindEnd) steps.push(moveEnds(null, [to[0] + coordinate(-90, 90), to[1] + coordinate(-90, 90)]));
      break;
    default:
      steps.push(renormalize());
  }
  steps.push(releaseSegment(), renormalize());
  record(`fixed segments ${i}`, [s, t], a, steps);
}

// 8. Shapes moving and resizing under bound arrows.
for (let i = 0; i < 200; i++) {
  const [s, t] = [shape(), shape()];
  const from = near(s, pick(SIDE_NAMES));
  const to = near(t, pick(SIDE_NAMES));
  const a = arrow(from[0], from[1]);
  record(`shape moved ${i}`, [s, t], a, [
    bind("start", s),
    moveEnds(null, to),
    bind("end", t),
    route(),
    moveShape(pick([s, t]), coordinate(-200, 200), coordinate(-200, 200), coordinate(-20, 60), coordinate(-20, 60)),
    moveShape(pick([s, t]), coordinate(-200, 200), coordinate(-200, 200)),
  ]);
}

// ---------------------------------------------------------------- self-check
// packages/element/tests/elbowArrow.test.tsx, "can properly generate orthogonal arrow
// points": a free arrow from (-45, -100.1) to (45, 99.9).
{
  const a = arrow(0, 0, [null, "arrow"]);
  const { map } = board([], a);
  mutateElement(a, map, { points: [[-45, -100.1], [45, 99.9]] });
  const got = JSON.stringify([a.points, a.x, a.y, a.width, a.height]);
  const want = JSON.stringify([[[0, 0], [0, 100], [90, 100], [90, 200]], -45, -100.1, 90, 200]);
  if (got !== want) fail(`self-check: the oracle's own test wants ${want}, got ${got}`, 2);
}

const steps = cases.reduce((n, c) => n + c.steps.length, 0);
writeFileSync(
  FIXTURE,
  `${JSON.stringify({ oracle: { excalidraw: PIN, seed: SEED, cases: cases.length, steps }, cases })}\n`,
);
for (const line of thrown) console.log(`[elbow-oracle] the oracle threw — ${line}`);
console.log(`[elbow-oracle] ${cases.length} cases, ${steps} steps → ${FIXTURE}`);
