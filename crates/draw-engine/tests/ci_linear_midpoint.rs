//! Where a segment's midpoint handle actually sits.
//!
//! A line or arrow gets a handle between each pair of points; dragging one inserts a
//! point there and bends the path. Those handles were put at the **chord** centre,
//! `(a + b) / 2`, which is right only for a path drawn as straight segments.
//!
//! It is not drawn that way. `roundness` defaults to `Some(8.0)` for every element, and
//! `render/shape.rs` sends a rounded linear element through `generator::curve` — a
//! Catmull-Rom curve through the points. The chord centre of a curved segment is not on
//! the curve, so every midpoint handle floated beside the line it belonged to: visible
//! as an offset, and worse than cosmetic, because the hit test reads the same positions,
//! so the place you have to click is not the place you can see.
//!
//! Excalidraw forks on exactly this case — `LinearElementEditor.getSegmentMidPoint`
//! (`packages/element/src/linearElementEditor.ts:971-1012`) at the SHA pinned in
//! `scripts/oracle-sha.txt`:
//!
//! ```ts
//! if (lines.length)  return pointCenter(segment[0], segment[1]);  // straight: the chord
//! if (curves.length) return curvePointAtLength(segment, 0.5);     // curved: on the curve
//! ```
//!
//! # How these assert it
//!
//! The invariant is "the handle is on the drawn path", so that is measured directly:
//! the curve is sampled densely *here*, independently of the code under test, and the
//! handle's distance to the nearest sample has to be about zero. That makes the tests
//! independent of how the midpoint is found — a different quadrature, a different
//! parameterisation, any of it stays passing as long as the answer is still on the line.
//!
//! Every case has its control. Asserting only "the handle is on the curve" would also
//! pass if the curve happened to be straight, so `the_chord_centre_is_visibly_off_the_curve`
//! establishes that these two answers genuinely differ before anything else claims to
//! have chosen between them.

mod common;
use common::*;
use draw_engine::selection::linear::handle_points;
use draw_engine::*;

/// Every segment gets a handle, however short. The minimum only exists so two handles a
/// few pixels apart cannot both be aimed at, which is not what any of this is about.
const EVERY_SEGMENT: f64 = 0.0;

/// A path with a real bend in it, so the curve and its chords are not the same line.
fn bent_line(roundness: Option<f64>) -> DrawElement {
    let mut element = create_element_default(
        DrawElementType::Line,
        Geometry {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 120.0,
        },
    );
    element.points = Some(vec![[0.0, 0.0], [100.0, 120.0], [200.0, 0.0]]);
    element.roundness = roundness;
    element
}

fn midpoints(element: &DrawElement) -> Vec<[f64; 2]> {
    handle_points(element, EVERY_SEGMENT)
        .into_iter()
        .filter_map(|handle| match handle.handle {
            selection::LinearHandle::Midpoint(_) => Some([handle.x, handle.y]),
            selection::LinearHandle::Point(_) => None,
        })
        .collect()
}

fn corners(element: &DrawElement) -> Vec<[f64; 2]> {
    handle_points(element, EVERY_SEGMENT)
        .into_iter()
        .filter_map(|handle| match handle.handle {
            selection::LinearHandle::Point(_) => Some([handle.x, handle.y]),
            selection::LinearHandle::Midpoint(_) => None,
        })
        .collect()
}

/// The drawn path, sampled densely, in world space.
///
/// Built here rather than borrowed from the code under test, so that a bug which moved
/// both the handle and the reference together could not hide behind it.
fn sample_path(element: &DrawElement) -> Vec<[f64; 2]> {
    let world: Vec<[f64; 2]> = selection::linear::world_points(element)
        .iter()
        .map(|p| [p.x, p.y])
        .collect();
    if element.roundness.is_none() {
        // Straight segments: sample along each chord.
        let mut out = Vec::new();
        for pair in world.windows(2) {
            for step in 0..=400 {
                let t = step as f64 / 400.0;
                out.push([
                    pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                    pair[0][1] + (pair[1][1] - pair[0][1]) * t,
                ]);
            }
        }
        return out;
    }
    catmull_rom_cubics(&world, CURVE_TIGHTNESS)
        .iter()
        .flat_map(|curve| (0..=400).map(move |step| bezier_point(curve, step as f64 / 400.0)))
        .collect()
}

/// How far a point is from the nearest sample of the drawn path.
fn distance_to_path(element: &DrawElement, point: [f64; 2]) -> f64 {
    sample_path(element)
        .into_iter()
        .map(|s| (s[0] - point[0]).hypot(s[1] - point[1]))
        .fold(f64::INFINITY, f64::min)
}

fn chord_centres(element: &DrawElement) -> Vec<[f64; 2]> {
    let world = selection::linear::world_points(element);
    world
        .windows(2)
        .map(|pair| [(pair[0].x + pair[1].x) / 2.0, (pair[0].y + pair[1].y) / 2.0])
        .collect()
}

// ---------------------------------------------------------------------------
// The control: the two answers are genuinely different
// ---------------------------------------------------------------------------

