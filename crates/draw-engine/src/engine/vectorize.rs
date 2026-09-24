//! Vectorizing an image already on the board: its trace, put where the image is.
//!
//! The trace itself is made elsewhere — by `draw-trace`, in a Web Worker, from the
//! image's pixels — and arrives here as fractions of the traced picture. What is left
//! is everything that depends on the board: where those pixels are drawn (the image's
//! box, mirrored and turned exactly as the painter draws it), what the new elements look
//! like, which group and frame they join, where they sit in the stack, and what is
//! refused. One call is one commit, so one undo takes the whole trace away and gives the
//! original back. See `docs/reference/vectorize.md`.

use serde::Deserialize;

use crate::engine::DrawEngine;
use crate::scene::element::rand_int;
use crate::scene::geometry::{mirror_signs, rotation_center};
use crate::scene::{
    create_element, default_element_style, local_box, new_element_id, DrawElement, DrawElementType,
    FillStyle, Geometry,
};
use crate::selection::{rotate_point, simplify_path};
use crate::Point;

/// The editable half of a trace, flat, as draw-trace's `TraceSession.rings()` hands it
/// over: typed arrays, not JSON, because a detailed trace is hundreds of thousands of
/// numbers.
///
/// Ring `i` is `lengths[i]` points in colour `colours[i]` (`0xRRGGBB`), taken in turn
/// from `coords` as `x, y` pairs. Rings are in paint order, each closed, any holes
/// already spliced in as zero-width keyholes. A coordinate is a **fraction of the traced
/// picture** — `0, 0` its top-left corner, `1, 1` its bottom-right — so the size it was
/// traced at does not matter here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceRings {
    pub colours: Vec<u32>,
    pub lengths: Vec<u32>,
    pub coords: Vec<f64>,
}

impl TraceRings {
    /// Whether the arrays describe rings at all — the check at the trust boundary.
    fn well_formed(&self) -> bool {
        let points: Option<usize> = self
            .lengths
            .iter()
            .try_fold(0usize, |sum, &n| sum.checked_add(n as usize));
        self.colours.len() == self.lengths.len()
            && points.and_then(|n| n.checked_mul(2)) == Some(self.coords.len())
            && self.colours.iter().all(|&rgb| rgb <= 0x00ff_ffff)
            && self.coords.iter().all(|value| value.is_finite())
    }

    /// Each ring's colour and its `x, y` pairs. Only for a trace that is [`Self::well_formed`].
    fn rings(&self) -> impl Iterator<Item = (u32, &[f64])> {
        let mut at = 0;
        self.colours
            .iter()
            .zip(&self.lengths)
            .map(move |(&rgb, &n)| {
                let end = at + 2 * n as usize;
                let ring = &self.coords[at..end];
                at = end;
                (rgb, ring)
            })
    }
}

/// What the board and the wire accept, handed in by the host from
/// `packages/contract/src/limits.ts` so the numbers exist once.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VectorizeLimits {
    /// Live elements on one board.
    pub max_elements: usize,
    /// Points on one line.
    pub max_points_per_element: usize,
    /// Elements one editable insert may add: past this a board slows for everyone, and
    /// the answer is a coarser trace rather than a slower board.
    pub max_trace_shapes: usize,
    /// Length of one picture's `data:` URL.
    pub max_data_url_length: usize,
    /// Groups one element may be nested in.
    pub max_group_depth: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VectorizeOptions {
    /// Leave the traced image where it was, under its trace. Off by default: a trace is
    /// asked for to replace a picture, and one left behind doubles what the board holds.
    pub keep_original: bool,
    pub limits: VectorizeLimits,
}

/// Why nothing was inserted. Returned rather than raised as a notice, because the dialog
/// that asked is still open and says it there, next to the dial that would fix it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorizeRefusal {
    /// No live image by that id.
    NotAnImage,
    Locked,
    /// Someone else has it selected.
    Held,
    /// The trace has nothing in it to insert.
    Empty,
    /// Not what draw-trace produces: lengths that do not add up to the coordinates, a
    /// number that is not finite, a colour that is not one.
    Malformed,
    TooManyShapes,
    BoardFull,
    /// The picture is not a base64 SVG `data:` URL.
    NotAPicture,
    TooLarge,
}

impl VectorizeRefusal {
    /// The name the host reads — `VectorizeRefusal` in `engine/src/vectorize.ts`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAnImage => "not-an-image",
            Self::Locked => "locked",
            Self::Held => "held",
            Self::Empty => "empty",
            Self::Malformed => "malformed",
            Self::TooManyShapes => "too-many-shapes",
            Self::BoardFull => "board-full",
            Self::NotAPicture => "not-a-picture",
            Self::TooLarge => "too-large",
        }
    }
}

