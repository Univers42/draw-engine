//! JavaScript number semantics that differ from Rust's.
//!
//! Each of these looks like it has an obvious Rust equivalent, and doesn't.

/// `Math.round(x)`.
///
/// JS rounds a half **toward +∞**; Rust's `f64::round` rounds a half **away from zero**.
/// They disagree on every negative half:
///
/// | x    | `Math.round` | `f64::round` |
/// |------|--------------|--------------|
/// | 0.5  | 1            | 1            |
/// | -0.5 | -0 (i.e. 0)  | -1           |
/// | -1.5 | -1           | -2           |
///
/// The hachure scanline rounds every line endpoint, and fills routinely sit at
/// negative coordinates, so using the wrong one shifts hatch lines by a whole pixel
/// on half the canvas.
#[inline]
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::js_round;

    #[test]
    fn rounds_halves_toward_positive_infinity_like_js() {
        assert_eq!(js_round(0.5), 1.0);
        assert_eq!(js_round(1.5), 2.0);
        assert_eq!(js_round(2.5), 3.0);
        // The cases where Rust's own `round` would disagree.
        assert_eq!(js_round(-0.5), 0.0);
        assert_eq!(js_round(-1.5), -1.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!((-0.5f64).round(), -1.0, "Rust still rounds away from zero");
    }

    #[test]
    fn agrees_with_rust_away_from_halves() {
        for x in [-3.7, -1.2, -0.1, 0.0, 0.1, 1.2, 3.7, 1e9, -1e9] {
            assert_eq!(js_round(x), x.round(), "disagreed at {x}");
        }
    }
}
