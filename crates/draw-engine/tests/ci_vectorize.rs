//! Vectorizing an image: a trace, put where the image is, in one undoable step.
//!
//! The trace arrives from draw-trace in the traced image's own pixels. Everything after
//! that is here: where those pixels are on the board — the image's box, turned and
//! mirrored as the painter draws it — what the elements look like, which group and frame
//! they join, and what is refused. See `engine/vectorize.rs` and
//! `docs/reference/vectorize.md`.

mod common;
use common::*;
use draw_engine::engine::vectorize::{
    TraceRings, VectorizeLimits, VectorizeOptions, VectorizeRefusal,
};
use draw_engine::*;

const LIMITS: VectorizeLimits = VectorizeLimits {
    max_elements: 20_000,
    max_points_per_element: 10_000,
    max_trace_shapes: 5_000,
    max_data_url_length: 6 * 1024 * 1024,
    max_group_depth: 32,
};

fn options(keep_original: bool) -> VectorizeOptions {
    VectorizeOptions {
        keep_original,
        limits: LIMITS,
    }
}

const SVG: &str = "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=";

fn picture_at(x: f64, y: f64, width: f64, height: f64) -> DrawElement {
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
    image.roughness = 0.0;
    image.roundness = None;
    image.opacity = 70.0;
    image
}

/// Rings given in the pixels of a `width` × `height` picture, as draw-trace hands them
/// over: flat, in fractions of the picture.
fn rings(width: f64, height: f64, rings: &[(u32, &[f64])]) -> TraceRings {
    TraceRings {
        colours: rings.iter().map(|(rgb, _)| *rgb).collect(),
        lengths: rings
            .iter()
            .map(|(_, ring)| (ring.len() / 2) as u32)
            .collect(),
        coords: rings
            .iter()
            .flat_map(|(_, ring)| ring.as_chunks::<2>().0.iter())
            .flat_map(|[x, y]| [x / width, y / height])
            .collect(),
    }
}

/// A 20 × 10 picture's trace: a ring over all of it, and a second colour with two rings.
fn trace() -> TraceRings {
    rings(
        20.0,
        10.0,
        &[
            (
                0xc83c3c,
                &[0.0, 0.0, 20.0, 0.0, 20.0, 10.0, 0.0, 10.0, 0.0, 0.0],
            ),
            (
                0x285ac8,
                &[5.0, 2.0, 10.0, 2.0, 10.0, 6.0, 5.0, 6.0, 5.0, 2.0],
            ),
            (
                0x285ac8,
                &[12.0, 2.0, 15.0, 2.0, 15.0, 4.0, 12.0, 4.0, 12.0, 2.0],
            ),
        ],
    )
}

fn live(engine: &DrawEngine) -> Vec<DrawElement> {
    engine
        .get_scene()
        .into_iter()
        .filter(|element| !element.is_deleted)
        .collect()
}

fn element(engine: &DrawEngine, id: &str) -> DrawElement {
    engine
        .get_scene()
        .into_iter()
        .find(|element| element.id == id)
        .expect("in the scene")
}

fn world_points(line: &DrawElement) -> Vec<Point> {
    line.points
        .as_deref()
        .expect("a line has points")
        .iter()
        .map(|p| Point {
            x: line.x + p[0],
            y: line.y + p[1],
        })
        .collect()
}

fn assert_near_point(a: Point, b: Point) {
    assert!(
        (a.x - b.x).abs() < 0.011 && (a.y - b.y).abs() < 0.011,
        "{a:?} is not {b:?}"
    );
}

/// The four corners a full-picture ring lands on, against the image's own four.
fn assert_same_corners(ring: &[Point], image: &DrawElement) {
    for corner in selection_corners(image) {
        assert!(
            ring.iter()
                .any(|p| (p.x - corner.x).abs() < 0.011 && (p.y - corner.y).abs() < 0.011),
            "no traced corner at {corner:?}: {ring:?}"
        );
    }
}

// ------------------------------------------------------------------- placement

