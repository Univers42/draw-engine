use std::collections::{HashMap, HashSet};

use crate::camera::{Camera, Point, WorldBounds};
use crate::interaction::DrawTool;
use crate::scene::DrawElementStylePatch;
use crate::selection::HandleKind;

#[derive(Clone, Debug, serde::Serialize)]
pub struct TextEditRequest {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
    pub color: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// **Resolved**, not the raw field: the overlay has to draw the text where the
    /// canvas will, and an unset element still has an alignment to honour. Sending
    /// `None` and letting the host guess is how the editor and the canvas end up
    /// disagreeing — the same failure the measured `width` above exists to prevent,
    /// where text visibly jumped the moment an edit was committed.
    pub text_align: crate::scene::TextAlign,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
}

/// Something the person should be told, as a code rather than a sentence.
///
/// A code because the wording is the host's half of the problem: it is the half that
/// knows the language, the tone and how much room there is on screen. The motor knows
/// only *what* happened, and every frontend built on it should be free to say it
/// differently — or, in a headless one, not at all.
///
/// Emitted only where silence would read as a bug. A gesture that does nothing because
/// there was nothing to do stays silent; one that does nothing because it could not do
/// what was asked says why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Notice {
    /// There is a shape under the bucket, but its outline does not close around the
    /// click, so there is no region to paint. Worth saying: the rule that a region must
    /// be enclosed by *visible* strokes is not discoverable by looking.
    FillRegionNotClosed,
    /// Too much geometry near the click to resolve a region in the time a click may take.
    FillRegionTooComplex,
}

#[derive(Clone, Debug, Default)]
pub struct EngineEvents {
    /// Only the elements that changed. Preferred over `scene_json`.
    pub scene_delta: Option<crate::scene::store::SceneDelta>,
    pub camera: Option<Camera>,
    pub tool: Option<DrawTool>,
    pub selection: Option<Vec<String>>,
    pub text_edit: Option<TextEditRequest>,
    pub scene_json: Option<String>,
    /// Something to tell the person about, if anything.
    pub notice: Option<Notice>,
}

pub(crate) enum Interaction {
    Draft {
        id: String,
        start: Point,
    },
    /// A text gesture in progress, before it is known to be a click or a drag.
    ///
    /// Separate from `Draft` because the two end differently: a drafted shape that is
    /// too small is thrown away, whereas a text gesture that is too small is a *click*,
    /// which is the commonest way to make text and must not be discarded.
    TextDraft {
        id: String,
        start: Point,
    },
    Linear {
        id: String,
        start: Point,
    },
    Freedraw {
        id: String,
        start: Point,
    },
    Erase {
        /// Where the last sample landed, so the next one erases the segment between them
        /// rather than a point. Frames are coalesced, so that segment is most of the
        /// gesture.
        last: Point,
    },
    Pan {
        last_x: f64,
        last_y: f64,
    },
    Move {
        ids: Vec<String>,
        start: Point,
        origins: HashMap<String, Point>,
        static_bounds: Vec<WorldBounds>,
    },
    Resize {
        id: String,
        handle: HandleKind,
        ratio: Option<f64>,
        /// The element's geometry when the drag began.
        ///
        /// Resizing reads from this, never from the live element. The anchor is the
        /// corner opposite the handle, and deriving it from the *current* geometry works
        /// only while the element stays the right way round: once a drag carries the
        /// pointer past the anchor and the element turns through, "the corner opposite
        /// the handle" names the far side of the new box, so the anchor walks along with
        /// the pointer and the element can never grow past it. Holding the original also
        /// keeps a long drag free of accumulated rounding.
        origin: crate::selection::Geometry,
        /// The ring as it was when the drag began, for a point-based element.
        ///
        /// A shape's geometry is generated from its box, so moving the box is the whole
        /// resize. A line's geometry *is* its points, so the box alone changes nothing
        /// that is drawn — which is why this went unnoticed for as long as point-based
        /// elements had no box to drag. Held for the same reason as `origin`: scaling the
        /// live points every move would compound, and a drag that passes through zero
        /// would collapse the ring and never recover it.
        origin_points: Option<Vec<[f64; 2]>>,
    },
    Rotate {
        id: String,
    },
    /// Resizing a multi-element selection within its frame.
    ///
    /// Carries the frame captured at the start of the gesture rather than recomputing
    /// it per move: recomputing from the elements as they change compounds rounding on
    /// every pointer event, and the group slowly drifts.
    ResizeGroup {
        ids: Vec<String>,
        handle: HandleKind,
        frame: crate::selection::GroupFrame,
    },
    /// Rotating a multi-element selection about its centre.
    RotateGroup {
        ids: Vec<String>,
        frame: crate::selection::GroupFrame,
    },
    /// Dragging one point of a line or arrow.
    ///
    /// Separate from `Resize` because a linear element is edited by its points, not by
    /// its bounding box: scaling a box cannot express "point this end somewhere else",
    /// and for a dead-horizontal or dead-vertical element the box is degenerate, so
    /// every box handle collapses onto the same line.
    LinearPoint {
        id: String,
        handle: crate::selection::LinearHandle,
    },
    /// A free-form selection loop in progress.
    ///
    /// The path is world-space and accumulates as the pointer moves; the engine
    /// simplifies it when deciding what it selects, so the raw trail is kept here and
    /// the host never has to thin it.
    Lasso {
        path: Vec<crate::camera::Point>,
        base: std::collections::HashSet<String>,
    },
    /// A laser stroke in progress.
    ///
    /// Carries nothing: the trail itself lives on the engine, because it has to outlive
    /// the gesture. Releasing the pointer ends the stroke but not the fade, and a second
    /// flick can start while the first is still on screen.
    Laser,
    Marquee {
        start: Point,
        current: Point,
        base: HashSet<String>,
    },
    /// Dragging a corner-radius handle.
    ///
    /// Holds the radius and the pointer position at the grab, and every move is measured
    /// from those — never from the previous move, so the drag cannot compound.
    CornerRadius {
        id: String,
        /// Which handle, clockwise from the top-left. Each measures inward from its own
        /// corner, though all four drive the one radius.
        corner: usize,
        start_radius: f64,
        grab: Point,
    },
    /// A press while a path is being placed point by point.
    ///
    /// Carries nothing, because the path itself is not part of the gesture: it lives on
    /// the engine and outlives every click that extends it. The variant exists so the
    /// *release* has somewhere to land — without it the press would fall through to the
    /// marquee arm, which would drop the selection and rubber-band across the drawing.
    MultiLinearPress,
}

