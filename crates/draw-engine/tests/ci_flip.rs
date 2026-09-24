//! Flip — Shift+H / Shift+V, the context menu and the panel's mirror buttons all run
//! `flip_selection` — for every kind of element.
//!
//! Held to Excalidraw's `actionFlip.ts`, which flips through `resizeMultipleElements`
//! with `flipByX | flipByY` and `shouldResizeFromCenter` (`packages/element/src/
//! resizeElements.ts:1209-1569`). The deliberate divergences — fixed points mirrored for
//! every arrow, labels left out of the arrows-only check, a negative extent standing for
//! an image's `scale` — are written down in `docs/reference/resize.md` › Flip.

mod common;
use common::*;
use draw_engine::scene::binding::{anchor, binding_gap, focus_point, End};
use draw_engine::scene::element::BindMode;
use draw_engine::scene::geometry::{element_outline_bounds, mirror_signs};
use draw_engine::selection::linear::world_points;
use draw_engine::*;
use std::f64::consts::PI;

fn get(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|el| el.id == id)
        .unwrap_or_else(|| panic!("{id} is gone"))
}

fn named(id: &str, mut element: DrawElement) -> DrawElement {
    element.id = id.into();
    element
}

fn flip(engine: &mut DrawEngine, ids: &[&str], axis: FlipAxis) {
    engine.select(ids.iter().map(|id| id.to_string()).collect());
    engine.flip_selection(axis);
}

/// An angle in `[0, 2π)`, so `-0.4` and `2π - 0.4` compare equal.
fn turn(angle: f64) -> f64 {
    angle.rem_euclid(2.0 * PI)
}

fn near(a: Point, b: Point, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

/// `p` reflected about the line `mid` on `axis`.
fn mirrored(p: Point, axis: FlipAxis, mid: f64) -> Point {
    match axis {
        FlipAxis::Horizontal => Point {
            x: 2.0 * mid - p.x,
            y: p.y,
        },
        FlipAxis::Vertical => Point {
            x: p.x,
            y: 2.0 * mid - p.y,
        },
    }
}

/// The middle of what `elements` draw, turned, on `axis` — the oracle's
/// `getCommonBoundingBox` (`actionFlip.ts:131`, `bounds.ts:1005-1029`), with a line
/// measured by its points rather than by its rendered path.
fn midline<'a>(elements: impl IntoIterator<Item = &'a DrawElement>, axis: FlipAxis) -> f64 {
    let (lo, hi) = elements.into_iter().map(element_outline_bounds).fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(lo, hi), b| {
            if axis == FlipAxis::Horizontal {
                (lo.min(b.min_x), hi.max(b.max_x))
            } else {
                (lo.min(b.min_y), hi.max(b.max_y))
            }
        },
    );
    (lo + hi) / 2.0
}

fn bind(arrow: &mut DrawElement, end: End, shape: &str, fixed_point: [f64; 2]) {
    let (id, point, mode) = match end {
        End::Start => (
            &mut arrow.start_binding,
            &mut arrow.start_fixed_point,
            &mut arrow.start_bind_mode,
        ),
        End::End => (
            &mut arrow.end_binding,
            &mut arrow.end_fixed_point,
            &mut arrow.end_bind_mode,
        ),
    };
    *id = Some(shape.into());
    *point = Some(fixed_point);
    *mode = Some(BindMode::Orbit);
}

/// A → B: from A's right side, a quarter down, to B's left side, three quarters down —
/// off-centre on both axes, so a vertical flip that forgot the anchors shows as clearly as
/// a horizontal one. Settled once, so "before" is where the bindings put the arrow.
fn shapes_and_arrow(extra: Vec<DrawElement>) -> DrawEngine {
    let a = named("A", filled(box_at(0.0, 0.0, 100.0, 80.0)));
    let b = named("B", filled(box_at(300.0, 200.0, 100.0, 80.0)));
    let mut arrow = named(
        "arr",
        connector(105.0, 20.0, 295.0, 260.0, DrawElementType::Arrow),
    );
    bind(&mut arrow, End::Start, "A", [1.0, 0.25]);
    bind(&mut arrow, End::End, "B", [0.0, 0.75]);
    let mut scene = vec![a, b, arrow];
    scene.extend(extra);
    let mut engine = engine_with_scene(scene);
    engine.select(vec!["A".into()]);
    engine.nudge_selection(0.0, 0.0);
    engine.clear_selection();
    engine
}

