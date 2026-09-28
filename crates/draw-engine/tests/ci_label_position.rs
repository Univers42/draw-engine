//! Dragging an arrow's label along the arrow.
//!
//! # What the oracle does, and which branch reaches it
//!
//! Excalidraw at `1118751f` **does** have a draggable label position, and the schema field
//! is real: `ExcalidrawTextElement.labelPosition?: number | null`
//! (`packages/element/src/types.ts@1118751f:290`). The plan's "the oracle's label position,
//! if it has one at the pin" is a yes, and the gesture is not a key and not a menu — it is a
//! **plain primary-button drag on the label**, reachable only through this chain:
//!
//! 1. `App.tsx@1118751f:8445` — hover: `arrowText.isBoundTextGrabbable` sets
//!    `CURSOR_TYPE.GRAB`. The point handles and the segment-midpoint knob are tested first
//!    and win, "so a labeled arrow can still be bent at its middle".
//! 2. `App.tsx@1118751f:9406-9442` — pointer down: the bounding-box handles are taken first;
//!    only a press that missed them reaches `LinearElementEditor.handlePointerDown`.
//! 3. `linearElementEditor.ts@1118751f:1150-1183` — inside that, the label is grabbed only
//!    when `!clickedPointIsHandle && !segmentMidpoint && boundTextElement && isArrowElement`
//!    and the press is on the label. The grab offset is measured from the label's **centre**:
//!
//!    ```ts
//!    boundTextGrabOffset = { x: scenePointer.x - (textX + boundTextElement.width / 2), ... }
//!    ```
//!
//!    and it becomes `pointerOffset` (`:1234-1242`), so the anchor follows the pointer by the
//!    delta from the grab and not by the pointer itself.
//! 4. `App.tsx@1118751f:10766` — pointer move: `arrowText.maybeDragLabel`, which owns the move
//!    whenever the press landed on the label, and drags only past
//!    `DRAGGING_THRESHOLD / zoom`.
//! 5. `linearElementEditor.ts@1118751f:1963-2030` — `handleBoundTextDragging`, which writes
//!    `labelPosition` **and** the label's `x`/`y` on every move.
//!
//! So: **an arrow, selected, with the line editor's handles showing; a press on its label
//! away from a point handle and the midpoint knob; then a drag.** A line's label does not
//! move (`isArrowElement` is in the guard), and a press on the label of an arrow that is
//! not the selection is a plain click on the label.
//!
//! # What the number is
//!
//! A **fraction of the arrow's own path arc length**, in `[0, 1]`:
//!
//! ```ts
//! const labelPosition = clamp(
//!   (prefixSums[bestSegmentIndex] + lengthWithinSegment) / totalLength, 0, 1);   // :2011
//! const targetLength = clamp(pathParameter, 0, 1) * totalLength;                // :2050
//! ```
//!
//! Not an offset in world units, not a fraction of the bounding box, not an index into a set
//! of slots, and not a parameter along the polyline's chords. The path is
//! `getLinearElementPathSegments` (`packages/element/src/utils.ts@1118751f:206-233`): the
//! **drawn curve** for a rounded arrow, the unrounded logical polyline for an elbow one
//! (`:211-227`), the chords for a sharp one. Its pieces are measured by arc length — a cubic
//! by integration, `curveLength` (`packages/math/src/curve.ts@1118751f:450-468`) — so
//! `0.5` is half the *ground covered*, not half the parameter.
//!
//! # The ends
//!
//! `0` and `1` are **legal and reachable**. Three separate clamps, all to `[0, 1]`: on write
//! (`:2011`), on read (`:2050`), and on load (`restore.ts@1118751f:573-575`, which drops a
//! non-finite one to `null` and clamps a finite one). `0` is the path's first point and `1`
//! its last. There is no dead zone at either end.
//!
//! # 5.5, and whether a drag can change convertibility
//!
//! It cannot, and for a reason worth stating because the connection is not obvious from
//! either side: `isEligibleLinearElement`
//! (`ConvertElementTypePopup.tsx@1118751f:666-672`) refuses an arrow with
//! `hasBoundTextElement(element)`. `labelPosition` is a field **on the label**, and a
//! labelled arrow is an arrow that *has* a bound text element — so the guard is on the
//! label's existence, not on where it sits. Dragging a label cannot make a labelled arrow
//! convertible and cannot make an unlabelled one non-convertible; the two features share
//! the label and nothing else. `a_dragged_label_does_not_make_the_arrow_convertible` says
//! so, and also covers a case 5.5 left untested: a labelled but *unbound* arrow.
//!
//! # What was here already
//!
//! - `text::layout::linear_label_center` (`text/layout.rs:285`) — the **only** label placement
//!   for a line or arrow, reached from `bound_text_position` (`:305`), which every bind, every
//!   re-layout and every text edit goes through. It was extended, not duplicated; there is no
//!   second place a linear label can be put.
//! - `math::bezier_point_at_fraction` (`math.rs:193`) — a point a fraction of the way **along**
//!   a cubic, already documented as the substitute for the oracle's `curvePointAtLength`. The
//!   arc-length table it walks was factored out into `bezier_length` / `bezier_length_to`, and
//!   both now share one walk; the numbers are unchanged, so 5.1′'s midpoint handles still sit
//!   where they did.
//! - `selection::linear::handle_points` already rebuilt the same cubics from the world points.
//!   `path_cubics` is that rebuild, named, so the sharp/curved/elbow fork is stated once
//!   instead of being implied by each caller.
//!
//! # How these are checked, and what they are checked against
//!
//! Three agents in two nights shipped a self-check that shared code with the thing it
//! checked, so this file uses **two** independent sources and says which is which:
//!
//! 1. **Hand-computed constants in a committed fixture** — `fixtures/label-position.json`.
//!    Every number there was worked out on paper from the path's own definition; nothing
//!    generates it and nothing reads it back from the engine. This is the one that settles
//!    the unit, because the right-angle case is built so that a box fraction, a per-segment
//!    fraction and a world-unit offset all give different, wrong answers, and
//!    `the_hand_computed_numbers_rule_the_wrong_readings_out` proves they do.
//! 2. **A second implementation of the point-on-curve maths**, in this file: the path's
//!    length integrated by Simpson's rule on `|B'(t)|` and the fraction located by a linear
//!    scan, where the engine walks a 64-point polyline and interpolates. Different
//!    quadrature, different search, same published Bezier. It is used where a *curve* is
//!    needed — a bowed arrow, many points, a turned one — and only to within half a pixel,
//!    because two honest quadratures of one integral differ slightly and pretending
//!    otherwise would only invite a magic tolerance.
//!
//! The pair that cannot fail here is a Q/R whose halves are compared on different points, so
//! every drag case is written as: **drag somewhere specific, assert the exact world point**,
//! paired with **the same scene with no drag, which must land on the default**. Both halves
//! are on the same scene, in this file, from the same measurement.

