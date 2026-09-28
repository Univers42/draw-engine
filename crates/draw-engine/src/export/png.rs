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

/// What an export is asked for. One type for both formats, because the oracle asks the
/// same six questions of a PNG and an SVG of the same drawing under the same six names —
/// `exportToCanvas` (`export.ts@1118751f:184-194`) and `exportToSvg`
/// (`export.ts@1118751f:293-305`) differ only in that the canvas is a raster.
///
/// **`scene_to_svg` does not take this yet.** Its `padding` and its always-drawn
/// background are 4.5's to move over, and until then this is where the policy lives for
/// the PNG path alone — which is still the point of putting it here rather than in the
/// painter: 4.2 and 4.5 change what is *in* this struct, not where they change it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportOptions {
    /// `exportPadding` — the oracle's 10 (`constants.ts@1118751f:398`), in world units.
    pub padding: f64,
    /// `exportScale` — device pixels per CSS pixel for the PNG
    /// (`export.ts@1118751f:199-203`), and the multiplier on the SVG root's `width` and
    /// `height` for the vector one (`export.ts@1118751f:358-359`).
    pub scale: f64,
    /// `exportBackground` — whether there is paper at all. False leaves the canvas as
    /// transparent as a fresh one already is, and drops the SVG's `<rect>`
    /// (`export.ts@1118751f:458`).
    pub background: bool,
    /// Whether a frame's name is drawn. The oracle gates this once, for both formats
    /// (`export.ts@1118751f:169-172`, inside `prepareElementsForRender`), so ours is one
    /// flag and not a per-format argument.
    pub frame_labels: bool,
    /// Whether the scene goes into the file with the picture — `appState.exportEmbedScene`
    /// (`export.ts@1118751f:378-391`, and the dialog's checkbox at
    /// `actionExport.tsx@1118751f:99-105`).
    ///
    /// **True here where the oracle's default is `false`**, and it is a deliberate
    /// difference: dropping a saved PNG back on the board has to restore the drawing, which
    /// is what `design.md:1352` asks for, and a picture that carries nothing is a lossy
    /// format wearing a `.png` name. What the oracle gates on is a *person's* choice to make
    /// a picture into a scene file, and we have no such picture: the lossless format is the
    /// `.osidraw` save, and the embed is the travel case.
    ///
    /// **False for a clipboard copy**, which is the oracle's own line rather than ours:
    /// `exportEmbedScene: appState.exportEmbedScene && type === "svg"`
    /// (`data/index.ts@1118751f:132`) is false for `type === "clipboard-svg"`.
    pub embed_scene: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            padding: DEFAULT_EXPORT_PADDING,
            // 1, not the device pixel ratio: `EXPORT_SCALES.includes(devicePixelRatio) ?
            // devicePixelRatio : 1` (`appState.ts@1118751f:20-22`) is a fact about the
            // browser the *host* can see and the engine cannot, and 1 is what that
            // expression answers on a dpr-1 machine anyway.
            scale: 1.0,
            background: true,
            frame_labels: true,
            embed_scene: true,
        }
    }
}

/// One export's target: the box the picture is cut to, and everything asked for about it.
///
/// The oracle's [`exportToCanvas`](packages/excalidraw/scene/export.ts@1118751f:180-284)
/// carries the same numbers around — `minX`, `minY`, `width`, `height` from
/// `getCanvasSize`, then `scale` — and this is them named. Built by [`Self::for_bounds`],
/// which takes bounds rather than elements, so a selection or a single frame frames
/// itself the same way a whole scene does.
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
    /// The margin, kept on the frame so the camera that consumes it is one call rather
    /// than a pair. Public because 4.2 reads it to place its own frame export, which the
    /// oracle does at `exportPadding = 0` (`export.ts@1118751f:228-230`).
    pub padding: f64,
    /// Whether a frame's name is drawn. See [`ExportOptions::frame_labels`].
    pub frame_labels: bool,
    /// Whether what a frame holds is clipped to the frame.
    ///
    /// False for an export *of* a frame, and the oracle says why in a comment rather than
    /// in a flag: "for canvas export, don't clip if exporting a specific frame as it would
    /// clip the corners of the content" (`export.ts@1118751f:217-219`). Everything else
    /// clips, which is what crops a child poking out past its frame's edge.
    pub frame_clip: bool,
}

