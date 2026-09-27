//! The three things that turn pointer samples into a drawn line.
//!
//! The painter is `wasm/paint.rs` and needs a canvas, so none of this could be asserted
//! if it lived there. It is pure arithmetic over slices instead, and this is where it is
//! pinned.

mod common;
use common::*;
use draw_engine::*;

mod streamlining {
    use super::*;

    /// The tremble fix, stated as the property that matters: the smoothed point is
    /// always between where the line was and where the pointer went, never at either
    /// end. That is what makes it a low-pass filter rather than a delay or a no-op.
    #[test]
    fn a_sample_lands_between_the_last_point_and_the_pointer() {
        let out = freehand::streamline([0.0, 0.0], [10.0, 20.0], 0.5);
        assert_close(out[0], 5.0);
        assert_close(out[1], 10.0);
    }

    #[test]
    fn no_strength_leaves_the_sample_alone() {
        let out = freehand::streamline([0.0, 0.0], [10.0, 20.0], 0.0);
        assert_close(out[0], 10.0);
        assert_close(out[1], 20.0);
    }

    /// Full strength never moves at all, which is why the default is not 1.0 — the line
    /// would never reach the pointer.
    #[test]
    fn full_strength_never_moves() {
        let out = freehand::streamline([3.0, 4.0], [99.0, 99.0], 1.0);
        assert_close(out[0], 3.0);
        assert_close(out[1], 4.0);
    }

    /// A shaky hand drawing along a straight line: the samples jitter either side of it,
    /// and the streamlined path has to be measurably straighter than the raw one.
    #[test]
    fn it_takes_the_shake_out_of_a_straight_stroke() {
        let raw: Vec<[f64; 2]> = (0..40)
            .map(|i| {
                let x = i as f64 * 4.0;
                // A tremor: alternating either side, roughly 1.5 units of wobble.
                let wobble = if i % 2 == 0 { 1.5 } else { -1.5 };
                [x, 100.0 + wobble]
            })
            .collect();

        let mut smoothed = vec![raw[0]];
        for &point in &raw[1..] {
            let previous = *smoothed.last().unwrap();
            smoothed.push(freehand::streamline(previous, point, freehand::STREAMLINE));
        }

        // Total vertical wandering, which is what a tremor is.
        let wander = |path: &[[f64; 2]]| {
            path.windows(2)
                .map(|w| (w[1][1] - w[0][1]).abs())
                .sum::<f64>()
        };
        let before = wander(&raw);
        let after = wander(&smoothed);
        assert!(
            after < before * 0.6,
            "streamlining should remove most of the shake: {after} vs {before}"
        );
    }
}

mod thinning {
    use super::*;

    #[test]
    fn a_slow_stroke_keeps_its_full_width() {
        assert_close(freehand::radius_at(10.0, 0.0, freehand::THINNING), 5.0);
    }

    #[test]
    fn a_fast_stroke_is_thinner() {
        let slow = freehand::radius_at(10.0, 1.0, freehand::THINNING);
        let fast = freehand::radius_at(10.0, 30.0, freehand::THINNING);
        assert!(fast < slow, "{fast} should be under {slow}");
    }

    /// A flick must not vanish. Letting the radius reach zero reads as a dropped input
    /// rather than as a taper, so it floors at a quarter of the nominal width.
    #[test]
    fn it_never_thins_away_to_nothing() {
        let very_fast = freehand::radius_at(10.0, 5000.0, 1.0);
        assert!(very_fast >= 5.0 * 0.25, "{very_fast}");
        assert!(very_fast > 0.0);
    }

    #[test]
    fn no_thinning_means_a_constant_width() {
        let slow = freehand::radius_at(10.0, 0.0, 0.0);
        let fast = freehand::radius_at(10.0, 100.0, 0.0);
        assert_close(slow, fast);
    }
}

mod outline {
    use super::*;

    fn straight_run() -> Vec<[f64; 2]> {
        (0..10).map(|i| [i as f64 * 5.0, 0.0]).collect()
    }

    /// A closed polygon, down one side and back up the other, so it has roughly two
    /// points per sample.
    #[test]
    fn it_returns_both_sides_of_the_stroke() {
        let outline = freehand::stroke_outline(&straight_run(), 8.0, 0.0);
        assert!(outline.len() >= 10, "got {}", outline.len());
    }

