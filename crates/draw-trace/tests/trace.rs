//! Tracing: that the editable rings paint exactly what the trace paints.
//!
//! The reference is vtracer's own document, filled the way its SVG is — non-zero, every
//! subpath of a shape together — and rasterised here at pixel centres with the curves
//! sampled densely and independently of the flattening under test. The payload's rings
//! are then filled one at a time, the way the engine fills a closed line, under both
//! rules a renderer might use. Anywhere they disagree further than the flattening
//! tolerance from an edge is visible geometry lost or invented.

use draw_trace::rings::{compose, flatten_cubic, flatten_subpath, signed_area, Pt};
use draw_trace::{PresetName, Rendered, TraceConfig, Tracer, FLATTEN_TOLERANCE, MAX_RING_POINTS};
use vtracer::ir::{PathCmd, VectorDoc};

// ----------------------------------------------------------------------------- images

fn image(width: usize, height: usize, pixel: impl Fn(f64, f64) -> [u8; 4]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&pixel(x as f64 + 0.5, y as f64 + 0.5));
        }
    }
    rgba
}

const DISC: [u8; 4] = [200, 60, 60, 255];
const PAPER: [u8; 4] = [240, 240, 240, 255];
const RING: [u8; 4] = [40, 90, 200, 255];
const ISLAND: [u8; 4] = [230, 160, 20, 255];

/// A 160px square with a disc of radius 50 in the middle.
fn disc() -> Vec<u8> {
    image(160, 160, |x, y| {
        if (x - 80.0).hypot(y - 80.0) <= 50.0 {
            DISC
        } else {
            PAPER
        }
    })
}

/// A donut — radius 60 out, 30 in — with an island of another colour in its hole. Every
/// layer of it has something the one above has to show through.
fn donut(ring: [u8; 4], island: [u8; 4], paper: [u8; 4]) -> Vec<u8> {
    image(160, 160, |x, y| {
        let r = (x - 80.0).hypot(y - 80.0);
        if r <= 12.0 {
            island
        } else if (30.0..=60.0).contains(&r) {
            ring
        } else {
            paper
        }
    })
}

fn config(preset: PresetName) -> TraceConfig {
    TraceConfig {
        preset,
        color_precision: None,
        layer_difference: None,
        filter_speckle: None,
        corner_threshold: None,
        mode: None,
    }
}

fn trace(rgba: Vec<u8>, preset: PresetName) -> Rendered {
    let mut tracer = Tracer::from_rgba(rgba, 160, 160).expect("an image");
    tracer
        .render(&config(preset), &mut |_| {})
        .expect("a trace")
        .clone()
}

// ------------------------------------------------------------------------ raster

fn points(flat: &[f64]) -> Vec<Pt> {
    flat.as_chunks::<2>().0.to_vec()
}

fn cubic_at(p0: Pt, p1: Pt, p2: Pt, p3: Pt, t: f64) -> Pt {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    ]
}

/// The document's shapes as polygons, curves sampled at 256 steps — independent of the
/// adaptive flattening under test.
fn reference(doc: &VectorDoc) -> Vec<Vec<Vec<Pt>>> {
    doc.shapes
        .iter()
        .map(|shape| {
            shape
                .path
                .subpaths
                .iter()
                .map(|sub| {
                    let mut ring: Vec<Pt> = Vec::new();
                    let mut at = [0.0, 0.0];
                    for command in &sub.commands {
                        match *command {
                            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => {
                                at = [p.x, p.y];
                                ring.push(at);
                            }
                            PathCmd::CubicTo(c1, c2, end) => {
                                let (c1, c2, end) = ([c1.x, c1.y], [c2.x, c2.y], [end.x, end.y]);
                                for step in 1..=256 {
                                    ring.push(cubic_at(at, c1, c2, end, step as f64 / 256.0));
                                }
                                at = end;
                            }
                            PathCmd::Close => {}
                        }
                    }
                    ring
                })
                .collect()
        })
        .collect()
}

fn winding(ring: &[Pt], x: f64, y: f64) -> i32 {
    let mut winding = 0;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let cross = (b[0] - a[0]) * (y - a[1]) - (x - a[0]) * (b[1] - a[1]);
        if a[1] <= y {
            if b[1] > y && cross > 0.0 {
                winding += 1;
            }
        } else if b[1] <= y && cross < 0.0 {
            winding -= 1;
        }
    }
    winding
}

fn crossings(ring: &[Pt], x: f64, y: f64) -> usize {
    let mut inside = 0;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            inside += 1;
        }
    }
    inside
}

