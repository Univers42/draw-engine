use crate::camera::{Point, WorldBounds};
use crate::math::round_half_up;
use std::cmp::Ordering;

/// One drawn guide segment.
///
/// **`axis` is the axis `at` is measured on, and the line runs across the other one**:
/// `Axis::X` is a vertical line at `x = at` running from `from` to `to` in y, and
/// `Axis::Y` is a horizontal line at `y = at` running in x. `wasm/paint.rs@1118751f` draws
/// it that way (`:1997-2013`) and a gap guide, whose line runs *along* the gap's axis, is
/// therefore on the **other** axis — the detail a sweep found and a hand-written example
/// would not have.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGuide {
    pub axis: Axis,
    pub at: f64,
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapResult {
    pub dx: f64,
    pub dy: f64,
    pub guides: Vec<SnapGuide>,
}

fn stops(min: f64, max: f64) -> [f64; 3] {
    [min, (min + max) / 2.0, max]
}

/// A box's three stops on one axis: its two edges and its centre.
///
/// The oracle's reference candidates are a group's four corners and its centre
/// (`getElementsCorners`, `snapping.ts@1118751f:198-313`), which on a single axis are
/// exactly these three numbers — so this is the same set, read per axis.
fn stops_of(bounds: &WorldBounds, axis: Axis) -> [f64; 3] {
    match axis {
        Axis::X => stops(bounds.min_x, bounds.max_x),
        Axis::Y => stops(bounds.min_y, bounds.max_y),
    }
}

fn low(bounds: &WorldBounds, axis: Axis) -> f64 {
    match axis {
        Axis::X => bounds.min_x,
        Axis::Y => bounds.min_y,
    }
}

fn high(bounds: &WorldBounds, axis: Axis) -> f64 {
    match axis {
        Axis::X => bounds.max_x,
        Axis::Y => bounds.max_y,
    }
}

fn other(axis: Axis) -> Axis {
    match axis {
        Axis::X => Axis::Y,
        Axis::Y => Axis::X,
    }
}

fn cross_range(bounds: WorldBounds, axis: Axis) -> (f64, f64) {
    (low(&bounds, axis), high(&bounds, axis))
}

/// `rangesOverlap` (`packages/common/src/ranges.ts@1118751f`), which is **inclusive**:
/// two ranges that merely share an endpoint overlap.
fn intersect(a: (f64, f64), b: (f64, f64)) -> Option<(f64, f64)> {
    let (lo, hi) = (a.0.max(b.0), a.1.min(b.1));
    (lo <= hi).then_some((lo, hi))
}

/// The oracle's `round` (`snapping.ts@1118751f:809-812`): six decimal places, and
/// `Math.round`, which breaks a tie toward +infinity — hence `round_half_up` and not
/// `f64::round`.
fn round6(value: f64) -> f64 {
    round_half_up(value * 1e6) / 1e6
}

/// Which candidate won on one axis, and which of our own stops it won against.
///
/// The offset it was admitted with is not kept: [`Nearest`] already holds the winner, and
/// a second copy of it is a second thing to be wrong.
#[derive(Clone, Copy)]
struct AxisHit {
    at: f64,
    mine: usize,
    other: WorldBounds,
    /// The [`Nearest::epoch`] this candidate was admitted in.
    epoch: u32,
}

/// The oracle's `minOffset`, and the one place in this engine that answers "which
/// candidate wins".
///
/// Its three lines are `snapping.ts@1118751f:660-672` for a point and `:485-489` for a
/// gap, and they are the **same** three lines on a different candidate: a newcomer within
/// reach is **kept**, the incumbent is discarded only when the newcomer is **strictly**
/// nearer, and the reach is then the new distance. So a tie keeps the incumbent *and*
/// admits the newcomer — that is how the oracle draws a chain rather than picking a pair —
/// and `snapDraggedElements` lands on the first entry found (`:752`).
///
/// **Both halves of that are load-bearing.** Replacing on `<=` instead of `<` lands the
/// box on the *later* candidate; taking the minimum afterwards lands it on whichever came
/// last among equals. Both are invisible on a board with one candidate and wrong on a
/// board with two, which is why the tie is pinned by a board built to produce one.
///
/// The discard is **shared across every pass on the axis**, which is what `epoch` is for:
/// `nearestSnapsX.length = 0` empties one list that the point pass and the equal-spacing
/// pass have both been pushing into, so a gap that beats a point snap takes its place
/// rather than joining it. A per-pass list gets that wrong in a way no single-pass test
/// can see, and the first version of this did.
struct Nearest {
    reach: f64,
    offset: Option<f64>,
    epoch: u32,
}