impl ExportFrame {
    /// A target of `bounds` — the whole scene's, a selection's, or one frame's. The one
    /// constructor: everything else is a question of *which* bounds.
    ///
    /// `distance(minX, maxX) + exportPadding * 2` — the same sum the SVG exporter makes
    /// (`export/svg.rs`), and the same one the oracle makes for both formats
    /// (`export.ts@1118751f:571-572`).
    pub fn for_bounds(bounds: WorldBounds, options: &ExportOptions) -> Self {
        Self {
            bounds,
            width: bounds.max_x - bounds.min_x + options.padding * 2.0,
            height: bounds.max_y - bounds.min_y + options.padding * 2.0,
            scale: options.scale,
            padding: options.padding,
            frame_labels: options.frame_labels,
            // True here, and false only where a caller has said it is exporting a specific
            // frame — the one question `for_bounds` is not asked, so it is not asked here.
            frame_clip: true,
        }
    }

    /// The export target for a set of elements: the same [`Self::for_bounds`], over
    /// `getRootElements(elements)`.
    ///
    /// The one the whole-scene arm is built from, and the one a selection is framed by.
    /// Exposing it rather than adding a second constructor is the point: there is exactly
    /// one place in the engine that turns a set of elements into an export's box, and
    /// `prepare_elements_for_export` (4.2) goes through here like `for_scene` does.
    pub fn for_elements<'a>(
        elements: impl IntoIterator<Item = &'a DrawElement> + Clone,
        options: &ExportOptions,
    ) -> Self {
        Self::for_bounds(root_bounds(elements), options)
    }

    /// The target for an export *of* a frame: the frame element's own box, at no padding,
    /// and not clipping.
    ///
    /// `exportingFrame ? [exportingFrame] : …` with `exportPadding = 0` in front of it
    /// (`export.ts@1118751f:228-233`, and `:337-342` for the SVG). So a frame export is
    /// measured by the frame and *not* by the union of what it holds — a child poking out
    /// past the edge is cropped, and that is the oracle's answer rather than an oversight.
    /// The frame's own turned box, because `getCommonBounds` folds `getElementBounds` and
    /// a frame is no exception (`bounds.ts@1118751f:210-235`).
    pub fn of_frame(frame: &DrawElement, options: &ExportOptions) -> Self {
        let mut target = Self::for_bounds(
            crate::scene::geometry::element_outline_bounds(frame),
            &ExportOptions {
                padding: 0.0,
                ..*options
            },
        );
        target.frame_clip = false;
        target
    }

    /// The whole scene as one export target — `getCanvasSize` over
    /// `getRootElements(elementsForRender)`, which is what the oracle measures
    /// (`export.ts@1118751f:232-235, 566-575`).
    ///
    /// Generic over the iterable so the caller can hand it the scene's own borrow: the
    /// render path measures without cloning the scene, and so does this.
    pub fn for_scene<'a>(
        elements: impl IntoIterator<Item = &'a DrawElement> + Clone,
        options: &ExportOptions,
    ) -> Self {
        Self::for_elements(elements, options)
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

    /// Whether this target is an export *of* a frame, which the oracle signals by handing
    /// `getCanvasSize` a one-element list instead of the roots
    /// (`export.ts@1118751f:233`, `:342`).
    ///
    /// Derived from the two facts that travel together — the frame's own box, and no padding
    /// — rather than stored as a third flag that could disagree with them.
    pub fn is_frame_export(&self) -> bool {
        self.padding == 0.0 && !self.frame_clip
    }

    /// The bounds of the export over `elements`: the union, measured the way
    /// `getCommonBounds` measures, with the same `getRootElements` filtering.
    ///
    /// The per-element form of [`ExportFrame::for_elements`], for a caller that is walking a
    /// scene one element at a time and needs each element's own contribution. Not a second
    /// framing path: it is the same function with one element in the list.
    pub fn bounds_of(&self, elements: &[&DrawElement]) -> WorldBounds {
        root_bounds(elements.iter().copied())
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