mod common;
use common::*;
use draw_engine::scene::element::DrawElementType;
use draw_engine::selection::linear;
use draw_engine::text::layout::bound_text_position;
use draw_engine::*;
use serde::Deserialize;

const ARROW: &str = "arrow";
const LABEL: &str = "label";

fn id_of(mut element: DrawElement, id: &str) -> DrawElement {
    element.id = id.into();
    element
}

/// A straight arrow of `length` from the origin, sharp unless `roundness` says otherwise.
fn straight(length: f64, roundness: Option<f64>) -> DrawElement {
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: length,
            height: 0.0,
        },
    );
    arrow.points = Some(vec![[0.0, 0.0], [length, 0.0]]);
    arrow.roundness = roundness;
    id_of(arrow, ARROW)
}

/// An arrow with a real bend in it, so its curve and its chords are not the same line.
fn bowed() -> DrawElement {
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        },
    );
    arrow.points = Some(vec![[0.0, 0.0], [100.0, 100.0], [200.0, 0.0]]);
    id_of(arrow, ARROW)
}

/// A five-point arrow with three bends: four chords, so the total is not any one of them
/// and a per-segment reading is off by construction.
fn many_point() -> DrawElement {
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 200.0,
        },
    );
    arrow.points = Some(vec![
        [0.0, 0.0],
        [100.0, 120.0],
        [220.0, 40.0],
        [300.0, 200.0],
        [400.0, 60.0],
    ]);
    arrow.roundness = None;
    id_of(arrow, ARROW)
}

/// A label holding one line of "label": 54 by 25 as `measure_text` measures it at size 20.
fn words(id: &str) -> DrawElement {
    let mut label = id_of(text_at(0.0, 0.0, 54.0, 25.0), id);
    label.text = Some("label".into());
    label.original_text = Some("label".into());
    label
}

/// `arrow` with `label` held on it, **and the label already where the engine puts it**.
///
/// The position is not decoration: a press is aimed at the label's box, so a scene whose
/// label sits at the origin would be a scene where the label is somewhere nobody drew it.
/// Laying it out through [`bound_text_position`] is also the honest way to build the
/// starting state, because that is the one function every bind and every re-layout goes
/// through — the same one `with_no_drag_the_label_stays_on_the_default` reads back.
fn hold(arrow: &DrawElement, label: DrawElement) -> Vec<DrawElement> {
    let mut arrow = arrow.clone();
    arrow.bound_text_id = Some(label.id.clone());
    let mut label = label;
    let at = bound_text_position(&arrow, &label);
    label.x = at.x;
    label.y = at.y;
    label.container_id = Some(arrow.id.clone());
    vec![arrow, label]
}

/// The label as the engine makes one on `arrow`, told to sit at `fraction` where `None` is
/// "nobody has said", which is the middle.
fn label_at(fraction: Option<f64>) -> DrawElement {
    let mut label = words(LABEL);
    label.label_position = fraction;
    label
}

/// `arrow`'s label laid out, as [`bound_text_position`] puts it: the centre of the words.
fn anchored(arrow: &DrawElement, fraction: Option<f64>) -> [f64; 2] {
    let label = label_at(fraction);
    let at = bound_text_position(arrow, &label);
    [at.x + label.width / 2.0, at.y + label.height / 2.0]
}

/// `label` as a scene element would hold it, laid out from `arrow` at the position `label`
/// carries. The reload half of the round trip, without a whole engine.
fn relaid(arrow: &DrawElement, label: &DrawElement) -> DrawElement {
    let mut placed = label.clone();
    let at = bound_text_position(arrow, label);
    placed.x = at.x;
    placed.y = at.y;
    placed
}

/// An engine holding `arrow` with its label, the arrow selected.
fn engine(arrow: DrawElement) -> DrawEngine {
    let mut engine = engine_with_measure(hold(&arrow, label_at(None)));
    engine.select(vec![arrow.id.clone()]);
    engine
}

fn element_of(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .unwrap_or_else(|| panic!("{id} should be in the scene"))
}