impl Nearest {
    fn new(reach: f64) -> Self {
        Self {
            reach,
            offset: None,
            epoch: 0,
        }
    }

    /// `None` is out of reach. `Some(epoch)` when the newcomer was admitted, in which case
    /// anything the caller kept under an older epoch is no longer tied with it and must be
    /// dropped before the newcomer is appended.
    fn offer(&mut self, offset: f64) -> Option<u32> {
        let distance = offset.abs();
        if distance > self.reach {
            return None;
        }
        if distance < self.reach {
            self.reach = distance;
            self.offset = None;
            self.epoch += 1;
        }
        self.offset.get_or_insert(offset);
        Some(self.epoch)
    }

    /// The candidates still tied with the winner, from a list its own pass built.
    fn epoch(&self) -> u32 {
        self.epoch
    }

    fn offset(&self) -> f64 {
        self.offset.unwrap_or(0.0)
    }
}

/// The candidates admitted in `epoch` and not since superseded, in the order found — the
/// oracle's `nearestSnapsX.filter(s => s was kept)` after both passes have run.
fn tied<T: Copy>(kept: &[T], epoch_of: impl Fn(&T) -> u32, epoch: u32) -> Vec<T> {
    kept.iter()
        .filter(|item| epoch_of(item) == epoch)
        .copied()
        .collect()
}

/// The nearest of our own stops against every reference stop on one axis — the oracle's
/// `getPointSnaps` (`:636-690`).
///
/// `ours` and the per-target stops are searched one axis at a time, which is what
/// `snapDraggedElements` does too: the nearest on x and the nearest on y may well be two
/// different candidates, and the oracle takes `nearestSnapsX[0]` and `nearestSnapsY[0]`
/// independently (`:751-754`).
fn nearest_axis(
    ours: &[f64],
    targets: &[WorldBounds],
    axis: Axis,
    near: &mut Nearest,
) -> Vec<AxisHit> {
    let mut kept = Vec::new();
    for other in targets {
        for target in stops_of(other, axis) {
            for (mine, &stop) in ours.iter().enumerate() {
                let Some(epoch) = near.offer(target - stop) else {
                    continue;
                };
                kept.push(AxisHit {
                    at: target,
                    mine,
                    other: *other,
                    epoch,
                });
            }
        }
    }
    kept
}

/// A gap between two reference boxes, on one axis.
///
/// `start` is the box the gap grows out of and `end` the one it reaches, so `length` is
/// `end`'s near edge less `start`'s far one, and `overlap` is the range the two share on
/// the *other* axis — which is what the guide is drawn across
/// (`getVisibleGaps`, `snapping.ts@1118751f:379-392`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gap {
    axis: Axis,
    start: WorldBounds,
    end: WorldBounds,
    overlap: (f64, f64),
    length: f64,
}

/// The oracle's `VISIBLE_GAPS_LIMIT_PER_AXIS` (`snapping.ts:1118751f:45`), a cap on
/// candidate *pairs* per axis that it annotates as a TODO to remove once the pass is
/// optimised. Carried over at the same number so the two cannot silently disagree, and
/// named so that changing it is a decision.
///
/// ponytail: no cache, so this is O(n²) in the visible elements on every frame of a drag,
/// where the oracle computes it once per gesture (`SnapCache.getVisibleGaps`, `:463`,
/// filled at `App.tsx@1118751f:10576-10579`). Fine at a few dozen visible shapes and the
/// cap keeps it bounded; upgrade path is to gather the gaps in `begin_move` beside
/// `static_bounds` and pass them down, which is what the oracle's cache amounts to.
const VISIBLE_GAPS_LIMIT_PER_AXIS: usize = 99999;

