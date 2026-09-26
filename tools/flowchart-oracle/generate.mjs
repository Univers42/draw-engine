// Parity fixtures for keyboard flowcharts, produced by Excalidraw's OWN code.
//
//   EXCALIDRAW_DIR=/path/to/excalidraw [ORACLE_SHA=<sha>] \
//     node --experimental-transform-types --import ./register.mjs generate.mjs
//
// Drives the oracle's `AppFlowchart` (packages/excalidraw/components/App.flowchart.ts) —
// the real keydown/keyup state machine over the real `FlowChartCreator` and
// `FlowChartNavigator` (packages/element/src/flowchart.ts) — through a sweep of key
// sequences, and records after every event what the oracle holds: the pending cluster,
// the scene, the selection, and where the camera lands. Writes, relative to this directory,
//
//   ../../crates/draw-engine/tests/fixtures/flowchart.oracle.json
//
// which tests/ci_flowchart_oracle.rs replays against the Rust port.
//
// `AppFlowchart` talks to `App`, which is a React component in a .tsx file and cannot run
// here. The fake below implements the six members it calls. Two of them hold logic, and
// are transcribed line for line from App.tsx at the pin, calling the oracle's own helpers
// (`isElementCompletelyInViewport`, `getCommonBounds`, `zoomToFitBounds`,
// `getFrameChildrenInsertionIndex`): `revealIfHidden` (App.tsx:5197-5222) with the
// `setViewport` path it takes (App.viewport.ts:634-727 -> getConstrainedTargetViewport ->
// getTargetViewport, with no lock), and `insertNewElements` (App.tsx:7754-7782). The
// camera is recorded where that animation lands; the frames in between are the port's to
// get right, and ci_flowchart.rs checks their shape.
//
// Before anything is written, the oracle's own expectations are replayed
// (packages/element/tests/flowchart.test.tsx, packages/element/src/__tests__/
// flowchart.test.ts); any mismatch exits 2 and writes nothing.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(HERE, "..", "..", "crates", "draw-engine", "tests", "fixtures", "flowchart.oracle.json");

const fail = (message, code = 1) => {
  console.error(`[flowchart-oracle] ${message}`);
  process.exit(code);
};

// ---------------------------------------------------------------- the pinned oracle
const DIR = process.env.EXCALIDRAW_DIR || fail("EXCALIDRAW_DIR is not set");
const previousPin = existsSync(FIXTURE)
  ? JSON.parse(readFileSync(FIXTURE, "utf8")).oracle?.excalidraw
  : undefined;