/// Where the label's words are centred, which is what the path question is about.
fn centre(label: &DrawElement) -> Point {
    Point {
        x: label.x + label.width / 2.0,
        y: label.y + label.height / 2.0,
    }
}

/// A press on the label `dx` to the right of its own centre, and a drag to `(to_x, to_y)`.
///
/// The press is off-centre on purpose: at the default the label sits under the line editor's
/// own midpoint knob, and the knob wins. That is the oracle's precedence, not an accident,
/// and `a_point_handle_keeps_precedence_over_the_label` pins it.
fn drag_label(engine: &mut DrawEngine, dx: f64, to_x: f64, to_y: f64) {
    let at = centre(&element_of(engine, LABEL));
    engine.begin_pointer(at.x + dx, at.y, false, false);
    engine.move_pointer(to_x, to_y, false, false);
    engine.end_pointer();
}

/// The gesture a press 20 to the right of the label's centre would start.
fn gesture(engine: &mut DrawEngine) -> Option<&'static str> {
    let at = centre(&element_of(engine, LABEL));
    engine.begin_pointer(at.x + 20.0, at.y, false, false);
    let kind = engine.debug_state().interaction.kind;
    engine.end_pointer();
    kind
}

// ---------------------------------------------------------------------------
// A second implementation of the point-on-curve maths, for the tests
// ---------------------------------------------------------------------------

/// The cubics a path is drawn as, from the published Catmull-Rom construction, with both
/// endpoints duplicated. The engine builds the same shape in `catmull_rom_cubics`; this is
/// written out again so that a bug which moved the engine's answer and this reference
/// together could not hide behind it.
fn reference_cubics(points: &[[f64; 2]]) -> Vec<[[f64; 2]; 4]> {
    if points.len() < 2 {
        return Vec::new();
    }
    if points.len() == 2 {
        return vec![chord(points[0], points[1])];
    }
    let mut ps = vec![points[0]];
    ps.extend_from_slice(points);
    ps.push(points[points.len() - 1]);
    (1..ps.len() - 2)
        .map(|i| {
            let (start, end) = (ps[i], ps[i + 1]);
            [
                start,
                [
                    start[0] + (ps[i + 1][0] - ps[i - 1][0]) / 6.0,
                    start[1] + (ps[i + 1][1] - ps[i - 1][1]) / 6.0,
                ],
                [
                    end[0] + (ps[i][0] - ps[i + 2][0]) / 6.0,
                    end[1] + (ps[i][1] - ps[i + 2][1]) / 6.0,
                ],
                end,
            ]
        })
        .collect()
}

/// A straight run given as a cubic, so that every path is a list of cubics here as it is in
/// the engine. The four control points are collinear and evenly spaced, so the Bezier is
/// exactly `a + t (b - a)`.
fn chord(a: [f64; 2], b: [f64; 2]) -> [[f64; 2]; 4] {
    let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    [a, at(1.0 / 3.0), at(2.0 / 3.0), b]
}

fn at(curve: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * t * u * u, 3.0 * t * t * u, t * t * t];
    [
        w[0] * curve[0][0] + w[1] * curve[1][0] + w[2] * curve[2][0] + w[3] * curve[3][0],
        w[0] * curve[0][1] + w[1] * curve[1][1] + w[2] * curve[2][1] + w[3] * curve[3][1],
    ]
}

/// A cubic's speed at `t`: `3 |(p1 - p0)(1-t)^2 + 2 (p2 - p1)(1-t) t + (p3 - p2) t^2|`.
fn speed(curve: &[[f64; 2]; 4], t: f64) -> f64 {
    let u = 1.0 - t;
    let component = |index: usize| {
        3.0 * ((curve[1][index] - curve[0][index]) * u * u
            + 2.0 * (curve[2][index] - curve[1][index]) * u * t
            + (curve[3][index] - curve[2][index]) * t * t)
    };
    (component(0).powi(2) + component(1).powi(2)).sqrt()
}

/// How finely the second implementation cuts a piece.
const PANELS: usize = 512;

/// A cubic's arc length over `[0, t]` by Simpson's rule on the speed.
///
/// The engine instead walks a 64-point polyline and sums the chords between the samples.
/// Both approximate the same integral — Excalidraw approximates it a third way, with 24-point
/// Legendre-Gauss — and none of the three is exact, which is why they are only ever compared
/// to within a pixel and never asserted equal.
fn simpson(curve: &[[f64; 2]; 4], t: f64, panels: usize) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    let h = t.min(1.0) / panels as f64;
    let mut sum = speed(curve, 0.0) + speed(curve, h * panels as f64);
    for k in 1..panels {
        let u = k as f64 * h;
        sum += speed(curve, u) * if k % 2 == 0 { 2.0 } else { 4.0 };
    }
    sum * h / 3.0
}

/// A path measured the second way: each piece with the ground covered to every one of its
/// [`PANELS`] samples, its own length, and the whole.
///
/// Built once per arrow because Simpson on a prefix is not a prefix of Simpson: the table has
/// to be integrated sample by sample rather than extrapolated from the whole.
struct ReferencePath {
    pieces: Vec<[[f64; 2]; 4]>,
    walked: Vec<Vec<f64>>,
    lengths: Vec<f64>,
    total: f64,
}