/// The candidate pairs on one axis: two boxes separated along it and overlapping across
/// it, `getVisibleGaps` (`:359-395` x, `:403-437` y).
///
/// Sorted by the near edge first, which is the oracle's order and therefore part of the
/// rule: a gap is offered in the order the boxes sit on the axis, so when two are equally
/// near the one between the earlier boxes is the one kept.
fn visible_gaps(targets: &[WorldBounds], axis: Axis) -> Vec<Gap> {
    let mut sorted = targets.to_vec();
    sorted.sort_by(|a, b| {
        low(a, axis)
            .partial_cmp(&low(b, axis))
            .unwrap_or(Ordering::Equal)
    });
    let mut gaps = Vec::new();
    let mut seen = 0usize;
    'pairs: for i in 0..sorted.len() {
        for j in i + 1..sorted.len() {
            seen += 1;
            if seen > VISIBLE_GAPS_LIMIT_PER_AXIS {
                break 'pairs;
            }
            if let Some(gap) = gap_between(sorted[i], sorted[j], axis) {
                gaps.push(gap);
            }
        }
    }
    gaps
}

fn gap_between(a: WorldBounds, b: WorldBounds, axis: Axis) -> Option<Gap> {
    if high(&a, axis) >= low(&b, axis) {
        return None;
    }
    let overlap = intersect(cross_range(a, other(axis)), cross_range(b, other(axis)))?;
    Some(Gap {
        axis,
        start: a,
        end: b,
        overlap,
        length: low(&b, axis) - high(&a, axis),
    })
}

/// Which end of a gap a box wants to equalise: the middle of it, the gap that continues
/// past its far box, or the one that continues before its near box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GapEnd {
    Centre,
    Ahead,
    Behind,
}

/// A gap candidate that reached the tie set, and the epoch that admitted it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GapHit {
    gap: Gap,
    end: GapEnd,
    epoch: u32,
}

/// The order the oracle tries them in — and **the two axes disagree**:
/// `side_right` before `side_left` on x (`:502`, `:523`) and `side_top` before
/// `side_bottom` on y (`:571`, `:592`). It is invisible on one box, because the two
/// landings are a whole box plus two gaps apart and the reach is 6, and it is written
/// down here so a later reader does not "fix" it.
fn gap_order(axis: Axis) -> [GapEnd; 3] {
    match axis {
        Axis::X => [GapEnd::Centre, GapEnd::Ahead, GapEnd::Behind],
        Axis::Y => [GapEnd::Centre, GapEnd::Behind, GapEnd::Ahead],
    }
}

/// The offset one candidate wants, or `None` when the oracle would not consider it.
///
/// `Centre` is refused unless the gap is wider than the box (`gapIsLargerThanSelection`,
/// `:483`); the two sides are considered regardless, because the centre branch
/// `continue`s only when it fires and control otherwise falls through to them.
fn gap_offset(moving: &WorldBounds, gap: &Gap, end: GapEnd) -> Option<f64> {
    let (near, far) = cross_range(*moving, gap.axis);
    match end {
        GapEnd::Centre if gap.length > far - near => Some(round6(
            high(&gap.start, gap.axis) + gap.length / 2.0 - (near + far) / 2.0,
        )),
        GapEnd::Centre => None,
        // The gap on our far side, equal: our near edge goes to `end`'s far edge plus it.
        GapEnd::Ahead => Some(round6(gap.length - (near - high(&gap.end, gap.axis)))),
        // The gap on our near side, equal: our far edge goes to `start`'s near edge less it.
        GapEnd::Behind => Some(round6(low(&gap.start, gap.axis) - far - gap.length)),
    }
}

/// `getGapSnaps` for one axis (`:475-543` x, `:544-612` y).
///
/// At most **one** candidate per gap: the first direction within reach `continue`s to the
/// next gap, and a direction that is out of reach falls through to the next one. `near` is
/// threaded through because the oracle shares one `minOffset` between the point pass and
/// this one (`:735`, `:745`) — which is what makes the tie-break in [`Nearest`] load
/// bearing rather than an internal detail.
fn gap_axis(
    moving: WorldBounds,
    targets: &[WorldBounds],
    axis: Axis,
    near: &mut Nearest,
) -> Vec<GapHit> {
    let mut kept = Vec::new();
    for gap in visible_gaps(targets, axis) {
        if intersect(gap.overlap, cross_range(moving, other(axis))).is_none() {
            continue;
        }
        for end in gap_order(axis) {
            let Some(offset) = gap_offset(&moving, &gap, end) else {
                continue;
            };
            let Some(epoch) = near.offer(offset) else {
                continue;
            };
            kept.push(GapHit { gap, end, epoch });
            break;
        }
    }
    kept
}