#[derive(Clone, Copy)]
enum Rule {
    NonZero,
    EvenOdd,
}

/// The topmost shape painting each pixel centre, or `None`.
fn paint(
    width: usize,
    height: usize,
    painted: impl Fn(usize, f64, f64) -> bool,
    shapes: usize,
) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
            out.push((0..shapes).rev().find(|&s| painted(s, cx, cy)));
        }
    }
    out
}

fn distance_to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

fn distance_to_edges(shapes: &[Vec<Vec<Pt>>], p: Pt) -> f64 {
    shapes
        .iter()
        .flatten()
        .flat_map(|ring| (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()])))
        .map(|(a, b)| distance_to_segment(p, a, b))
        .fold(f64::INFINITY, f64::min)
}

/// Asserts that the payload paints what the document paints, everywhere further than
/// the flattening tolerance from an edge.
fn assert_rings_paint_the_trace(rendered: &Rendered, rule: Rule) {
    let reference = reference(&rendered.doc);
    let shapes: Vec<Vec<Vec<Pt>>> = rendered
        .shapes
        .shapes
        .iter()
        .map(|shape| shape.rings.iter().map(|ring| points(ring)).collect())
        .collect();
    assert_eq!(
        shapes.len(),
        reference.len(),
        "one payload shape per traced shape"
    );

    let (w, h) = (160, 160);
    let expected = paint(
        w,
        h,
        |s, x, y| {
            reference[s]
                .iter()
                .map(|ring| winding(ring, x, y))
                .sum::<i32>()
                != 0
        },
        reference.len(),
    );
    let actual = paint(
        w,
        h,
        |s, x, y| {
            shapes[s].iter().any(|ring| match rule {
                Rule::NonZero => winding(ring, x, y) != 0,
                Rule::EvenOdd => crossings(ring, x, y) % 2 == 1,
            })
        },
        shapes.len(),
    );

    let mut differing = 0;
    for (i, (want, got)) in expected.iter().zip(&actual).enumerate() {
        if want == got {
            continue;
        }
        differing += 1;
        let centre = [(i % w) as f64 + 0.5, (i / w) as f64 + 0.5];
        let from_edge = distance_to_edges(&reference, centre);
        assert!(
            from_edge <= FLATTEN_TOLERANCE + 0.05,
            "pixel {centre:?} is {from_edge:.2}px from any edge and paints {got:?}, \
             where the trace paints {want:?}"
        );
    }
    assert!(
        differing < w * h / 100,
        "{differing} pixels differ along the edges"
    );
}

// ------------------------------------------------------------------------- tests

#[test]
fn a_disc_becomes_one_closed_ring_with_the_disc_s_area() {
    let rendered = trace(disc(), PresetName::Poster);
    let disc_shape = rendered
        .shapes
        .shapes
        .iter()
        .find(|shape| shape.color().eq_ignore_ascii_case("#C83C3C"))
        .unwrap_or_else(|| panic!("no shape in the disc's colour: {:?}", rendered.shapes));
    assert_eq!(disc_shape.rings.len(), 1, "one disc, one ring");

    let ring = points(&disc_shape.rings[0]);
    assert_eq!(ring.first(), ring.last(), "closed explicitly");
    let area = signed_area(&ring).abs();
    let expected = std::f64::consts::PI * 50.0 * 50.0;
    assert!(
        (area - expected).abs() / expected < 0.02,
        "area {area:.1} against {expected:.1}"
    );
}