fn ends(element: &DrawElement) -> (Point, Point) {
    let points = world_points(element);
    (points[0], points[points.len() - 1])
}

// ---------------------------------------------------------------------------- arrows

/// Flipped with both its shapes, a bound arrow is the mirror of itself — and stays the
/// mirror once the bindings are re-resolved, then again when a shape is nudged. Before,
/// the anchors were left as they were and the refresh after the flip pulled both ends
/// back onto the old sides: the arrow ran between the *outer* sides of the two shapes.
#[test]
fn flipping_shapes_with_their_arrow_mirrors_the_arrow() {
    for axis in [FlipAxis::Horizontal, FlipAxis::Vertical] {
        let mut engine = shapes_and_arrow(vec![]);
        let scene = engine.get_scene();
        let mid = midline(&scene, axis);
        let (start, end) = ends(&get(&engine, "arr"));

        flip(&mut engine, &["A", "B", "arr"], axis);
        let after = get(&engine, "arr");
        let (s, e) = ends(&after);
        let (want_s, want_e) = (mirrored(start, axis, mid), mirrored(end, axis, mid));
        assert!(
            near(s, want_s, 1.0) && near(e, want_e, 1.0),
            "{axis:?}: {s:?} -> {e:?}, the mirror is {want_s:?} -> {want_e:?}"
        );

        // The anchors are mirrored in each shape's own frame.
        let (start_fp, end_fp) = match axis {
            FlipAxis::Horizontal => ([0.0, 0.25], [1.0, 0.75]),
            FlipAxis::Vertical => ([1.0, 0.75], [0.0, 0.25]),
        };
        let a = anchor(&after, End::Start).expect("start still bound");
        let b = anchor(&after, End::End).expect("end still bound");
        assert_eq!((a.element_id.as_str(), a.fixed_point), ("A", start_fp));
        assert_eq!((b.element_id.as_str(), b.fixed_point), ("B", end_fp));

        // A later move of a bound shape re-resolves from the anchors, and finds the
        // arrow where the flip put it. The oracle's non-elbow arrows jump here.
        engine.select(vec!["A".into()]);
        engine.nudge_selection(0.0, 0.0);
        let (s, e) = ends(&get(&engine, "arr"));
        assert!(
            near(s, want_s, 1.0) && near(e, want_e, 1.0),
            "{axis:?}: a nudge of A moved the arrow to {s:?} -> {e:?}"
        );
    }
}

/// Only bound arrows selected: the heads swap and nothing moves (`actionFlip.ts:116-129`).
/// Before, the points were mirrored and the refresh put them straight back — the flip
/// changed nothing visible, yet was saved as an edit.
#[test]
fn flipping_only_bound_arrows_swaps_their_heads() {
    // Bound at both ends.
    let mut engine = shapes_and_arrow(vec![]);
    let before = get(&engine, "arr");
    flip(&mut engine, &["arr"], FlipAxis::Horizontal);
    let after = get(&engine, "arr");
    assert_eq!(world_points(&after), world_points(&before), "no geometry");
    assert_eq!(
        (
            default_arrowhead(&after, "start"),
            default_arrowhead(&after, "end")
        ),
        (Arrowhead::Arrow, Arrowhead::None),
        "the resolved heads swap, so the implicit end head moves to the start"
    );
    assert_eq!(anchor(&after, End::Start), anchor(&before, End::Start));
    assert_eq!(anchor(&after, End::End), anchor(&before, End::End));
    assert!(after.version > before.version, "the swap is an edit");

    // Bound at one end only: the same swap, and the free end is not bent.
    let a = named("A", filled(box_at(0.0, 0.0, 100.0, 80.0)));
    let mut arrow = named(
        "one",
        connector(105.0, 40.0, 305.0, 240.0, DrawElementType::Arrow),
    );
    bind(&mut arrow, End::Start, "A", [1.0, 0.5]);
    arrow.start_arrowhead = Some(Arrowhead::Dot);
    let mut engine = engine_with_scene(vec![a, arrow]);
    engine.select(vec!["A".into()]);
    engine.nudge_selection(0.0, 0.0);
    let before = get(&engine, "one");
    flip(&mut engine, &["one"], FlipAxis::Vertical);
    let after = get(&engine, "one");
    assert_eq!(world_points(&after), world_points(&before), "no geometry");
    assert_eq!(
        (after.start_arrowhead, after.end_arrowhead),
        (Some(Arrowhead::Arrow), Some(Arrowhead::Dot))
    );
}

