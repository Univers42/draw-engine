//! Keyboard flowcharts against Excalidraw's own code.
//!
//! `tests/fixtures/flowchart.oracle.json` is what the oracle's `AppFlowchart`
//! (`App.flowchart.ts`, over `flowchart.ts`) did at the pin, run unmodified by
//! `tools/flowchart-oracle` through 285 key sequences: every direction, repeated presses,
//! direction changes, Escape, nodes of every kind, size and style, turned parents, frames,
//! existing children in the way, Alt+Arrow walks, and the camera at several zooms and UI
//! layouts. After every key event it holds the pending cluster, the scene, the selection
//! and where the camera landed; this replays each sequence through the engine the way
//! `src/host/keys.ts` drives it and asks for the same, within 1e-9.
//!
//! Arrows are compared whole: both are elbow-routed, so each arrow's position, size,
//! points, anchors and bindings are the oracle's. Not compared, and why (the fixture's
//! `note` says the same): the camera after a reveal that measured different bounds —
//! `getElementBounds` takes an arrow by its rough path, which wobbles a pixel or so with
//! the element's random seed, so such a step is not held to the oracle's camera, and ours
//! is put where theirs landed for the steps after it — and the geometry of an arrow at a
//! diamond with an explicit corner radius, which the engine reads past the oracle's cap
//! on purpose ([`Replay::radius_beyond_the_oracle`]); its bindings are still compared.

mod common;
use common::*;
use draw_engine::*;

use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn load() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/flowchart.oracle.json"
    ))
    .expect("fixture present — regenerate with the host's `make oracle-fixtures`");
    serde_json::from_str(&raw).expect("fixture parses")
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("a number, got {v}"))
}

fn kind_of(name: &str) -> DrawElementType {
    match name {
        "rectangle" => DrawElementType::Rectangle,
        "ellipse" => DrawElementType::Ellipse,
        "diamond" => DrawElementType::Diamond,
        "stickynote" => DrawElementType::StickyNote,
        "frame" => DrawElementType::Frame,
        "line" => DrawElementType::Line,
        "arrow" => DrawElementType::Arrow,
        other => panic!("no such kind in the sweep: {other}"),
    }
}

/// An oracle element as this engine holds it.
fn element_from(v: &Value) -> DrawElement {
    let mut el = create_element_default(
        kind_of(v["type"].as_str().unwrap()),
        Geometry {
            x: num(&v["x"]),
            y: num(&v["y"]),
            width: num(&v["width"]),
            height: num(&v["height"]),
        },
    );
    el.id = v["id"].as_str().unwrap().to_string();
    el.angle = num(&v["angle"]);
    el.stroke_color = v["strokeColor"].as_str().unwrap().to_string();
    el.background_color = v["backgroundColor"].as_str().unwrap().to_string();
    el.fill_style = serde_json::from_value(v["fillStyle"].clone()).unwrap();
    el.stroke_style = serde_json::from_value(v["strokeStyle"].clone()).unwrap();
    el.stroke_width = num(&v["strokeWidth"]);
    el.roughness = num(&v["roughness"]);
    el.opacity = num(&v["opacity"]);
    // The oracle's `roundness` is null or `{ type, value? }`; here a flag and a radius.
    el.roundness = (!v["roundness"].is_null()).then_some(8.0);
    el.corner_radius = v["roundness"]["value"].as_f64();
    el.frame_id = v["frameId"].as_str().map(str::to_string);
    el.base_height = v["baseHeight"].as_f64();
    if let Some(points) = v["points"].as_array() {
        el.points = Some(points.iter().map(|p| [num(&p[0]), num(&p[1])]).collect());
    }
    el
}

/// Which way the oracle's case is still comparable, and the ids the two sides gave the
/// same element.
struct Replay<'a> {
    case: &'a str,
    step: usize,
    /// Oracle id → engine id.
    ids: HashMap<String, String>,
    /// Every element the oracle has shown, by id — to look an arrow's ends up.
    known: HashMap<String, Value>,
    compared: Counts,
}

#[derive(Default)]
struct Counts {
    elements: usize,
    anchors: usize,
    arrows: usize,
    arrows_by_radius: usize,
    cameras: usize,
    cameras_skipped: usize,
}

