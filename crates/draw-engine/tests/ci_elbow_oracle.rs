//! Elbow arrow routing held to Excalidraw's own code.
//!
//! `fixtures/elbow.oracle.json` is written by `tools/elbow-oracle/generate.mjs`, which runs
//! the oracle's unmodified router through its own entry points over a seeded sweep: free
//! ends, one end bound, both bound with every heading pair, overlapping and nested shapes,
//! routes while dragging, fixed segments moved, carried through end drags and shape moves,
//! and released. Every step starts from the oracle's state before it and must land on the
//! oracle's state after it, to 1e-9: the port is operation for operation, so a miss is a
//! bug, never noise.
//!
//! Why 1e-9 and not 0: natively, 11014 of the 11176 steps are identical to the bit. The
//! other 162 all involve a shape turned 2.5 rad, where the host's `sin` (glibc) and
//! V8's disagree in the last place (0.5984721441039565 against …564). In the browser the
//! engine's `sin` is compiled from Rust's own libm, not the host's.

use draw_engine::scene::elbow::{self, Board, ElbowBinding, FixedSegment, Options, Updates};
use draw_engine::scene::{
    create_element, Anchor, BindMode, DrawElement, DrawElementType, End, Geometry,
};
use draw_engine::scene::{set_anchor, Arrowhead, DrawElementStyle};
use serde::Deserialize;

const TOLERANCE: f64 = 1e-9;