impl ReferencePath {
    fn of(arrow: &DrawElement) -> Self {
        let world: Vec<[f64; 2]> = linear::world_points(arrow)
            .iter()
            .map(|p| [p.x, p.y])
            .collect();
        let pieces = if arrow.roundness.is_some() {
            reference_cubics(&world)
        } else {
            world
                .windows(2)
                .map(|pair| chord(pair[0], pair[1]))
                .collect()
        };
        let walked: Vec<Vec<f64>> = pieces
            .iter()
            .map(|piece| {
                (0..=PANELS)
                    .map(|k| {
                        // As many panels as the prefix has samples, so the table is as fine as
                        // it can be rather than as coarse as it can get away with. A prefix
                        // integrated with a fixed handful of panels is the kind of error that
                        // makes a reference disagree with the thing it is checking.
                        let panels = if k % 2 == 0 { k.max(2) } else { k + 1 };
                        simpson(piece, k as f64 / PANELS as f64, panels)
                    })
                    .collect()
            })
            .collect();
        let lengths: Vec<f64> = walked.iter().map(|table| table[PANELS]).collect();
        let total = lengths.iter().sum();
        ReferencePath {
            pieces,
            walked,
            lengths,
            total,
        }
    }

    /// The point `fraction` along the path, by walking the table — where the engine walks a
    /// table it built by a different rule at a coarser resolution.
    fn point_at(&self, fraction: f64) -> [f64; 2] {
        let target = fraction.clamp(0.0, 1.0) * self.total;
        let mut before = 0.0;
        for (index, length) in self.lengths.iter().enumerate() {
            if target > before + length {
                before += length;
                continue;
            }
            return self.within(index, target - before);
        }
        at(&self.pieces[self.pieces.len() - 1], 1.0)
    }

    /// The point `length` into piece `index`, interpolating inside the sample that reaches it.
    fn within(&self, index: usize, length: f64) -> [f64; 2] {
        let piece = &self.pieces[index];
        let table = &self.walked[index];
        let step = table
            .iter()
            .position(|&walked| walked >= length)
            .unwrap_or(PANELS);
        if step == 0 {
            return at(piece, 0.0);
        }
        let span = table[step] - table[step - 1];
        let f = if span <= f64::EPSILON {
            0.0
        } else {
            (length - table[step - 1]) / span
        };
        let before = at(piece, (step - 1) as f64 / PANELS as f64);
        let here = at(piece, step as f64 / PANELS as f64);
        [
            before[0] + (here[0] - before[0]) * f,
            before[1] + (here[1] - before[1]) * f,
        ]
    }
}

/// The chords of `arrow`, sampled — what the answer would be if the label were placed on the
/// polyline rather than on the drawn curve. The control for the "on the curve" assertions.
fn polyline_of(arrow: &DrawElement) -> Vec<[f64; 2]> {
    let world: Vec<[f64; 2]> = linear::world_points(arrow)
        .iter()
        .map(|p| [p.x, p.y])
        .collect();
    let mut out = Vec::new();
    for pair in world.windows(2) {
        for step in 0..=200 {
            let t = step as f64 / 200.0;
            out.push([
                pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                pair[0][1] + (pair[1][1] - pair[0][1]) * t,
            ]);
        }
    }
    out
}

/// The drawn curve, sampled densely.
fn curve_of(arrow: &DrawElement) -> Vec<[f64; 2]> {
    let world: Vec<[f64; 2]> = linear::world_points(arrow)
        .iter()
        .map(|p| [p.x, p.y])
        .collect();
    let pieces = if arrow.roundness.is_some() {
        reference_cubics(&world)
    } else {
        world
            .windows(2)
            .map(|pair| chord(pair[0], pair[1]))
            .collect()
    };
    pieces
        .iter()
        .flat_map(|piece| (0..=400).map(move |step| at(piece, step as f64 / 400.0)))
        .collect()
}

fn distance_to(samples: &[[f64; 2]], point: [f64; 2]) -> f64 {
    samples
        .iter()
        .map(|s| (s[0] - point[0]).hypot(s[1] - point[1]))
        .fold(f64::INFINITY, f64::min)
}

fn off_by(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// How close a **dragged** position has to land, in world units.
///
/// Two different precisions live in this file and they are not interchangeable. Putting a
/// label at a fraction it was *told* is exact: locating it walks a straight piece, and the
/// interpolation inside a piece is exact, so the hand-computed fixture is asserted at 1e-9.
///
/// A fraction that came from a **drag** is a different matter. The pointer is projected onto
/// the path by bisecting for the nearest parameter, so the reading carries the resolution of
/// that search — about 1e-6 of the path, which on a 300-unit arrow is 3e-4 of a pixel. That
/// is not sloppiness to be papered over, it is the honest precision of the operation, and
/// Excalidraw is looser still: its `curvePointAtLength` stops once it is within
/// `totalLength * 0.0001` (`packages/math/src/curve.ts@1118751f:534-541`).
const DRAG_TOLERANCE: f64 = 1e-3;

/// The two halves of a dragged position, asserted together: the number and the place, to the
/// same tolerance, so neither can be right while the other is not.
fn assert_dragged(label: &DrawElement, fraction: f64, at: (f64, f64)) {
    let got = label
        .label_position
        .expect("a dragged label has a position");
    assert!(
        (got - fraction).abs() < DRAG_TOLERANCE,
        "the fraction should be {fraction}; it was {got}"
    );
    let centre = centre(label);
    assert!(
        (centre.x - at.0).abs() < DRAG_TOLERANCE && (centre.y - at.1).abs() < DRAG_TOLERANCE,
        "the label should be centred on {at:?}; it was at ({}, {})",
        centre.x,
        centre.y
    );
}

// ---------------------------------------------------------------------------
// The committed fixture: hand-computed numbers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    points: Vec<[f64; 2]>,
    roundness: Option<f64>,
    angle: f64,
    expected: Vec<Hop>,
}