impl Replay<'_> {
    fn at(&self) -> String {
        format!("{} / step {}", self.case, self.step)
    }

    fn close(&self, what: &str, ours: f64, theirs: f64) {
        assert!(
            (ours - theirs).abs() < EPS,
            "{}: {what} is {ours}, the oracle's {theirs}",
            self.at()
        );
    }

    /// Pairs the two sides' ids position by position.
    fn pair(&mut self, ours: &[DrawElement], theirs: &[Value], what: &str) {
        assert_eq!(
            ours.len(),
            theirs.len(),
            "{}: {what} has {} elements, the oracle's {}",
            self.at(),
            ours.len(),
            theirs.len()
        );
        for (o, t) in ours.iter().zip(theirs) {
            let id = t["id"].as_str().unwrap().to_string();
            self.known.insert(id.clone(), t.clone());
            if let Some(previous) = self.ids.insert(id.clone(), o.id.clone()) {
                assert_eq!(
                    previous,
                    o.id,
                    "{}: {what} put a different element where the oracle has {id}",
                    self.at()
                );
            }
        }
    }

    fn mapped(&self, theirs: &Value) -> Option<String> {
        theirs.as_str().map(|id| {
            self.ids
                .get(id)
                .cloned()
                .unwrap_or_else(|| panic!("{}: no element paired with {id}", self.at()))
        })
    }

    fn element(&mut self, what: &str, ours: &DrawElement, theirs: &Value) {
        let what = format!("{what} {}", theirs["id"]);
        let kind = theirs["type"].as_str().unwrap();
        assert_eq!(ours.kind, kind_of(kind), "{}: {what} kind", self.at());
        let arrow = kind == "arrow";
        if !arrow {
            self.close(&format!("{what} x"), ours.x, num(&theirs["x"]));
            self.close(&format!("{what} y"), ours.y, num(&theirs["y"]));
            self.close(&format!("{what} width"), ours.width, num(&theirs["width"]));
            self.close(
                &format!("{what} height"),
                ours.height,
                num(&theirs["height"]),
            );
        }
        self.close(&format!("{what} angle"), ours.angle, num(&theirs["angle"]));
        let at = self.at();
        assert_eq!(
            ours.stroke_color, theirs["strokeColor"],
            "{at}: {what} stroke"
        );
        assert_eq!(
            ours.background_color, theirs["backgroundColor"],
            "{at}: {what} background"
        );
        assert_eq!(
            serde_json::to_value(ours.fill_style).unwrap(),
            theirs["fillStyle"],
            "{at}: {what} fill"
        );
        assert_eq!(
            serde_json::to_value(ours.stroke_style).unwrap(),
            theirs["strokeStyle"],
            "{at}: {what} stroke style"
        );
        self.close(
            &format!("{what} stroke width"),
            ours.stroke_width,
            num(&theirs["strokeWidth"]),
        );
        self.close(
            &format!("{what} roughness"),
            ours.roughness,
            num(&theirs["roughness"]),
        );
        self.close(
            &format!("{what} opacity"),
            ours.opacity,
            num(&theirs["opacity"]),
        );
        assert_eq!(
            ours.roundness.is_some(),
            !theirs["roundness"].is_null(),
            "{at}: {what} corners"
        );
        assert_eq!(
            ours.corner_radius,
            theirs["roundness"]["value"].as_f64(),
            "{at}: {what} radius"
        );
        assert_eq!(
            ours.frame_id,
            self.mapped(&theirs["frameId"]),
            "{at}: {what} frame"
        );
        assert!(ours.group_ids.is_empty() && theirs["groupIds"] == Value::Array(vec![]));
        assert_eq!(ours.locked(), theirs["locked"] == true, "{at}: {what} lock");
        assert_eq!(
            ours.is_deleted,
            theirs["isDeleted"] == true,
            "{at}: {what} deleted"
        );
        if kind == "stickynote" {
            assert_eq!(
                ours.base_height,
                theirs["baseHeight"].as_f64(),
                "{at}: {what} base height"
            );
        }
        if arrow {
            self.arrow(&what, ours, theirs);
        }
        self.compared.elements += 1;
    }

    /// Whether an end of `arrow` is on a diamond with an explicit corner radius. The
    /// oracle reads that radius through `getCornerRadius`, capped at a quarter of the
    /// span; the engine's corner-radius handle takes it up to half the span on purpose
    /// (`render/shape.rs` › `corner_radius`), so the rounded tip an anchor snaps to is a
    /// different outline. A rectangle's side and an ellipse are untouched by it.
    fn radius_beyond_the_oracle(&self, arrow: &Value) -> bool {
        ["startBinding", "endBinding"].iter().any(|end| {
            arrow[end]["elementId"].as_str().is_some_and(|id| {
                let node = &self.known[id];
                node["type"] == "diamond" && !node["roundness"]["value"].is_null()
            })
        })
    }

    fn arrow(&mut self, what: &str, ours: &DrawElement, theirs: &Value) {
        let at = self.at();
        assert_eq!(
            ours.start_arrowhead.unwrap_or(Arrowhead::None),
            Arrowhead::None,
            "{at}: {what} tail"
        );
        assert!(
            theirs["startArrowhead"].is_null(),
            "{at}: {what} the oracle's tail"
        );
        assert_eq!(
            serde_json::to_value(ours.end_arrowhead).unwrap(),
            theirs["endArrowhead"],
            "{at}: {what} head"
        );
        assert_eq!(
            ours.elbowed == Some(true),
            theirs["elbowed"] == true,
            "{at}: {what} elbowed"
        );
        let geometry = !self.radius_beyond_the_oracle(theirs);
        if geometry {
            self.close(&format!("{what} x"), ours.x, num(&theirs["x"]));
            self.close(&format!("{what} y"), ours.y, num(&theirs["y"]));
            self.close(&format!("{what} width"), ours.width, num(&theirs["width"]));
            self.close(
                &format!("{what} height"),
                ours.height,
                num(&theirs["height"]),
            );
            let points = ours.points.as_deref().unwrap_or_default();
            let their_points = theirs["points"].as_array().unwrap();
            assert_eq!(
                points.len(),
                their_points.len(),
                "{at}: {what} has {} points, the oracle's {}: {:?} against {}",
                points.len(),
                their_points.len(),
                points,
                theirs["points"]
            );
            for (i, (o, t)) in points.iter().zip(their_points).enumerate() {
                self.close(&format!("{what} point {i} x"), o[0], num(&t[0]));
                self.close(&format!("{what} point {i} y"), o[1], num(&t[1]));
            }
            self.compared.arrows += 1;
        } else {
            self.compared.arrows_by_radius += 1;
        }
        for (end, target, fixed_point, mode) in [
            (
                "startBinding",
                &ours.start_binding,
                ours.start_fixed_point,
                ours.start_bind_mode,
            ),
            (
                "endBinding",
                &ours.end_binding,
                ours.end_fixed_point,
                ours.end_bind_mode,
            ),
        ] {
            let binding = &theirs[end];
            if binding.is_null() {
                assert!(
                    target.is_none(),
                    "{at}: {what} {end} is unbound in the oracle"
                );
                continue;
            }
            assert_eq!(
                target.clone(),
                self.mapped(&binding["elementId"]),
                "{at}: {what} {end} target"
            );
            assert_eq!(
                serde_json::to_value(mode).unwrap(),
                binding["mode"],
                "{at}: {what} {end} mode"
            );
            if geometry {
                let fixed = binding["fixedPoint"].as_array().unwrap();
                let ours = fixed_point.unwrap_or_else(|| panic!("{at}: {what} {end} anchor"));
                self.close(&format!("{what} {end} anchor x"), ours[0], num(&fixed[0]));
                self.close(&format!("{what} {end} anchor y"), ours[1], num(&fixed[1]));
                self.compared.anchors += 1;
            }
        }
    }

    fn elements(&mut self, what: &str, ours: &[DrawElement], theirs: &[Value]) {
        self.pair(ours, theirs, what);
        for (o, t) in ours.iter().zip(theirs) {
            self.element(what, o, t);
        }
    }
}