#[test]
fn the_trace_lands_on_the_image_s_box() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);

    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(false))
        .expect("inserted");
    assert_eq!(ids.len(), 3, "one element per ring");

    let full = element(&engine, &ids[0]);
    let points = world_points(&full);
    for (got, want) in points.iter().zip([
        (100.0, 50.0),
        (300.0, 50.0),
        (300.0, 150.0),
        (100.0, 150.0),
        (100.0, 50.0),
    ]) {
        assert_near_point(
            *got,
            Point {
                x: want.0,
                y: want.1,
            },
        );
    }
    // Scaled on both axes: trace pixel (12, 2) is 10 units a pixel across, 10 down.
    assert_near_point(
        world_points(&element(&engine, &ids[2]))[0],
        Point { x: 220.0, y: 70.0 },
    );
}

#[test]
fn a_turned_image_s_trace_turns_with_it() {
    let mut image = picture_at(100.0, 50.0, 200.0, 100.0);
    image.angle = std::f64::consts::FRAC_PI_3;
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image.clone()]);

    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(false))
        .unwrap();
    let ring = world_points(&element(&engine, &ids[0]));
    assert_same_corners(&ring, &image);

    // And the right way round: the picture's top-left is the box's top-left, turned.
    let turned = rotate_point(-100.0, -50.0, image.angle);
    assert_near_point(
        ring[0],
        Point {
            x: 200.0 + turned.x,
            y: 100.0 + turned.y,
        },
    );
    let line = element(&engine, &ids[0]);
    assert_eq!(line.angle, 0.0, "the turn is in the points");
}

#[test]
fn a_flipped_image_s_trace_is_mirrored_with_it() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    engine.select(vec![image_id.clone()]);
    engine.flip_selection(FlipAxis::Horizontal);
    let flipped = element(&engine, &image_id);
    assert!(flipped.width < 0.0, "a flip is a negative extent");

    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(true))
        .unwrap();
    let ring = world_points(&element(&engine, &ids[0]));
    // The picture's left edge is drawn on the box's right now, and its trace with it.
    assert_near_point(ring[0], Point { x: 300.0, y: 50.0 });
    assert_near_point(ring[1], Point { x: 100.0, y: 50.0 });
    // Trace pixel (12, 2), right of centre in the picture, is left of centre on the board.
    assert_near_point(
        world_points(&element(&engine, &ids[2]))[0],
        Point { x: 180.0, y: 70.0 },
    );
}

#[test]
fn a_flipped_and_turned_image_keeps_its_corners() {
    let mut image = picture_at(-40.0, 10.0, 120.0, 90.0);
    image.height = -90.0;
    image.y = 100.0;
    image.angle = 2.2;
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image.clone()]);
    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(false))
        .unwrap();
    let ring = world_points(&element(&engine, &ids[0]));
    assert_same_corners(&ring, &image);

    // Where the painter draws the picture's top-left: T(centre) · R · S(mirror) · (−half).
    let (w, h) = (image.width.abs(), image.height.abs());
    let centre = Point {
        x: image.x + w / 2.0,
        y: image.y - h / 2.0,
    };
    let drawn = rotate_point(-w / 2.0, h / 2.0, image.angle);
    assert_near_point(
        ring[0],
        Point {
            x: centre.x + drawn.x,
            y: centre.y + drawn.y,
        },
    );
}

// --------------------------------------------------------------- what is made

#[test]
fn every_ring_is_a_closed_filled_line_that_paints_like_the_trace() {
    let image = picture_at(0.0, 0.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(false))
        .unwrap();

    for (id, colour) in ids.iter().zip(["#c83c3c", "#285ac8", "#285ac8"]) {
        let line = element(&engine, id);
        assert_eq!(line.kind, DrawElementType::Line);
        assert!(
            is_path_a_loop_within(line.points.as_deref().unwrap(), 0.0),
            "closed"
        );
        assert_eq!(
            line.points.as_deref().unwrap()[0],
            [0.0, 0.0],
            "anchored on its first point"
        );
        assert_eq!(line.background_color, colour);
        assert_eq!(line.fill_style, FillStyle::Solid);
        assert!(
            is_transparent(&line.stroke_color),
            "no stroke to widen the region"
        );
        assert_eq!(line.roughness, 0.0);
        assert_eq!(line.roundness, None);
        assert_eq!(line.opacity, 70.0, "as see-through as the image was");
        let span = local_box(&line);
        assert!((line.width - span.width).abs() < 1e-9 && (line.height - span.height).abs() < 1e-9);
    }
}