/// A label does not count against the arrows-only rule: an arrow with words on it is still
/// only an arrow. Excalidraw counts the label (`actionFlip.ts:87-94`), so there the same
/// arrow is mirrored and let go of both shapes instead — a divergence, written down.
#[test]
fn a_labelled_bound_arrow_alone_swaps_its_heads() {
    let mut label = named("lbl", text_at(0.0, 0.0, 40.0, 25.0));
    label.text = Some("hi".into());
    label.container_id = Some("arr".into());
    let mut engine = shapes_and_arrow(vec![label]);
    let mut arrow = get(&engine, "arr");
    arrow.bound_text_id = Some("lbl".into());
    let mut scene = engine.get_scene();
    scene.retain(|el| el.id != "arr");
    scene.push(arrow);
    engine.set_scene(Scene::new(scene));
    let before = get(&engine, "arr");

    flip(&mut engine, &["arr", "lbl"], FlipAxis::Horizontal);
    let after = get(&engine, "arr");
    assert_eq!(world_points(&after), world_points(&before), "no geometry");
    assert_eq!(default_arrowhead(&after, "start"), Arrowhead::Arrow);
    assert!(after.start_binding.is_some() && after.end_binding.is_some());
}

/// Flipped with something that is not its shapes, an arrow is mirrored and lets go of the
/// shapes that stayed (`resizeElements.ts:1558-1569`). Before, it stayed bound, and the
/// refresh snapped it back to where it had been.
#[test]
fn flipping_an_arrow_without_its_shapes_unbinds_it() {
    let c = named("C", filled(box_at(600.0, 0.0, 50.0, 50.0)));
    let mut engine = shapes_and_arrow(vec![c]);
    let arrow = get(&engine, "arr");
    let mid = midline([&arrow, &get(&engine, "C")], FlipAxis::Horizontal);
    let before = world_points(&arrow);

    flip(&mut engine, &["arr", "C"], FlipAxis::Horizontal);
    let after = get(&engine, "arr");
    for (p, q) in before.iter().zip(world_points(&after)) {
        assert!(
            near(q, mirrored(*p, FlipAxis::Horizontal, mid), 1e-9),
            "{p:?} -> {q:?}"
        );
    }
    assert_eq!(
        (
            &after.start_binding,
            after.start_fixed_point,
            after.start_bind_mode
        ),
        (&None, None, None),
        "the start let go of A"
    );
    assert_eq!(
        (
            &after.end_binding,
            after.end_fixed_point,
            after.end_bind_mode
        ),
        (&None, None, None),
        "the end let go of B"
    );

    // Let go for good: moving A no longer drags it anywhere.
    let flipped = world_points(&after);
    engine.select(vec!["A".into()]);
    engine.nudge_selection(10.0, 0.0);
    assert_eq!(world_points(&get(&engine, "arr")), flipped);
}

