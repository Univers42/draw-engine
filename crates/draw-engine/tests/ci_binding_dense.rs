//! Aiming an arrow into a dense pack of overlapping shapes.
//!
//! A pack is what Ctrl+D leaves: one rectangle duplicated again and again, each copy ten
//! units down and right of the last and on top of it. Almost every point in it is inside
//! several squares at once. The shape an arrow end takes is the one whose **outline** is
//! nearest the pointer — positive distance inside, negative outside — so a click just
//! outside a packed square's edge binds that square in orbit and finishes the arrow,
//! however many other squares the point happens to be inside.
//!
//! The rule is Excalidraw's live one, `getBindingCandidates` +
//! `getHoveredElementForBinding` at 1118751f (`packages/element/src/collision.ts:301-486`,
//! #10753 4850bf33 and dc2c16d9), and every expected answer in the probe table below
//! was measured on excalidraw.com, which serves that build.

mod common;
use common::*;
use draw_engine::scene::binding::{arrow_target_among, is_inside, max_binding_distance};
use draw_engine::scene::element::BindMode;
use draw_engine::scene::geometry::distance_to_segment;
use draw_engine::scene::outline_distance::signed_outline_distance;
use draw_engine::*;

const INSIDE: BindMode = BindMode::Inside;
const ORBIT: BindMode = BindMode::Orbit;

/// Where every arrow in the pack tests starts: the centre of a lone square, well away
/// from the pack.
const FROM: (f64, f64) = (150.0, 750.0);

/// The oracle's scene: a lone square `S` at (100, 700), then a square at (400, 200) and
/// 29 Ctrl+D copies of it, all 100×100, rounded and transparent. Named `r0..r29` bottom to
/// top — the names the probe table uses.
fn pack(step: f64) -> Vec<DrawElement> {
    let mut lone = box_at(100.0, 700.0, 100.0, 100.0);
    lone.id = "S".into();
    let mut seed = box_at(400.0, 200.0, 100.0, 100.0);
    seed.id = "seed".into();
    let mut engine = engine_with_scene(vec![lone, seed]);
    engine.select(vec!["seed".to_string()]);
    for _ in 1..30 {
        engine.duplicate_selection(step, step);
    }
    let mut out = engine.get_scene();
    let mut k = 0;
    for el in out.iter_mut().filter(|el| el.id != "S") {
        el.id = format!("r{k}");
        k += 1;
    }
    assert_eq!(k, 30);
    out
}

fn arrow_engine(scene: &[DrawElement], scale: f64) -> DrawEngine {
    let mut engine = engine_with_scene(scene.to_vec());
    engine.set_viewport(1600.0, 1200.0, 1.0);
    engine.set_camera(Camera {
        x: 0.0,
        y: 0.0,
        scale,
    });
    engine.set_tool(DrawTool::Arrow);
    engine
}

fn the_arrow(engine: &DrawEngine) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.kind == DrawElementType::Arrow && !el.is_deleted)
        .expect("an arrow was drawn")
}

fn end_of(engine: &DrawEngine) -> Option<(String, BindMode)> {
    let arrow = the_arrow(engine);
    Some((arrow.end_binding?, arrow.end_bind_mode.unwrap_or(ORBIT)))
}

/// Pointer moves from `from` to `to` in ten steps, in world units at `scale`.
fn glide(engine: &mut DrawEngine, scale: f64, from: (f64, f64), to: (f64, f64)) {
    for k in 1..=10 {
        let t = k as f64 / 10.0;
        engine.move_pointer(
            (from.0 + (to.0 - from.0) * t) * scale,
            (from.1 + (to.1 - from.1) * t) * scale,
            false,
            false,
        );
    }
}

fn click(engine: &mut DrawEngine, scale: f64, at: (f64, f64)) {
    engine.begin_pointer(at.0 * scale, at.1 * scale, false, false);
    engine.end_pointer();
}

/// A dragged arrow from `FROM` to `to`: where its end binds.
fn drag_to(scene: &[DrawElement], scale: f64, to: (f64, f64)) -> Option<(String, BindMode)> {
    let mut engine = arrow_engine(scene, scale);
    engine.begin_pointer(FROM.0 * scale, FROM.1 * scale, false, false);
    glide(&mut engine, scale, FROM, to);
    engine.end_pointer();
    assert!(engine.linear_in_progress().is_none());
    end_of(&engine)
}