#[derive(Deserialize)]
struct Hop {
    fraction: f64,
    at: [f64; 2],
}

fn load() -> Vec<Case> {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/label-position.json"
    ))
    .expect("the hand-computed fixture is committed");
    serde_json::from_str::<Fixture>(&raw)
        .expect("the fixture parses")
        .cases
}

fn case_starting(prefix: &str) -> Case {
    load()
        .into_iter()
        .find(|case| case.name.starts_with(prefix))
        .unwrap_or_else(|| panic!("the fixture has a case starting {prefix:?}"))
}

fn case_containing(needle: &str) -> Case {
    load()
        .into_iter()
        .find(|case| case.name.contains(needle))
        .unwrap_or_else(|| panic!("the fixture has a case containing {needle:?}"))
}

/// The arrow a fixture case describes, as an element of a scene.
fn of(case: &Case) -> DrawElement {
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &case.points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    let mut arrow = create_element_default(
        DrawElementType::Arrow,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: max_x - min_x,
            height: max_y - min_y,
        },
    );
    arrow.points = Some(case.points.clone());
    arrow.roundness = case.roundness;
    arrow.angle = case.angle;
    id_of(arrow, ARROW)
}

/// **Independent check 1 — hand-computed constants.** Every number in the fixture was
/// worked out on paper, so this asks nothing of the code under test but that it agree.
#[test]
fn the_path_positions_match_the_hand_computed_fixture() {
    for case in load() {
        for hop in &case.expected {
            let got = anchored(&of(&case), Some(hop.fraction));
            assert!(
                off_by(got, hop.at) < 1e-9,
                "{}: at fraction {} the label should be at {:?}; it was at {got:?}",
                case.name,
                hop.fraction,
                hop.at,
            );
        }
    }
}

/// The control for the fixture. At a quarter of the right-angle case the three readings a
/// label position is most often confused with all give a *different* answer, so the fixture
/// cannot be passing for the wrong reason.
#[test]
fn the_hand_computed_numbers_rule_the_wrong_readings_out() {
    let case = case_starting("a right angle");
    let quarter = anchored(&of(&case), Some(0.25));
    let as_box_fraction = [30.0, 0.0];
    let as_per_segment_fraction = [30.0, 0.0];
    let as_world_offset = [0.25, 0.0];
    assert!(off_by(quarter, as_box_fraction) > 1.0);
    assert!(off_by(quarter, as_per_segment_fraction) > 1.0);
    assert!(off_by(quarter, as_world_offset) > 1.0);
    // And the reading that is right.
    assert!(off_by(quarter, [52.5, 0.0]) < 1e-9);
}

