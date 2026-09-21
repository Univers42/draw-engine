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
            next.fill_style = style.fill_style;
            let id = next.id.clone();
            self.push_history();
            self.scene.put(bump_version(next, self.now_ms));
            self.request_draw();
            return Ok(id);
        }

        let element = self.fill_element(&fill);
        let id = element.id.clone();
        self.push_history();
        self.scene.put(element);
        self.place_fill(&id, &fill);
        self.select(vec![id.clone()]);
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
        let origin = fill.scene_points[0];
        let mut element = create_element_default(
            DrawElementType::Line,
            Geometry {
                x: origin.x,
                y: origin.y,
                width: 0.0,
                height: 0.0,
            },
        );
        element.points = Some(
            fill.scene_points
                .iter()
                .map(|p| [p.x - origin.x, p.y - origin.y])
                .collect(),
        );
        let style = self.get_next_style();
        element.background_color = fill_color(&style.background_color);
        element.fill_style = match style.fill_style {
            // A fill is paint. Asking for a hatched one would leave the region looking
            // half-filled, which is never what a bucket is for.
            FillStyle::Hachure | FillStyle::CrossHatch => FillStyle::Solid,
            other => other,
        };
        // No stroke: the region is bounded by strokes that are already there, and drawing
        // another one along the ring would double every line it was derived from.
        element.stroke_color = "transparent".into();
        element
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

/// Default paint for the bucket, for when nothing else is chosen.
///
/// Excalidraw's first background swatch.
pub const DEFAULT_BUCKET_FILL_COLOR: &str = "#a5d8ff";

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
