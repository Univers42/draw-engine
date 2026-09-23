//! Dragging a corner-radius handle.
//!
//! The geometry lives in [`crate::selection::radius`]; this is the gesture around it —
//! which element offers handles, which one a press lands on, and what a drag writes.

use crate::camera::Point;
use crate::engine::{DrawEngine, Interaction};
use crate::scene::DrawElement;
use crate::selection::radius::{
    current_radius, radius_after_drag, radius_handles, supports_corner_radius,
};

/// How far inside its corner a handle sits at minimum, in screen pixels.
///
/// On a sharp or barely-rounded corner "at the radius" would put the handle on the
/// corner itself, under the resize handle. This keeps it clear and grabbable.
const RADIUS_INSET_PX: f64 = 12.0;

/// A rectangle whose short side is less than this on screen offers no radius handles.
///
/// Four handles inside a shape a few pixels across would cover it and fight the resize
/// handles for every press. Screen pixels, so zooming in brings them back.
const RADIUS_HANDLES_MIN_PX: f64 = 48.0;

/// Reach of a radius handle. Smaller than the resize handles', which sit outside the
/// shape: this one sits *inside* a filled shape, where a near-miss has to fall through to
/// moving it rather than be captured.
const RADIUS_HIT_PX: f64 = 8.0;

/// A radius at or below this is a sharp corner. Half a pixel is invisible at any zoom
/// anyone draws at, and "Round, radius zero" would leave the panel saying Round.
const SHARP_BELOW: f64 = 0.5;

impl DrawEngine {
    /// The radius handles on offer, in world space, or none.
    ///
    /// Only for exactly one unlocked rectangle, large enough on screen to hold them.
    pub(crate) fn radius_handles(&self) -> Vec<Point> {
        let Some(element) = self.single_selected() else {
            return Vec::new();
        };
        if element.locked() || !supports_corner_radius(&element) {
            return Vec::new();
        }
        let short = element.width.abs().min(element.height.abs()) * self.camera.scale;
        if short < RADIUS_HANDLES_MIN_PX {
            return Vec::new();
        }
        radius_handles(&element, RADIUS_INSET_PX / self.camera.scale).to_vec()
    }

    /// Which radius handle, if any, sits under a world point — by corner index, clockwise
    /// from the top-left.
    pub(crate) fn radius_handle_at(&self, world: Point) -> Option<usize> {
        let reach = RADIUS_HIT_PX / self.camera.scale;
        self.radius_handles()
            .iter()
            .position(|h| (h.x - world.x).hypot(h.y - world.y) <= reach)
    }

    pub(crate) fn begin_corner_radius(
        &mut self,
        element: &DrawElement,
        corner: usize,
        world: Point,
    ) {
        self.interaction = Some(Interaction::CornerRadius {
            id: element.id.clone(),
            corner,
            start_radius: current_radius(element),
            grab: world,
        });
        self.request_draw();
    }

    /// Writes the radius a drag to `world` implies.
    ///
    /// Recomputed from the gesture's starting radius every move, never from the last one,
    /// so the drag cannot compound — the class of bug the group resize had.
    pub(crate) fn move_corner_radius(
        &mut self,
        id: &str,
        corner: usize,
        start_radius: f64,
        grab: Point,
        world: Point,
    ) {
        let Some(mut element) = self.scene.get(id).cloned() else {
            return;
        };
        let radius = radius_after_drag(&element, corner, start_radius, grab, world);
        if radius <= SHARP_BELOW {
            // Sharp, and the panel should say so. The stored radius goes too, so that
            // choosing Round afterwards gives the ordinary corner rather than a zero one.
            element.roundness = None;
            element.corner_radius = None;
        } else {
            element.roundness = element
                .roundness
                .or(crate::scene::default_element_style().roundness)
                .or(Some(8.0));
            element.corner_radius = Some(radius);
        }
        self.scene.put(element);
        self.request_draw();
    }

    /// Commits a radius drag: one version bump, one history entry.
    ///
    /// The bump is not bookkeeping. The history deduplicates on a signature of id,
    /// `version` and geometry — and a radius changes none of the geometry — so without it
    /// the change was indistinguishable from the snapshot before it and was dropped from
    /// history entirely, leaving nothing for undo to take back. The same number is what
    /// the server's merge ranks concurrent edits by, so an unbumped edit would also lose
    /// to a collaborator's stale copy.
    ///
    /// Once, on release, rather than per move: a drag is one edit, and a version that
    /// climbed by sixty over one gesture would say otherwise.
    pub(crate) fn end_corner_radius(&mut self, id: &str) {
        if let Some(element) = self.scene.get(id).cloned() {
            self.scene
                .put(crate::scene::bump_version(element, self.now_ms));
        }
        self.push_history();
        self.request_draw();
    }
}