/// Hundredths of a unit: finer than any zoom shows, and it keeps a trace of thousands of
/// rings from spending most of its bytes on digits that mean nothing.
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// From the traced picture to the board, the way the painter draws the image: scaled to
/// its box, mirrored by the sign of its extent, turned about its centre.
///
/// `T(centre) · R(angle) · S(mirror) · T(−local centre)`, as `wasm/paint.rs`'s
/// `element_matrix`, after scaling the trace's fractions up to the box.
struct ImageMapping {
    scale: (f64, f64),
    half: (f64, f64),
    mirror: (f64, f64),
    angle: f64,
    centre: Point,
}

impl ImageMapping {
    fn new(image: &DrawElement) -> Self {
        let (w, h) = (image.width.abs(), image.height.abs());
        Self {
            scale: (w, h),
            half: (w / 2.0, h / 2.0),
            mirror: mirror_signs(image),
            angle: image.angle,
            centre: rotation_center(image),
        }
    }

    fn map(&self, u: f64, v: f64) -> Point {
        let lx = (u * self.scale.0 - self.half.0) * self.mirror.0;
        let ly = (v * self.scale.1 - self.half.1) * self.mirror.1;
        let turned = rotate_point(lx, ly, self.angle);
        Point {
            x: round2(self.centre.x + turned.x),
            y: round2(self.centre.y + turned.y),
        }
    }
}

/// One ring on the board, closed, without repeated points, within the point cap —
/// `None` when nothing with an inside is left of it.
fn board_ring(flat: &[f64], mapping: &ImageMapping, max_points: usize) -> Option<Vec<Point>> {
    let mut ring: Vec<Point> = Vec::with_capacity(flat.len() / 2 + 1);
    for &[u, v] in flat.as_chunks::<2>().0 {
        let p = mapping.map(u, v);
        if ring.last() != Some(&p) {
            ring.push(p);
        }
    }
    let first = *ring.first()?;
    if ring.last() != Some(&first) {
        ring.push(first);
    }
    // draw-trace fits every ring under the cap before it splices holes in, which is the
    // order that keeps a keyhole zero-width. This is the guard for a payload from
    // anywhere else: simplified rather than refused, and the ends stay where they are.
    let mut tolerance = 0.1;
    while ring.len() > max_points {
        ring = simplify_path(&ring, tolerance);
        tolerance *= 2.0;
    }
    // Three corners and the point that closes them.
    (ring.len() >= 4).then_some(ring)
}

impl DrawEngine {
    /// Replaces an image with its trace as editable shapes: one filled, closed line per
    /// ring, grouped, directly above the image, selected.
    ///
    /// Returns the new elements' ids in paint order.
    pub fn vectorize_to_shapes(
        &mut self,
        image_id: &str,
        trace: &TraceRings,
        options: VectorizeOptions,
    ) -> Result<Vec<String>, VectorizeRefusal> {
        let image = self.vectorizable(image_id)?;
        let limits = options.limits;
        if trace.lengths.len() > limits.max_trace_shapes {
            return Err(VectorizeRefusal::TooManyShapes);
        }
        if !trace.well_formed() {
            return Err(VectorizeRefusal::Malformed);
        }

        let mapping = ImageMapping::new(&image);
        // One group of its own, so the trace moves as the picture did and a double-click
        // gets at one region — inside whatever groups the image was already in, unless
        // that is as deep as groups go.
        let mut group_ids = image.group_ids.clone();
        if group_ids.len() < limits.max_group_depth {
            group_ids.insert(0, new_element_id());
        }
        let lines: Vec<DrawElement> = trace
            .rings()
            .filter_map(|(rgb, ring)| {
                let points = board_ring(ring, &mapping, limits.max_points_per_element)?;
                Some(self.trace_line(&points, rgb, &image, &group_ids))
            })
            .collect();
        if lines.is_empty() {
            return Err(VectorizeRefusal::Empty);
        }
        self.check_room(lines.len(), options)?;

        let ids: Vec<String> = lines.iter().map(|line| line.id.clone()).collect();
        for line in lines {
            self.scene.put(line);
        }
        self.commit_trace(&image.id, &ids, options.keep_original);
        Ok(ids)
    }

    /// Replaces an image with its trace as one picture: an SVG in the same box, mirror
    /// and turn, which stays sharp however far it is zoomed.
    pub fn vectorize_to_picture(
        &mut self,
        image_id: &str,
        data_url: &str,
        options: VectorizeOptions,
    ) -> Result<String, VectorizeRefusal> {
        let image = self.vectorizable(image_id)?;
        // Base64, as the contract's `dataUrl` pattern requires: a `;utf8,` SVG would be
        // accepted here and refused by every save after.
        if !data_url.starts_with("data:image/svg+xml;base64,") {
            return Err(VectorizeRefusal::NotAPicture);
        }
        if data_url.len() > options.limits.max_data_url_length {
            return Err(VectorizeRefusal::TooLarge);
        }
        self.check_room(1, options)?;

        // Everything the image was — box, mirror, turn, opacity, group, frame — as a new
        // element, because a new picture under an old id would reach a peer that has the
        // old picture cached against that id and be drawn as the raster it replaced.
        let mut picture = image.clone();
        picture.id = new_element_id();
        picture.data_url = Some(data_url.to_string());
        picture.version = 1;
        picture.version_nonce = rand_int();
        picture.updated = self.now_ms;
        let id = picture.id.clone();
        self.scene.put(picture);
        self.commit_trace(&image.id, std::slice::from_ref(&id), options.keep_original);
        Ok(id)
    }