    /// The width has to actually be there. With no thinning, a horizontal run should be
    /// exactly `size` tall.
    #[test]
    fn the_polygon_is_as_wide_as_the_stroke() {
        let outline = freehand::stroke_outline(&straight_run(), 8.0, 0.0);
        let [_, min_y, _, max_y] = freehand::points_bounds(&outline);
        assert_close(max_y - min_y, 8.0);
    }

    /// One sample has no direction, and inventing one puts a dash on the canvas pointing
    /// somewhere arbitrary. The painter draws a dot for that case instead.
    #[test]
    fn a_single_sample_has_no_outline() {
        assert!(freehand::stroke_outline(&[[0.0, 0.0]], 8.0, 0.5).is_empty());
        assert!(freehand::stroke_outline(&[], 8.0, 0.5).is_empty());
    }

    /// Repeated samples in the same place — a pointer held still — must not produce a
    /// zero-length direction and a NaN normal.
    #[test]
    fn a_stationary_pointer_produces_no_nonsense() {
        let stuck = vec![[5.0, 5.0]; 6];
        let outline = freehand::stroke_outline(&stuck, 8.0, 0.5);
        assert!(
            outline.iter().all(|p| p[0].is_finite() && p[1].is_finite()),
            "no NaNs: {outline:?}"
        );
    }

    /// The taper, end to end: a stroke that starts slow and ends fast is wider at the
    /// start than at the finish.
    #[test]
    fn a_stroke_that_speeds_up_gets_thinner() {
        let mut points = Vec::new();
        let mut x = 0.0;
        for i in 0..20 {
            // Each step longer than the last: the pointer is accelerating.
            x += 1.0 + i as f64 * 2.0;
            points.push([x, 0.0]);
        }
        let outline = freehand::stroke_outline(&points, 12.0, freehand::THINNING);
        let half = outline.len() / 2;
        // The first half is one side walked forwards, so index 1 is near the start and
        // `half - 2` near the end.
        let thickness_at = |i: usize| outline[i][1].abs();
        assert!(
            thickness_at(1) > thickness_at(half - 2),
            "slow start {} should be wider than fast end {}",
            thickness_at(1),
            thickness_at(half - 2)
        );
    }
}

// -----------------------------------------------------------------------------
// The hand-drawn look
// -----------------------------------------------------------------------------

/// `design.md:351`, "Rough rendering" — the sixth row of §8 "Freedraw / pencil", which
/// says of this pipeline only that "it needs its own".
///
/// The claim is *what carries the hand*, and the answer is not rough.js. A pencil stroke is
/// not a jittered line: it is the **filled outline of the ink**, one closed polygon laid a
/// radius either side of the path and closed down the far side, painted as a shape
/// (`freehand::stroke_outline`, `src/freehand.rs:64-106`). The oracle paints a freedraw the
/// same way and for the same reason — its shape list is a rough-generated *fill* for a
/// loop and then `getFreeDrawSvgPath` for the stroke, which is an SVG path string, filled
/// straight onto the canvas rather than handed to `rc.draw`
/// (`packages/element/src/shape.ts@1118751f:976-995` and
/// `packages/element/src/renderElement.ts@1118751f:496-512`).
///
/// So the hand is in the points, and this asserts where each half of that lives: the
/// outline is traced from the ink and the rough generator is not involved at all.
mod rough_look {
    use super::*;
    use draw_engine::render::outline::{element_outline, Outline};
    use draw_engine::render::shape::element_drawable;

    fn a_hand() -> Vec<[f64; 2]> {
        // A shaky run: the tremor is the point, and a straight line would not show it.
        (0..24)
            .map(|i| {
                let x = i as f64 * 6.0;
                let wobble = if i % 2 == 0 { 2.0 } else { -2.0 };
                [x, 50.0 + wobble]
            })
            .collect()
    }

    fn stroke(points: Vec<[f64; 2]>) -> DrawElement {
        let mut element = create_element_default(
            DrawElementType::Freedraw,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
        );
        element.points = Some(points);
        element
    }

