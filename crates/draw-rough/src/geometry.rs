//! Transcription of roughjs 4.6.4 `bin/geometry.js`.

/// A `[start, end]` pair, rough's `Line`.
pub type Line = [[f64; 2]; 2];

/// `lineLength(line)`
///
/// The JS uses `Math.pow(d, 2)`; `d * d` is exactly equal for an exponent of 2 and
/// avoids depending on the engine's `pow` implementation.
pub fn line_length(line: Line) -> f64 {
    let p1 = line[0];
    let p2 = line[1];
    ((p1[0] - p2[0]) * (p1[0] - p2[0]) + (p1[1] - p2[1]) * (p1[1] - p2[1])).sqrt()
}