#[test]
fn a_ring_over_the_point_cap_is_simplified_to_fit() {
    let image = picture_at(0.0, 0.0, 400.0, 400.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    let mut ring: Vec<f64> = (0..12_000)
        .flat_map(|i| {
            let a = i as f64 / 12_000.0 * std::f64::consts::TAU;
            [200.0 + 150.0 * a.cos(), 200.0 + 150.0 * a.sin()]
        })
        .collect();
    ring.extend_from_slice(&[350.0, 200.0]);
    let big = rings(400.0, 400.0, &[(0x000000, &ring)]);
    let ids = engine
        .vectorize_to_shapes(&image_id, &big, options(false))
        .unwrap();
    let line = element(&engine, &ids[0]);
    let points = line.points.as_deref().unwrap();
    assert!(points.len() <= 10_000, "{} points", points.len());
    assert!(
        points.len() >= 32,
        "still a circle: {} points",
        points.len()
    );
    assert!(is_path_a_loop_within(points, 0.0));
    for p in world_points(&line) {
        let radius = (p.x - 200.0).hypot(p.y - 200.0);
        assert!((radius - 150.0).abs() < 0.02, "{p:?} is off the circle");
    }
}

#[test]
fn the_trace_sits_directly_above_the_image_grouped_and_selected() {
    let below = box_at(0.0, 0.0, 50.0, 50.0);
    let image = picture_at(100.0, 100.0, 200.0, 100.0);
    let above = box_at(0.0, 300.0, 50.0, 50.0);
    let (below_id, image_id, above_id) = (below.id.clone(), image.id.clone(), above.id.clone());
    let mut engine = engine_with_scene(vec![below, image, above]);

    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(true))
        .unwrap();
    let order: Vec<String> = live(&engine).into_iter().map(|e| e.id).collect();
    let mut expected = vec![below_id, image_id];
    expected.extend(ids.iter().cloned());
    expected.push(above_id);
    assert_eq!(order, expected);

    let groups: Vec<Vec<String>> = ids
        .iter()
        .map(|id| element(&engine, id).group_ids)
        .collect();
    assert_eq!(groups[0].len(), 1, "one new group");
    assert!(
        groups.iter().all(|g| *g == groups[0]),
        "all in the same group"
    );

    let mut selected = engine.get_selection();
    selected.sort();
    let mut want = ids.clone();
    want.sort();
    assert_eq!(selected, want);
}

#[test]
fn the_trace_joins_the_image_s_groups_and_frame() {
    let frame = {
        let mut frame = create_element_default(
            DrawElementType::Frame,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 600.0,
                height: 400.0,
            },
        );
        frame.name = Some("Frame 1".into());
        frame
    };
    let mut image = picture_at(100.0, 100.0, 200.0, 100.0);
    image.frame_id = Some(frame.id.clone());
    image.group_ids = vec!["inner".into(), "outer".into()];
    let mut sibling = box_at(350.0, 100.0, 50.0, 50.0);
    sibling.group_ids = vec!["inner".into(), "outer".into()];
    let (frame_id, image_id) = (frame.id.clone(), image.id.clone());
    let mut engine = engine_with_scene(vec![frame, image, sibling]);

    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(false))
        .unwrap();
    for id in &ids {
        let line = element(&engine, id);
        assert_eq!(line.frame_id.as_deref(), Some(frame_id.as_str()));
        assert_eq!(
            line.group_ids.len(),
            3,
            "its own group, inside the image's two"
        );
        assert_eq!(
            &line.group_ids[1..],
            ["inner".to_string(), "outer".to_string()]
        );
    }
}

