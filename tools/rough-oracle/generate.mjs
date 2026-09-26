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
  svgPath,
} from "roughjs/bin/renderer.js";
import { parsePath, absolutize, normalize } from "path-data-parser";
import { RoughGenerator } from "roughjs/bin/generator.js";

// One generator instance; it is stateless apart from defaultOptions, which we never set.
const gen = new RoughGenerator();

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

// --- pattern fills ---------------------------------------------------------------
//
// Fills go through RoughGenerator, never straight to a filler. The generator draws the
// outline first, which is what creates the randomizer; a fill computed on its own falls
// back to `Math.random()` for the hachure skipOffset and is non-deterministic *in
// rough.js itself*. It is also how Excalidraw calls rough, so this exercises the real
// path — including the ordering quirk that the outline is computed first but pushed last.
//
// `dots` is deliberately absent: DotFiller jitters every dot with Math.random(), so it
// cannot be reproduced by anyone, us included. Excalidraw does not expose it either.
const FILL_STYLES = ["hachure", "cross-hatch", "zigzag", "dashed", "zigzag-line", "solid"];

const emitDrawable = (fn, args, over, drawable) => {
  const o = opts(over);
  const { stroke, randomizer, ...geometryOptions } = o;
  cases.push({ fn, args, options: geometryOptions, sets: drawable.sets });
};

for (const fillStyle of FILL_STYLES) {
  for (const seed of [1, 7, 12345, 2147483647]) {
    // roughness crosses 1, the threshold at which polygonHachureLines draws from the
    // random stream to pick skipOffset — so the stream position downstream differs
    // above and below it.
    for (const roughness of [0, 0.5, 1, 2]) {
      for (const [w, h] of [[100, 60], [20, 20], [400, 300], [1200, 40]]) {
        for (const strokeWidth of [1, 4]) {
          const base = { seed, roughness, fillStyle, strokeWidth, fill: "#f00" };

          emitDrawable("gen.rectangle", [0, 0, w, h], base,
            gen.rectangle(0, 0, w, h, opts(base)));

          emitDrawable("gen.ellipse", [0, 0, w, h], base,
            gen.ellipse(0, 0, w, h, opts(base)));

          // A diamond exercises sloped edges in the active-edge table, where the islope
          // accumulation and the rounding of span endpoints actually bite.
          const diamond = [[w / 2, 0], [w, h / 2], [w / 2, h], [0, h / 2]];
          emitDrawable("gen.polygon", diamond, base,
            gen.polygon(structuredClone(diamond), opts(base)));
        }
      }
    }
  }
}

// Hachure angle and gap drive the scanline directly; sweep them on one shape.
for (const hachureAngle of [-41, 0, 45, 90, 137, -90]) {
  for (const hachureGap of [-1, 2, 8, 30]) {
    for (const fillStyle of ["hachure", "cross-hatch"]) {
      const base = { seed: 12345, hachureAngle, hachureGap, fillStyle, fill: "#f00" };
      emitDrawable("gen.rectangle", [0, 0, 120, 80], base,
        gen.rectangle(0, 0, 120, 80, opts(base)));
    }
  }
}

// Unfilled shapes through the generator too, so the sets-shape is covered both ways.
for (const seed of [1, 12345]) {
  for (const roughness of [0, 1, 2]) {
    const base = { seed, roughness };
    emitDrawable("gen.rectangle", [0, 0, 100, 60], base, gen.rectangle(0, 0, 100, 60, opts(base)));
    emitDrawable("gen.ellipse", [0, 0, 100, 60], base, gen.ellipse(0, 0, 100, 60, opts(base)));
    emitDrawable("gen.linearPath", [[0, 0], [50, 30], [100, 0]], base,
      gen.linearPath([[0, 0], [50, 30], [100, 0]], opts(base)));
    emitDrawable("gen.curve", [[0, 0], [50, 30], [100, 0], [150, 40]], base,
      gen.curve([[0, 0], [50, 30], [100, 0], [150, 40]], opts(base)));
    emitDrawable("gen.line", [0, 0, 300, 220], base, gen.line(0, 0, 300, 220, opts(base)));
  }
}

// Filled curves: a rounded closed line is `gen.curve` with a fill. A pattern fill is
// hatched inside the curve flattened by points-on-curve, a solid one is a second curve.
// The loop is what the line tool leaves when it closes on its start; three points is the
// case points-on-curve special-cases.
const CURVES = [
  [[0, 0], [220, 0], [110, 180], [0, 0]],
  [[0, 0], [60, -40], [140, 30], [200, -10], [260, 50]],
  [[0, 0], [120, 90], [240, 0]],
];
for (const fillStyle of FILL_STYLES) {
  for (const seed of [1, 12345]) {
    for (const roughness of [0, 1, 2]) {
      for (const points of CURVES) {
        const base = { seed, roughness, fillStyle, fill: "#f00" };
        emitDrawable("gen.curve", points, base, gen.curve(structuredClone(points), opts(base)));
      }
    }
  }
}