/// A line or arrow being placed point by point, between the clicks that place them.
///
/// Held on the engine rather than in [`Interaction`] because it is the one gesture that
/// spans several of them: press, release, move, press, release, and only then — perhaps
/// a dozen clicks later — an end.
#[derive(Clone, Debug)]
pub(crate) struct MultiLinear {
    pub id: String,
    /// How many of the element's points have been placed.
    ///
    /// The path also carries at most one *preview* point, which follows the cursor and
    /// sits at exactly this index. So `points.len() == committed` means the cursor is
    /// resting inside the last point's commit zone with nothing pending, and
    /// `points.len() == committed + 1` means a segment is being aimed. Counting rather
    /// than holding the point itself is what makes "throw the preview away" a truncate,
    /// with no chance of dropping a point somebody placed.
    pub committed: usize,
}

/// The measurement used when no browser is available — host tests, and a server-side
/// render.
///
/// An estimate, unavoidably, but over **characters** rather than bytes. `str::len()` is
/// the UTF-8 byte length, which made "café" a fifth too wide, "日本語" three times too
/// wide and every emoji four times too wide. In the browser this is replaced by a real
/// `measureText` against the font the painter draws with.
pub(crate) fn default_measure(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.chars().count() as f64 * font_size * 0.6)
        .fold(0.0_f64, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * crate::TEXT_LINE_HEIGHT).max(font_size),
    )
}

pub fn merge_style_patch(
    base: &DrawElementStylePatch,
    patch: &DrawElementStylePatch,
) -> DrawElementStylePatch {
    DrawElementStylePatch {
        stroke_color: patch
            .stroke_color
            .clone()
            .or_else(|| base.stroke_color.clone()),
        background_color: patch
            .background_color
            .clone()
            .or_else(|| base.background_color.clone()),
        fill_style: patch.fill_style.or(base.fill_style),
        stroke_width: patch.stroke_width.or(base.stroke_width),
        stroke_style: patch.stroke_style.or(base.stroke_style),
        roughness: patch.roughness.or(base.roughness),
        opacity: patch.opacity.or(base.opacity),
        roundness: patch.roundness.or(base.roundness),
    }
}
