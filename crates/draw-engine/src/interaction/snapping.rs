use crate::camera::{Point, WorldBounds};

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

/// Which candidate won on one axis, and which of our own stops it won against.
struct AxisHit {
    delta: f64,
    at: f64,
    mine: usize,
    other: WorldBounds,
}

/// The nearest of our own stops against every reference stop on one axis.
///
/// The oracle's `getPointSnaps` (`snapping.ts@1118751f:636-690`), and the one place in
/// this engine that answers "which candidate wins": a candidate is **kept** on `<=` and
/// **replaces** the incumbent only on `<`, so a tie keeps the first one found and the
/// answer never depends on the order the candidates happen to arrive in.
///
/// `ours` and the per-target stops are searched one axis at a time, which is what
/// `snapDraggedElements` does too: the nearest on x and the nearest on y may well be two
/// different candidates, and the oracle takes `nearestSnapsX[0]` and `nearestSnapsY[0]`
/// independently (`:751-754`).
fn nearest_axis(
    ours: &[f64],
    targets: &[WorldBounds],
    axis: Axis,
    threshold: f64,
) -> Option<AxisHit> {
    let mut best: Option<AxisHit> = None;
    for other in targets {
        for target in stops_of(other, axis) {
            for (mine, &stop) in ours.iter().enumerate() {
                let delta = target - stop;
                if delta.abs() <= threshold
                    && best
                        .as_ref()
                        .is_none_or(|hit| delta.abs() < hit.delta.abs())
                {
                    best = Some(AxisHit {
                        delta,
                        at: target,
                        mine,
                        other: *other,
                    });
                }
            }
        }
    }
    best
}

/// Where a gesture's own points land.
///
/// One point while drawing (`snapNewElement`, `snapping.ts@1118751f:1261-1263`) and one or
/// two while resizing (`:1146-1183`); the maths is the same either way, and the same one a
/// move uses. Only **where the point lands** changes: the caller still owns the element
/// being placed, and a guide spans the winning pair.
pub fn snap_points(moving: &[Point], targets: &[WorldBounds], threshold: f64) -> SnapResult {
    let ours = |axis: Axis| -> Vec<f64> { moving.iter().map(|p| axis_of(p, axis)).collect() };
    let x = nearest_axis(&ours(Axis::X), targets, Axis::X, threshold);
    let y = nearest_axis(&ours(Axis::Y), targets, Axis::Y, threshold);
    SnapResult {
        dx: x.as_ref().map_or(0.0, |hit| hit.delta),
        dy: y.as_ref().map_or(0.0, |hit| hit.delta),
        guides: guides_of(moving, x, y),
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
fn guides_of(moving: &[Point], x: Option<AxisHit>, y: Option<AxisHit>) -> Vec<SnapGuide> {
    let mut guides = Vec::new();
    if let Some(hit) = x {
        let other = &hit.other;
        guides.push(SnapGuide {
            axis: Axis::X,
            at: hit.at,
            from: moving[hit.mine].y.min(other.min_y),
            to: moving[hit.mine].y.max(other.max_y),
        });
    }
    if let Some(hit) = y {
        let other = &hit.other;
        guides.push(SnapGuide {
            axis: Axis::Y,
            at: hit.at,
            from: moving[hit.mine].x.min(other.min_x),
            to: moving[hit.mine].x.max(other.max_x),
        });
    }
    guides
}

pub fn snap_move(moving: WorldBounds, statics: &[WorldBounds], threshold: f64) -> SnapResult {
    let x = nearest_axis(
        &stops(moving.min_x, moving.max_x),
        statics,
        Axis::X,
        threshold,
    );
    let y = nearest_axis(
        &stops(moving.min_y, moving.max_y),
        statics,
        Axis::Y,
        threshold,
    );
    let mut guides = Vec::new();
    if let Some(ref hit) = x {
        let y_delta = y.as_ref().map_or(0.0, |item| item.delta);
        guides.push(SnapGuide {
            axis: Axis::X,
            at: hit.at,
            from: (moving.min_y + y_delta).min(hit.other.min_y),
            to: (moving.max_y + y_delta).max(hit.other.max_y),
        });
    }
    if let Some(ref hit) = y {
        let x_delta = x.as_ref().map_or(0.0, |item| item.delta);
        guides.push(SnapGuide {
            axis: Axis::Y,
            at: hit.at,
            from: (moving.min_x + x_delta).min(hit.other.min_x),
            to: (moving.max_x + x_delta).max(hit.other.max_x),
        });
    }
    SnapResult {
        dx: x.as_ref().map_or(0.0, |hit| hit.delta),
        dy: y.as_ref().map_or(0.0, |hit| hit.delta),
        guides,
    }
}