    /// The ink is a filled region, not a stroked path — the only way to draw a width that
    /// varies along the stroke, since `lineWidth` is one number for a whole path
    /// (`src/freehand.rs:64-69`). The tremor is still in it: a straight run of the same
    /// points and the same size has a *smaller* bounding box, because the hand's own
    /// sideways movement adds to the extent the ink covers.
    #[test]
    fn the_hand_is_in_the_ink_and_the_ink_is_filled() {
        let shaky = a_hand();
        let outline = freehand::stroke_outline(&shaky, 8.0, 0.0);
        let [_, min_y, _, max_y] = freehand::points_bounds(&outline);
        assert!(
            max_y - min_y > 8.0,
            "a shaky stroke covers more than its own width: {} tall",
            max_y - min_y
        );

        // The same samples without the tremor: the same size, but a band exactly `size`
        // tall, because the wobble is no longer adding to it.
        let straight: Vec<[f64; 2]> = shaky.iter().map(|p| [p[0], 50.0]).collect();
        let [_, s_min, _, s_max] =
            freehand::points_bounds(&freehand::stroke_outline(&straight, 8.0, 0.0));
        assert_close(s_max - s_min, 8.0);
    }

    /// Rough.js never sees a pencil stroke. `element_drawable` — the one function that
    /// hands an element to the rough generator — returns nothing for a `Freedraw`
    /// (`src/render/shape.rs:239-240`, "freedraw is a stroke outline"), and
    /// `is_roughable` does not list it either (`src/render/paint.rs:244-252`). The oracle
    /// agrees: freedraw is not among the kinds `generateRoughOptions` is asked for
    /// (`shape.ts@1118751f:992` gives the stroke as a path, and `:998-1007` lists the kinds
    /// that are painted directly and return `null` for the same reason).
    #[test]
    fn a_freedraw_is_never_handed_to_the_rough_generator() {
        let element = stroke(a_hand());
        assert!(
            element_drawable(&element).is_none(),
            "no rough ops: the outline is the ink, drawn directly"
        );
        assert!(
            !draw_engine::is_roughable(DrawElementType::Freedraw),
            "and it is not one of the kinds that can be"
        );
        // The contrast that makes the assertion above mean something: a rectangle of the
        // same size *is* rough-drawn.
        assert!(element_drawable(&box_at(0.0, 0.0, 100.0, 50.0)).is_some());
    }

    /// So roughness is not what makes it rough, and changing it must not redraw the
    /// stroke. This is the part a reader would most reasonably assume works the other way
    /// round, so it is asserted rather than left implicit: the pencil's look comes from the
    /// pointer samples, and a "sloppiness" of 0 and one of 2 give the same ink.
    #[test]
    fn roughness_does_not_change_a_freedraws_ink() {
        let clean = stroke(a_hand());
        let mut sloppy = clean.clone();
        sloppy.roughness = 2.0;
        let mut architect = clean.clone();
        architect.roughness = 0.0;

        assert_eq!(clean.points, sloppy.points);
        assert_eq!(
            freehand::stroke_outline(clean.points.as_ref().unwrap(), 8.0, freehand::THINNING),
            freehand::stroke_outline(sloppy.points.as_ref().unwrap(), 8.0, freehand::THINNING),
            "the same ink either way"
        );
        assert_eq!(element_drawable(&clean), element_drawable(&sloppy));
        assert_eq!(element_outline(&clean), element_outline(&sloppy));
        // ...and the element the painter traces is the same `Freehand` variant, not a
        // rough path that happens to look the same.
        assert_eq!(element_outline(&clean), Some(Outline::Freehand));
        assert_eq!(element_outline(&architect), Some(Outline::Freehand));
    }

    /// One sample is a tap, not a stroke: no direction, so no outline to fill, and the
    /// painter draws the dot itself (`src/freehand.rs:70-73`). Pinned here as well as in
    /// `outline` above because it is the same boundary from the element's side — a tapped
    /// pencil has nothing for the selection to trace.
    #[test]
    fn a_tapped_pencil_has_no_stroke_outline() {
        let tapped = stroke(vec![[10.0, 10.0]]);
        assert_eq!(element_outline(&tapped), None);
        assert!(freehand::stroke_outline(tapped.points.as_ref().unwrap(), 8.0, 0.5).is_empty());
    }
}