/// Plays one key event on `engine` the way `src/host/keys.ts` does, with `held` the
/// modifiers down while it happens. `keys.test.ts` pins the key-to-call mapping itself.
fn key_down(engine: &mut DrawEngine, held: &HashSet<&str>, key: &str) {
    let direction = match key {
        "ArrowUp" | "up" => Some(LinkDirection::Up),
        "ArrowDown" | "down" => Some(LinkDirection::Down),
        "ArrowLeft" | "left" => Some(LinkDirection::Left),
        "ArrowRight" | "right" => Some(LinkDirection::Right),
        _ => None,
    };
    if key == "Escape" && engine.is_creating_flowchart() {
        engine.flowchart_cancel();
        return;
    }
    let Some(direction) = direction else {
        return;
    };
    if held.contains("ctrlKey") && !held.contains("shiftKey") {
        engine.flowchart_create(direction);
    } else if held.contains("altKey") && engine.get_selection().len() == 1 {
        engine.flowchart_navigate(direction);
    }
}

fn key_up(engine: &mut DrawEngine, held: &HashSet<&str>) {
    if !held.contains("altKey") {
        engine.flowchart_navigation_end();
    }
    if !held.contains("ctrlKey") {
        engine.flowchart_commit();
    }
}

fn camera_of(v: &Value) -> Camera {
    let zoom = num(&v["zoom"]);
    Camera {
        x: num(&v["scrollX"]) * zoom,
        y: num(&v["scrollY"]) * zoom,
        scale: zoom,
    }
}