#[derive(Deserialize)]
struct Fixture {
    oracle: Oracle,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Oracle {
    excalidraw: String,
    cases: usize,
    steps: usize,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Shape {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    angle: f64,
    stroke_width: f64,
    rounded: bool,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Binding {
    element_id: String,
    fixed_point: [f64; 2],
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ArrowState {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    points: Vec<[f64; 2]>,
    #[serde(default)]
    start_binding: Option<Binding>,
    #[serde(default)]
    end_binding: Option<Binding>,
    #[serde(default)]
    start_arrowhead: Option<String>,
    #[serde(default)]
    end_arrowhead: Option<String>,
    fixed_segments: Option<Vec<FixedSegment>>,
    start_is_special: Option<bool>,
    end_is_special: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MutateUpdates {
    points: Option<Vec<[f64; 2]>>,
    start_binding: Option<Binding>,
    end_binding: Option<Binding>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Op {
    #[serde(rename_all = "camelCase")]
    Mutate {
        updates: MutateUpdates,
        is_dragging: bool,
    },
    #[serde(rename_all = "camelCase")]
    Bind {
        end: String,
        shape: String,
        fixed_point: [f64; 2],
    },
    MoveSegment {
        index: usize,
        x: f64,
        y: f64,
    },
    ReleaseSegment {
        index: usize,
    },
    MoveShape {
        shape: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

#[derive(Deserialize)]
struct Step {
    #[serde(flatten)]
    op: Op,
    arrow: ArrowState,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    shapes: Vec<Shape>,
    arrow: ArrowState,
    steps: Vec<Step>,
}

fn load() -> Fixture {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/elbow.oracle.json"
    ))
    .expect("fixture present — regenerate with the host's `make oracle-fixtures`");
    serde_json::from_str(&raw).expect("fixture parses")
}

fn shape(s: &Shape) -> DrawElement {
    let kind = match s.kind.as_str() {
        "rectangle" => DrawElementType::Rectangle,
        "diamond" => DrawElementType::Diamond,
        "ellipse" => DrawElementType::Ellipse,
        other => panic!("no shape kind {other}"),
    };
    let geometry = Geometry {
        x: s.x,
        y: s.y,
        width: s.width,
        height: s.height,
    };
    let mut element = create_element(kind, geometry, DrawElementStyle::default(), 0.0);
    element.id = s.id.clone();
    element.angle = s.angle;
    element.stroke_width = s.stroke_width;
    element.roundness = s.rounded.then_some(8.0);
    element
}

fn head(name: &Option<String>) -> Option<Arrowhead> {
    name.as_deref().map(|n| {
        serde_json::from_value(serde_json::Value::String(n.to_owned())).expect("arrowhead")
    })
}

fn bind(element: &mut DrawElement, end: End, binding: Option<&Binding>) {
    set_anchor(
        element,
        end,
        binding.map(|b| Anchor {
            element_id: b.element_id.clone(),
            fixed_point: b.fixed_point,
            mode: BindMode::Orbit,
        }),
    );
}

/// The arrow as the case begins.
fn arrow(state: &ArrowState) -> DrawElement {
    let geometry = Geometry {
        x: state.x,
        y: state.y,
        width: state.width,
        height: state.height,
    };
    let mut element = create_element(
        DrawElementType::Arrow,
        geometry,
        DrawElementStyle::default(),
        0.0,
    );
    element.id = "arrow".into();
    element.roundness = None;
    element.elbowed = Some(true);
    element.start_arrowhead = head(&state.start_arrowhead);
    element.end_arrowhead = head(&state.end_arrowhead);
    bind(&mut element, End::Start, state.start_binding.as_ref());
    bind(&mut element, End::End, state.end_binding.as_ref());
    set_route(&mut element, state);
    element
}

/// What a step changes, from the oracle's record.
fn set_route(element: &mut DrawElement, state: &ArrowState) {
    element.x = state.x;
    element.y = state.y;
    element.width = state.width;
    element.height = state.height;
    element.points = Some(state.points.clone());
    element.fixed_segments = state.fixed_segments.clone();
    element.start_is_special = state.start_is_special;
    element.end_is_special = state.end_is_special;
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE || (a.is_nan() && b.is_nan())
}

fn close_point(a: [f64; 2], b: [f64; 2]) -> bool {
    close(a[0], b[0]) && close(a[1], b[1])
}

/// Where `got` differs from the oracle's `want`, if anywhere.
fn differs(got: &DrawElement, want: &ArrowState) -> Option<String> {
    let points = got.points.clone().unwrap_or_default();
    if points.len() != want.points.len()
        || !points
            .iter()
            .zip(&want.points)
            .all(|(a, b)| close_point(*a, *b))
    {
        return Some(format!("points {points:?}, oracle {:?}", want.points));
    }
    if !close(got.x, want.x) || !close(got.y, want.y) {
        return Some(format!(
            "at ({}, {}), oracle ({}, {})",
            got.x, got.y, want.x, want.y
        ));
    }
    if !close(got.width, want.width) || !close(got.height, want.height) {
        return Some(format!(
            "size {}×{}, oracle {}×{}",
            got.width, got.height, want.width, want.height
        ));
    }
    let same_segments = match (&got.fixed_segments, &want.fixed_segments) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|(s, t)| {
                    s.index == t.index && close_point(s.start, t.start) && close_point(s.end, t.end)
                })
        }
        _ => false,
    };
    if !same_segments {
        return Some(format!(
            "fixed segments {:?}, oracle {:?}",
            got.fixed_segments, want.fixed_segments
        ));
    }
    if got.start_is_special != want.start_is_special || got.end_is_special != want.end_is_special {
        return Some(format!(
            "special {:?}/{:?}, oracle {:?}/{:?}",
            got.start_is_special, got.end_is_special, want.start_is_special, want.end_is_special
        ));
    }
    None
}

fn end_of(name: &str) -> End {
    if name == "start" {
        End::Start
    } else {
        End::End
    }
}

#[test]
fn routes_match_the_oracle() {
    let fixture = load();
    assert_eq!(
        fixture.oracle.excalidraw.len(),
        40,
        "the fixture names its pin"
    );
    assert_eq!(fixture.cases.len(), fixture.oracle.cases);
    let mut failures: Vec<String> = Vec::new();
    let mut steps = 0;
    for case in &fixture.cases {
        let mut shapes: Vec<DrawElement> = case.shapes.iter().map(shape).collect();
        let mut current = arrow(&case.arrow);
        for (i, step) in case.steps.iter().enumerate() {
            steps += 1;
            let mut got = current.clone();
            match &step.op {
                Op::Mutate {
                    updates,
                    is_dragging,
                } => {
                    let board = Board::new(shapes.iter());
                    let binding = |b: &Option<Binding>| {
                        b.as_ref().map(|b| {
                            Some(ElbowBinding {
                                element_id: b.element_id.clone(),
                                fixed_point: b.fixed_point,
                            })
                        })
                    };
                    let options = Options {
                        is_dragging: *is_dragging,
                        ..Options::default()
                    };
                    let updates = Updates {
                        points: updates.points.clone(),
                        start_binding: binding(&updates.start_binding),
                        end_binding: binding(&updates.end_binding),
                        ..Updates::default()
                    };
                    elbow::mutate(&mut got, &board, updates, &options);
                    if let Some(b) = &step_binding(&step.op, End::Start) {
                        bind(&mut current, End::Start, Some(b));
                    }
                    if let Some(b) = &step_binding(&step.op, End::End) {
                        bind(&mut current, End::End, Some(b));
                    }
                }
                Op::Bind {
                    end,
                    shape,
                    fixed_point,
                } => {
                    let target = shapes.iter().find(|s| &s.id == shape).expect("bound shape");
                    let ours = elbow::fixed_point_for(&got, target, end_of(end), 1.0, true)
                        .expect("a bindable shape");
                    if !close_point(ours, *fixed_point) {
                        failures.push(format!(
                            "{} · step {i} bind {end}: fixed point {ours:?}, oracle {fixed_point:?}",
                            case.name
                        ));
                    }
                    let recorded = Binding {
                        element_id: shape.clone(),
                        fixed_point: *fixed_point,
                    };
                    bind(&mut got, end_of(end), Some(&recorded));
                    bind(&mut current, end_of(end), Some(&recorded));
                }
                Op::MoveSegment { index, x, y } => {
                    let board = Board::new(shapes.iter());
                    elbow::move_segment(&mut got, &board, *index, *x, *y);
                }
                Op::ReleaseSegment { index } => {
                    let board = Board::new(shapes.iter());
                    elbow::release_segment(&mut got, &board, *index);
                }
                Op::MoveShape {
                    shape,
                    x,
                    y,
                    width,
                    height,
                } => {
                    let moved = shapes
                        .iter_mut()
                        .find(|s| &s.id == shape)
                        .expect("moved shape");
                    moved.x = *x;
                    moved.y = *y;
                    moved.width = *width;
                    moved.height = *height;
                    let board = Board::new(shapes.iter());
                    elbow::reroute(&mut got, &board);
                }
            }
            if let Some(why) = differs(&got, &step.arrow) {
                failures.push(format!(
                    "{} · step {i} {}: {why}",
                    case.name,
                    op_name(&step.op)
                ));
            }
            // The next step starts from the oracle's state, not ours.
            set_route(&mut current, &step.arrow);
        }
    }
    assert_eq!(steps, fixture.oracle.steps);
    if !failures.is_empty() {
        let shown: Vec<&String> = failures.iter().take(25).collect();
        panic!(
            "{} of {steps} steps differ from the oracle:\n{}",
            failures.len(),
            shown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    println!(
        "{} cases, {steps} steps at parity with the oracle",
        fixture.cases.len()
    );
}

fn step_binding(op: &Op, end: End) -> Option<Binding> {
    match op {
        Op::Mutate { updates, .. } => match end {
            End::Start => updates.start_binding.clone(),
            End::End => updates.end_binding.clone(),
        },
        _ => None,
    }
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Mutate {
            is_dragging: true, ..
        } => "drag",
        Op::Mutate { .. } => "mutate",
        Op::Bind { .. } => "bind",
        Op::MoveSegment { .. } => "move segment",
        Op::ReleaseSegment { .. } => "release segment",
        Op::MoveShape { .. } => "move shape",
    }
}