/// Establishes the bug before anything claims to fix it. If the chord centre were already
/// on the curve there would be nothing here to get wrong, and every other assertion in
/// this file would pass for the wrong reason.
#[test]
fn the_chord_centre_is_visibly_off_the_curve() {
    let line = bent_line(Some(8.0));
    let worst = chord_centres(&line)
        .into_iter()
        .map(|centre| distance_to_path(&line, centre))
        .fold(0.0_f64, f64::max);
    assert!(
        worst > 4.0,
        "the chord centre should be well off a curved path; it was {worst} away"
    );
}

// ---------------------------------------------------------------------------
// A curved path
// ---------------------------------------------------------------------------

#[test]
fn a_midpoint_handle_sits_on_a_curved_path() {
    let line = bent_line(Some(8.0));
    for centre in midpoints(&line) {
        let off = distance_to_path(&line, centre);
        assert!(off < 0.5, "handle at {centre:?} was {off} off the path");
    }
}

/// Two points, two handles — one per segment — wherever they are put.
#[test]
fn there_is_one_midpoint_handle_per_segment() {
    let line = bent_line(Some(8.0));
    assert_eq!(midpoints(&line).len(), 2);
}

/// Half the segment, not half the parameter. On an asymmetric bend a cubic covers
/// noticeably more ground in one half of `t` than the other, so the handle would sit
/// away from the middle of the segment a person sees even while staying on the line.
#[test]
fn a_midpoint_handle_is_halfway_along_its_segment() {
    let line = bent_line(Some(8.0));
    let curves = catmull_rom_cubics(
        &selection::linear::world_points(&line)
            .iter()
            .map(|p| [p.x, p.y])
            .collect::<Vec<_>>(),
        CURVE_TIGHTNESS,
    );
    let handles = midpoints(&line);

    for (curve, handle) in curves.iter().zip(&handles) {
        // Arc length up to the handle, against the whole segment's.
        let samples: Vec<[f64; 2]> = (0..=2000)
            .map(|step| bezier_point(curve, step as f64 / 2000.0))
            .collect();
        let mut total = 0.0;
        let mut upto = 0.0;
        let mut best = f64::INFINITY;
        for pair in samples.windows(2) {
            let seg = (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
            let d = (pair[0][0] - handle[0]).hypot(pair[0][1] - handle[1]);
            if d < best {
                best = d;
                upto = total;
            }
            total += seg;
        }
        let fraction = upto / total;
        assert!(
            (fraction - 0.5).abs() < 0.02,
            "the handle should be halfway along the segment; it was at {fraction}"
        );
    }
}

// ---------------------------------------------------------------------------
// A straight path keeps the old answer
// ---------------------------------------------------------------------------

/// Sharp edges are still chords. This is the oracle's `if (lines.length)` branch, and it
/// is the half a fix is most likely to break by making everything a curve.
#[test]
fn a_sharp_path_puts_its_handles_at_the_chord_centres() {
    let line = bent_line(None);
    let handles = midpoints(&line);
    let chords = chord_centres(&line);
    assert_eq!(handles.len(), chords.len());
    for (handle, chord) in handles.iter().zip(&chords) {
        assert_close(handle[0], chord[0]);
        assert_close(handle[1], chord[1]);
    }
}

// ---------------------------------------------------------------------------
// Turned, and moved
// ---------------------------------------------------------------------------

/// The handles are reported in world space, so a rotation has to carry them. A midpoint
/// computed in local space and never turned would sit on the *unrotated* ghost of the
/// path — the same class of bug as the chord centre, and just as invisible in a test that
/// only ever looks at an axis-aligned line.
#[test]
fn midpoint_handles_follow_a_rotated_path() {
    let mut line = bent_line(Some(8.0));
    line.angle = 0.7;
    for centre in midpoints(&line) {
        let off = distance_to_path(&line, centre);
        assert!(
            off < 0.5,
            "handle at {centre:?} was {off} off the turned path"
        );
    }
}

/// And the corner handles stay exactly on their points, turned or not — a curve passes
/// through the points it is built from, so nothing here should have moved them.
#[test]
fn corner_handles_stay_on_their_points() {
    let mut line = bent_line(Some(8.0));
    line.angle = 0.7;
    let world = selection::linear::world_points(&line);
    let handles = corners(&line);
    assert_eq!(handles.len(), world.len());
    for (handle, point) in handles.iter().zip(&world) {
        assert_close(handle[0], point.x);
        assert_close(handle[1], point.y);
    }
}

// ---------------------------------------------------------------------------
// What you can click is what you can see
// ---------------------------------------------------------------------------

/// The hit test reads the same positions the painter does, which is the whole reason the
/// offset mattered rather than merely looked wrong: aiming at the handle you can see has
/// to grab it.
#[test]
fn a_midpoint_handle_is_grabbable_where_it_is_drawn() {
    let line = bent_line(Some(8.0));
    let handles = handle_points(&line, EVERY_SEGMENT);
    let centre = midpoints(&line)[0];

    let hit = selection::linear::hit_handle(&handles, centre[0], centre[1], 1.0);
    assert_eq!(hit, Some(selection::LinearHandle::Midpoint(0)));
}