    /// The image by that id, if it may be replaced by its trace.
    fn vectorizable(&self, image_id: &str) -> Result<DrawElement, VectorizeRefusal> {
        let image = self
            .scene
            .get(image_id)
            .filter(|el| el.kind == DrawElementType::Image && !el.is_deleted)
            .ok_or(VectorizeRefusal::NotAnImage)?;
        if image.locked() {
            return Err(VectorizeRefusal::Locked);
        }
        if self.untouchable(image) {
            return Err(VectorizeRefusal::Held);
        }
        Ok(image.clone())
    }

    /// Whether the board has room for `adding` more, counting the image that goes.
    fn check_room(&self, adding: usize, options: VectorizeOptions) -> Result<(), VectorizeRefusal> {
        let live = self
            .scene
            .iter_ordered()
            .filter(|el| !el.is_deleted)
            .count();
        let leaving = usize::from(!options.keep_original);
        if live - leaving + adding > options.limits.max_elements {
            return Err(VectorizeRefusal::BoardFull);
        }
        Ok(())
    }

    /// One traced region as the element a bucket fill makes (`bucket.rs`'s
    /// `fill_element`): a closed line with a solid background and nothing else.
    ///
    /// **No stroke**, and not for looks alone. The trace's regions are stacked — each
    /// colour is painted over the ones below it rather than cut out of them — so the
    /// edges are already where the colours meet. A stroke would widen every region by
    /// half its width, eating into the one above, and draw along each keyhole's bridge,
    /// where a hole was spliced in as a zero-width cut. The SVG vtracer writes has none
    /// either, so the two insert modes look alike.
    fn trace_line(
        &self,
        ring: &[Point],
        rgb: u32,
        image: &DrawElement,
        group_ids: &[String],
    ) -> DrawElement {
        // Anchored on its first point, as a line is (`points[0] == [0, 0]`).
        let origin = ring[0];
        let points: Vec<[f64; 2]> = ring
            .iter()
            .map(|p| [round2(p.x - origin.x), round2(p.y - origin.y)])
            .collect();
        let mut line = create_element(
            DrawElementType::Line,
            Geometry {
                x: origin.x,
                y: origin.y,
                width: 0.0,
                height: 0.0,
            },
            default_element_style(),
            self.now_ms,
        );
        line.points = Some(points);
        // The span the points cover, for everything that cannot read them — the API's
        // bounds mirror frames thumbnails with it.
        let span = local_box(&line);
        line.width = span.width;
        line.height = span.height;
        line.background_color = format!("#{rgb:06x}");
        line.fill_style = FillStyle::Solid;
        line.stroke_color = "transparent".into();
        line.stroke_width = 1.0;
        line.roughness = 0.0;
        line.roundness = None;
        line.opacity = image.opacity;
        line.group_ids = group_ids.to_vec();
        line.frame_id = image.frame_id.clone();
        line
    }

    /// The part both modes share: directly above the image, the image gone unless kept,
    /// the trace selected — all one step of history.
    fn commit_trace(&mut self, image_id: &str, ids: &[String], keep_original: bool) {
        self.scene.place_above(ids, image_id);
        if !keep_original {
            self.scene.remove(image_id, self.now_ms);
        }
        self.set_selection(ids.to_vec());
        self.push_history();
        self.request_draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_that_do_not_add_up_are_malformed() {
        let square = vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        let good = TraceRings {
            colours: vec![0xc83c3c],
            lengths: vec![4],
            coords: square.clone(),
        };
        assert!(good.well_formed());
        assert_eq!(
            good.rings().collect::<Vec<_>>(),
            vec![(0xc83c3c, &square[..])]
        );
        let short = TraceRings {
            lengths: vec![5],
            ..good.clone()
        };
        let uncoloured = TraceRings {
            colours: Vec::new(),
            ..good.clone()
        };
        let alpha = TraceRings {
            colours: vec![0x01c8_3c3c],
            ..good.clone()
        };
        let overflowing = TraceRings {
            colours: vec![0, 0],
            lengths: vec![u32::MAX, u32::MAX],
            coords: Vec::new(),
        };
        let mut nan = good.clone();
        nan.coords[2] = f64::NAN;
        for bad in [short, uncoloured, alpha, overflowing, nan] {
            assert!(!bad.well_formed(), "{bad:?}");
        }
    }

    #[test]
    fn a_ring_that_collapses_to_a_point_is_dropped() {
        let image = crate::create_element_default(
            DrawElementType::Image,
            Geometry {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        );
        // A thousandth of a one-unit image: this ring rounds to a single point.
        let mapping = ImageMapping::new(&image);
        assert_eq!(
            board_ring(
                &[0.0, 0.0, 0.001, 0.0, 0.001, 0.001, 0.0, 0.0],
                &mapping,
                10_000
            ),
            None
        );
    }
}