/// What a click-mode arrow does with a press at `to`.
struct ClickOutcome {
    /// The shape the outline highlight showed while hovering there.
    highlight: Option<String>,
    /// Whether that press finished the arrow.
    finished: bool,
    /// Where the end bound, once the arrow is finished (by the press, or by Escape).
    end: Option<(String, BindMode)>,
}

fn click_to(scene: &[DrawElement], scale: f64, to: (f64, f64)) -> ClickOutcome {
    let mut engine = arrow_engine(scene, scale);
    click(&mut engine, scale, FROM);
    assert!(
        engine.linear_in_progress().is_some(),
        "a click starts a path"
    );
    glide(&mut engine, scale, FROM, to);
    let highlight = engine
        .paint_view()
        .binding_highlight
        .map(|el| el.id.clone());
    click(&mut engine, scale, to);
    let finished = engine.linear_in_progress().is_none();
    if !finished {
        engine.finish_linear();
    }
    ClickOutcome {
        highlight,
        finished,
        end: end_of(&engine),
    }
}

/// Measured on excalidraw.com @1118751f (the probe script drags and clicks from `S`'s
/// centre to each point; the answer is the end binding it saved). Points are chosen off
/// every exact tie.
const ORACLE_PROBES: [(f64, f64, &str, BindMode); 48] = [
    (471.3, 270.7, "r6", INSIDE),
    (506.3, 235.7, "r1", INSIDE),
    (436.3, 305.7, "r4", ORBIT),
    (482.4, 251.9, "r5", INSIDE),
    (501.3, 300.7, "r1", INSIDE),
    (536.3, 265.7, "r4", INSIDE),
    (466.3, 335.7, "r7", ORBIT),
    (512.4, 281.9, "r8", INSIDE),
    (541.3, 340.7, "r5", INSIDE),
    (576.3, 305.7, "r8", INSIDE),
    (506.3, 375.7, "r11", ORBIT),
    (552.4, 321.9, "r12", INSIDE),
    (591.3, 390.7, "r10", INSIDE),
    (626.3, 355.7, "r13", INSIDE),
    (556.3, 425.7, "r16", ORBIT),
    (602.4, 371.9, "r17", INSIDE),
    (651.3, 450.7, "r16", INSIDE),
    (686.3, 415.7, "r19", INSIDE),
    (616.3, 485.7, "r22", ORBIT),
    (662.4, 431.9, "r23", INSIDE),
    (711.3, 510.7, "r22", INSIDE),
    (746.3, 475.7, "r25", INSIDE),
    (676.3, 545.7, "r28", ORBIT),
    (722.4, 491.9, "r29", INSIDE),
    // Around the right (R) and top (T) edges of r3, r9, r15 and r21: 2.3 inside, 0.4
    // outside, 2.3 outside.
    (527.7, 280.7, "r8", INSIDE),
    (481.3, 232.3, "r3", INSIDE),
    (530.4, 280.7, "r3", ORBIT),
    (481.3, 229.6, "r3", ORBIT),
    (532.3, 280.7, "r8", INSIDE),
    (481.3, 227.7, "r3", ORBIT),
    (587.7, 340.7, "r14", INSIDE),
    (541.3, 292.3, "r4", ORBIT),
    (590.4, 340.7, "r9", ORBIT),
    (541.3, 289.6, "r9", ORBIT),
    (592.3, 340.7, "r14", INSIDE),
    (541.3, 287.7, "r4", ORBIT),
    (647.7, 400.7, "r20", INSIDE),
    (601.3, 352.3, "r10", ORBIT),
    (650.4, 400.7, "r15", ORBIT),
    (601.3, 349.6, "r15", ORBIT),
    (652.3, 400.7, "r20", INSIDE),
    (601.3, 347.7, "r10", ORBIT),
    (707.7, 460.7, "r26", INSIDE),
    (661.3, 412.3, "r16", ORBIT),
    (710.4, 460.7, "r21", ORBIT),
    (661.3, 409.6, "r21", ORBIT),
    (712.3, 460.7, "r26", INSIDE),
    (661.3, 407.7, "r16", ORBIT),
];

/// The oracle's Ctrl+D offset: `DEFAULT_GRID_SIZE / 2`
/// (`packages/excalidraw/actions/actionDuplicateSelection.tsx:78-79`).
const CTRL_D: f64 = 10.0;