#[test]
fn an_image_as_deep_in_groups_as_they_go_adds_no_group() {
    let mut image = picture_at(100.0, 100.0, 200.0, 100.0);
    image.group_ids = (0..32).map(|depth| format!("g{depth}")).collect();
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image.clone()]);
    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(false))
        .unwrap();
    for id in &ids {
        assert_eq!(element(&engine, id).group_ids, image.group_ids);
    }
}

// --------------------------------------------------------------------- history

#[test]
fn one_undo_takes_the_trace_away_and_gives_the_original_back() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);

    let ids = engine
        .vectorize_to_shapes(&image_id, &trace(), options(false))
        .unwrap();
    assert!(
        element(&engine, &image_id).is_deleted,
        "replaced by its trace"
    );
    assert_eq!(live(&engine).len(), 3);
    let tombstone = element(&engine, &image_id);
    assert!(
        tombstone.version > 1,
        "the removal is stamped, so it is saved and sent"
    );

    engine.undo();
    let back = live(&engine);
    assert_eq!(back.len(), 1, "only the image: {back:?}");
    assert_eq!(back[0].id, image_id);
    assert!(back[0].data_url.is_some(), "with its picture");
    assert!(ids.iter().all(|id| element(&engine, id).is_deleted));

    engine.redo();
    assert_eq!(live(&engine).len(), 3);
    assert!(element(&engine, &image_id).is_deleted);
}

#[test]
fn keeping_the_original_leaves_it_under_the_trace() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    engine
        .vectorize_to_shapes(&image_id, &trace(), options(true))
        .unwrap();
    let order = live(&engine);
    assert_eq!(order.len(), 4);
    assert_eq!(order[0].id, image_id);

    engine.undo();
    assert_eq!(live(&engine).len(), 1);
}

// --------------------------------------------------------------------- refusals

#[test]
fn a_locked_image_is_refused_and_nothing_changes() {
    let mut image = picture_at(100.0, 50.0, 200.0, 100.0);
    image.locked = Some(true);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    let before = engine.get_scene();

    assert_eq!(
        engine.vectorize_to_shapes(&image_id, &trace(), options(false)),
        Err(VectorizeRefusal::Locked)
    );
    assert_eq!(
        engine.vectorize_to_picture(&image_id, SVG, options(false)),
        Err(VectorizeRefusal::Locked)
    );
    assert_eq!(engine.get_scene(), before);
    engine.undo();
    assert_eq!(engine.get_scene(), before, "no step was recorded");
}

#[test]
fn an_image_a_peer_holds_is_refused() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    engine.set_peers(vec![Peer {
        id: "ana".into(),
        name: "Ana".into(),
        color: "#e03131".into(),
        holds: vec![image_id.clone()],
        preview: Vec::new(),
    }]);
    assert_eq!(
        engine.vectorize_to_shapes(&image_id, &trace(), options(false)),
        Err(VectorizeRefusal::Held)
    );
}

#[test]
fn only_an_image_can_be_vectorized() {
    let shape = box_at(0.0, 0.0, 10.0, 10.0);
    let id = shape.id.clone();
    let mut engine = engine_with_scene(vec![shape]);
    assert_eq!(
        engine.vectorize_to_shapes(&id, &trace(), options(false)),
        Err(VectorizeRefusal::NotAnImage)
    );
    assert_eq!(
        engine.vectorize_to_shapes("nobody", &trace(), options(false)),
        Err(VectorizeRefusal::NotAnImage)
    );
}

#[test]
fn a_trace_the_board_has_no_room_for_is_refused() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image, box_at(0.0, 0.0, 5.0, 5.0)]);

    // Two live, three to add: four fits only because the image goes.
    let tight = |max_elements| VectorizeOptions {
        keep_original: false,
        limits: VectorizeLimits {
            max_elements,
            ..LIMITS
        },
    };
    assert_eq!(
        engine.vectorize_to_shapes(&image_id, &trace(), tight(3)),
        Err(VectorizeRefusal::BoardFull)
    );
    let keeping = VectorizeOptions {
        keep_original: true,
        ..tight(4)
    };
    assert_eq!(
        engine.vectorize_to_shapes(&image_id, &trace(), keeping),
        Err(VectorizeRefusal::BoardFull)
    );
    assert_eq!(live(&engine).len(), 2);
    assert!(engine
        .vectorize_to_shapes(&image_id, &trace(), tight(4))
        .is_ok());
}