/// An arrow that was not flipped but is bound to a shape that was keeps its binding and
/// follows the shape to where the flip put it, as the oracle's `updateBoundElements`
/// does. The shape is what the flip committed; the arrow is only bound to it — so this
/// holds whether the refresh after a commit walks the whole board or only what the
/// commit touched and the arrows bound to it.
#[test]
fn an_arrow_left_out_follows_its_flipped_shape() {
    let c = named("C", filled(box_at(700.0, 200.0, 100.0, 80.0)));
    let mut engine = shapes_and_arrow(vec![c]);
    let before = get(&engine, "arr");

    flip(&mut engine, &["B", "C"], FlipAxis::Horizontal);
    let b = get(&engine, "B");
    assert_eq!(
        normalize_rect(b.x, b.y, b.width, b.height).x,
        700.0,
        "setup: B swapped places with C"
    );
    let after = get(&engine, "arr");
    assert_eq!(anchor(&after, End::Start), anchor(&before, End::Start));
    assert_eq!(anchor(&after, End::End), anchor(&before, End::End));
    // On B's left side, where it is now: a gap clear of x = 700, and level with it.
    let (_, end) = ends(&after);
    let side = focus_point(&b, [0.0, 0.75]).x;
    assert!(
        end.x < side && side - end.x <= binding_gap(&b) + 1e-6,
        "the end is at {end:?}, B's left side at x = {side}"
    );
    assert!((200.0..=280.0).contains(&end.y), "{end:?}");
}

// ------------------------------------------------------------- what the flip carries

/// A frame flips with what it holds (`getSelectedElements(…, includeElementsInFrames)`,
/// `actionFlip.ts:87-94`), and nobody changes frame. Before, only the outline mirrored.
#[test]
fn a_frame_flips_with_its_children() {
    let frame = named(
        "f",
        create_element_default(
            DrawElementType::Frame,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 200.0,
            },
        ),
    );
    let mut child = named("c", filled(box_at(30.0, 30.0, 40.0, 40.0)));
    child.frame_id = Some("f".into());
    let mut engine = engine_with_scene(vec![frame, child]);

    flip(&mut engine, &["f"], FlipAxis::Horizontal);
    let child = get(&engine, "c");
    let frame = get(&engine, "f");
    assert_eq!((child.x, child.width), (230.0, 40.0));
    assert_eq!(child.frame_id.as_deref(), Some("f"));
    assert_eq!(
        (frame.x, frame.width),
        (0.0, 300.0),
        "the frame mirrors onto itself"
    );
}

/// A group is one thing, locked members included (`groups.ts:94-132` has no lock
/// filter). Before, the locked member stayed put while the rest of its group mirrored.
#[test]
fn a_group_flips_with_its_locked_member() {
    let mut a = named("a", filled(box_at(0.0, 0.0, 50.0, 50.0)));
    let mut b = named("b", filled(box_at(200.0, 0.0, 50.0, 50.0)));
    a.group_ids = vec!["g".into()];
    b.group_ids = vec!["g".into()];
    b.locked = Some(true);
    let mut engine = engine_with_scene(vec![a, b]);

    flip(&mut engine, &["a", "b"], FlipAxis::Horizontal);
    assert_eq!(get(&engine, "a").x, 200.0);
    assert_eq!(get(&engine, "b").x, 0.0);
}

// ------------------------------------------------------------------ boxes and text

/// Text turns the other way like everything else (`resizeElements.ts:1417-1419` has no
/// text exception; excalidraw.com: 0.3 → 5.9832). Its glyphs are never mirrored.
#[test]
fn turned_text_turns_the_other_way() {
    let mut text = named("t", text_at(0.0, 0.0, 80.0, 25.0));
    text.text = Some("hello".into());
    text.angle = 0.4;
    let other = named("o", box_at(300.0, 0.0, 100.0, 60.0));
    let mid = midline([&text, &other], FlipAxis::Horizontal);
    let mut engine = engine_with_scene(vec![text, other]);

    flip(&mut engine, &["t", "o"], FlipAxis::Horizontal);
    let t = get(&engine, "t");
    assert!(
        (turn(t.angle) - turn(-0.4)).abs() < 1e-12,
        "angle {}",
        t.angle
    );
    assert_eq!(t.width, 80.0, "glyphs are never mirrored");
    assert!((t.x - (2.0 * mid - 80.0)).abs() < 1e-9, "x {}", t.x);
}