/// The two segments one gap snap draws, both across the midpoint of where the two boxes
/// and the dragged box overlap on the *other* axis (`createGapSnapLines`, `:932-1093`).
///
/// `moving` is the box **after** the move, because the oracle recomputes the dragged
/// bounds at the snapped position before drawing (`:766-775`).
fn gap_guides(moving: &WorldBounds, gap: &Gap, end: GapEnd) -> [SnapGuide; 2] {
    let axis = gap.axis;
    let across = intersect(gap.overlap, cross_range(*moving, other(axis)))
        .map_or((gap.overlap.0 + gap.overlap.1) / 2.0, |(lo, hi)| {
            (lo + hi) / 2.0
        });
    let [first, second] = gap_spans(moving, gap, end);
    // The guide is drawn ALONG the gap's axis, so its `at` is a coordinate on the OTHER
    // one. See the note on `SnapGuide`.
    let along = other(axis);
    [span(along, across, first), span(along, across, second)]
}

/// The two ranges, on the gap's axis, that the pair of segments runs between.
fn gap_spans(moving: &WorldBounds, gap: &Gap, end: GapEnd) -> [(f64, f64); 2] {
    let axis = gap.axis;
    let (mine, theirs) = (cross_range(*moving, axis), cross_range(gap.start, axis));
    let their_far = cross_range(gap.end, axis);
    let starts = high(&gap.start, axis);
    let reaches = low(&gap.end, axis);
    match end {
        GapEnd::Centre => [(starts, mine.0), (mine.1, reaches)],
        GapEnd::Ahead => [(starts, reaches), (their_far.1, mine.0)],
        GapEnd::Behind => [(mine.1, theirs.0), (starts, reaches)],
    }
}

fn span(axis: Axis, across: f64, ends: (f64, f64)) -> SnapGuide {
    SnapGuide {
        axis,
        at: across,
        from: ends.0,
        to: ends.1,
    }
}

/// Where a gesture's own points land.
///
/// One point while drawing (`snapNewElement`, `snapping.ts@1118751f:1261-1263`) and one or
/// two while resizing (`:1146-1183`); the maths is the same either way, and the same one a
/// move uses. Only **where the point lands** changes: the caller still owns the element
/// being placed, and a guide spans the winning pair.
///
/// **No equal spacing here, and that is the oracle's shape rather than an omission**:
/// `getGapSnaps` is called from two places and both are inside `snapDraggedElements`
/// (`:738`, `:783`), so a box being drawn or an edge being dragged is never offered a
/// gap candidate.
pub fn snap_points(moving: &[Point], targets: &[WorldBounds], threshold: f64) -> SnapResult {
    let ours = |axis: Axis| -> Vec<f64> { moving.iter().map(|p| axis_of(p, axis)).collect() };
    let mut near_x = Nearest::new(threshold);
    let mut near_y = Nearest::new(threshold);
    let x = nearest_axis(&ours(Axis::X), targets, Axis::X, &mut near_x);
    let y = nearest_axis(&ours(Axis::Y), targets, Axis::Y, &mut near_y);
    SnapResult {
        dx: near_x.offset(),
        dy: near_y.offset(),
        guides: guides_of(
            moving,
            tied(&x, |h| h.epoch, near_x.epoch()),
            tied(&y, |h| h.epoch, near_y.epoch()),
        ),
    }
}

fn axis_of(point: &Point, axis: Axis) -> f64 {
    match axis {
        Axis::X => point.x,
        Axis::Y => point.y,
    }
}