#[test]
fn a_trace_over_the_shape_cap_or_empty_is_refused() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    let capped = VectorizeOptions {
        keep_original: false,
        limits: VectorizeLimits {
            max_trace_shapes: 2,
            ..LIMITS
        },
    };
    assert_eq!(
        engine.vectorize_to_shapes(&image_id, &trace(), capped),
        Err(VectorizeRefusal::TooManyShapes)
    );
    let empty = TraceRings::default();
    assert_eq!(
        engine.vectorize_to_shapes(&image_id, &empty, options(false)),
        Err(VectorizeRefusal::Empty)
    );
}

#[test]
fn a_payload_draw_trace_would_not_make_is_refused() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    let mut short = trace();
    short.coords.pop();
    let mut colour = trace();
    colour.colours[1] = 0xff00_0000;
    let mut unmatched = trace();
    unmatched.lengths.pop();
    let mut infinite = trace();
    infinite.coords[3] = f64::INFINITY;
    for payload in [short, colour, unmatched, infinite] {
        assert_eq!(
            engine.vectorize_to_shapes(&image_id, &payload, options(false)),
            Err(VectorizeRefusal::Malformed)
        );
    }
    assert_eq!(live(&engine).len(), 1);
}

// --------------------------------------------------------------------- picture

#[test]
fn a_picture_takes_the_image_s_place() {
    let mut image = picture_at(100.0, 50.0, -200.0, 100.0);
    image.angle = 0.4;
    image.group_ids = vec!["g".into()];
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![box_at(0.0, 0.0, 5.0, 5.0), image.clone()]);

    let id = engine
        .vectorize_to_picture(&image_id, SVG, options(false))
        .expect("inserted");
    let picture = element(&engine, &id);
    assert_eq!(picture.kind, DrawElementType::Image);
    assert_eq!(picture.data_url.as_deref(), Some(SVG));
    assert_eq!(
        (
            picture.x,
            picture.y,
            picture.width,
            picture.height,
            picture.angle
        ),
        (image.x, image.y, image.width, image.height, image.angle),
        "the same box, mirror and turn"
    );
    assert_eq!(picture.opacity, image.opacity);
    assert_eq!(picture.group_ids, image.group_ids);
    assert_eq!(picture.version, 1);
    assert!(element(&engine, &image_id).is_deleted);
    assert_eq!(engine.get_selection(), vec![id.clone()]);

    engine.undo();
    assert!(!element(&engine, &image_id).is_deleted);
    assert!(element(&engine, &id).is_deleted);
}

#[test]
fn a_picture_too_large_or_not_an_svg_is_refused() {
    let image = picture_at(100.0, 50.0, 200.0, 100.0);
    let image_id = image.id.clone();
    let mut engine = engine_with_scene(vec![image]);
    let small = VectorizeOptions {
        keep_original: false,
        limits: VectorizeLimits {
            max_data_url_length: 20,
            ..LIMITS
        },
    };
    assert_eq!(
        engine.vectorize_to_picture(&image_id, SVG, small),
        Err(VectorizeRefusal::TooLarge)
    );
    assert_eq!(
        engine.vectorize_to_picture(&image_id, "data:image/png;base64,AAAA", options(false)),
        Err(VectorizeRefusal::NotAPicture)
    );
    assert_eq!(
        engine.vectorize_to_picture(&image_id, "data:image/svg+xml;utf8,<svg/>", options(false)),
        Err(VectorizeRefusal::NotAPicture),
        "the contract takes base64 only"
    );
    assert_eq!(live(&engine).len(), 1);
}