/// The mirror line is the middle of the *turned* shapes, as the oracle measures the
/// selection. A 200×20 bar stood on end occupies x 90..110; with a box at 300..350 the line
/// is 220 and the box lands at 90. The unturned box put the line at 175 and the box at 0.
#[test]
fn the_axis_is_the_turned_selection_box() {
    let mut bar = named("bar", filled(box_at(0.0, 0.0, 200.0, 20.0)));
    bar.angle = PI / 2.0;
    let other = named("o", filled(box_at(300.0, 0.0, 50.0, 50.0)));
    let mut engine = engine_with_scene(vec![bar, other]);

    flip(&mut engine, &["bar", "o"], FlipAxis::Horizontal);
    let o = get(&engine, "o");
    assert!((o.x - 90.0).abs() < 1e-9, "x {}", o.x);
}

/// Each kind reaches as far as what it draws, not as far as its box, once turned — the
/// oracle's `getElementBounds` (`packages/element/src/bounds.ts:147-240` at 1118751f): an
/// ellipse by its own curve, a diamond by its four corners, a line or a freehand stroke by
/// its turned points. Each is turned by π/4 beside a box at x 300..350, so the box lands on
/// the kind's left edge. The turned box put all four further left, the line and the stroke
/// 70 units off: both of theirs stand on end at x = 50.
#[test]
fn the_axis_is_what_each_kind_draws() {
    let (sin, cos) = (PI / 4.0).sin_cos();
    let mut stroke = create_element_default(
        DrawElementType::Freedraw,
        Geometry {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    );
    stroke.points = Some(vec![[0.0, 0.0], [50.0, 50.0], [100.0, 100.0]]);
    let cases = [
        // `cx - hypot(w/2 · cos, h/2 · sin)`, bounds.ts:202-209.
        (
            "ellipse",
            ellipse_at(0.0, 0.0, 200.0, 20.0),
            100.0 - (100.0 * cos).hypot(10.0 * sin),
        ),
        // The corners on the long axis reach furthest, bounds.ts:176-201.
        (
            "diamond",
            diamond_at(0.0, 0.0, 200.0, 20.0),
            100.0 - 100.0 * cos,
        ),
        // The turned points, bounds.ts:934-995.
        (
            "line",
            connector(0.0, 0.0, 100.0, 100.0, DrawElementType::Line),
            50.0,
        ),
        // The turned points, bounds.ts:157-173.
        ("freehand", stroke, 50.0),
    ];
    for (kind, mut shape, left) in cases {
        shape.id = "s".into();
        shape.angle = PI / 4.0;
        let other = named("o", filled(box_at(300.0, 0.0, 50.0, 50.0)));
        let mut engine = engine_with_scene(vec![shape, other]);

        flip(&mut engine, &["s", "o"], FlipAxis::Horizontal);
        let o = get(&engine, "o");
        assert!((o.x - left).abs() < 1e-9, "{kind}: x {}, want {left}", o.x);
    }
}

/// The words on an arrow count toward the box, as the oracle adds an arrow's label to it
/// (`resizeElements.ts:1280-1310`): a label wider than its arrow widens the selection.
#[test]
fn an_arrows_words_count_toward_the_axis() {
    let mut arrow = named(
        "arr",
        connector(0.0, 100.0, 100.0, 100.0, DrawElementType::Arrow),
    );
    arrow.bound_text_id = Some("words".into());
    let mut words = named("words", text_at(0.0, 0.0, 200.0, 25.0));
    words.text = Some("a long label".into());
    words.container_id = Some("arr".into());
    let words = layout_label(words, &arrow);
    assert_eq!(words.x, -50.0, "setup: the label reaches past the arrow");
    let other = named("o", filled(box_at(300.0, 0.0, 50.0, 50.0)));
    let mut engine = engine_with_scene(vec![arrow, words, other]);

    flip(&mut engine, &["arr", "o"], FlipAxis::Horizontal);
    // The line is the middle of -50..350, not of 0..350.
    assert_eq!(get(&engine, "o").x, -50.0);
}

fn frame_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    create_element_default(
        DrawElementType::Frame,
        Geometry {
            x,
            y,
            width,
            height,
        },
    )
}

fn embed_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    let mut embed = create_element_default(
        DrawElementType::Embed,
        Geometry {
            x,
            y,
            width,
            height,
        },
    );
    embed.embed_url = Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".into());
    embed
}

