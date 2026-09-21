// Emits rough.js's own output as JSON fixtures, which `crates/draw-rough/tests/oracle.rs`
// replays against the Rust port. This is the oracle that makes "identical to Excalidraw"
// an assertion instead of an opinion.
//
// We call into `roughjs/bin/renderer.js` directly rather than through RoughGenerator so
// each primitive is isolated: when a fixture fails you learn *which* function drifted,
// not merely that some shape came out wrong.
//
//   npm install && npm run generate
//
// Output is committed, so `cargo test` needs neither Node nor a network.

import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  line,
  linearPath,
  polygon,
  rectangle,
  curve,
  ellipse,
  generateEllipseParams,
  ellipseWithParams,
  solidFillPolygon,
} from "roughjs/bin/renderer.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(HERE, "..", "..", "crates", "draw-rough", "tests", "fixtures");

// rough's own defaults (RoughGenerator.defaultOptions). Every case starts from these and
// overrides only what it is exercising, so a fixture diff points at one variable.
const DEFAULTS = {
  maxRandomnessOffset: 2,
  roughness: 1,
  bowing: 1,
  stroke: "#000",
  strokeWidth: 1,
  curveTightness: 0,
  curveFitting: 0.95,
  curveStepCount: 9,
  fillStyle: "hachure",
  fillWeight: -1,
  hachureAngle: -41,
  hachureGap: -1,
  dashOffset: -1,
  dashGap: -1,
  zigzagOffset: -1,
  seed: 0,
  disableMultiStroke: false,
  disableMultiStrokeFill: false,
  preserveVertices: false,
  fillShapeRoughnessGain: 0.8,
};

// A fresh options object per call: rough caches its randomizer ON the options object,
// so reusing one would carry the random stream across cases and make every fixture
// depend on the order they were generated in.
const opts = (over) => ({ ...DEFAULTS, ...over });

const cases = [];
const emit = (fn, args, over, opset) => {
  const o = opts(over);
  // Strip the fields that do not affect geometry, plus the randomizer rough attached.
  const { stroke, randomizer, ...geometryOptions } = o;
  cases.push({ fn, args, options: geometryOptions, opset });
};

// --- the sweep ------------------------------------------------------------------

// Seeds chosen to cover the PRNG's behaviour rather than to look random: the boundary
// values, a negative state, and a spread of ordinary ones.
const SEEDS = [1, 2, 7, 12345, 99991, 524287, 1082196789, 2147483647];

// Excalidraw's sloppiness levels are 0 / 1 / 2; 0.5 and 3 probe either side.
const ROUGHNESS = [0, 0.5, 1, 2, 3];

// Sizes that matter: degenerate, tiny, ordinary, large, and extreme aspect ratios —
// the `length < 200` / `> 500` branches in _line and the step-count ceiling in
// generateEllipseParams all switch on magnitude.
const SIZES = [
  [0, 0],
  [1, 1],
  [5, 5],
  [20, 20],
  [100, 60],
  [400, 300],
  [1200, 40],
  [40, 1200],
  [-80, -50], // negative extents: rough is handed these by a right-to-left drag
];

for (const seed of SEEDS) {
  for (const roughness of ROUGHNESS) {
    for (const [w, h] of SIZES) {
      const base = { seed, roughness };

      emit("rectangle", [10, 20, w, h], base, rectangle(10, 20, w, h, opts(base)));
      emit("ellipse", [10, 20, w, h], base, ellipse(10, 20, w, h, opts(base)));
      emit("line", [10, 20, 10 + w, 20 + h], base, line(10, 20, 10 + w, 20 + h, opts(base)));

      // Diamond: what Excalidraw generates for the diamond tool.
      const diamond = [
        [10 + w / 2, 20],
        [10 + w, 20 + h / 2],
        [10 + w / 2, 20 + h],
        [10, 20 + h / 2],
      ];
      emit("polygon", diamond, base, polygon(diamond, opts(base)));

      // A multi-point open path: the line/arrow tool with more than two points.
      const path = [
        [10, 20],
        [10 + w / 3, 20 + h],
        [10 + (2 * w) / 3, 20],
        [10 + w, 20 + h],
      ];
      emit("linearPath", path, base, linearPath(path, false, opts(base)));
      emit("curve", path, base, curve(path, opts(base)));
    }
  }
}

// Option flags that change the op *structure*, not just the coordinates.
for (const seed of [1, 12345]) {
  for (const flag of [
    { disableMultiStroke: true },
    { preserveVertices: true },
    { disableMultiStroke: true, preserveVertices: true },
    { curveTightness: 0.5 },
    { curveFitting: 0.5 },
    { curveStepCount: 3 },
    { curveStepCount: 30 },
    { bowing: 0 },
    { bowing: 4 },
    { maxRandomnessOffset: 0 },
    { maxRandomnessOffset: 8 },
  ]) {
    const base = { seed, ...flag };
    emit("rectangle", [0, 0, 100, 60], base, rectangle(0, 0, 100, 60, opts(base)));
    emit("ellipse", [0, 0, 100, 60], base, ellipse(0, 0, 100, 60, opts(base)));
    emit("line", [0, 0, 300, 220], base, line(0, 0, 300, 220, opts(base)));
    // Long enough to cross both roughnessGain thresholds (200 and 500).
    emit("line", [0, 0, 900, 10], base, line(0, 0, 900, 10, opts(base)));
  }
}

// generateEllipseParams / ellipseWithParams are exercised separately because a
// pattern-filled ellipse shares one params struct between outline and fill; a drift in
// the params alone would otherwise be invisible until fills land.
for (const seed of [1, 12345, 2147483647]) {
  for (const [w, h] of [[100, 60], [5, 5], [400, 300]]) {
    const o = opts({ seed });
    const params = generateEllipseParams(w, h, o);
    const { opset, estimatedPoints } = ellipseWithParams(10, 20, o, params);
    cases.push({
      fn: "ellipseWithParams",
      args: [10, 20, w, h],
      options: (({ stroke, randomizer, ...rest }) => rest)(o),
      params: { increment: params.increment, rx: params.rx, ry: params.ry },
      opset,
      estimatedPoints,
    });
  }
}

// solidFillPolygon: the only fill path that does not go through a filler.
for (const seed of [1, 12345]) {
  const quad = [
    [
      [0, 0],
      [100, 0],
      [100, 60],
      [0, 60],
    ],
  ];
  emit("solidFillPolygon", quad, { seed }, solidFillPolygon(quad, opts({ seed })));
}

// --- write ----------------------------------------------------------------------

mkdirSync(OUT_DIR, { recursive: true });
const out = join(OUT_DIR, "renderer.json");

// JSON.stringify emits the shortest decimal that round-trips a double, and serde_json
// parses it back to the identical bit pattern — so plain JSON is lossless for f64.
writeFileSync(out, JSON.stringify({ roughjs: "4.6.4", cases }, null, 0));

const ops = cases.reduce((n, c) => n + (c.opset?.ops?.length ?? 0), 0);
console.log(`${cases.length} cases, ${ops} ops -> ${out}`);
