// Parity fixture for arrowhead geometry, produced by the oracle's OWN
// `getArrowheadPoints` / `getArrowheadSize` / `getArrowheadAngle`
// (`packages/element/src/bounds.ts`), run unmodified (see `hooks.mjs`) — fed a `Drawable`
// the real, installed `roughjs@4.6.4` generated, exactly as `getArrowheadShapes`
// (`shape.ts`) is.
//
//   EXCALIDRAW_DIR=/path/to/excalidraw [ORACLE_SHA=<sha>] \
//     node --import ./register.mjs generate.mjs
//
// Writes, relative to this directory:
//   ../../crates/draw-engine/tests/fixtures/arrowhead.oracle.json
// replayed by `tests/ci_arrowhead_oracle.rs`: for every case, it regenerates the SAME
// `Drawable` through `draw_rough::generator::{curve,linear_path}` (already proven
// bit-for-bit against this same roughjs version by `crates/draw-rough/tests/oracle.rs`,
// so this fixture does not re-prove op fidelity — it exists to prove
// `get_arrowhead_points`'s own arithmetic) and asserts the Rust port's output against the
// oracle's, coordinate by coordinate, within 1e-9.
//
// Run inside mcr.microsoft.com/playwright:v1.63.0-noble (the root's `make oracle-fixtures`
// does, once wired there): Node's unflagged TypeScript stripping is what lets this import
// `bounds.ts` at all.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { RoughGenerator } from "roughjs/bin/generator.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(HERE, "..", "..", "crates", "draw-engine", "tests", "fixtures", "arrowhead.oracle.json");

const fail = (message, code = 1) => {
  console.error(`[arrowhead-oracle] ${message}`);
  process.exit(code);
};