// --- rounded paths ---------------------------------------------------------------
//
// Excalidraw's DEFAULT rectangle is rounded, and rounded shapes go through
// generator.path() with an SVG string. Rather than port a path parser, the Rust side
// constructs the normalized segments directly — so the fixture records those segments
// alongside the ops, and the Rust test replays them through svg_path.
//
// The `d` Excalidraw builds (packages/element/src/shape.ts, case "rectangle"):
//   M r 0 L w-r 0 Q w 0, w r L w h-r Q w h, w-r h L r h Q 0 h, 0 h-r L 0 r Q 0 0, r 0
// Note it never closes: there is no Z.
const roundedRectPath = (w, h, r) =>
  `M ${r} 0 L ${w - r} 0 Q ${w} 0, ${w} ${r} L ${w} ${h - r} Q ${w} ${h}, ${w - r} ${h} ` +
  `L ${r} ${h} Q 0 ${h}, 0 ${h - r} L 0 ${r} Q 0 0, ${r} 0`;

for (const seed of [1, 7, 12345, 2147483647]) {
  for (const roughness of [0, 0.5, 1, 2]) {
    for (const [w, h, r] of [
      [100, 60, 8],
      [20, 20, 5],
      [400, 300, 32],
      [1200, 40, 10],
      [50, 50, 25], // r == min/2: the corners meet and the straight runs vanish
    ]) {
      for (const preserveVertices of [false, true]) {
        const base = { seed, roughness, preserveVertices };
        const d = roundedRectPath(w, h, r);
        const segments = normalize(absolutize(parsePath(d)));
        const o = opts(base);
        const opset = svgPath(d, o);
        const { stroke, randomizer, ...geometryOptions } = o;
        cases.push({
          fn: "svgPath",
          args: [w, h, r],
          segments: segments.map((s) => ({ key: s.key, data: s.data })),
          options: geometryOptions,
          opset,
        });
      }
    }
  }
}

// --- write ----------------------------------------------------------------------
//
// The full sweep is ~1.4M ops / 136MB, which has no business in git. It is split two
// ways, and the split is not merely a size trick — the two halves check different
// things:
//
//   curated.json      full ops for a representative subset. The debugging surface:
//                     when something breaks, this is what tells you which operand of
//                     which op moved.
//   sweep.summary.json  every case reduced to a structural signature plus numeric
//                     aggregates. Broad coverage, small file.
//
// The signature deliberately hashes only op KINDS, never coordinates. Hashing floats
// would make the test flake: a coordinate sitting within 1e-9 of a quantisation
// boundary lands in a different bucket for a last-ulp difference, and across 1.4M
// coordinates that is a near-certainty. Structure is exact and hashes cleanly;
// magnitudes are checked as sums against a relative tolerance instead.

// FNV-1a over the op-kind string. Deterministic, trivially reimplemented in Rust.
const fnv1a = (str) => {
  let h = 0x811c9dc5;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
};

const KIND_LETTER = { move: "m", lineTo: "l", bcurveTo: "b" };

const summariseSet = (opset) => {
  let sum = 0;
  let sumAbs = 0;
  let maxAbs = 0;
  let kinds = "";
  for (const op of opset.ops) {
    kinds += KIND_LETTER[op.op] ?? "?";
    for (const v of op.data) {
      sum += v;
      sumAbs += Math.abs(v);
      maxAbs = Math.max(maxAbs, Math.abs(v));
    }
  }
  return { type: opset.type, opCount: opset.ops.length, kindHash: fnv1a(kinds), sum, sumAbs, maxAbs };
};

// A case is either a single renderer opset or a generator drawable's list of sets.
const setsOf = (c) => (c.sets ? c.sets : [c.opset]);
const summarise = (c) => setsOf(c).map(summariseSet);
const opCountOf = (c) => setsOf(c).reduce((n, s) => n + s.ops.length, 0);

mkdirSync(OUT_DIR, { recursive: true });

// Curated: keep every distinct (fn, fillStyle) shape, capped per bucket and by op
// count so one enormous hachure fill cannot dominate the file.
const CURATED_PER_BUCKET = 40;
const CURATED_MAX_OPS = 400;
const bucketCount = new Map();
const curated = [];
for (const c of cases) {
  if (opCountOf(c) > CURATED_MAX_OPS) continue;
  const bucket = `${c.fn}:${c.options.fillStyle ?? ""}`;
  const n = bucketCount.get(bucket) ?? 0;
  if (n >= CURATED_PER_BUCKET) continue;
  bucketCount.set(bucket, n + 1);
  curated.push(c);
}

writeFileSync(
  join(OUT_DIR, "curated.json"),
  JSON.stringify({ roughjs: "4.6.4", cases: curated }, null, 0),
);

writeFileSync(
  join(OUT_DIR, "sweep.summary.json"),
  JSON.stringify(
    {
      roughjs: "4.6.4",
      cases: cases.map((c) => ({
        fn: c.fn,
        args: c.args,
        options: c.options,
        ...(c.params ? { params: c.params } : {}),
        // svgPath cases replay from their segments, so the sweep needs them too.
        ...(c.segments ? { segments: c.segments } : {}),
        summary: summarise(c),
      })),
    },
    null,
    0,
  ),
);

const ops = cases.reduce((n, c) => n + opCountOf(c), 0);
console.log(
  `${cases.length} cases / ${ops} ops -> sweep.summary.json` +
    ` (+ ${curated.length} cases with full ops -> curated.json)`,
);