fn image_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
    let mut image = create_element_default(
        DrawElementType::Image,
        Geometry {
            x,
            y,
            width,
            height,
        },
    );
    image.data_url = Some("data:image/png;base64,iVBORw0KGgo=".into());
    image
}

/// Every box kind but a picture keeps a positive width and height, as the oracle's do
/// (`resizeElements.ts:1409-1444`): a flip reflects where it is and turns it the other
/// way, so a rectangle's hand-drawn stroke and hatching look the same as before, as they
/// do on excalidraw.com. Before, the extent went negative and the painter mirrored them.
#[test]
fn box_kinds_keep_a_positive_extent() {
    type Make = fn(f64, f64, f64, f64) -> DrawElement;
    let kinds: [(&str, Make); 6] = [
        ("rectangle", box_at),
        ("ellipse", ellipse_at),
        ("diamond", diamond_at),
        ("text", text_at),
        ("frame", frame_at),
        ("embed", embed_at),
    ];
    for (kind, make) in kinds {
        for axis in [FlipAxis::Horizontal, FlipAxis::Vertical] {
            let mut shape = named("s", make(0.0, 0.0, 100.0, 60.0));
            shape.angle = 0.3;
            let other = named("o", box_at(300.0, 200.0, 50.0, 50.0));
            let mut engine = engine_with_scene(vec![shape.clone(), other.clone()]);
            let mid = midline([&shape, &other], axis);
            let centre = mirrored(rotation_center(&shape), axis, mid);

            flip(&mut engine, &["s", "o"], axis);
            let s = get(&engine, "s");
            assert_eq!((s.width, s.height), (100.0, 60.0), "{kind} {axis:?}");
            assert!(
                near(rotation_center(&s), centre, 1e-9),
                "{kind} {axis:?}: centre {:?}, the mirror is {centre:?}",
                rotation_center(&s)
            );
            assert!(
                (turn(s.angle) - turn(-0.3)).abs() < 1e-12,
                "{kind} {axis:?}"
            );
        }
    }
}

/// A picture is the one box whose content mirrors: a negative extent does the job of the
/// oracle's `scale` (`resizeElements.ts:1484-1489`, painted by `renderElement.ts:824-841`)
/// on the canvas and in the export. An embed's page never mirrors — Excalidraw turns its
/// iframe and nothing else (`App.tsx:2046-2051`) — so its frame keeps a positive box.
#[test]
fn an_image_mirrors_its_pixels_and_an_embed_does_not() {
    let mut engine = engine_with_scene(vec![named("img", image_at(0.0, 0.0, 200.0, 100.0))]);
    flip(&mut engine, &["img"], FlipAxis::Horizontal);
    let image = get(&engine, "img");
    assert_eq!(mirror_signs(&image), (-1.0, 1.0));
    let scene = engine.get_scene();
    let svg = scene_to_svg(&scene, scene_bounds(&scene).unwrap(), 10.0, "#ffffff");
    assert!(svg.contains("scale(-1 1)"), "{svg}");
    flip(&mut engine, &["img"], FlipAxis::Vertical);
    assert_eq!(mirror_signs(&get(&engine, "img")), (-1.0, -1.0));

    let mut embed = named("e", embed_at(100.0, 100.0, 400.0, 225.0));
    embed.angle = 0.3;
    let mut engine = engine_with_scene(vec![embed]);
    let before = engine.embed_frames();
    flip(&mut engine, &["e"], FlipAxis::Horizontal);
    let after = engine.embed_frames();
    assert_eq!(get(&engine, "e").width, 400.0, "stored unmirrored");
    assert!((after[0].x - before[0].x).abs() < 1e-9);
    assert!((after[0].width - before[0].width).abs() < 1e-9);
    assert!((turn(after[0].angle) - turn(-0.3)).abs() < 1e-12);
}

// --------------------------------------------------------------------- round trips

/// Everything a flip may touch, and nothing it may not.
#[derive(Debug, PartialEq)]
struct Pose {
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    angle: f64,
    points: Option<Vec<[f64; 2]>>,
    start: Option<draw_engine::scene::binding::Anchor>,
    end: Option<draw_engine::scene::binding::Anchor>,
    heads: (Arrowhead, Arrowhead),
    frame_id: Option<String>,
    group_ids: Vec<String>,
}