/// One guide per axis that moved, spanning our point and the box it took (`at`), which is
/// what the oracle's `PointSnapLine` draws between the two (`snapping.ts@1118751f:828`).
fn guides_of(moving: &[Point], x: Vec<AxisHit>, y: Vec<AxisHit>) -> Vec<SnapGuide> {
    let mut guides = Vec::new();
    for (axis, hits) in [(Axis::X, &x), (Axis::Y, &y)] {
        for hit in hits.iter().take(1) {
            // The two spans the oracle's `PointSnapLine` joins: our point's own cross
            // coordinate out to the reference box's far edge (`:844-846`, and the same
            // pair read on the other axis at `:860-862`).
            let (from, to) = match axis {
                Axis::X => (moving[hit.mine].y, hit.other.max_y),
                Axis::Y => (moving[hit.mine].x, hit.other.max_x),
            };
            let near = match axis {
                Axis::X => hit.other.min_y,
                Axis::Y => hit.other.min_x,
            };
            guides.push(SnapGuide {
                axis,
                at: hit.at,
                from: from.min(near),
                to: from.max(to),
            });
        }
    }
    guides
}

/// `snapDraggedElements` (`:692-807`): the point pass, then the equal-spacing pass, then
/// the guides for both.
pub fn snap_move(moving: WorldBounds, statics: &[WorldBounds], threshold: f64) -> SnapResult {
    let mut near_x = Nearest::new(threshold);
    let mut near_y = Nearest::new(threshold);
    let hits_x = nearest_axis(
        &stops(moving.min_x, moving.max_x),
        statics,
        Axis::X,
        &mut near_x,
    );
    let hits_y = nearest_axis(
        &stops(moving.min_y, moving.max_y),
        statics,
        Axis::Y,
        &mut near_y,
    );
    // One `minOffset` for both passes, in this order: a gap nearer than a point takes the
    // offset from it, and a gap equally near is kept alongside it without moving anything.
    let gaps_x = gap_axis(moving, statics, Axis::X, &mut near_x);
    let gaps_y = gap_axis(moving, statics, Axis::Y, &mut near_y);
    let (dx, dy) = (near_x.offset(), near_y.offset());
    let landed = WorldBounds {
        min_x: moving.min_x + dx,
        min_y: moving.min_y + dy,
        max_x: moving.max_x + dx,
        max_y: moving.max_y + dy,
    };
    let guides = move_guides(
        landed,
        &hits_x,
        &hits_y,
        &gaps_x,
        &gaps_y,
        (&near_x, &near_y),
    );
    SnapResult { dx, dy, guides }
}

/// Every guide a move publishes: the point candidates still tied on each axis, then the
/// gap candidates, all drawn at where the box **landed**.
///
/// Both lists are filtered by epoch at the end rather than as they are built, because the
/// gap pass runs after the point pass and can supersede it — the discard is the shared
/// `nearestSnapsX.length = 0`, not a per-pass one.
fn move_guides(
    landed: WorldBounds,
    hits_x: &[AxisHit],
    hits_y: &[AxisHit],
    gaps_x: &[GapHit],
    gaps_y: &[GapHit],
    near: (&Nearest, &Nearest),
) -> Vec<SnapGuide> {
    let (x, y) = near;
    let mut guides = point_guides(
        landed,
        &tied(hits_x, |h| h.epoch, x.epoch()),
        &tied(hits_y, |h| h.epoch, y.epoch()),
    );
    for hit in
        tied(gaps_x, |g| g.epoch, x.epoch())
            .into_iter()
            .chain(tied(gaps_y, |g| g.epoch, y.epoch()))
    {
        guides.extend(gap_guides(&landed, &hit.gap, hit.end));
    }
    guides
}

/// A move's point guides, drawn at the box's landed place.
///
/// The oracle's two `snapLines.push` calls around the offset
/// (`snapping.ts@1118751f:151-168`): one per axis, each spanning our box and the box it
/// took on the *other* axis. One per axis and not one per tied candidate — see
/// [`guides_of`], which is the same choice on the other two paths.
fn point_guides(landed: WorldBounds, x: &[AxisHit], y: &[AxisHit]) -> Vec<SnapGuide> {
    let mut guides = Vec::new();
    for (axis, hits) in [(Axis::X, x), (Axis::Y, y)] {
        for hit in hits.iter().take(1) {
            let (mine, theirs) = (
                cross_range(landed, other(axis)),
                cross_range(hit.other, other(axis)),
            );
            guides.push(SnapGuide {
                axis,
                at: hit.at,
                from: mine.0.min(theirs.0),
                to: mine.1.max(theirs.1),
            });
        }
    }
    guides
}
