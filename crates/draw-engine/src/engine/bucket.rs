use crate::engine::types::Notice;
use crate::engine::DrawEngine;
use crate::scene::is_transparent;
use crate::scene::{
    bump_version, compute_bucket_fill, create_element_default, is_restylable_fill, BucketFill,
    BucketFillFailure, BucketFillOptions, DrawElement, DrawElementType, FillStyle, Geometry,
    Placement,
};

impl DrawEngine {
    /// Fill the region under a click.
    ///
    /// Returns `Ok(id)` with the element that now paints the region — a fresh polygon, or
    /// an existing one recoloured in place when the same region is clicked twice. The
    /// geometry is entirely `scene::bucket_fill`'s; this is only the part that turns a
    /// polygon into an element and puts it in the right place in the stack.
    pub fn bucket_fill_at(&mut self, sx: f64, sy: f64) -> Result<String, BucketFillFailure> {
        let point = self.screen_to_world(sx, sy);
        let options = BucketFillOptions::default();

        let live: Vec<DrawElement> = self
            .scene
            .ordered_cloned()
            .into_iter()
            .filter(|e| !e.is_deleted)
            .collect();
        let refs: Vec<&DrawElement> = live.iter().collect();
        let fill = compute_bucket_fill(point, &refs, &options)?;

        // Clicking a region that is already painted recolours that paint rather than
        // stacking an identical polygon on top of it — otherwise every click leaves
        // another invisible element behind and the scene grows without bound.
        if let Some(existing) = live
            .iter()
            .rev()
            .find(|e| is_restylable_fill(e, &fill.scene_points))
        {
            let mut next = existing.clone();
            let style = self.get_next_style();
            next.background_color = fill_color(&style.background_color);
            next.fill_style = paint_fill_style(style.fill_style);
            let id = next.id.clone();
            self.scene.put(bump_version(next, self.now_ms));
            self.push_history();
            self.request_draw();
            return Ok(id);
        }

        let element = self.fill_element(&fill);
        let id = element.id.clone();
        self.scene.put(element);
        self.place_fill(&id, &fill);
        // After the scene is final, never before. `push_history` both snapshots and tells
        // the host what changed, so calling it first published the scene *without* the
        // fill in it — the paint appeared on the canvas, the autosaver was handed a scene
        // that did not contain it, and the fill was gone on the next load. It also left
        // the history top one state behind, so an undo went back too far.
        self.push_history();
        // Deliberately nothing selected. The tool stays active so that regions can be
        // painted one after another, and a selection left on the last one puts handles
        // over the paint, takes the next Delete, and keeps the overlay non-empty so every
        // frame has to be composited again. Excalidraw leaves nothing selected for the
        // same reason (`App.bucketFill.ts:281-284`).
        //
        // This used to select the fill, and that was load-bearing for the wrong reason:
        // the style panel offered nothing while the bucket was the active tool, so the
        // selection was the only thing that brought the colour swatches back — *after*
        // the first fill had already been painted in a colour nobody chose.
        self.request_draw();
        Ok(id)
    }