// ---------------------------------------------------------------- the pinned oracle
const DIR = process.env.EXCALIDRAW_DIR || fail("EXCALIDRAW_DIR is not set");
const previousPin = existsSync(FIXTURE) ? JSON.parse(readFileSync(FIXTURE, "utf8")).oracle?.excalidraw : undefined;
const PIN = process.env.ORACLE_SHA || previousPin || fail("no pin: set ORACLE_SHA");
const HEAD = execFileSync("git", ["-c", "safe.directory=*", "-C", DIR, "rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
if (HEAD !== PIN) fail(`${DIR} is at ${HEAD}, the pin is ${PIN}`);

const ORACLE_FILE = "packages/element/src/bounds.ts";
const B = await import(pathToFileURL(join(DIR, ORACLE_FILE)).href);
const { getArrowheadPoints, getArrowheadSize, getArrowheadAngle } = B;
if (typeof getArrowheadPoints !== "function") fail("bounds.ts did not export getArrowheadPoints");

// ---------------------------------------------------------------- rough, for real bodies
const gen = new RoughGenerator();
const roughOpts = (seed, roughness, strokeWidth) => ({
  maxRandomnessOffset: 2,
  roughness,
  bowing: 1,
  strokeWidth,
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
  seed,
  disableMultiStroke: false,
  disableMultiStrokeFill: false,
  preserveVertices: false,
  fillShapeRoughnessGain: 0.8,
});

// The same branch `render/shape.rs::element_drawable` takes for `Line | Arrow`: rounded
// -> `curve`, sharp -> `linearPath` (never `polygon` — an arrow is never filled).
//
// `RoughGenerator.prototype.linearPath` takes exactly `(points, options)` — no `close`
// parameter (the renderer it delegates to always draws open; closing is a `polygon`
// concern). A third argument is not a type error, only silently discarded, so a bug that
// once stood here (`gen.linearPath(points, false, o)`) did not fail loudly: it passed the
// literal `false` as `options`, and `_o(options) { return options ? Object.assign({},
// this.defaultOptions, options) : this.defaultOptions; }` treats falsy `options` as "no
// override" and hands back `this.defaultOptions` itself — the *same* object, not a copy.
// The first `random(ops)` call against it lazily sets `ops.randomizer = new
// Random(ops.seed || 0)` on that shared object, i.e. `new Random(0)`; `0` is falsy too, so
// `Random#next` takes its own `Math.random()` fallback forever. Because it is
// `this.defaultOptions` that got the randomizer, not a per-call copy, every later `_o()`
// call that does not itself override `.randomizer` — including a correctly-called
// `curve(points, o)`, whose `Object.assign({}, this.defaultOptions, o)` copies the
// poisoned `.randomizer` across since `o` never sets one — inherits it too. One bad
// `linearPath` call early in the sweep silently made the *entire* fixture, curves
// included, read real entropy instead of the requested seed.
const bodyDrawable = (points, roundness, seed, roughness, strokeWidth) => {
  const o = roughOpts(seed, roughness, strokeWidth);
  return roundness ? gen.curve(points, o) : gen.linearPath(points, o);
};

// ---------------------------------------------------------------- the sweep
const ARROWHEADS = [
  "arrow",
  "triangle",
  "triangle_outline",
  "circle",
  "circle_outline",
  "diamond",
  "diamond_outline",
  "bar",
  "cardinality_one",
  "cardinality_many",
  "cardinality_one_or_many",
  "cardinality_exactly_one",
  "cardinality_zero_or_one",
  "cardinality_zero_or_many",
];

// Chosen for what they exercise, not to look random: a short and a long straight run (the
// `length * lengthMultiplier` scale-down bites only on the short one), a near-degenerate
// one, and two multi-point paths so `getCurvePathOps`'s chosen op has a `bcurveTo` (not a
// `move`) before it and `element.points[1]` / `[len-2]` are not the last point either.
const POINT_CONFIGS = [
  { name: "short", points: [[0, 0], [40, 0]] },
  { name: "long_diagonal", points: [[0, 0], [300, 150]] },
  { name: "near_degenerate", points: [[0, 0], [5, 2]] },
  { name: "three_point", points: [[0, 0], [50, 80], [150, 10]] },
  { name: "four_point_negative", points: [[0, 0], [-40, 60], [-100, 20], [-160, 90]] },
];

const STROKE_WIDTHS = [0.5, 1, 2, 4, 8];
const ROUGHNESS = [0, 1, 2];
const SEEDS = [1, 12345, 2147483647];
const POSITIONS = ["start", "end"];

const cases = [];
const addCase = (config, roundness, strokeWidth, roughness, seed, position, arrowhead, offsetMultiplier) => {
  const drawable = bodyDrawable(config.points, roundness, seed, roughness, strokeWidth);
  const element = { points: config.points, strokeWidth };
  const result = getArrowheadPoints(element, [drawable], position, arrowhead, offsetMultiplier);
  cases.push({
    config: config.name,
    points: config.points,
    roundness,
    strokeWidth,
    roughness,
    seed,
    position,
    arrowhead,
    offsetMultiplier,
    expected: result,
  });
};

// Primary sweep: every head, every point shape, a spread of stroke widths / roughness /
// seeds, both ends. offsetMultiplier pinned at 0 — the default every direct head uses.
for (const config of POINT_CONFIGS) {
  for (const roundness of [false, true]) {
    for (const strokeWidth of STROKE_WIDTHS) {
      for (const roughness of ROUGHNESS) {
        for (const seed of SEEDS) {
          for (const position of POSITIONS) {
            for (const arrowhead of ARROWHEADS) {
              addCase(config, roundness, strokeWidth, roughness, seed, position, arrowhead, 0);
            }
          }
        }
      }
    }
  }
}

// offsetMultiplier sweep: the values the compound heads actually pass
// (`shape.ts@1118751f:390,507,519,524,537,543,554,561` — 0, -0.25, -0.5, 1.5), on one
// representative shape per head so the offset arithmetic itself is covered without
// multiplying the primary sweep by four.
const OFFSETS = [0, -0.25, -0.5, 1.5];
for (const config of [POINT_CONFIGS[0], POINT_CONFIGS[3]]) {
  for (const position of POSITIONS) {
    for (const arrowhead of ARROWHEADS) {
      for (const offsetMultiplier of OFFSETS) {
        addCase(config, true, 2, 1, 12345, position, arrowhead, offsetMultiplier);
      }
    }
  }
}

// getArrowheadSize / getArrowheadAngle: a direct dump, not swept — both are pure
// switches with no geometry to sweep.
const sizesAndAngles = Object.fromEntries(
  ARROWHEADS.map((arrowhead) => [arrowhead, { size: getArrowheadSize(arrowhead), angle: getArrowheadAngle(arrowhead) }]),
);

// ---------------------------------------------------------------- write
mkdirSync(dirname(FIXTURE), { recursive: true });
const head = {
  generator: "engine/tools/arrowhead-oracle/generate.mjs",
  oracle: { excalidraw: PIN, file: ORACLE_FILE },
  roughjs: "4.6.4",
  node: process.version,
  sizesAndAngles,
};
writeFileSync(FIXTURE, `${JSON.stringify(head, null, 1).slice(0, -2)},\n "cases": ${JSON.stringify(cases)}\n}\n`);

console.error(`[arrowhead-oracle] ${cases.length} cases, ${ARROWHEADS.length} heads -> ${FIXTURE}`);