fn poses(engine: &DrawEngine) -> Vec<Pose> {
    let mut all: Vec<Pose> = engine
        .get_scene()
        .into_iter()
        .map(|el| Pose {
            x: el.x,
            y: el.y,
            width: el.width,
            height: el.height,
            angle: el.angle,
            start: anchor(&el, End::Start),
            end: anchor(&el, End::End),
            heads: (
                default_arrowhead(&el, "start"),
                default_arrowhead(&el, "end"),
            ),
            points: el.points,
            frame_id: el.frame_id,
            group_ids: el.group_ids,
            id: el.id,
        })
        .collect();
    all.sort_by(|a, b| a.id.cmp(&b.id));
    all
}

/// One board holding every kind, far enough apart not to meet.
fn every_kind() -> DrawEngine {
    let mut turned = named("turned", filled(box_at(0.0, 600.0, 120.0, 40.0)));
    turned.angle = 0.5;
    let mut text = named("text", text_at(200.0, 600.0, 80.0, 25.0));
    text.text = Some("hello".into());
    text.angle = 0.25;
    let mut labelled = named("labelled", filled(box_at(400.0, 600.0, 160.0, 80.0)));
    labelled.angle = 0.2;
    labelled.bound_text_id = Some("label".into());
    let mut label = named("label", text_at(0.0, 0.0, 60.0, 25.0));
    label.text = Some("hi".into());
    label.container_id = Some("labelled".into());
    // Laid out here rather than by whichever commit next refreshes the board, so the
    // original is where the label belongs however much of the board a commit refreshes.
    let label = layout_label(label, &labelled);

    let mut line = named(
        "line",
        connector(0.0, 900.0, 200.0, 1000.0, DrawElementType::Line),
    );
    line.angle = 0.3;
    let mut curve = named(
        "curve",
        connector(300.0, 900.0, 500.0, 950.0, DrawElementType::Arrow),
    );
    curve.points = Some(vec![[0.0, 0.0], [100.0, -60.0], [200.0, 50.0]]);
    curve.roundness = Some(1.0);
    let mut multi = named(
        "multi",
        connector(700.0, 900.0, 580.0, 980.0, DrawElementType::Arrow),
    );
    multi.points = Some(vec![[0.0, 0.0], [-60.0, 50.0], [-120.0, 80.0]]);
    multi.start_arrowhead = Some(Arrowhead::Triangle);

    let mut stroke = named(
        "stroke",
        create_element_default(
            DrawElementType::Freedraw,
            Geometry {
                x: 0.0,
                y: 1200.0,
                width: 100.0,
                height: 50.0,
            },
        ),
    );
    stroke.points = Some(vec![[0.0, 0.0], [30.0, 50.0], [100.0, 10.0]]);
    // Excalidraw's shape for a stroke: the origin on the first point, the rest anywhere.
    let mut sketch = named(
        "sketch",
        create_element_default(
            DrawElementType::Freedraw,
            Geometry {
                x: 300.0,
                y: 1200.0,
                width: 80.0,
                height: 40.0,
            },
        ),
    );
    sketch.points = Some(vec![[0.0, 0.0], [-40.0, 20.0], [-80.0, 40.0]]);
    let mut scrawl = named(
        "scrawl",
        create_element_default(
            DrawElementType::Freedraw,
            Geometry {
                x: 600.0,
                y: 1200.0,
                width: 100.0,
                height: 50.0,
            },
        ),
    );
    scrawl.points = Some(vec![[0.0, 0.0], [30.0, 50.0], [100.0, 10.0]]);
    scrawl.angle = 0.6;
    let mut oval = named("oval", filled(ellipse_at(600.0, 300.0, 120.0, 50.0)));
    oval.angle = 0.7;
    let mut kite = named("kite", filled(diamond_at(800.0, 300.0, 100.0, 60.0)));
    kite.angle = 0.4;

    let mut image = named("image", image_at(0.0, 1500.0, 200.0, 100.0));
    image.angle = 0.4;
    let mut embed = named("embed", embed_at(300.0, 1500.0, 200.0, 112.0));
    embed.angle = 0.1;
    let frame = named("frame", frame_at(0.0, 1800.0, 300.0, 200.0));
    let mut child = named("child", filled(ellipse_at(30.0, 1830.0, 40.0, 40.0)));
    child.frame_id = Some("frame".into());
    let mut g1 = named("g1", filled(box_at(400.0, 1800.0, 50.0, 50.0)));
    let mut g2 = named("g2", filled(diamond_at(600.0, 1850.0, 50.0, 60.0)));
    g1.group_ids = vec!["g".into()];
    g2.group_ids = vec!["g".into()];
    g2.locked = Some(true);

    shapes_and_arrow(vec![
        named("far", filled(box_at(1000.0, 0.0, 50.0, 50.0))),
        named("rect", filled(box_at(0.0, 300.0, 100.0, 60.0))),
        named("ellipse", filled(ellipse_at(200.0, 300.0, 100.0, 60.0))),
        named("diamond", filled(diamond_at(400.0, 300.0, 100.0, 60.0))),
        oval,
        kite,
        turned,
        text,
        labelled,
        label,
        line,
        curve,
        multi,
        stroke,
        sketch,
        scrawl,
        image,
        embed,
        frame,
        child,
        g1,
        g2,
    ])
}