/// A rounded two-point path is the straight run through its two points, so the same
/// fractions land in the same places. Named in the fixture as a finding of its own: it is
/// what stops "always build a spline" from being taken as a harmless simplification.
#[test]
fn a_two_point_path_gives_the_same_place_rounded_or_sharp() {
    let sharp = of(&case_containing("three hundred long, sharp"));
    let rounded = of(&case_containing("three hundred long, rounded"));
    for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let a = anchored(&sharp, Some(fraction));
        let b = anchored(&rounded, Some(fraction));
        assert!(
            off_by(a, b) < 1e-9,
            "at {fraction}: sharp {a:?} against rounded {b:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The drag itself
// ---------------------------------------------------------------------------

/// **Q, the first half of the pair.** A straight arrow 300 long, its label grabbed 20 to the
/// right of its own centre — so the anchor starts 20 along, at x = 170 — and dragged to a
/// pointer at x = 95, which moves the anchor 75 along. 75 of 300 is a quarter, worked out in
/// the fixture; what is asserted here is the exact world point the label's centre ends on,
/// which is the only thing this test is allowed to say.
#[test]
fn dragging_the_label_lands_it_on_the_exact_world_point() {
    let mut engine = engine(straight(300.0, None));
    let before = centre(&element_of(&engine, LABEL));
    drag_label(&mut engine, 20.0, 95.0, 0.0);
    let after = element_of(&engine, LABEL);

    assert_dragged(&after, 0.25, (75.0, 0.0));
    assert!(before != centre(&after), "the label should have moved");
}

/// **R, the second half of the pair, and the same scene.** Nothing is dragged, so the label
/// must be exactly where it was: the middle of the path, 150 along, with no position of its
/// own. Both halves are on the same scene, so a Q that "agrees" for the wrong reason — a
/// default that happened to equal the drag's answer — cannot pass.
#[test]
fn with_no_drag_the_label_stays_on_the_default() {
    let engine = engine(straight(300.0, None));
    let label = element_of(&engine, LABEL);

    assert_eq!(
        label.label_position, None,
        "a label nobody moved has no position"
    );
    assert_point_close(centre(&label), Point { x: 150.0, y: 0.0 });
}

/// The other way a fraction and an offset are told apart, on the *rendered* point rather
/// than on the stored number: the same fraction on a longer arrow is further along.
#[test]
fn the_same_fraction_on_a_longer_arrow_is_further_along() {
    let short = anchored(&straight(300.0, None), Some(0.25));
    let long = anchored(&straight(900.0, None), Some(0.25));
    assert_close(short[0], 75.0);
    assert_close(long[0], 225.0);
    assert!(
        off_by(short, long) > 100.0,
        "a world-unit offset would put both at the same place"
    );
}

/// Dragging a label moves the label and **nothing else**. Without this, a test that only
/// watches the label passes for an implementation that dragged the whole arrow.
#[test]
fn dragging_the_label_moves_nothing_but_the_label() {
    let mut engine = engine(straight(300.0, None));
    let before = element_of(&engine, ARROW);
    let label_before = centre(&element_of(&engine, LABEL));
    drag_label(&mut engine, 20.0, 95.0, 40.0);
    let after = element_of(&engine, ARROW);

    assert_eq!(after.points, before.points, "the arrow's points moved");
    assert_close(after.x, before.x);
    assert_close(after.y, before.y);
    assert_close(after.width, before.width);
    assert_close(after.height, before.height);
    assert_eq!(after.start_binding, before.start_binding);
    assert_eq!(after.end_binding, before.end_binding);
    assert_eq!(after.version, before.version, "the arrow was restamped");
    assert_ne!(
        centre(&element_of(&engine, LABEL)),
        label_before,
        "the label did not move"
    );
}

/// The same, with the arrow bound at both ends, because that is where a careless
/// implementation reaches for `apply_bindings` and quietly re-aims the arrow.
#[test]
fn dragging_the_label_leaves_a_bound_arrow_bound_where_it_was() {
    let mut arrow = straight(300.0, None);
    arrow.start_binding = Some("box-a".into());
    arrow.end_binding = Some("box-b".into());
    let box_at = |id: &str, x: f64| {
        id_of(
            create_element_default(
                DrawElementType::Rectangle,
                Geometry {
                    x,
                    y: -100.0,
                    width: 50.0,
                    height: 50.0,
                },
            ),
            id,
        )
    };
    let mut elements = vec![box_at("box-a", -200.0), box_at("box-b", 350.0)];
    elements.extend(hold(&arrow, label_at(None)));
    let mut engine = engine_with_measure(elements);
    engine.select(vec![ARROW.into()]);

    let before = element_of(&engine, ARROW);
    drag_label(&mut engine, 20.0, 95.0, 0.0);
    let after = element_of(&engine, ARROW);

    assert_eq!(after.start_binding.as_deref(), Some("box-a"));
    assert_eq!(after.end_binding.as_deref(), Some("box-b"));
    assert_eq!(after.points, before.points);
    assert_eq!(after.start_fixed_point, before.start_fixed_point);
    assert_eq!(after.end_fixed_point, before.end_fixed_point);
    assert_close(element_of(&engine, "box-a").x, -200.0);
    assert_close(element_of(&engine, "box-b").x, 350.0);
}

/// The label follows the arrow. A label told where to sit is laid out from the arrow's
/// **current** path every time, so bending the arrow carries it and the fraction it carries
/// is unchanged. This is what keeps a dragged label from being a second placement that goes
/// stale the moment the arrow is edited.
#[test]
fn a_dragged_label_follows_the_arrow_when_it_is_bent() {
    let arrow = bowed();
    let mut bent = arrow.clone();
    bent.points = Some(vec![[0.0, 0.0], [100.0, 220.0], [200.0, 0.0]]);

    let was = anchored(&arrow, Some(0.25));
    let now = anchored(&bent, Some(0.25));

    assert!(
        off_by(was, now) > 1.0,
        "bending the arrow should have carried the label"
    );
    for (name, one, at) in [("before", &arrow, was), ("after", &bent, now)] {
        let off = distance_to(&curve_of(one), at);
        assert!(
            off < 0.5,
            "{name}: the label was {off} off the curve, at {at:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Which gesture reaches it
// ---------------------------------------------------------------------------

/// The branch, named by what the engine says it is doing: a press on the label of the
/// selected arrow starts a label drag.
#[test]
fn a_press_on_the_label_of_the_selected_arrow_starts_the_label_drag() {
    let mut engine = engine(straight(300.0, None));
    assert_eq!(gesture(&mut engine), Some("label-drag"));
}

/// The same press on the arrow's own stroke is not a label drag.
#[test]
fn a_press_on_the_arrow_itself_is_not_a_label_drag() {
    let mut engine = engine(straight(300.0, None));
    engine.begin_pointer(200.0, 0.0, false, false);
    let kind = engine.debug_state().interaction.kind;
    engine.end_pointer();
    assert_ne!(kind, Some("label-drag"));
}

/// An unselected arrow's label is a click on the label, which selects it: the drag needs
/// the line editor's own selection, because that is what the oracle's `selectedLinearElement`
/// is.
#[test]
fn a_label_drag_needs_the_arrow_to_be_the_selection() {
    let mut engine = engine_with_measure(hold(&straight(300.0, None), label_at(None)));
    assert_ne!(gesture(&mut engine), Some("label-drag"));
}

/// A **line's** label does not move. `isArrowElement` is inside the oracle's guard
/// (`:1158`), and this is where that shows up.
#[test]
fn a_lines_label_is_not_dragged() {
    let mut line = straight(300.0, None);
    line.kind = DrawElementType::Line;
    let mut engine = engine(line);
    assert_ne!(gesture(&mut engine), Some("label-drag"));
    drag_label(&mut engine, 20.0, 95.0, 0.0);
    assert_eq!(element_of(&engine, LABEL).label_position, None);
}

/// The handles keep precedence over the label that sits on top of them, "so a labeled arrow
/// can still be bent at its middle" (`App.tsx@1118751f:1149-1151`). At the default the
/// label's centre **is** the midpoint knob, so a press there is a point drag — which is
/// also why [`drag_label`] presses 20 to the side.
#[test]
fn a_point_handle_keeps_precedence_over_the_label() {
    let mut engine = engine(straight(300.0, None));
    let at_centre = centre(&element_of(&engine, LABEL));
    engine.begin_pointer(at_centre.x, at_centre.y, false, false);
    let kind = engine.debug_state().interaction.kind;
    engine.end_pointer();

    assert_eq!(kind, Some("linear-point"));
}

// ---------------------------------------------------------------------------
// The ends
// ---------------------------------------------------------------------------

/// Dragged off the far end, the label lands on the path's last point and stays there: the
/// oracle clamps to `[0, 1]` on write (`:2011`) rather than refusing the move.
#[test]
fn dragging_past_the_end_lands_on_the_end() {
    let mut engine = engine(straight(300.0, None));
    drag_label(&mut engine, 20.0, 5000.0, 0.0);
    assert_dragged(&element_of(&engine, LABEL), 1.0, (300.0, 0.0));
}

/// And off the near end, on the first.
#[test]
fn dragging_past_the_start_lands_on_the_start() {
    let mut engine = engine(straight(300.0, None));
    drag_label(&mut engine, 20.0, -5000.0, 0.0);
    assert_dragged(&element_of(&engine, LABEL), 0.0, (0.0, 0.0));
}

/// Both ends are legal values, not merely the ends of a drag: a label told to sit at `0` or
/// `1` is drawn on the path's own first or last point, on the curve and not a chord.
#[test]
fn both_ends_are_legal_and_land_on_the_path() {
    let arrow = of(&case_containing("symmetric arch"));
    for (fraction, expected) in [(0.0, [0.0, 0.0]), (1.0, [200.0, 0.0])] {
        let got = anchored(&arrow, Some(fraction));
        assert!(
            off_by(got, expected) < 1e-9,
            "at {fraction}: {got:?} against {expected:?}"
        );
        assert!(distance_to(&curve_of(&arrow), got) < 0.5);
    }
}

/// A saved position outside `[0, 1]` is read clamped, exactly as the oracle's restore does
/// (`restore.ts@1118751f:573-575`), rather than drawn off the end of the arrow.
#[test]
fn a_position_outside_the_unit_range_is_read_clamped() {
    for (saved, fraction) in [(-3.0, 0.0), (7.5, 1.0)] {
        let at = anchored(&straight(300.0, None), Some(saved));
        assert_close(at[0], 300.0 * fraction);
        assert_close(at[1], 0.0);
    }
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

/// **Property one: the label is on the drawn curve at the fraction it says.** Fractions
/// across `[0, 1]`, two, three and five points, bowed and straight, turned and not. The
/// assertion is that the rendered point is on the curve, measured against a curve sampled
/// by the second implementation rather than by anything the engine exposes.
#[test]
fn the_label_is_on_the_curve_at_the_fraction_it_says() {
    let mut turned = many_point();
    turned.angle = 0.7;
    let mut turned_bowed = bowed();
    turned_bowed.angle = 0.7;
    let mut sharp_bowed = bowed();
    sharp_bowed.roundness = None;
    let mut rounded_many = many_point();
    rounded_many.roundness = Some(8.0);
    let cases = [
        ("two points, sharp", straight(300.0, None)),
        ("two points, rounded", straight(300.0, Some(8.0))),
        ("three points, bowed", bowed()),
        ("three points, sharp", sharp_bowed),
        ("five points, sharp", many_point()),
        ("five points, rounded", rounded_many),
        ("five points, sharp and turned", turned),
        ("three points, bowed and turned", turned_bowed),
    ];
    for (name, arrow) in cases {
        let mut first = None;
        let mut last = None;
        for step in 0..=20 {
            let fraction = step as f64 / 20.0;
            let got = anchored(&arrow, Some(fraction));
            let off = distance_to(&curve_of(&arrow), got);
            assert!(
                off < 0.5,
                "{name} at {fraction}: the label was {off} off the drawn curve, at {got:?}"
            );
            first.get_or_insert(got);
            last = Some(got);
        }
        // The guard on the guard. "On the curve" holds trivially for a label that ignored
        // its fraction and sat in the middle, so the two ends of the sweep have to differ:
        // if they ever agree again, the fraction has stopped being read and every
        // on-the-curve assertion in this test has stopped meaning anything.
        assert!(
            off_by(first.unwrap(), last.unwrap()) > 1.0,
            "{name}: the sweep's first and last places agree, so the fraction was not read"
        );
    }
}

/// **Independent check 2 — a second implementation of the point-on-curve maths, on the
/// value rather than on membership.** [`ReferencePath`] integrates the path by Simpson's
/// rule on `|B'(t)|` at 512 samples a piece, where the engine walks a 64-point polyline and
/// sums the chords between its samples. Different integrand rule, different resolution,
/// same published Bezier.
///
/// Compared to within a pixel rather than exactly, and deliberately so: two honest
/// approximations of one integral differ slightly, and a test that demanded they agree to
/// the last bit would be pinning the method rather than the answer. A pixel is loose enough
/// for that and far tighter than any of the errors this is here to catch.
#[test]
fn the_second_implementation_agrees_with_the_engine() {
    let mut turned = many_point();
    turned.angle = 0.7;
    let mut rounded = many_point();
    rounded.roundness = Some(8.0);
    let cases = [
        ("three points, bowed", bowed()),
        ("three points, bowed and turned", {
            let mut arrow = bowed();
            arrow.angle = 0.7;
            arrow
        }),
        ("five points, sharp", many_point()),
        ("five points, rounded", rounded),
        ("five points, sharp and turned", turned),
    ];
    for (name, arrow) in cases {
        let path = ReferencePath::of(&arrow);
        for step in 0..=20 {
            let fraction = step as f64 / 20.0;
            let got = anchored(&arrow, Some(fraction));
            let want = path.point_at(fraction);
            assert!(
                off_by(got, want) < 1.0,
                "{name} at {fraction}: the engine said {got:?}, the second implementation {want:?}"
            );
        }
    }
}

/// **Property two: the answer is on the curve, not on the chords.** The control for
/// property one — on a bowed arrow the chord reading is measurably elsewhere, so a test that
/// only asserted "near the path" could pass for an implementation that used the polyline.
#[test]
fn on_a_bowed_arrow_the_anchor_is_measurably_off_the_chords() {
    let arrow = bowed();
    let chord_reading = anchored(
        &{
            let mut sharp = arrow.clone();
            sharp.roundness = None;
            sharp
        },
        Some(0.25),
    );
    let on_the_curve = anchored(&arrow, Some(0.25));

    assert!(distance_to(&polyline_of(&arrow), on_the_curve) > 1.0);
    assert!(distance_to(&curve_of(&arrow), chord_reading) > 1.0);
    assert!(off_by(chord_reading, on_the_curve) > 1.0);
}

/// **Property three: idempotence.** Dragging the label to the position it already occupies
/// changes nothing, which is what makes it safe to write on every pointer move — otherwise a
/// stationary pointer kept rewriting the element and restamping the scene.
#[test]
fn dragging_to_where_the_label_already_is_changes_nothing() {
    let mut engine = engine(straight(300.0, None));
    drag_label(&mut engine, 20.0, 95.0, 0.0);
    let first = element_of(&engine, LABEL);
    let arrow_first = element_of(&engine, ARROW);

    // A second press 20 to the right of the label's new centre, released where it went up.
    let at = centre(&first);
    engine.begin_pointer(at.x + 20.0, at.y, false, false);
    engine.move_pointer(at.x + 20.0, at.y, false, false);
    engine.end_pointer();

    let second = element_of(&engine, LABEL);
    assert_eq!(second.label_position, first.label_position);
    assert_close(second.x, first.x);
    assert_close(second.y, first.y);
    assert_eq!(
        second.version, first.version,
        "a no-op drag restamped the label"
    );
    assert_eq!(element_of(&engine, ARROW).version, arrow_first.version);
}

/// The other half of idempotence: dragging **back** to where it started puts it back. A drag
/// that accumulated would not, and would creep away from the pointer on the way out.
#[test]
fn a_drag_is_reversible() {
    let mut engine = engine(straight(300.0, None));
    let start = centre(&element_of(&engine, LABEL));
    drag_label(&mut engine, 20.0, 95.0, 0.0);
    drag_label(&mut engine, 20.0, 170.0, 0.0);
    let back = centre(&element_of(&engine, LABEL));

    assert!(
        (back.x - start.x).abs() < 0.5 && (back.y - start.y).abs() < 0.5,
        "dragging out and back should land on {start:?}; it landed on {back:?}"
    );
}

/// **Property four: a position survives a save and a load.** A number nobody can reload is a
/// number that quietly becomes the default, so the round trip is the test.
#[test]
fn a_position_survives_a_save_and_a_load() {
    let mut engine = engine(straight(300.0, None));
    drag_label(&mut engine, 20.0, 95.0, 0.0);
    let label = element_of(&engine, LABEL);
    let json = serde_json::to_string(&label).expect("a label serialises");
    assert!(
        json.contains("\"labelPosition\":0.25"),
        "the position should be written: {json}"
    );

    let back: DrawElement = serde_json::from_str(&json).expect("a label parses");
    let arrow = element_of(&engine, ARROW);
    // Laid out from the reloaded number, which is the point: the scene JSON is what a
    // reload reads, so this is the answer a person gets back.
    assert_dragged(&relaid(&arrow, &back), 0.25, (75.0, 0.0));
}

/// **Property five: the fraction is of the path's length, not of its box and not of any one
/// chord.** Told two fractions, a sharp right-angle arrow has to answer on two different
/// chords. A box fraction would put both on the first.
#[test]
fn the_fraction_is_of_the_paths_length_not_its_box() {
    let arrow = of(&case_starting("a right angle"));
    let quarter = anchored(&arrow, Some(0.25));
    let three_quarters = anchored(&arrow, Some(0.75));
    assert!(off_by(quarter, three_quarters) > 1.0);
    assert_close(quarter[1], 0.0);
    assert_close(three_quarters[1], 37.5);
}

// ---------------------------------------------------------------------------
// What must not have changed
// ---------------------------------------------------------------------------

/// 5.5 landed the day before this. A labelled arrow is **not** convertible, and moving the
/// label must not change that — the guard is on the label's *existence*
/// (`isEligibleLinearElement`, `ConvertElementTypePopup.tsx@1118751f:666-672`), not on where
/// it sits. This also covers a case 5.5 left untested: a labelled but *unbound* arrow.
#[test]
fn a_dragged_label_does_not_make_the_arrow_convertible() {
    let mut engine = engine(straight(300.0, None));
    assert!(
        !engine.can_convert_selection(),
        "a labelled arrow is not switchable, bound or not"
    );
    drag_label(&mut engine, 20.0, 95.0, 0.0);
    assert!(
        !engine.can_convert_selection(),
        "dragging a label did not make it switchable"
    );
    assert!(!engine.convert_selection(None, true));
    assert_eq!(element_of(&engine, ARROW).kind, DrawElementType::Arrow);
}