#[test]
fn nearest_outline_wins_in_a_ctrl_d_pack() {
    let scene = pack(CTRL_D);
    let mut wrong = Vec::new();
    for (x, y, id, mode) in ORACLE_PROBES {
        let got = drag_to(&scene, 1.0, (x, y));
        if got != Some((id.to_string(), mode)) {
            wrong.push(format!("({x}, {y}): want {id}/{mode:?}, got {got:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} probes disagree with excalidraw.com:\n{}",
        wrong.len(),
        ORACLE_PROBES.len(),
        wrong.join("\n")
    );
}

/// The user's report: in a pack, clicking a square never finished the arrow. A press
/// whose end would orbit a square finishes it there; one inside the nearest square places
/// a waypoint (Excalidraw's `binding.test.tsx:240/259`). Either way the end is the one
/// the oracle chose.
#[test]
fn a_click_just_outside_a_packed_square_binds_and_finishes() {
    let scene = pack(CTRL_D);
    // 2.3 above r3's top edge (inside r0..r2), 0.4 right of r9's (inside r10..r14).
    for (at, id) in [((481.3, 227.7), "r3"), ((590.4, 340.7), "r9")] {
        let got = click_to(&scene, 1.0, at);
        assert!(got.finished, "a click at {at:?} finishes the arrow");
        assert_eq!(got.end, Some((id.to_string(), ORBIT)), "at {at:?}");
    }
    let mut wrong = Vec::new();
    for (x, y, id, mode) in ORACLE_PROBES {
        let got = click_to(&scene, 1.0, (x, y));
        if got.finished != (mode == ORBIT) || got.end != Some((id.to_string(), mode)) {
            wrong.push(format!(
                "({x}, {y}): want {id}/{mode:?} finished={}, got {:?} finished={}",
                mode == ORBIT,
                got.end,
                got.finished
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Guards against over-fixing: a click inside the square whose outline is nearest still
/// places a waypoint, and a second click on it — a double click — ends the arrow there.
#[test]
fn a_click_inside_the_nearest_square_still_places_a_waypoint() {
    let scene = pack(CTRL_D);
    let at = (471.3, 270.7);
    let mut engine = arrow_engine(&scene, 1.0);
    click(&mut engine, 1.0, FROM);
    glide(&mut engine, 1.0, FROM, at);
    click(&mut engine, 1.0, at);
    assert!(
        engine.linear_in_progress().is_some(),
        "a waypoint, not a finish"
    );
    click(&mut engine, 1.0, at);
    assert!(engine.linear_in_progress().is_none());
    assert_eq!(end_of(&engine), Some(("r6".to_string(), INSIDE)));
}

/// Walks a grid over the pack. At every point the square the hover outlined is the one
/// the click binds, and the click finishes exactly when that binding is an orbit — so
/// what the highlight shows is what the click does. This is the deliberate divergence
/// from Excalidraw, which re-tests the press at its moved preview point
/// (`App.tsx@1118751f:10170`) and so turns some orbit highlights into waypoints.
#[test]
fn the_highlight_is_what_the_click_does() {
    let scene = pack(CTRL_D);
    for scale in [1.0, 1.61] {
        let mut probes = 0;
        let mut y = 203.0;
        while y < 790.0 {
            let mut x = 403.0 + (y - 203.0) % 17.0;
            while x < 790.0 {
                let got = click_to(&scene, scale, (x, y));
                let (id, mode) = got.end.clone().unwrap_or_else(|| ("-".into(), INSIDE));
                assert_eq!(
                    got.highlight.as_deref(),
                    got.end.as_ref().map(|_| id.as_str()),
                    "({x}, {y}) at {scale}: highlight vs end"
                );
                assert_eq!(
                    got.finished,
                    got.end.is_some() && mode == ORBIT,
                    "({x}, {y}) at {scale}: finished vs {id}/{mode:?}"
                );
                probes += 1;
                x += 23.0;
            }
            y += 19.0;
        }
        assert!(probes > 100, "the grid covers the pack ({probes})");
    }
}

/// How often a click in a pack finishes, over a grid of 9 in both directions. Before
/// the nearest-outline rule none did: every point in a pack is inside some square, and
/// the end always bound inside the topmost of them.
#[test]
fn a_dense_pack_finishes_most_clicks() {
    let scene = pack(CTRL_D);
    let squares: Vec<&DrawElement> = scene.iter().filter(|el| el.id != "S").collect();
    let in_pack = |x: f64, y: f64| {
        squares
            .iter()
            .any(|r| x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height)
    };
    let (mut total, mut finished) = (0, 0);
    let mut y = 203.0;
    while y < 790.0 {
        let mut x = 403.0;
        while x < 790.0 {
            if in_pack(x, y) {
                total += 1;
                if click_to(&scene, 1.0, (x, y)).finished {
                    finished += 1;
                }
            }
            x += 9.0;
        }
        y += 9.0;
    }
    println!("dense pack: {finished} of {total} grid clicks finish");
    assert!(total > 800, "the grid covers the pack ({total})");
    assert!(
        finished * 100 >= total * FINISH_FLOOR_PERCENT,
        "only {finished} of {total} clicks in the pack finished"
    );
}

/// Measured with the nearest-outline rule: 457 of the 816 grid clicks (56%) finish; the
/// rest are points whose nearest outline belongs to a square they are inside, which
/// place a waypoint as in Excalidraw.
const FINISH_FLOOR_PERCENT: usize = 52;

// ------------------------------------------------------------------------- the outline

/// A lone rounded square at the origin: its corner radius is 25 (Excalidraw's adaptive
/// radius), so (2, 2) is inside its box but well outside its painted outline.
#[test]
fn a_rounded_corner_is_outside_the_shape() {
    let mut rect = box_at(0.0, 0.0, 100.0, 100.0);
    rect.id = "rect".into();
    assert!(rect.roundness.is_some());
    let corner = Point { x: 2.0, y: 2.0 };
    assert!(!is_inside(&rect, corner));
    assert!(is_inside(&rect, Point { x: 20.0, y: 20.0 }));

    let scene = vec![rect];
    let mut engine = arrow_engine(&scene, 1.0);
    engine.set_camera(Camera {
        x: 400.0,
        y: 400.0,
        scale: 1.0,
    });
    let screen = |p: (f64, f64)| (p.0 + 400.0, p.1 + 400.0);
    let (sx, sy) = screen((-200.0, 50.0));
    engine.begin_pointer(sx, sy, false, false);
    engine.end_pointer();
    let (tx, ty) = screen((2.0, 2.0));
    engine.move_pointer(tx, ty, false, false);
    engine.begin_pointer(tx, ty, false, false);
    engine.end_pointer();
    assert!(
        engine.linear_in_progress().is_none(),
        "a click beside the rounded corner finishes the arrow"
    );
    assert_eq!(end_of(&engine), Some(("rect".to_string(), ORBIT)));
}

/// A point exactly on the outline is outside: excalidraw.com bound (530, 280), on r8's
/// top edge, in orbit.
#[test]
fn on_the_outline_is_outside() {
    let mut rect = box_at(0.0, 0.0, 100.0, 100.0);
    rect.id = "rect".into();
    rect.roundness = None;
    for p in [(50.0, 0.0), (100.0, 50.0), (50.0, 100.0), (0.0, 50.0)] {
        assert!(!is_inside(&rect, Point { x: p.0, y: p.1 }), "{p:?}");
    }
    let scene = vec![rect];
    let mut engine = arrow_engine(&scene, 1.0);
    engine.begin_pointer(300.0, 50.0, false, false);
    glide(&mut engine, 1.0, (300.0, 50.0), (100.0, 50.0));
    engine.end_pointer();
    assert_eq!(end_of(&engine), Some(("rect".to_string(), ORBIT)));
}

// ------------------------------------------------------------------ the oracle's tests

fn target(elements: Vec<DrawElement>, x: f64, y: f64, tolerance: f64) -> Option<String> {
    let scene = Scene::new(elements);
    arrow_target_among(
        scene.iter_ordered().rev(),
        &|id| scene.get(id),
        x,
        y,
        tolerance,
        None,
    )
    .map(|el| el.id.clone())
}

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> DrawElement {
    let mut el = box_at(x, y, w, h);
    el.id = id.into();
    el
}

/// The reach at zoom 1: Excalidraw's `maxBindingDistance_simple`.
const REACH: f64 = 15.0;

/// `collision.test.tsx@1118751f:787-816`.
#[test]
fn upstream_overlap_cases() {
    let container = || rect("container", 0.0, 0.0, 200.0, 200.0);
    // 18 inside the container's left edge.
    let child = || rect("child", 18.0, 80.0, 60.0, 40.0);
    // 5 inside the container's edge, 13 outside the child: the container's edge.
    assert_eq!(
        target(vec![container(), child()], 5.0, 100.0, REACH).as_deref(),
        Some("container")
    );
    // Closer to the child's outline than to the container's.
    assert_eq!(
        target(vec![container(), child()], 10.0, 100.0, REACH).as_deref(),
        Some("child")
    );

    let badge = || rect("badge", -30.0, 80.0, 60.0, 40.0);
    // 3 inside the container's edge, inside the badge: the smaller shape nested over
    // the container's edge.
    assert_eq!(
        target(vec![container(), badge()], 3.0, 92.0, REACH).as_deref(),
        Some("badge")
    );
    // 5 outside the container's edge, inside the badge: the closer container outline.
    assert_eq!(
        target(vec![container(), badge()], -5.0, 108.0, REACH).as_deref(),
        Some("container")
    );
}

/// `collision.test.tsx@1118751f:693-760` and the circle case after it.
#[test]
fn upstream_occlusion_cases() {
    let hidden = || rect("hidden", 30.0, 30.0, 40.0, 40.0);
    let cover = |background: &str| {
        let mut el = rect("cover", 0.0, 0.0, 100.0, 100.0);
        el.background_color = background.into();
        el
    };
    assert_eq!(
        target(vec![hidden(), cover("#ffc9c9")], 50.0, 50.0, REACH).as_deref(),
        Some("cover")
    );
    assert_eq!(
        target(vec![hidden(), cover("transparent")], 50.0, 50.0, REACH).as_deref(),
        Some("hidden")
    );

    // A picture hides what is behind it, whatever its background says.
    let mut image = create_element_default(
        DrawElementType::Image,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    );
    image.id = "image".into();
    assert_eq!(
        target(vec![hidden(), image], 50.0, 50.0, REACH).as_deref(),
        Some("image")
    );

    // A locked shape cannot be bound, and an opaque one cannot be bound through either.
    let locked = |background: &str| {
        let mut el = cover(background);
        el.id = "locked".into();
        el.locked = Some(true);
        el
    };
    assert_eq!(
        target(vec![hidden(), locked("#ffc9c9")], 50.0, 50.0, REACH),
        None
    );
    assert_eq!(
        target(vec![hidden(), locked("transparent")], 50.0, 50.0, REACH).as_deref(),
        Some("hidden")
    );

    // At a circle's exact centre: bound, and an opaque one still hides what is under it.
    let circle = |background: &str| {
        let mut el = ellipse_at(0.0, 0.0, 200.0, 200.0);
        el.id = "circle".into();
        el.background_color = background.into();
        el
    };
    let small = || rect("small", 90.0, 90.0, 20.0, 20.0);
    assert_eq!(
        target(vec![circle("transparent")], 100.0, 100.0, REACH).as_deref(),
        Some("circle")
    );
    assert_eq!(
        target(vec![small(), circle("#ffc9c9")], 100.0, 100.0, REACH).as_deref(),
        Some("circle")
    );
}

/// `collision.test.tsx@1118751f:672-691`: the reach is 15 world units at zoom 1 and grows
/// as the view zooms out (25 at 0.4), whatever tool asks — the hover outline here.
#[test]
fn binding_reach_matches_oracle() {
    let scene = vec![rect("rect", 0.0, 0.0, 100.0, 100.0)];
    let hovered = |scale: f64, at: (f64, f64)| {
        let mut engine = arrow_engine(&scene, scale);
        engine.hover_pointer(at.0 * scale, at.1 * scale);
        engine
            .paint_view()
            .binding_highlight
            .map(|el| el.id.clone())
    };
    // 20 outside the right edge.
    assert_eq!(hovered(1.0, (120.0, 50.0)), None);
    assert_eq!(hovered(0.4, (120.0, 50.0)).as_deref(), Some("rect"));
    // 14 outside binds at zoom 1, and zooming in never shrinks the reach below 15.
    assert_eq!(hovered(1.0, (114.0, 50.0)).as_deref(), Some("rect"));
    assert_eq!(hovered(4.0, (114.0, 50.0)).as_deref(), Some("rect"));
    // Zoomed far out the reach stops growing at 30.
    assert_eq!(hovered(0.1, (129.0, 50.0)).as_deref(), Some("rect"));
    assert_eq!(hovered(0.1, (131.0, 50.0)), None);
}

// ------------------------------------------------------------------ the double click

fn texts(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|el| el.kind == DrawElementType::Text && !el.is_deleted)
        .collect()
}

/// A double click that finishes a click-mode arrow — its second click lands on the
/// waypoint its first placed — goes on to open a label on **the arrow**, never on the
/// square under the pointer. Checked on excalidraw.com with element ids: the typed text
/// was bound to the new arrow's id, inside the pack and over an empty board alike. The
/// oracle gets there because the finished arrow is the one selected element
/// (`getTextBindableContainerAtPosition`, `App.tsx@1118751f:6831-6838`).
#[test]
fn a_double_click_that_finishes_an_arrow_labels_the_arrow() {
    let scene = pack(CTRL_D);
    for at in [(471.3, 270.7), (1000.0, 150.0)] {
        let mut engine = arrow_engine(&scene, 1.0);
        click(&mut engine, 1.0, FROM);
        glide(&mut engine, 1.0, FROM, at);
        click(&mut engine, 1.0, at);
        click(&mut engine, 1.0, at);
        assert!(engine.linear_in_progress().is_none());
        engine.handle_double_click(at.0, at.1);
        let arrow = the_arrow(&engine);
        let labels: Vec<Option<String>> =
            texts(&engine).into_iter().map(|t| t.container_id).collect();
        assert_eq!(labels, vec![Some(arrow.id.clone())], "at {at:?}");
    }
}

/// Double clicks whose **first** click already ends the arrow, because it lands beside a
/// shape the arrow did not start from (`boundOutsideFromElsewhere`,
/// `App.tsx@1118751f:10189-10215`), and what excalidraw.com typed into, by element id:
/// the arrow (`true`) or a free text at the pointer (`false`). The finished arrow is
/// still the one selected element when the `dblclick` arrives, so it is the only
/// container on offer (`App.tsx@1118751f:6831-6838`), and it takes the text only when the
/// double click is on it (`:7356-7392`). Where the orbit moved the end away from the
/// pointer, along the line to the square's centre, it is not.
const ORBIT_DOUBLE_CLICKS: [((f64, f64), bool); 5] = [
    ((481.3, 227.7), false),
    ((590.4, 340.7), false),
    ((436.3, 305.7), true),
    ((466.3, 335.7), true),
    // Beside the lone square `T`.
    ((1106.0, 550.0), false),
];

/// The pack, and a lone square `T` at (1000, 500) away from it.
fn pack_and_lone_square() -> Vec<DrawElement> {
    let mut scene = pack(CTRL_D);
    let mut lone = box_at(1000.0, 500.0, 100.0, 100.0);
    lone.id = "T".into();
    scene.push(lone);
    scene
}

/// An arrow from `FROM` whose first click, at `at`, ends it.
fn finish_by_one_click(scene: &[DrawElement], at: (f64, f64)) -> DrawEngine {
    let mut engine = arrow_engine(scene, 1.0);
    click(&mut engine, 1.0, FROM);
    glide(&mut engine, 1.0, FROM, at);
    click(&mut engine, 1.0, at);
    assert!(
        engine.linear_in_progress().is_none(),
        "the first click at {at:?} ends the arrow"
    );
    engine
}

/// The browser's sequence for a double click whose first click ended the arrow: that
/// click, a second press and release on the same spot, then `dblclick`. It never labels
/// a square — not the one the arrow bound, nor one the point is inside.
#[test]
fn a_double_click_whose_first_click_ends_the_arrow() {
    let scene = pack_and_lone_square();
    for (at, labels_arrow) in ORBIT_DOUBLE_CLICKS {
        let mut engine = finish_by_one_click(&scene, at);
        click(&mut engine, 1.0, at);
        engine.handle_double_click(at.0, at.1);
        let arrow = the_arrow(&engine);
        let labels: Vec<Option<String>> =
            texts(&engine).into_iter().map(|t| t.container_id).collect();
        let expected = labels_arrow.then(|| arrow.id.clone());
        assert_eq!(labels, vec![expected], "at {at:?}");
    }
}

// The finished arrow is what the double click of the press that finished it is about —
// not a later one elsewhere, nor one whose presses went to a pan or to a host tool, which
// never reach the engine's press handling. Each starts from a first click that ends the
// arrow where its end stays under the pointer, so the arrow would take a label.
const ON_ITS_END: (f64, f64) = (436.3, 305.7);

fn arrow_labelled(engine: &DrawEngine) -> bool {
    let arrow = the_arrow(engine);
    texts(engine)
        .iter()
        .any(|t| t.container_id.as_deref() == Some(arrow.id.as_str()))
}

/// Far away, with no press in between: a free text there, as on any empty spot.
#[test]
fn a_double_click_elsewhere_is_not_about_the_finished_arrow() {
    let mut engine = finish_by_one_click(&pack_and_lone_square(), ON_ITS_END);
    engine.handle_double_click(1200.0, 1000.0);
    let made: Vec<(Option<String>, f64, f64)> = texts(&engine)
        .into_iter()
        .map(|t| (t.container_id, t.x, t.y))
        .collect();
    assert_eq!(made, vec![(None, 1200.0, 1000.0)]);
}

/// Both presses of the double click went to a pan (Space held).
#[test]
fn a_double_click_after_a_pan_is_not_about_the_finished_arrow() {
    let at = ON_ITS_END;
    let mut engine = finish_by_one_click(&pack_and_lone_square(), at);
    for _ in 0..2 {
        engine.begin_pan(at.0, at.1);
        engine.end_pointer();
    }
    engine.handle_double_click(at.0, at.1);
    assert!(!arrow_labelled(&engine));
}

/// With the tool locked the arrow tool stays on; the host's sticky tool then parks the
/// engine on select and claims both presses itself.
#[test]
fn a_double_click_after_another_tool_is_not_about_the_finished_arrow() {
    let at = ON_ITS_END;
    let mut engine = arrow_engine(&pack_and_lone_square(), 1.0);
    engine.set_tool_locked(true);
    click(&mut engine, 1.0, FROM);
    glide(&mut engine, 1.0, FROM, at);
    click(&mut engine, 1.0, at);
    assert!(engine.linear_in_progress().is_none());
    engine.set_tool(DrawTool::Select);
    engine.handle_double_click(at.0, at.1);
    assert!(!arrow_labelled(&engine));
}

/// A double click that finishes a line types nothing — no label on the square it ends
/// inside — and leaves the line selected. excalidraw.com opened its line editor on the
/// new line, with no text anywhere (`App.tsx@1118751f:7222-7234`).
#[test]
fn a_double_click_that_finishes_a_line_types_nothing() {
    let scene = pack(CTRL_D);
    let at = (471.3, 270.7);
    let mut engine = arrow_engine(&scene, 1.0);
    engine.set_tool(DrawTool::Line);
    click(&mut engine, 1.0, FROM);
    glide(&mut engine, 1.0, FROM, at);
    click(&mut engine, 1.0, at);
    click(&mut engine, 1.0, at);
    assert!(engine.linear_in_progress().is_none());
    engine.handle_double_click(at.0, at.1);
    assert!(texts(&engine).is_empty());
    let selected: Vec<DrawElementType> = engine
        .get_selected_elements()
        .into_iter()
        .map(|el| el.kind)
        .collect();
    assert_eq!(selected, vec![DrawElementType::Line]);
}

/// A double click on an empty board with the arrow tool places nothing: its path of one
/// point is thrown away, and no text box is left in its place. Excalidraw's double click
/// does nothing while a path is being placed (`App.tsx@1118751f:7197-7201`).
#[test]
fn a_double_click_that_places_no_arrow_opens_nothing() {
    let mut engine = arrow_engine(&[], 1.0);
    click(&mut engine, 1.0, (300.0, 300.0));
    click(&mut engine, 1.0, (300.0, 300.0));
    engine.handle_double_click(300.0, 300.0);
    assert!(engine.get_scene().iter().all(|el| el.is_deleted));
}

// ------------------------------------------------------------- the distance, in itself

/// The painted outline of a rounded box, as a dense polyline: the sides and each corner's
/// quadratic sampled finely. What the closed-form distance is checked against.
fn sampled_outline(w: f64, h: f64, r: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let corners = [
        ((w - r, 0.0), (w, 0.0), (w, r)),
        ((w, h - r), (w, h), (w - r, h)),
        ((r, h), (0.0, h), (0.0, h - r)),
        ((0.0, r), (0.0, 0.0), (r, 0.0)),
    ];
    for (a, c, b) in corners {
        for i in 0..=4000 {
            let t = i as f64 / 4000.0;
            let s = 1.0 - t;
            out.push((
                s * s * a.0 + 2.0 * s * t * c.0 + t * t * b.0,
                s * s * a.1 + 2.0 * s * t * c.1 + t * t * b.1,
            ));
        }
    }
    out
}

fn sampled_distance(outline: &[(f64, f64)], p: (f64, f64)) -> f64 {
    let n = outline.len();
    (0..n)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            distance_to_segment(p.0, p.1, a.0, a.1, b.0, b.1)
        })
        .fold(f64::INFINITY, f64::min)
}

fn sampled_inside(outline: &[(f64, f64)], p: (f64, f64)) -> bool {
    let ring: Vec<Point> = outline.iter().map(|&(x, y)| Point { x, y }).collect();
    polygon_includes_point_non_zero(Point { x: p.0, y: p.1 }, &ring)
}

fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[test]
fn signed_outline_distance_of_a_rounded_box() {
    // 100×100 with Excalidraw's adaptive radius, 25.
    let rect = box_at(0.0, 0.0, 100.0, 100.0);
    let d = |x: f64, y: f64| signed_outline_distance(&rect, at(x, y));
    assert_close(d(50.0, 50.0), 50.0);
    assert_eq!(d(50.0, 0.0), 0.0);
    assert!(!is_inside(&rect, at(50.0, 0.0)));
    assert_close(d(50.0, -3.0), -3.0);
    assert_close(d(97.0, 50.0), 3.0);
    // The box corner is a quarter of the radius, times √2, from the curve's midpoint.
    assert!((d(0.0, 0.0) + 25.0 * 2f64.sqrt() / 4.0).abs() < 1e-9);

    // Everywhere round a corner, against the painted curve sampled finely.
    let outline = sampled_outline(100.0, 100.0, 25.0);
    let mut y = -20.0;
    while y <= 45.0 {
        let mut x = -20.0;
        while x <= 45.0 {
            let got = d(x, y);
            let want = sampled_distance(&outline, (x, y));
            assert!(
                (got.abs() - want).abs() < 1e-3,
                "({x}, {y}): {got} against the sampled {want}"
            );
            if want > 1e-3 {
                assert_eq!(got > 0.0, sampled_inside(&outline, (x, y)), "({x}, {y})");
            }
            x += 1.3;
        }
        y += 1.3;
    }
}

#[test]
fn signed_outline_distance_of_a_sharp_box_a_text_and_a_turned_box() {
    let mut sharp = box_at(0.0, 0.0, 100.0, 60.0);
    sharp.roundness = None;
    assert_close(signed_outline_distance(&sharp, at(0.0, 0.0)), 0.0);
    assert_close(signed_outline_distance(&sharp, at(-3.0, -4.0)), -5.0);
    assert_close(signed_outline_distance(&sharp, at(50.0, 30.0)), 30.0);

    // A line of text has no painted outline to round, whatever its style says.
    let text = text_at(0.0, 0.0, 100.0, 60.0);
    assert_close(signed_outline_distance(&text, at(-3.0, -4.0)), -5.0);

    // Turned about its centre: the distance turns with it.
    let mut turned = box_at(0.0, 0.0, 100.0, 60.0);
    let plain = turned.clone();
    turned.angle = 0.5;
    let (sin, cos) = 0.5f64.sin_cos();
    for (x, y) in [(3.0, 4.0), (-10.0, 20.0), (97.0, 58.0), (120.0, -5.0)] {
        let (dx, dy) = (x - 50.0, y - 30.0);
        let world = at(50.0 + dx * cos - dy * sin, 30.0 + dx * sin + dy * cos);
        assert!(
            (signed_outline_distance(&turned, world) - signed_outline_distance(&plain, at(x, y)))
                .abs()
                < 1e-9
        );
    }
}

#[test]
fn signed_outline_distance_of_an_ellipse_and_a_diamond() {
    // A circle is exact.
    let circle = ellipse_at(0.0, 0.0, 100.0, 100.0);
    assert_close(signed_outline_distance(&circle, at(110.0, 50.0)), -10.0);
    assert_close(signed_outline_distance(&circle, at(50.0, 50.0)), 50.0);
    // An ellipse follows the oracle's three iterations: close, and signed by its equation.
    let ellipse = ellipse_at(0.0, 0.0, 200.0, 100.0);
    assert!((signed_outline_distance(&ellipse, at(210.0, 50.0)) + 10.0).abs() < 0.05);
    assert!((signed_outline_distance(&ellipse, at(100.0, -10.0)) + 10.0).abs() < 0.05);
    assert!(signed_outline_distance(&ellipse, at(100.0, 45.0)) > 0.0);
    // Inside the box, outside the ellipse.
    assert!(signed_outline_distance(&ellipse, at(10.0, 10.0)) < 0.0);

    let diamond = diamond_at(0.0, 0.0, 100.0, 100.0);
    assert_close(signed_outline_distance(&diamond, at(100.0, 50.0)), 0.0);
    assert!(!is_inside(&diamond, at(100.0, 50.0)));
    assert_close(signed_outline_distance(&diamond, at(110.0, 50.0)), -10.0);
    assert_close(
        signed_outline_distance(&diamond, at(50.0, 50.0)),
        50.0 / 2f64.sqrt(),
    );
    assert!(signed_outline_distance(&diamond, at(5.0, 5.0)) < 0.0);
}

#[test]
fn the_reach_follows_the_oracle() {
    assert_close(max_binding_distance(1.0), 15.0);
    assert_close(max_binding_distance(4.0), 15.0);
    assert_close(max_binding_distance(0.4), 25.0);
    assert_close(max_binding_distance(0.1), 30.0);
}
