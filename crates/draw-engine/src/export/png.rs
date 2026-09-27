//! Framing a whole-scene raster export: the box the picture is cut to, the canvas it is
//! cut into, and the camera that puts the scene in it.
//!
//! The numbers here are Excalidraw's, and each is cited at the line it came from. What this
//! module deliberately does **not** do is paint: rasterising needs a browser, and that half
//! lives in `crate::wasm::export`. What lives here is the part that has to be right for the
//! picture to be the right size, which is the part that can be tested without one.

use std::collections::HashSet;

use crate::camera::{Camera, WorldBounds};
use crate::scene::element::DrawElement;
use crate::scene::geometry::scene_outline_bounds;
use crate::scene::is_frame;

/// Excalidraw's `DEFAULT_EXPORT_PADDING`
/// (`packages/common/src/constants.ts@1118751f:398`), in world units, added to every side.
pub const DEFAULT_EXPORT_PADDING: f64 = 10.0;

/// One export's target: the scene cut to its own bounds, at a chosen scale.
///
/// The oracle's [`exportToCanvas`](packages/excalidraw/scene/export.ts@1118751f:180-284)
/// carries the same four numbers around — `minX`, `minY`, `width`, `height` from
/// `getCanvasSize`, then `scale` — and this is them named.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportFrame {
    /// The scene's own bounds: the union of what each element *draws*, turned ones
    /// included, padding not included.
    pub bounds: WorldBounds,
    /// The padded box, in CSS pixels. What the export measures before it is scaled.
    pub width: f64,
    pub height: f64,
    /// Device pixels per CSS pixel. What the export is asked for, not clamped to a set:
    /// `getExportSize` multiplies by whatever `exportScale` holds
    /// (`export.ts@1118751f:582-584`), and the oracle's own default is the device pixel
    /// ratio when that happens to be one of its three and 1 otherwise
    /// (`appState.ts@1118751f:20-22`).
    pub scale: f64,
    padding: f64,
}

impl ExportFrame {
    /// The whole scene as one export target — `getCanvasSize` over
    /// `getRootElements(elementsForRender)`, which is what the oracle measures
    /// (`export.ts@1118751f:232-235, 566-575`).
    ///
    /// Generic over the iterable so the caller can hand it the scene's own borrow: the
    /// render path measures without cloning the scene, and so does this.
    pub fn for_scene<'a>(
        elements: impl IntoIterator<Item = &'a DrawElement> + Clone,
        padding: f64,
        scale: f64,
    ) -> Self {
        let bounds = root_bounds(elements);
        Self {
            bounds,
            // `distance(minX, maxX) + exportPadding * 2` — the same sum the SVG exporter
            // makes (`export/svg.rs`), and the same one the oracle makes for both formats.
            width: bounds.max_x - bounds.min_x + padding * 2.0,
            height: bounds.max_y - bounds.min_y + padding * 2.0,
            scale,
            padding,
        }
    }

    /// The canvas's width in device pixels.
    ///
    /// `Math.trunc(dimension * scale)` (`export.ts@1118751f:582-584`). Truncating is not a
    /// detail: a canvas holds a whole number of device pixels and silently rounds a
    /// fractional assignment, and `getExportSize` truncates rather than rounds so the
    /// number it reports is the number the canvas will have.
    pub fn pixel_width(&self) -> u32 {
        (self.width * self.scale).trunc() as u32
    }

    /// The canvas's height in device pixels. See [`Self::pixel_width`].
    pub fn pixel_height(&self) -> u32 {
        (self.height * self.scale).trunc() as u32
    }

    /// The camera that puts the scene's top-left corner `padding` in from the canvas's own.
    ///
    /// `scrollX = -minX + exportPadding` at zoom 1 (`export.ts@1118751f:264-266`) is how
    /// the oracle shifts the scene into the export. The scale is deliberately **not** in
    /// here: the oracle hands `scale` to the renderer as the factor it draws at and leaves
    /// `zoom` at its default (`export.ts@1118751f:259, 266`), so on this engine the scale
    /// is the device ratio and the camera stays at 1.
    pub fn camera(&self) -> Camera {
        Camera {
            x: self.padding - self.bounds.min_x,
            y: self.padding - self.bounds.min_y,
            scale: 1.0,
        }
    }
}

/// The bounds the export is measured against: every live element that is not inside a frame.
///
/// `getCommonBounds` over `getRootElements` (`export.ts@1118751f:232-235`), where
/// `getRootElements` keeps an element unless a frame in the scene holds it
/// (`packages/element/src/frame.ts@1118751f:271-281`). A frame is a window onto a region,
/// so what is behind the glass is already inside the frame's box — and a child that pokes
/// out past the edge is cropped, which is what the oracle does too.
///
/// Empty is `[0, 0, 0, 0]` rather than nothing (`bounds.ts@1118751f:1009-1011`), so an
/// empty board exports the padding alone instead of a canvas with no size in it.
fn root_bounds<'a>(elements: impl IntoIterator<Item = &'a DrawElement> + Clone) -> WorldBounds {
    let frames: HashSet<&str> = elements
        .clone()
        .into_iter()
        .filter(|element| is_frame(element) && !element.is_deleted)
        .map(|element| element.id.as_str())
        .collect();
    scene_outline_bounds(elements.into_iter().filter(|element| {
        element
            .frame_id
            .as_deref()
            .is_none_or(|holder| !frames.contains(holder))
    }))
    .unwrap_or(WorldBounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 0.0,
        max_y: 0.0,
    })
}
