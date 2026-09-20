use crate::camera::WorldBounds;

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

struct AxisHit {
    delta: f64,
    at: f64,
    other: WorldBounds,
}

fn best_axis_snap(
    moving_stops: [f64; 3],
    statics: &[WorldBounds],
    pick: fn(WorldBounds) -> [f64; 3],
    threshold: f64,
) -> Option<AxisHit> {
    let mut best: Option<AxisHit> = None;
    for &other in statics {
        for target in pick(other) {
            for stop in moving_stops {
                let delta = target - stop;
                if delta.abs() <= threshold
                    && best
                        .as_ref()
                        .is_none_or(|hit| delta.abs() < hit.delta.abs())
                {
                    best = Some(AxisHit {
                        delta,
                        at: target,
                        other,
                    });
                }
            }
        }
    }
    best
}

pub fn snap_move(moving: WorldBounds, statics: &[WorldBounds], threshold: f64) -> SnapResult {
    let x = best_axis_snap(
        stops(moving.min_x, moving.max_x),
        statics,
        |bounds| stops(bounds.min_x, bounds.max_x),
        threshold,
    );
    let y = best_axis_snap(
        stops(moving.min_y, moving.max_y),
        statics,
        |bounds| stops(bounds.min_y, bounds.max_y),
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