const PIN = process.env.ORACLE_SHA || previousPin || fail("no pin: set ORACLE_SHA");
const HEAD = execFileSync("git", ["-c", "safe.directory=*", "-C", DIR, "rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
if (HEAD !== PIN) fail(`${DIR} is at ${HEAD}, the pin is ${PIN}`);

const ORACLE_FILES = [
  "packages/element/src/flowchart.ts",
  "packages/excalidraw/components/App.flowchart.ts",
  "packages/excalidraw/viewport.ts",
  "packages/element/src/index.ts",
  "packages/common/src/index.ts",
];
const load = (file) => import(pathToFileURL(join(DIR, file)).href);
const [, APP_FLOWCHART, VIEWPORT, E, C] = await Promise.all(ORACLE_FILES.map(load));

// ---------------------------------------------------------------- a fake App
// Only what App.flowchart.ts touches: scene, state, triggerRender, revealIfHidden,
// insertNewElements, setState, syncActionResult.

/** App.viewport.ts:586-606 `getOffsets` with nothing measured beyond `ui`: the padding. */
const PADDING = 24;

class FakeApp {
  constructor({ elements, selected, viewport }) {
    this.scene = new E.Scene(elements, { skipValidation: true });
    this.ui = viewport.ui;
    this.state = {
      width: viewport.width,
      height: viewport.height,
      offsetLeft: 0,
      offsetTop: 0,
      scrollX: viewport.scrollX,
      scrollY: viewport.scrollY,
      zoom: { value: viewport.zoom },
      selectedElementIds: Object.fromEntries(selected.map((id) => [id, true])),
      currentItemEndArrowhead: viewport.endArrowhead ?? "arrow",
    };
    this.canvas = { width: viewport.width, height: viewport.height };
    this.ownerWindow = { devicePixelRatio: 1 };
    this.reveals = [];
    this.flowchart = new APP_FLOWCHART.AppFlowchart(this);
  }

  triggerRender() {}

  syncActionResult() {}

  setState(update) {
    const patch = typeof update === "function" ? update(this.state) : update;
    if (patch) Object.assign(this.state, patch);
  }

  /** App.viewport.ts:502-607 `getOffsets()`: what is measured, plus the default padding. */
  getOffsets() {
    return {
      top: this.ui.top + PADDING,
      right: this.ui.right + PADDING,
      bottom: this.ui.bottom + PADDING,
      left: this.ui.left + PADDING,
    };
  }

  // App.tsx:5197-5222, verbatim but for `this.viewport.*`, which is the method above and
  // the landing point of `setViewport` below.
  revealIfHidden(elements) {
    if (
      !elements.length ||
      E.isElementCompletelyInViewport(
        elements,
        this.canvas.width / this.ownerWindow.devicePixelRatio,
        this.canvas.height / this.ownerWindow.devicePixelRatio,
        {
          offsetLeft: this.state.offsetLeft,
          offsetTop: this.state.offsetTop,
          scrollX: this.state.scrollX,
          scrollY: this.state.scrollY,
          zoom: this.state.zoom,
        },
        this.scene.getNonDeletedElementsMap(),
        this.getOffsets(),
      )
    ) {
      this.reveals.push({ bounds: [...E.getCommonBounds(elements)], target: null });
      return;
    }
    // setViewport({ target: getCommonBounds(elements), fit: "scale-down",
    //   animation: { duration: 300 }, offsets: { ui: true } }) — the viewport its animation
    // ends on (App.viewport.ts:667-671, :714-724): `resolveOffsets({ ui: true })` is
    // `getOffsets()`, and with no `lock` the target is `zoomToFitBounds` unconstrained.
    const bounds = E.getCommonBounds(elements);
    const { appState } = VIEWPORT.zoomToFitBounds({
      bounds,
      appState: this.state,
      fit: "scale-down",
      canvasOffsets: this.getOffsets(),
      steppedZoom: false,
    });
    const target = { scrollX: appState.scrollX, scrollY: appState.scrollY, zoom: appState.zoom };
    this.reveals.push({ bounds: [...bounds], target: { ...target, zoom: target.zoom.value } });
    Object.assign(this.state, target);
  }

  // App.tsx:7754-7782, verbatim.
  insertNewElements(elements) {
    if (!elements.length) {
      return;
    }
    const chunkedElements = [];
    for (const element of elements) {
      const currentChunk = chunkedElements[chunkedElements.length - 1];
      if (currentChunk?.[0].frameId === element.frameId) {
        currentChunk.push(element);
      } else {
        chunkedElements.push([element]);
      }
    }
    for (const chunk of chunkedElements) {
      const frameId = chunk[0].frameId;
      const insertionIndex = frameId
        ? E.getFrameChildrenInsertionIndex(this.scene.getElementsIncludingDeleted(), frameId)
        : null;
      this.scene.insertElementsAtIndex(chunk, insertionIndex);
    }
  }
}

// ---------------------------------------------------------------- recording
/** The fields the port is held to, as the oracle has them — an arrow's elbow route
 *  included (see the fixture's `note`). */
const snapshot = (el) => {
  const out = {
    id: el.id,
    type: el.type,
    x: el.x,
    y: el.y,
    width: el.width,
    height: el.height,
    angle: el.angle,
    strokeColor: el.strokeColor,
    backgroundColor: el.backgroundColor,
    fillStyle: el.fillStyle,
    strokeWidth: el.strokeWidth,
    strokeStyle: el.strokeStyle,
    roughness: el.roughness,
    opacity: el.opacity,
    roundness: el.roundness,
    frameId: el.frameId,
    groupIds: el.groupIds,
    locked: el.locked,
    isDeleted: el.isDeleted,
  };
  if (el.type === "stickynote") out.baseHeight = el.baseHeight;
  if (el.type === "arrow" || el.type === "line") {
    out.points = el.points;
    out.startBinding = el.startBinding ?? null;
    out.endBinding = el.endBinding ?? null;
    out.startArrowhead = el.startArrowhead ?? null;
    out.endArrowhead = el.endArrowhead ?? null;
    out.elbowed = !!el.elbowed;
  }
  return out;
};

const selectedIds = (app) =>
  Object.keys(app.state.selectedElementIds).filter((id) => app.state.selectedElementIds[id]);

const KEY = { right: "ArrowRight", left: "ArrowLeft", up: "ArrowUp", down: "ArrowDown" };

function run(scenario) {
  C.reseed(7);
  const built = scenario.build();
  const app = new FakeApp({
    elements: built.elements,
    selected: built.selected ?? [],
    viewport: { width: 1000, height: 800, scrollX: 0, scrollY: 0, zoom: 1, ui: { top: 0, right: 0, bottom: 0, left: 0 }, ...scenario.viewport },
  });
  const held = { ctrlKey: false, altKey: false, shiftKey: false };
  const initial = app.scene.getElementsIncludingDeleted().map(snapshot);
  const steps = [];
  let previousScene = JSON.stringify(initial);
  const fire = (type, key) => {
    app.flowchart.handleKeyEvent({ type, key, ...held, metaKey: false, preventDefault() {} });
  };
  for (let step of scenario.steps(built)) {
    const revealsBefore = app.reveals.length;
    if (step.op === "select") {
      // `at`: positions in the live scene, negative from the end, for nodes the sweep made.
      const live = app.scene.getNonDeletedElements();
      step = { op: "select", ids: step.ids ?? step.at.map((i) => live.at(i).id) };
      app.setState({ selectedElementIds: Object.fromEntries(step.ids.map((id) => [id, true])) });
    } else if (step.op === "hold") {
      held[step.modifier] = true;
    } else if (step.op === "release") {
      held[step.modifier] = false;
      fire("keyup", { ctrlKey: "Control", altKey: "Alt", shiftKey: "Shift" }[step.modifier]);
    } else if (step.op === "press") {
      // Keyboard.keyPress in the oracle's own tests: keydown, then keyup of the same key.
      const key = KEY[step.key] ?? step.key;
      fire("keydown", key);
      fire("keyup", key);
    } else {
      fail(`unknown step ${JSON.stringify(step)}`);
    }
    const scene = app.scene.getElementsIncludingDeleted().map(snapshot);
    const sceneJson = JSON.stringify(scene);
    const reveal = app.reveals.length > revealsBefore ? app.reveals[app.reveals.length - 1] : undefined;
    steps.push({
      ...step,
      pending: (app.flowchart.pendingNodes ?? []).map(snapshot),
      creating: app.flowchart.isCreatingChart,
      // Only when it moved: absent means "as at the step before".
      ...(sceneJson === previousScene ? {} : { scene }),
      selected: selectedIds(app),
      // What was revealed, if anything was asked to be: its bounds, and where the camera
      // went — `target: null` when it was already wholly in view.
      ...(reveal === undefined ? {} : { reveal }),
      camera: { scrollX: app.state.scrollX, scrollY: app.state.scrollY, zoom: app.state.zoom.value },
    });
    previousScene = sceneJson;
  }
  return { name: scenario.name, viewport: { width: app.state.width, height: app.state.height, ...scenario.viewport }, initial, selected: built.selected ?? [], steps, app };
}

// ---------------------------------------------------------------- scenarios
const node = (type, props = {}) =>
  type === "stickynote"
    ? E.newStickyNoteElement({ type, x: 0, y: 0, width: 240, height: 260, baseHeight: 220, backgroundColor: "#ffec99", ...props })
    : type === "frame"
      ? E.newFrameElement({ x: 0, y: 0, width: 600, height: 400, ...props })
      : E.newElement({ type, x: 0, y: 0, width: 200, height: 100, ...props });

const hold = { op: "hold", modifier: "ctrlKey" };
const release = { op: "release", modifier: "ctrlKey" };
const holdAlt = { op: "hold", modifier: "altKey" };
const releaseAlt = { op: "release", modifier: "altKey" };
const press = (key) => ({ op: "press", key });
const select = (...ids) => ({ op: "select", ids });
const selectAt = (...at) => ({ op: "select", at });
/** One commit of `count` presses toward `key`, from `id`. */
const grow = (id, key, count = 1) => [select(id), hold, ...Array(count).fill(press(key)), release];

const scenarios = [];
const add = (name, build, steps, viewport) => scenarios.push({ name, build, steps, viewport });
const single = (type, props) => () => {
  const parent = node(type, props);
  return { elements: [parent], selected: [parent.id], parent };
};

// Every direction, one press, one commit — for every node kind and a spread of sizes,
// strokes and styles. The parent is copied: size, roundness, roughness, colours, stroke,
// opacity, fill, stroke style, and a sticky note's base height.
const STYLES = [
  {},
  { strokeColor: "#e03131", backgroundColor: "#ffc9c9", fillStyle: "cross-hatch", strokeWidth: 4, strokeStyle: "dashed", roughness: 2, opacity: 60 },
  { strokeWidth: 1, strokeStyle: "dotted", roughness: 0, fillStyle: "hachure", roundness: { type: 3 } },
  { roundness: { type: 3, value: 24 }, opacity: 100, backgroundColor: "#a5d8ff", fillStyle: "zigzag" },
];
for (const type of ["rectangle", "ellipse", "diamond"]) {
  for (const [w, h] of [[200, 100], [100, 200], [60, 60], [10, 30]]) {
    for (const [i, style] of STYLES.entries()) {
      for (const dir of ["right", "down", "left", "up"]) {
        add(`${type} ${w}x${h} style${i} ${dir}`, single(type, { x: 37, y: -12, width: w, height: h, ...style }), ({ parent }) => grow(parent.id, dir));
      }
    }
  }
}
for (const dir of ["right", "down", "left", "up"]) {
  add(`stickynote ${dir}`, single("stickynote", { x: 100, y: 100, roughness: 2, strokeWidth: 2 }), ({ parent }) => grow(parent.id, dir));
}
// A turned parent: placed by its unturned box, the new nodes are not turned, and the
// obstacles are judged by their turned boxes.
for (const angle of [0.3, 1.2, Math.PI / 2]) {
  for (const type of ["rectangle", "ellipse", "diamond"]) {
    for (const dir of ["right", "down", "left", "up"]) {
      add(`${type} turned ${angle.toFixed(3)} ${dir}`, single(type, { x: 10, y: 20, width: 160, height: 90, angle }), ({ parent }) => grow(parent.id, dir));
    }
  }
}

// Repeated presses fan out siblings, and the pending nodes already shown stay put.
for (const count of [2, 3, 4, 5]) {
  for (const dir of ["right", "down", "left", "up"]) {
    add(`rectangle x${count} ${dir}`, single("rectangle", { width: 200, height: 100 }), ({ parent }) => grow(parent.id, dir, count));
  }
}
add("diamond x3 down", single("diamond", { width: 140, height: 80 }), ({ parent }) => grow(parent.id, "down", 3));
add("ellipse x4 right", single("ellipse", { width: 90, height: 150 }), ({ parent }) => grow(parent.id, "right", 4));

// Changing direction mid-hold restarts the cluster at one; only the last direction lands.
add("direction change (oracle test)", single("rectangle", { width: 200, height: 100 }), ({ parent }) => [
  select(parent.id), hold,
  press("right"), press("right"), press("right"), press("left"), press("up"), press("up"), press("up"),
  release,
]);
add("direction pivot right-down-left-up", single("rectangle", { width: 120, height: 80 }), ({ parent }) => [
  select(parent.id), hold, press("right"), press("down"), press("left"), press("up"), press("up"), press("right"), release,
]);
add("escape cancels", single("rectangle", { width: 200, height: 100 }), ({ parent }) => [
  select(parent.id), hold, press("right"), press("left"), press("up"), press("down"), press("Escape"), release,
]);
add("escape then again", single("rectangle", { width: 200, height: 100 }), ({ parent }) => [
  select(parent.id), hold, press("right"), press("right"), press("Escape"), press("down"), release,
]);
add("modifier with shift does not create", single("rectangle"), ({ parent }) => [
  select(parent.id), hold, { op: "hold", modifier: "shiftKey" }, press("right"), { op: "release", modifier: "shiftKey" }, release,
]);

// Nothing to grow from.
add("nothing selected", single("rectangle"), () => [select(), hold, press("right"), release]);
add("two selected", () => {
  const a = node("rectangle");
  const b = node("rectangle", { x: 400 });
  return { elements: [a, b], selected: [a.id, b.id], a, b };
}, ({ a, b }) => [select(a.id, b.id), hold, press("right"), release]);
add("a line is not a node", () => {
  const line = E.newLinearElement({ type: "line", x: 0, y: 0, points: [[0, 0], [120, 40]] });
  return { elements: [line], selected: [line.id], line };
}, ({ line }) => [select(line.id), hold, press("right"), release]);

// Existing children in the way: sequential commits stack, a gap is found, a batch clears
// a batch, a sibling reached through a shared parent is avoided (#8518).
add("one at a time (oracle test)", single("rectangle", { width: 200, height: 100 }), ({ parent }) => [
  ...grow(parent.id, "right"), ...grow(parent.id, "right"), ...grow(parent.id, "right"),
]);
add("four children down (oracle test)", single("rectangle", { width: 400, height: 300 }), ({ parent }) =>
  [0, 1, 2, 3].flatMap(() => grow(parent.id, "down")),
);
add("batch then batch (oracle test)", single("rectangle", { width: 400, height: 300 }), ({ parent }) => [
  ...grow(parent.id, "down", 3), ...grow(parent.id, "down", 2),
]);
add("every side twice", single("rectangle", { width: 160, height: 90 }), ({ parent }) =>
  ["right", "down", "left", "up", "right", "down", "left", "up"].flatMap((dir) => grow(parent.id, dir)),
);
add("mixed shapes chain", single("ellipse", { width: 150, height: 90 }), ({ parent }) => [
  ...grow(parent.id, "right", 2), ...grow(parent.id, "down"), ...grow(parent.id, "right", 3),
]);

// Grow from a child, not only from the root: each commit selects its first new node, and
// the next press grows from there. The obstacle set is the whole connected flowchart.
const chainFromSelection = (dirs) => ({ parent }) => [select(parent.id), ...dirs.flatMap((dir) => [hold, press(dir), release])];
add("chain from the new selection", single("rectangle", { width: 200, height: 100 }), chainFromSelection(["right", "down", "right", "up", "right"]));
add("chain back onto itself", single("rectangle", { width: 120, height: 60 }), chainFromSelection(["right", "down", "left", "left", "up", "up", "right"]));

// Frames: every pending node inside or overlapping the parent's frame joins it, and is
// inserted with the frame's children; one falling outside keeps all of them out.
const inFrame = (frameProps, parentProps) => () => {
  const frame = node("frame", frameProps);
  const other = node("rectangle", { x: frameProps.x + 10, y: frameProps.y + 10, width: 40, height: 30, frameId: frame.id });
  const parent = node("rectangle", { width: 120, height: 60, frameId: frame.id, ...parentProps });
  const outside = node("ellipse", { x: 5000, y: 5000, width: 50, height: 50 });
  return { elements: [other, parent, frame, outside], selected: [parent.id], parent, frame };
};
add("frame: lands inside", inFrame({ x: 0, y: 0, width: 800, height: 600 }, { x: 100, y: 100 }), ({ parent }) => grow(parent.id, "right"));
add("frame: straddles the edge", inFrame({ x: 0, y: 0, width: 400, height: 600 }, { x: 100, y: 100 }), ({ parent }) => grow(parent.id, "right"));
add("frame: falls outside", inFrame({ x: 0, y: 0, width: 300, height: 600 }, { x: 100, y: 100 }), ({ parent }) => grow(parent.id, "right"));
add("frame: some in, some out", inFrame({ x: 0, y: 0, width: 800, height: 330 }, { x: 100, y: 100 }), ({ parent }) => grow(parent.id, "right", 3));

// The camera. `revealIfHidden` after every press (the pending cluster) and at the commit
// (the first new node): nothing when already fully in view past the UI and padding;
// otherwise a `scale-down` fit — never past 100%, so it zooms *in* up to 100% as readily
// as out — centred in the space the UI leaves.
add("camera: in view, no move", single("rectangle", { x: 100, y: 100 }), ({ parent }) => grow(parent.id, "right", 2));
add("camera: off the right edge", single("rectangle", { x: 700, y: 300 }), ({ parent }) => grow(parent.id, "right"));
add("camera: off the bottom, zoomed in", single("rectangle", { x: 100, y: 200 }), ({ parent }) => grow(parent.id, "down", 2), { zoom: 2 });
add("camera: zoomed out, zooms in", single("rectangle", { x: 2900, y: 100 }), ({ parent }) => grow(parent.id, "right"), { zoom: 0.35 });
add("camera: cluster outgrows the screen", single("rectangle", { x: 300, y: 300 }), ({ parent }) => grow(parent.id, "down", 6));
add("camera: under the UI", single("rectangle", { x: 200, y: 20 }), ({ parent }) => grow(parent.id, "up"), { ui: { top: 60, right: 0, bottom: 0, left: 230 } });
add("camera: scrolled and small zoom", single("ellipse", { x: -400, y: -300, width: 90, height: 60 }), ({ parent }) => grow(parent.id, "left", 3), { scrollX: 500, scrollY: 250, zoom: 0.8, ui: { top: 62, right: 0, bottom: 0, left: 232 } });
add("camera: tall viewport", single("diamond", { x: 400, y: 600, width: 140, height: 90 }), ({ parent }) => grow(parent.id, "down"), { ui: { top: 60, right: 0, bottom: 0, left: 0 } });

// Alt+Arrow: the oracle's three navigation tests, then cycling, pivoting and falling back.
const navAfter = (build, dirs) => [...build, holdAlt, ...dirs.map(press), releaseAlt];
add("navigate: single node at each level (oracle test)", single("rectangle", { width: 200, height: 100 }), ({ parent }) => [
  ...chainFromSelection(["right", "right", "right", "right"])({ parent }),
  holdAlt, press("left"), press("left"), press("left"), press("left"), releaseAlt,
  holdAlt, press("right"), press("right"), press("right"), press("right"), releaseAlt,
]);
add("navigate: multiple nodes at each level (oracle test)", single("rectangle", { width: 200, height: 100 }), ({ parent }) => [
  ...chainFromSelection(["right", "right", "right", "right"])({ parent }),
  ...grow(parent.id, "right"), ...grow(parent.id, "right"), ...grow(parent.id, "right"),
  select(parent.id),
  holdAlt, press("right"), press("right"), press("right"), press("right"), press("right"), releaseAlt,
  holdAlt, press("right"), press("right"), press("right"), releaseAlt,
  holdAlt, press("left"), press("left"), press("left"), press("left"), releaseAlt,
]);
add("navigate: most obvious link (oracle test)", single("rectangle", { width: 200, height: 100 }), ({ parent }) => [
  ...chainFromSelection(["right", "down", "right", "up", "right"])({ parent }),
  holdAlt, press("left"), press("left"), press("left"), press("left"), press("left"), releaseAlt,
  // Any direction from the last node reaches its predecessor.
  ...["right", "up", "down"].flatMap((dir) => [selectAt(-2), holdAlt, press(dir), releaseAlt]),
]);
add("navigate: fan out and cycle", single("diamond", { width: 140, height: 90 }), ({ parent }) =>
  navAfter([...grow(parent.id, "down", 3), select(parent.id)], ["down", "down", "down", "down", "up", "left", "right"]),
);
add("navigate: every direction from a hub", single("ellipse", { width: 120, height: 80 }), ({ parent }) =>
  navAfter(
    [...grow(parent.id, "right"), ...grow(parent.id, "down", 2), ...grow(parent.id, "left"), ...grow(parent.id, "up", 2), select(parent.id)],
    ["up", "up", "up", "left", "down", "down", "right", "right"],
  ),
);
add("navigate: fallback to any unvisited link", single("rectangle", { width: 100, height: 60 }), ({ parent }) =>
  navAfter([...grow(parent.id, "right"), select(parent.id)], ["up", "up", "left", "down"]),
);
add("navigate: nothing linked", single("rectangle"), ({ parent }) => [select(parent.id), holdAlt, press("right"), press("up"), releaseAlt]);
add("navigate: off screen follows", single("rectangle", { x: 600, y: 300, width: 200, height: 100 }), ({ parent }) => [
  ...chainFromSelection(["right", "right", "right"])({ parent }),
  select(parent.id), holdAlt, press("right"), press("right"), press("right"), press("left"), releaseAlt,
]);

// ---------------------------------------------------------------- the oracle's own tests
const results = scenarios.map(run);
const byName = Object.fromEntries(results.map((r) => [r.name, r]));
const expect = (ok, what) => {
  if (!ok) fail(`the oracle's own expectation failed: ${what}`, 2);
};
/** The scene as it stands after a case's last step (steps only carry it when it moved). */
const finalScene = (r) => r.steps.findLast((s) => s.scene)?.scene ?? r.initial;
const last = (name) => ({ scene: finalScene(byName[name]) });
const liveOf = (step, type) => step.scene.filter((el) => !el.isDeleted && (!type || el.type === type));
{
  // flowchart.test.tsx:310-334 — the first child exactly one offset away.
  for (const [dir, x, y] of [["right", 300, 0], ["left", -300, 0], ["down", 0, 200], ["up", 0, -200]]) {
    const r = run({ name: "check", build: single("rectangle", { width: 200, height: 100 }), steps: ({ parent }) => grow(parent.id, dir) });
    const child = liveOf({ scene: finalScene(r) }, "rectangle").find((el) => el.id !== r.initial[0].id);
    expect(child && child.x === x && child.y === y, `first child ${dir} at ${x},${y}`);
  }
  // :49-60, :62-80, :82-94
  expect(liveOf(last("rectangle x2 right")).length === 5, "two presses make 5 elements");
  expect(liveOf(last("direction change (oracle test)")).length === 7, "direction change keeps the last run");
  expect(liveOf(last("escape cancels")).length === 1, "escape adds nothing");
  // :366-394 — pending nodes stay put while the cluster grows.
  const grows = byName["rectangle x3 right"].steps.filter((s) => s.op === "press");
  const rects = (s) => s.pending.filter((el) => el.type === "rectangle").map((el) => [el.x, el.y]);
  expect(JSON.stringify(grows.map(rects)) === JSON.stringify([[[300, 0]], [[300, 0], [300, 200]], [[300, -200], [300, 0], [300, 200]]]), "cluster grows in place");
  // :156-157, :179-194 — sequential children share a column / a row, and never overlap.
  const oneAtATime = liveOf(last("one at a time (oracle test)"), "rectangle").slice(1);
  expect(new Set(oneAtATime.map((el) => el.x)).size === 1, "one at a time: same column");
  const four = liveOf(last("four children down (oracle test)"), "rectangle").slice(1);
  expect(new Set(four.map((el) => el.y)).size === 1, "four children: same row");
  // Navigation (:435-682): where each Alt run ends.
  const selAfterRelease = (name) => byName[name].steps.filter((s) => s.op === "release" && s.modifier === "altKey").map((s) => s.selected[0]);
  const single_ = byName["navigate: single node at each level (oracle test)"];
  const singleRects = liveOf({ scene: finalScene(single_) }, "rectangle");
  expect(JSON.stringify(selAfterRelease(single_.name)) === JSON.stringify([singleRects[0].id, singleRects.at(-1).id]), "single node at each level");
  const multi = byName["navigate: multiple nodes at each level (oracle test)"];
  const multiLive = liveOf({ scene: finalScene(multi) });
  const multiRects = multiLive.filter((el) => el.type === "rectangle");
  expect(JSON.stringify(selAfterRelease(multi.name)) === JSON.stringify([multiLive[1].id, multiRects[4].id, multiRects[0].id]), "multiple nodes at each level");
  const obvious = byName["navigate: most obvious link (oracle test)"];
  const obviousLive = liveOf({ scene: finalScene(obvious) });
  const [toFirst, ...toPredecessor] = selAfterRelease(obvious.name);
  expect(toFirst === obviousLive[0].id, "most obvious link");
  expect(toPredecessor.every((id) => id === obviousLive.at(-4).id) && toPredecessor.length === 3, "any direction reaches the predecessor");
  // __tests__/flowchart.test.ts — a sticky note grows a sticky note, bound both ends.
  const sticky = last("stickynote right").scene;
  expect(sticky[1].type === "stickynote" && sticky[1].baseHeight === 220 && sticky[2].startBinding.elementId === sticky[0].id, "sticky notes");
}

// ---------------------------------------------------------------- write
const fixture = {
  oracle: {
    excalidraw: PIN,
    files: ORACLE_FILES,
    node: process.version,
    generator: "engine/tools/flowchart-oracle/generate.mjs",
  },
  note:
    "Replayed by tests/ci_flowchart_oracle.rs. The oracle's flowchart arrow is elbow-routed " +
    "(flowchart.ts:383), and so is the engine's: node geometry and style, frame membership, " +
    "scene order, each arrow's position, size, points, anchors and bindings, arrowheads, " +
    "selection, the pending cluster at every press, and the camera are compared within " +
    "1e-9. Two exceptions. The camera is compared wherever a reveal measured the same " +
    "bounds on both sides, which it does not when the oracle's include an arrow's " +
    "rough-path wobble. And an arrow at a diamond with an explicit roundness value keeps " +
    "only its bindings compared: the engine reads that radius past getCornerRadius's cap on " +
    "purpose (its corner-radius handle), so the rounded tip an anchor snaps to differs.",
  cases: results.map(({ app, ...rest }) => rest),
};
writeFileSync(FIXTURE, `${JSON.stringify(fixture, null, 1)}\n`);
console.log(`[flowchart-oracle] ${results.length} cases, ${results.reduce((n, r) => n + r.steps.length, 0)} steps -> ${FIXTURE}`);
