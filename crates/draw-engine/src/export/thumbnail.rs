//! The thumbnail export: the whole scene as a PNG whose longest side is at most
//! [`THUMBNAIL_MAX_SIDE`] pixels, with no scene embedded.
//!
//! Lives apart from `png.rs` so its line numbers stay where the hosts that cite them
//! expect. The scale is decided here, in Rust, because the web app may not do arithmetic
//! on scene sizes (BUNNY.md §2).
//!
//! Caveat: one uniform scale cannot give a very long sliver both a capped long side and a
//! visible short one. The short side is then given a blank strip of one pixel instead of
//! vanishing, so such a thumbnail is a little taller or wider than the scene is.

use super::png::{ExportFrame, ExportOptions};

/// The longest side a thumbnail may have, in pixels.
pub const THUMBNAIL_MAX_SIDE: f64 = 480.0;

/// The factor that brings the longer of `w` and `h` down to `max`, never above 1.
///
/// 1 for any side that is not a positive finite number, so a broken frame is left as it
/// is rather than turned into a NaN canvas.
pub fn thumbnail_scale(w: f64, h: f64, max: f64) -> f64 {
    let sound = |side: f64| side.is_finite() && side > 0.0;
    if !sound(w) || !sound(h) || !sound(max) {
        return 1.0;
    }
    (max / w.max(h)).min(1.0)
}

/// What a thumbnail is exported with: scale 1 (the scale is applied to the frame by
/// [`fit_thumbnail`]) and, unlike the PNG default, no embedded scene.
pub fn thumbnail_options(dark_mode: bool) -> ExportOptions {
    ExportOptions {
        dark_mode,
        embed_scene: false,
        ..Default::default()
    }
}

/// `frame` scaled to the thumbnail cap, with each side at least one pixel.
///
/// A side that truncates to zero pixels is widened in world units to just over one pixel;
/// the extra is blank paper on the far edge.
pub fn fit_thumbnail(mut frame: ExportFrame) -> ExportFrame {
    frame.scale = thumbnail_scale(frame.width, frame.height, THUMBNAIL_MAX_SIDE);
    // 1.5 px, not 1: `trunc(side * scale)` of an exact 1.0 can land on 0.999….
    let at_least_one_pixel = 1.5 / frame.scale;
    if is_under_a_pixel(frame.width * frame.scale) {
        frame.width = at_least_one_pixel;
    }
    if is_under_a_pixel(frame.height * frame.scale) {
        frame.height = at_least_one_pixel;
    }
    frame
}

fn is_under_a_pixel(side_in_pixels: f64) -> bool {
    !side_in_pixels.is_finite() || side_in_pixels < 1.0
}
