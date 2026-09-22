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