    /// Turn the computed polygon into a line element carrying the region as its points.
    ///
    /// A line rather than a new element type on purpose: a closed line with a background
    /// and no stroke already renders as exactly this, so the fill moves, scales, exports
    /// and serialises like anything else on the board, with no schema change and nothing
    /// for another frontend to special-case.
    fn fill_element(&self, fill: &BucketFill) -> DrawElement {
        // The origin is the ring's *first* point and deliberately not its top-left corner:
        // a point-based element is anchored so that `points[0]` is `[0, 0]`, so the rest
        // of the ring may well be negative. Excalidraw anchors it the same way and does
        // not normalise either (`App.bucketFill.ts:216-222`).
        let origin = fill.scene_points[0];
        let points: Vec<[f64; 2]> = fill
            .scene_points
            .iter()
            .map(|p| [p.x - origin.x, p.y - origin.y])
            .collect();
        // The extent the ring actually spans. This used to be hard-coded to zero, and the
        // engine did not notice because `local_box` derives a point-based element's box
        // from its points and ignores these two numbers — but everything that cannot see
        // the points has only these to go on. The API has no WASM, so its bounds mirror
        // read a fill as a zero-area box and framed board thumbnails on it.
        let (width, height) = span_of(&points);
        let mut element = create_element_default(
            DrawElementType::Line,
            Geometry {
                x: origin.x,
                y: origin.y,
                width,
                height,
            },
        );
        element.points = Some(points);
        let style = self.get_next_style();
        element.background_color = fill_color(&style.background_color);
        element.fill_style = paint_fill_style(style.fill_style);
        // No stroke: the region is bounded by strokes that are already there, and drawing
        // another one along the ring would double every line it was derived from.
        element.stroke_color = "transparent".into();
        // Sharp corners. Every vertex of this ring is a real junction between two strokes
        // that were traced to find it, so rounding them pulls the paint away from the
        // outline it was derived from — visibly, at exactly the corners the eye checks.
        //
        // It also decides how the renderer builds the shape: with a roundness a line is
        // drawn as a curve, and rough has no pattern fill for a curve at all.
        element.roundness = None;
        // Paint follows the region it was traced from, exactly. Roughness would wobble the
        // ring away from the strokes that bound it, at precisely the edges the eye checks,
        // and the ring is already a faithful trace of those strokes.
        // `App.bucketFill.ts:245-265` sets the same.
        element.roughness = 0.0;
        element.stroke_width = 1.0;
        self.inherit_belonging(&mut element, fill);
        element
    }

    /// The two things a fill inherits from what it was painted inside.
    ///
    /// Nothing links a fill back to the shape it was traced from — no back-reference, no
    /// marker — because a region is frequently not a shape at all: the overlap of two
    /// rectangles, an area walled in by four loose lines, a ring with a hole through it.
    /// None of those can be written as some element's background, so the paint has to be
    /// its own element and moving the shape leaves it behind. That is the design, here
    /// and upstream.
    ///
    /// Frame and group are the exceptions, and they are *membership* rather than a link:
    /// the paint joins whatever its region already belonged to. That is what makes a fill
    /// travel with a frame that is dragged, and with a group that is moved — without it,
    /// paint inside a frame is abandoned the instant the frame moves, which reads as the
    /// colour coming unstuck from the drawing. Excalidraw inherits these two and nothing
    /// else (`App.bucketFill.ts:227-243`).
    fn inherit_belonging(&self, element: &mut DrawElement, fill: &BucketFill) {
        if let Some(owner) = fill.owner_id.as_ref().and_then(|id| self.scene.get(id)) {
            // A frame is a container, so paint filling one belongs *inside* it rather
            // than beside it. Anything else passes on the frame it is itself in.
            element.frame_id = if crate::scene::is_frame(owner) {
                Some(owner.id.clone())
            } else {
                owner.frame_id.clone()
            };
            element.group_id = owner.group_id.clone();
            return;
        }

        // No owner: the region is held up by loose walls, so it belongs wherever *all* of
        // them agree it does. Anything less than unanimous means there is no one place
        // the region sits, and guessing would drag a stranger's shape along with the
        // paint every time the group moved.
        element.group_id = shared_among(
            fill.boundary_element_ids
                .iter()
                .map(|id| self.scene.get(id).and_then(|el| el.group_id.clone())),
        );
        // The frame is asked of the finished ring rather than of the walls, because a
        // frame holds whatever is drawn within its bounds — which is the same question
        // asked of every other element the moment it is created.
        element.frame_id = crate::scene::frame_for_element(self.scene.iter_ordered(), element);
    }