fn replay(case: &Value, counts: &mut Counts) {
    let name = case["name"].as_str().unwrap();
    let viewport = &case["viewport"];
    let initial: Vec<Value> = case["initial"].as_array().unwrap().clone();
    let mut engine = DrawEngine::new();
    engine.set_viewport(num(&viewport["width"]), num(&viewport["height"]), 1.0);
    engine.set_scene(Scene::new(initial.iter().map(element_from)));
    let ui = &viewport["ui"];
    if !ui.is_null() {
        engine.set_viewport_offsets(Offsets {
            top: num(&ui["top"]),
            right: num(&ui["right"]),
            bottom: num(&ui["bottom"]),
            left: num(&ui["left"]),
        });
    }
    engine.set_camera(Camera {
        x: viewport["scrollX"].as_f64().unwrap_or(0.0) * viewport["zoom"].as_f64().unwrap_or(1.0),
        y: viewport["scrollY"].as_f64().unwrap_or(0.0) * viewport["zoom"].as_f64().unwrap_or(1.0),
        scale: viewport["zoom"].as_f64().unwrap_or(1.0),
    });

    let mut replay = Replay {
        case: name,
        step: 0,
        ids: HashMap::new(),
        known: HashMap::new(),
        compared: Counts::default(),
    };
    replay.pair(&engine.get_scene(), &initial, "the initial scene");

    let mut held: HashSet<&str> = HashSet::new();
    let mut scene = initial;
    let mut now = 0.0;
    for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        replay.step = index;
        now += 1000.0;
        engine.set_now(now);
        match step["op"].as_str().unwrap() {
            "select" => {
                let ids: Vec<String> = step["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| replay.mapped(id).unwrap())
                    .collect();
                engine.select(ids);
            }
            "hold" => {
                held.insert(step["modifier"].as_str().unwrap());
            }
            "release" => {
                held.remove(step["modifier"].as_str().unwrap());
                key_up(&mut engine, &held);
            }
            "press" => {
                let key = step["key"].as_str().unwrap();
                key_down(&mut engine, &held, key);
                key_up(&mut engine, &held);
            }
            other => panic!("{name}: unknown step {other}"),
        }
        // Past any ease the step started: the oracle's camera is recorded where it lands.
        engine.set_now(now + 500.0);

        let pending: Vec<Value> = step["pending"].as_array().unwrap().clone();
        replay.elements("pending", &engine.pending_flowchart_elements(), &pending);
        assert_eq!(
            engine.is_creating_flowchart(),
            step["creating"] == true,
            "{}: creating",
            replay.at()
        );
        if let Some(next) = step["scene"].as_array() {
            scene = next.clone();
        }
        replay.elements("scene", &engine.get_scene(), &scene);

        let selected: HashSet<String> = step["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| replay.mapped(id).unwrap())
            .collect();
        let ours: HashSet<String> = engine.get_selection().into_iter().collect();
        assert_eq!(ours, selected, "{}: selection", replay.at());

        // The same bounds must give the same camera. Only a reveal of the pending cluster
        // can measure differently — `getElementBounds` takes an arrow by its rough path,
        // which wobbles a pixel or so with the seed — so where the two disagree the step is
        // not held to the oracle's camera, and ours is put where theirs landed for the
        // steps after it.
        let theirs = camera_of(&step["camera"]);
        let same_bounds = step.get("reveal").is_none_or(|reveal| {
            pending.is_empty()
                || scene_outline_bounds(engine.pending_flowchart_elements().iter()).is_some_and(
                    |b| {
                        [b.min_x, b.min_y, b.max_x, b.max_y]
                            .iter()
                            .zip(reveal["bounds"].as_array().unwrap())
                            .all(|(ours, theirs)| (ours - num(theirs)).abs() < EPS)
                    },
                )
        });
        if same_bounds {
            replay.close("camera x", engine.camera.x, theirs.x);
            replay.close("camera y", engine.camera.y, theirs.y);
            replay.close("camera zoom", engine.camera.scale, theirs.scale);
            replay.compared.cameras += 1;
        } else {
            engine.set_camera(theirs);
            replay.compared.cameras_skipped += 1;
        }
    }
    counts.elements += replay.compared.elements;
    counts.anchors += replay.compared.anchors;
    counts.arrows += replay.compared.arrows;
    counts.arrows_by_radius += replay.compared.arrows_by_radius;
    counts.cameras += replay.compared.cameras;
    counts.cameras_skipped += replay.compared.cameras_skipped;
}

#[test]
fn every_case_matches_the_oracle() {
    let fixture = load();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() > 250, "the sweep is all there");
    let mut counts = Counts::default();
    for case in cases {
        replay(case, &mut counts);
    }
    // What was actually held to the oracle, so a sweep that quietly compared nothing
    // cannot pass.
    eprintln!(
        "flowchart oracle: {} elements, {} arrows ({} at a radius past the oracle's, \
         skipped), {} anchors, {} cameras ({} skipped)",
        counts.elements,
        counts.arrows,
        counts.arrows_by_radius,
        counts.anchors,
        counts.cameras,
        counts.cameras_skipped
    );
    assert!(counts.arrows > 1000 && counts.anchors == 2 * counts.arrows);
    assert!(counts.arrows > 20 * counts.arrows_by_radius);
    assert!(counts.cameras > 5 * counts.cameras_skipped);
}
