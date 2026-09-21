use std::collections::{HashMap, HashSet};

use crate::camera::{Camera, Point, WorldBounds};
use crate::interaction::DrawTool;
use crate::scene::{DrawElement, DrawElementStylePatch};
use crate::selection::HandleKind;

#[derive(Clone, Debug, serde::Serialize)]
pub struct TextEditRequest {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
    pub color: String,
    pub text: String,
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
}

pub(crate) enum Interaction {
    Draft {
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
    Erase,
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
    Marquee {
        start: Point,
        current: Point,
        base: HashSet<String>,
    },
}

pub(crate) fn default_measure(text: &str, font_size: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.len() as f64 * font_size * 0.6)
        .fold(0.0_f64, f64::max);
    (
        width.max(4.0),
        (lines.len() as f64 * font_size * crate::TEXT_LINE_HEIGHT).max(font_size),
    )
}

/// A cheap fingerprint of the scene, used to skip history pushes that change nothing.
///
/// Hashes exactly the fields the previous string form encoded — id, version, rounded
/// geometry, angle and the tombstone flag — so the dedup behaviour is unchanged. What
/// is gone is the allocation: this used to build one `String` per element and join
/// them, which at 20k elements is several megabytes per keystroke-equivalent.
///
/// Coordinates are rounded before hashing, as before, so a sub-pixel jitter does not
/// create a history entry.
pub(crate) fn history_signature(elements: &[std::rc::Rc<DrawElement>]) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    elements.len().hash(&mut hasher);
    for el in elements {
        el.id.hash(&mut hasher);
        el.version.hash(&mut hasher);
        el.x.round().to_bits().hash(&mut hasher);
        el.y.round().to_bits().hash(&mut hasher);
        el.width.round().to_bits().hash(&mut hasher);
        el.height.round().to_bits().hash(&mut hasher);
        // The string form printed the angle to 3 decimals; quantise to match, so a
        // rotation smaller than a thousandth of a radian still does not record history.
        ((el.angle * 1000.0).round() as i64).hash(&mut hasher);
        el.is_deleted.hash(&mut hasher);
    }
    hasher.finish()
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
