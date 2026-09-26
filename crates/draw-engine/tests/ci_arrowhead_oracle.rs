//! Arrowhead geometry held to Excalidraw's own code.
//!
//! `fixtures/arrowhead.oracle.json` is written by `tools/arrowhead-oracle/generate.mjs`,
//! which runs the oracle's unmodified `getArrowheadPoints` / `getArrowheadSize` /
//! `getArrowheadAngle` (`packages/element/src/bounds.ts`) over a swept corpus, each case
//! fed a real `roughjs@4.6.4` `Drawable` for the arrow body.
//!
//! This test does not regenerate that `Drawable` from scratch and hope: it rebuilds it
//! through `draw_rough::generator::{curve, linear_path}`, the same port
//! `crates/draw-rough/tests/oracle.rs` already holds to rough.js op-for-op — so a mismatch
//! here is `get_arrowhead_points`'s own arithmetic, not the sketch underneath it. Every
//! coordinate is compared within `1e-9`, the same tolerance and for the same reason as
//! that test: `+ - * /` and `sqrt` are exact; `sin`/`cos` are not guaranteed bit-identical
//! between V8 and Rust.

use draw_engine::render::arrowheads::{
    curve_path_ops, get_arrowhead_angle, get_arrowhead_points, get_arrowhead_size, Position,
};
use draw_engine::Arrowhead;
use draw_rough::{generator, Options};

use serde::Deserialize;
use std::collections::HashMap;

const TOLERANCE: f64 = 1e-9;

#[derive(Deserialize)]
struct Fixture {
    oracle: Oracle,
    #[serde(rename = "sizesAndAngles")]
    sizes_and_angles: HashMap<String, SizeAngle>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Oracle {
    excalidraw: String,
}

#[derive(Deserialize)]
struct SizeAngle {
    size: f64,
    angle: f64,
}

#[derive(Deserialize)]
struct Case {
    points: Vec<[f64; 2]>,
    roundness: bool,
    #[serde(rename = "strokeWidth")]
    stroke_width: f64,
    roughness: f64,
    seed: i32,
    position: String,
    arrowhead: Arrowhead,
    #[serde(rename = "offsetMultiplier")]
    offset_multiplier: f64,
    expected: Option<Vec<f64>>,
}

fn load() -> Fixture {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/arrowhead.oracle.json"
    ))
    .expect("fixture present — regenerate with `engine/tools/arrowhead-oracle`");
    serde_json::from_str(&raw).expect("fixture parses")
}

/// The same branch `render/shape.rs::element_drawable` takes for `Line | Arrow`.
fn body_ops(case: &Case) -> Vec<draw_rough::ops::Op> {
    let o = Options {
        seed: case.seed,
        roughness: case.roughness,
        stroke_width: case.stroke_width,
        ..Options::default()
    };
    let drawable = if case.roundness {
        generator::curve(&case.points, o)
    } else {
        generator::linear_path(&case.points, o)
    };
    curve_path_ops(&drawable).to_vec()
}

#[test]
fn the_fixture_names_its_oracle() {
    let fixture = load();
    assert_eq!(
        fixture.oracle.excalidraw,
        "1118751f3e4958a0dc3d71934c093584fdb7c6f5"
    );
}

#[test]
fn sizes_and_angles_match_the_oracle() {
    let fixture = load();
    for (name, want) in &fixture.sizes_and_angles {
        let arrowhead: Arrowhead = serde_json::from_value(serde_json::Value::String(name.clone()))
            .unwrap_or_else(|e| panic!("fixture head name {name:?} is not a known Arrowhead: {e}"));
        assert_eq!(
            get_arrowhead_size(arrowhead),
            want.size,
            "getArrowheadSize({name}) drifted from the oracle"
        );
        assert_eq!(
            get_arrowhead_angle(arrowhead),
            want.angle,
            "getArrowheadAngle({name}) drifted from the oracle"
        );
    }
}

#[test]
fn every_case_matches_the_oracles_getarrowheadpoints() {
    let fixture = load();
    assert!(
        !fixture.cases.is_empty(),
        "the fixture has no cases to replay"
    );

    let mut worst_drift = 0.0f64;
    let mut worst_case = String::new();
    let mut checked = 0usize;

    for case in &fixture.cases {
        let ops = body_ops(case);
        let position = match case.position.as_str() {
            "start" => Position::Start,
            "end" => Position::End,
            other => panic!("unknown position {other:?} in fixture"),
        };

        let got = get_arrowhead_points(
            &case.points,
            case.stroke_width,
            &ops,
            position,
            case.arrowhead,
            case.offset_multiplier,
        );

        let what = || {
            format!(
                "points={:?} roundness={} strokeWidth={} roughness={} seed={} position={} arrowhead={:?} offset={}",
                case.points,
                case.roundness,
                case.stroke_width,
                case.roughness,
                case.seed,
                case.position,
                case.arrowhead,
                case.offset_multiplier
            )
        };

        match (&got, &case.expected) {
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                panic!(
                    "null-ness differs for {}: got {:?}, want {:?}",
                    what(),
                    got,
                    case.expected
                );
            }
            (Some(got), Some(want)) => {
                assert_eq!(
                    got.len(),
                    want.len(),
                    "coordinate count differs for {}: got {}, want {}",
                    what(),
                    got.len(),
                    want.len()
                );
                for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
                    let drift = (g - w).abs();
                    if drift > worst_drift {
                        worst_drift = drift;
                        worst_case = format!("{} coordinate {i}: got {g}, want {w}", what());
                    }
                }
            }
        }
        checked += 1;
    }

    assert_eq!(checked, fixture.cases.len());
    assert!(
        worst_drift <= TOLERANCE,
        "worst drift {worst_drift} exceeds {TOLERANCE} at {worst_case}"
    );
}