#[test]
fn flattening_stays_within_the_tolerance() {
    let curves: [[Pt; 4]; 4] = [
        [[0.0, 0.0], [30.0, 80.0], [70.0, -80.0], [100.0, 0.0]],
        [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
        [[10.0, 10.0], [200.0, 300.0], [-150.0, 250.0], [40.0, 12.0]],
        [[0.0, 0.0], [0.3, 0.1], [0.6, -0.1], [1.0, 0.0]],
    ];
    for [p0, p1, p2, p3] in curves {
        let mut line = vec![p0];
        flatten_cubic(p0, p1, p2, p3, FLATTEN_TOLERANCE, &mut line);
        assert_eq!(*line.last().unwrap(), p3, "ends where the curve ends");
        for step in 0..=2000 {
            let on_curve = cubic_at(p0, p1, p2, p3, step as f64 / 2000.0);
            let off = line
                .windows(2)
                .map(|w| distance_to_segment(on_curve, w[0], w[1]))
                .fold(f64::INFINITY, f64::min);
            assert!(
                off <= FLATTEN_TOLERANCE + 1e-9,
                "{on_curve:?} is {off} from the polyline of {p0:?}..{p3:?}"
            );
        }
    }
}

#[test]
fn a_straight_cubic_flattens_to_one_segment() {
    let mut line = Vec::new();
    flatten_cubic(
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        0.25,
        &mut line,
    );
    assert_eq!(line, vec![[30.0, 0.0]]);
}

/// What the brief assumed, checked on the fork's output: a stacked *colour* trace paints
/// each layer solid and lets the one above cover it, so it has no holes. Line art does
/// not stack — only the dark parts are traced, and the paper inside an `O` is nothing at
/// all — so its shapes keep their holes, and those are what the splice is for.
#[test]
fn colour_layers_are_solid_but_line_art_keeps_its_holes() {
    let has_hole = |rendered: &Rendered| {
        rendered
            .doc
            .shapes
            .iter()
            .any(|shape| shape.path.subpaths.len() > 1)
    };
    for preset in [PresetName::Poster, PresetName::Photo] {
        assert!(
            !has_hole(&trace(donut(RING, ISLAND, PAPER), preset)),
            "{preset:?}: a stacked colour layer with a hole"
        );
    }
    let black = [0, 0, 0, 255];
    assert!(has_hole(&trace(
        donut(black, black, [255, 255, 255, 255]),
        PresetName::Bw
    )));
}

#[test]
fn every_hole_is_spliced_so_the_rings_paint_what_the_trace_paints() {
    let black = [0, 0, 0, 255];
    let white = [255, 255, 255, 255];
    for (rgba, preset) in [
        (donut(RING, ISLAND, PAPER), PresetName::Poster),
        (donut(RING, ISLAND, PAPER), PresetName::Photo),
        (donut(black, black, white), PresetName::Bw),
        (disc(), PresetName::Poster),
    ] {
        let rendered = trace(rgba, preset);
        assert_rings_paint_the_trace(&rendered, Rule::NonZero);
        assert_rings_paint_the_trace(&rendered, Rule::EvenOdd);
    }
}

#[test]
fn the_hole_shows_what_was_under_it_in_the_source() {
    // Between the island and the ring, in the donut's hole: paper, not ring.
    let rendered = trace(donut(RING, ISLAND, PAPER), PresetName::Poster);
    let top = rendered
        .shapes
        .shapes
        .iter()
        .rev()
        .find(|shape| {
            shape
                .rings
                .iter()
                .any(|ring| winding(&points(ring), 80.5, 100.5) != 0)
        })
        .expect("something paints the hole");
    assert!(
        top.color().eq_ignore_ascii_case("#F0F0F0"),
        "the hole is painted {}",
        top.color()
    );

    // A black donut traced as line art leaves its hole unpainted: the paper is not traced.
    let black = [0, 0, 0, 255];
    let rendered = trace(donut(black, black, [255, 255, 255, 255]), PresetName::Bw);
    assert!(!rendered.shapes.shapes.iter().any(|shape| {
        shape
            .rings
            .iter()
            .any(|ring| winding(&points(ring), 80.5, 100.5) != 0)
    }));
}

fn circle(cx: f64, cy: f64, r: f64, n: usize, clockwise: bool) -> Vec<Pt> {
    let mut ring: Vec<Pt> = (0..n)
        .map(|i| {
            let a =
                i as f64 / n as f64 * std::f64::consts::TAU * if clockwise { 1.0 } else { -1.0 };
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect();
    ring.push(ring[0]);
    ring
}

#[test]
fn a_ring_over_the_budget_is_simplified_before_its_holes_are_spliced() {
    let rings = vec![
        circle(0.0, 0.0, 100.0, 3000, true),
        circle(-40.0, 0.0, 20.0, 1500, false),
        circle(40.0, 0.0, 20.0, 1500, false),
        circle(0.0, 50.0, 15.0, 1500, false),
    ];
    let parts = compose(rings, 2000);
    assert_eq!(parts.len(), 1, "one outer, its holes inside it");
    let part = &parts[0];
    assert!(part.len() <= 2000, "{} points", part.len());
    assert_eq!(part.first(), part.last());
    for (x, y) in [(-40.0, 0.0), (40.0, 0.0), (0.0, 50.0)] {
        assert_eq!(winding(part, x, y), 0, "hole at ({x}, {y}) painted");
        assert_eq!(crossings(part, x, y) % 2, 0);
    }
    for (x, y) in [(0.0, 0.0), (0.0, -60.0), (80.0, 10.0)] {
        assert_ne!(winding(part, x, y), 0, "({x}, {y}) should be painted");
    }
}

#[test]
fn a_hole_outside_every_outer_still_paints_as_its_own_ring() {
    // Non-zero paints a lone ring whichever way it turns; so must the payload.
    let rings = vec![
        circle(0.0, 0.0, 10.0, 64, true),
        circle(100.0, 0.0, 10.0, 64, false),
    ];
    let parts = compose(rings, MAX_RING_POINTS);
    assert_eq!(parts.len(), 2);
    assert!(parts.iter().any(|part| winding(part, 100.0, 0.0) != 0));
}

#[test]
fn rings_are_closed_small_enough_and_free_of_repeated_points() {
    let rendered = trace(donut(RING, ISLAND, PAPER), PresetName::Poster);
    for shape in &rendered.shapes.shapes {
        for flat in &shape.rings {
            let ring = points(flat);
            assert!(
                ring.len() >= 4,
                "a ring needs three corners and its closure"
            );
            assert!(ring.len() <= MAX_RING_POINTS);
            assert_eq!(ring.first(), ring.last());
            assert!(ring.windows(2).all(|w| w[0] != w[1]), "a repeated point");
        }
    }
}

#[test]
fn flatten_subpath_closes_what_it_is_given() {
    let rendered = trace(disc(), PresetName::Poster);
    for shape in &rendered.doc.shapes {
        for sub in &shape.path.subpaths {
            let ring = flatten_subpath(sub, FLATTEN_TOLERANCE);
            assert_eq!(ring.first(), ring.last());
        }
    }
}

#[test]
fn the_picture_scales_to_the_box_it_is_put_in() {
    // Without a viewBox an SVG drawn into a box of another size is cropped, not scaled —
    // and an image element is resized freely.
    let rendered = trace(disc(), PresetName::Poster);
    assert!(
        rendered.svg.contains(r#"viewBox="0 0 160 160""#),
        "{}",
        &rendered.svg[..200]
    );
    assert!(rendered.svg.contains(r#"preserveAspectRatio="none""#));
}

#[test]
fn the_flat_rings_are_the_rings_as_fractions_of_the_picture() {
    let rendered = trace(disc(), PresetName::Poster);
    let flat = rendered.shapes.flat();
    let rings = rendered.shapes.shapes.iter().flat_map(|s| &s.rings).count();
    assert_eq!((flat.colours.len(), flat.lengths.len()), (rings, rings));
    let points: usize = flat.lengths.iter().map(|&n| n as usize).sum();
    assert_eq!(points * 2, flat.coords.len());

    // The disc's ring, found by its colour, spans pixels 30 to 130 of 160 each way.
    let at = flat
        .colours
        .iter()
        .position(|&rgb| rgb == 0xC8_3C_3C)
        .expect("a ring in the disc's colour");
    let start: usize = flat.lengths[..at].iter().map(|&n| 2 * n as usize).sum();
    let ring = &flat.coords[start..start + 2 * flat.lengths[at] as usize];
    for axis in 0..2 {
        let values = ring.iter().skip(axis).step_by(2);
        let min = values.clone().copied().fold(f64::INFINITY, f64::min);
        let max = values.copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (min - 30.0 / 160.0).abs() < 1.0 / 160.0,
            "axis {axis} from {min}"
        );
        assert!(
            (max - 130.0 / 160.0).abs() < 1.0 / 160.0,
            "axis {axis} to {max}"
        );
    }
}

#[test]
fn stats_count_what_an_insert_costs() {
    let rendered = trace(donut(RING, ISLAND, PAPER), PresetName::Poster);
    let rings: usize = rendered.shapes.shapes.iter().map(|s| s.rings.len()).sum();
    let points: usize = rendered
        .shapes
        .shapes
        .iter()
        .flat_map(|s| &s.rings)
        .map(|ring| ring.len() / 2)
        .sum();
    assert_eq!(rendered.stats.shapes, rings);
    assert_eq!(rendered.stats.points, points);
    assert_eq!(rendered.stats.regions, rendered.doc.shapes.len());
    assert_eq!(rendered.stats.svg_bytes, rendered.svg.len());
    assert_eq!((rendered.stats.width, rendered.stats.height), (160, 160));
}

#[test]
fn a_wrong_buffer_is_refused() {
    assert!(Tracer::from_rgba(vec![0; 10], 4, 4).is_err());
    assert!(Tracer::from_rgba(Vec::new(), 0, 0).is_err());
}
