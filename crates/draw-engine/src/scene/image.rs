//! Placing and sizing an image.
//!
//! Decoding a file is the host's job — only a browser knows how to turn a PNG into
//! pixels, and only it can report the natural size. Everything that follows from that
//! size is arithmetic, and lives here: how big the image should appear, where it lands
//! relative to the pointer, and how its proportions behave while it is resized.
//!
//! Transcribed from Excalidraw's `getImageNaturalDimensions` at the SHA pinned in
//! `scripts/oracle-sha.txt`.

use crate::camera::Point;
use crate::scene::element::{DrawElement, DrawElementType};

/// The most of the viewport's height an inserted image may take.
pub const IMAGE_VIEWPORT_FRACTION: f64 = 0.5;
/// The floor under the viewport allowance, so an image is still usable in a short window.
pub const IMAGE_MIN_VIEWPORT_HEIGHT: f64 = 160.0;
/// Room left around an inserted image, for the toolbars that sit over the canvas.
pub const IMAGE_VIEWPORT_MARGIN: f64 = 120.0;

pub fn is_image(element: &DrawElement) -> bool {
    element.kind == DrawElementType::Image
}

/// Where an image should sit, and how big it should be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageFit {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Size an image to the viewport and centre it on a point.
///
/// Never enlarged: a small icon is inserted at its own size rather than blown up to fill
/// the screen. What the viewport bounds is how *large* an image may arrive, because a
/// photograph dropped at its natural size can be several screens tall and lands as a wall
/// of pixels with its handles somewhere off-screen.
///
/// `viewport_height` is in screen pixels and `zoom` is the camera scale, so the division
/// converts the allowance into world units — an image dropped while zoomed out is
/// physically larger on the board, and still appears the same size on screen.
///
/// * `centre` — where the image should be centred, in world coordinates.
pub fn fit_image(
    natural_width: f64,
    natural_height: f64,
    viewport_height: f64,
    zoom: f64,
    centre: Point,
) -> ImageFit {
    // A zero or negative natural size means the host could not decode the file. Falling
    // through would divide by zero and produce an element with NaN bounds, which cannot
    // be selected, moved or deleted — an image you can only get rid of by clearing the
    // board.
    let measurable = natural_width.is_finite()
        && natural_height.is_finite()
        && natural_width > 0.0
        && natural_height > 0.0;
    if !measurable {
        return ImageFit {
            x: centre.x,
            y: centre.y,
            width: 0.0,
            height: 0.0,
        };
    }

    let zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom
    } else {
        1.0
    };
    let min_height = (viewport_height - IMAGE_VIEWPORT_MARGIN).max(IMAGE_MIN_VIEWPORT_HEIGHT);
    let max_height = min_height.min((viewport_height * IMAGE_VIEWPORT_FRACTION).floor() / zoom);

    let height = natural_height.min(max_height);
    let width = height * (natural_width / natural_height);

    ImageFit {
        x: centre.x - width / 2.0,
        y: centre.y - height / 2.0,
        width,
        height,
    }
}

/// Whether a resize should hold the element's proportions.
///
/// Excalidraw's rule, and their comment states it exactly: images are proportional by
/// default and Shift frees them, while every other shape is free by default and Shift
/// constrains it. A photograph stretched by accident is a mistake you often do not
/// notice until much later, which is why the safe behaviour is the default for images.
pub fn locks_aspect_ratio(element: &DrawElement, shift_held: bool) -> bool {
    if is_image(element) {
        !shift_held
    } else {
        shift_held
    }
}