/// A flip is its own inverse: twice on the same axis lands on the original numbers
/// exactly, for every kind — and once changes something, and one undo takes it back.
#[test]
fn every_kind_round_trips() {
    let cases: [&[&str]; 19] = [
        &["rect", "far"],
        &["ellipse", "far"],
        &["diamond", "far"],
        &["oval", "far"],
        &["kite", "far"],
        &["turned"],
        &["text", "far"],
        &["labelled", "far"],
        &["line"],
        &["curve"],
        &["multi", "far"],
        &["stroke", "far"],
        &["sketch"],
        &["scrawl", "far"],
        &["image"],
        &["embed", "far"],
        &["frame"],
        &["g1", "g2"],
        &["A", "B", "arr"],
    ];
    let arrow_alone: &[&str] = &["arr"];
    for ids in cases.into_iter().chain([arrow_alone]) {
        for axis in [FlipAxis::Horizontal, FlipAxis::Vertical] {
            let mut engine = every_kind();
            let original = poses(&engine);

            flip(&mut engine, ids, axis);
            let once = poses(&engine);
            assert_ne!(once, original, "{ids:?} {axis:?}: one flip changed nothing");

            engine.undo();
            assert_eq!(poses(&engine), original, "{ids:?} {axis:?}: undo");

            flip(&mut engine, ids, axis);
            flip(&mut engine, ids, axis);
            let twice = poses(&engine);
            // Two cases are exact to the last bits of a float but not beyond. A bound
            // arrow's ends are not reflected, they are re-resolved from its mirrored
            // anchors after each flip — orbit geometry. And a turned ellipse, diamond or
            // stroke beside something else sets the line through a sine and a cosine of
            // its own, which the mirror image rounds differently.
            let rounded = (ids.contains(&"arr") && ids.len() > 1)
                || ["oval", "kite", "scrawl"]
                    .iter()
                    .any(|turned| ids.contains(turned));
            if rounded {
                assert_eq!(
                    to_nanos(twice),
                    to_nanos(original),
                    "{ids:?} {axis:?}: twice"
                );
            } else {
                assert_eq!(twice, original, "{ids:?} {axis:?}: twice");
            }
        }
    }
}

/// Every coordinate held to a billionth of a unit.
fn to_nanos(poses: Vec<Pose>) -> Vec<Pose> {
    let nano = |v: f64| (v * 1e9).round() / 1e9;
    poses
        .into_iter()
        .map(|p| Pose {
            x: nano(p.x),
            y: nano(p.y),
            width: nano(p.width),
            height: nano(p.height),
            angle: nano(p.angle),
            points: p
                .points
                .map(|points| points.iter().map(|q| q.map(nano)).collect()),
            ..p
        })
        .collect()
}