    /// Say why a click painted nothing — when it is worth saying.
    ///
    /// A click on bare canvas is deliberately silent, and so is one whose fallback search
    /// found nothing: aiming at empty space is not a mistake worth interrupting someone
    /// over, and a tool that complains every time the pointer slips is one people stop
    /// reading. Excalidraw draws the line in the same place (`App.bucketFill.ts:197`).
    ///
    /// Everything else means the person aimed at something and it did not work, which is
    /// the case where silence reads as a broken tool.
    pub(crate) fn report_fill_failure(&mut self, failure: BucketFillFailure) {
        self.events.notice = match failure {
            BucketFillFailure::NoOwner => None,
            BucketFillFailure::TooComplex => Some(Notice::FillRegionTooComplex),
            // All three mean the same thing to the person clicking: there is no region
            // here to paint. Which stage refused is a fact about the algorithm.
            BucketFillFailure::OpenRegion
            | BucketFillFailure::TooSmall
            | BucketFillFailure::InvalidPolygon => Some(Notice::FillRegionNotClosed),
        };
    }

    /// Move the new fill to where the geometry said it belongs in the stack.
    fn place_fill(&mut self, id: &str, fill: &BucketFill) {
        let mut order = self.scene.ordered_cloned();
        let Some(from) = order.iter().position(|e| e.id == id) else {
            return;
        };
        let element = order.remove(from);
        let Some(anchor) = order.iter().position(|e| e.id == fill.insertion.element_id) else {
            // The anchor is gone, which should not happen — a face always has
            // contributors. Leave the fill on top rather than dropping it.
            order.push(element);
            self.scene.set_order(order);
            return;
        };
        let target = match fill.insertion.placement {
            Placement::Above => anchor + 1,
            Placement::Below => anchor,
        };
        order.insert(target.min(order.len()), element);
        self.scene.set_order(order);
    }
}

/// The fill style a bucket actually paints with.
///
/// A bucket is paint. A hatched one would leave the region looking half-filled, which is
/// never what the tool is for, so hachure and cross-hatch become solid whatever the
/// current style says.
///
/// Shared by both paths on purpose. This rule used to be spelled out only where a fill is
/// created, and the restyle branch assigned the raw style instead — and since the default
/// style *is* hachure, a second click on a painted region turned its paint into a
/// pattern-filled curve, which rough has no implementation for. The module aborted and
/// the board stopped responding until the page was reloaded.
fn paint_fill_style(style: FillStyle) -> FillStyle {
    match style {
        FillStyle::Hachure | FillStyle::CrossHatch => FillStyle::Solid,
        other => other,
    }
}

/// The one value every item agrees on, or `None`.
///
/// Unanimity rather than a majority or a first-wins: the question is "is there a single
/// place this region sits", and anything short of every wall saying the same place means
/// there is not one.
fn shared_among(values: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let mut items = values.into_iter();
    let first = items.next().flatten()?;
    for value in items {
        if value.as_deref() != Some(first.as_str()) {
            return None;
        }
    }
    Some(first)
}

/// The box a ring of points spans, as a width and a height.
///
/// Signed extents are not a thing here: a ring has no drag direction to remember, so the
/// span is always positive and `normalize_rect` has nothing to undo.
fn span_of(points: &[[f64; 2]]) -> (f64, f64) {
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    if !min_x.is_finite() {
        return (0.0, 0.0);
    }
    (max_x - min_x, max_y - min_y)
}

/// Default paint for the bucket, for when nothing else is chosen.
///
/// Excalidraw's green swatch, which is what their bucket falls back to when the shared
/// background colour is transparent (`App.bucketFill.ts:293-300`) — not the first swatch
/// in the picker, which is what this used to be.
pub const DEFAULT_BUCKET_FILL_COLOR: &str = "#b2f2bb";

/// The colour a fill should actually paint with.
///
/// The current background is used when there is one, but it starts out transparent — and
/// a bucket that produces invisible paint reads as a tool that silently did nothing. The
/// element is there, selected, and undoable; it just cannot be seen, which is the worst
/// of the available failures.
fn fill_color(background: &str) -> String {
    if is_transparent(background) {
        DEFAULT_BUCKET_FILL_COLOR.to_string()
    } else {
        background.to_string()
    }
}
