use serde::{Deserialize, Serialize};

use crate::math::clamp;

pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 30.0;

/// One zoom step, in linear zoom units — Excalidraw's `ZOOM_STEP`.
///
/// "Linear" is the part that matters: the step is a flat tenth of the *scale*, not a
/// tenth of the picture, so the same step is a fifth of the view at 50% and a
/// three-hundredth of it at 3000%. `WHEEL_AMPLIFY_FROM` is what compensates.
pub const ZOOM_STEP: f64 = 0.1;

/// The largest wheel delta one event may spend.
///
/// The single most important number here. A wheel delta is not a quantity the browser
/// agrees on with anyone: Firefox reports 3 (lines) for the notch Chrome reports as 100
/// (pixels), a Windows mouse sends 120, and a trackpad fling coalesces into whatever it
/// likes. Clamping means the *worst* an event can do is bounded no matter which of those
/// it is, and that bound is what makes zooming feel like a movement rather than a jump.
const MAX_WHEEL_DELTA: f64 = ZOOM_STEP * 100.0;

/// Above this zoom the step grows with `log10`, to keep the felt step roughly constant.
const WHEEL_AMPLIFY_FROM: f64 = 1.0;

/// The delta at which that amplification reaches full strength.
///
/// A trackpad sends a stream of small deltas where a mouse sends one large one. Giving
/// every small delta the full amplification would make a slow drag zoom further than a
/// fast flick, so it is faded in over the first `20`.
const WHEEL_AMPLIFY_FULL_AT: f64 = 20.0;

/// `Math.sign`, which is 0 at 0 — unlike `f64::signum`, which is 1.0.
///
/// Not a nicety: the amplification term below is multiplied by the sign, so borrowing
/// `signum`'s answer at zero would drift the zoom during a purely horizontal wheel.
fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Excalidraw's `getNormalizedZoom`: six decimal places, then the limits.
///
/// The rounding keeps a long run of wheel ticks from leaving the scale a hair off every
/// round number — visible as a toolbar reading 99% when the board is plainly at rest.
/// `+ f64::EPSILON` before rounding is theirs too; it is what makes 1.0000004999 land on
/// 1 rather than on the float below it.
pub fn normalize_zoom(zoom: f64) -> f64 {
    const PLACES: f64 = 1e6;
    clamp(
        ((zoom + f64::EPSILON) * PLACES).round() / PLACES,
        MIN_ZOOM,
        MAX_ZOOM,
    )
}

/// The scale one wheel event moves to, given the scale it starts from.
///
/// A port of Excalidraw's `App.wheel.ts::zoomBy`, pinned in `tests/ci_zoom_wheel.rs`.
/// Positive `delta_y` zooms out, matching the event.
///
/// Pure, and takes the scale rather than reading the camera, because wheel events arrive
/// faster than frames: each one has to be applied to the scale the one before it produced
/// or a burst of ticks all start from the same scale and collapse into a single step.
pub fn wheel_zoom_scale(scale: f64, delta_y: f64) -> f64 {
    let sign = js_sign(delta_y);
    let abs_delta = delta_y.abs();
    let delta = if abs_delta > MAX_WHEEL_DELTA {
        MAX_WHEEL_DELTA * sign
    } else {
        delta_y
    };

    let mut next = scale - delta / 100.0;
    next += scale.max(WHEEL_AMPLIFY_FROM).log10()
        * -sign
        * (abs_delta / WHEEL_AMPLIFY_FULL_AT).min(1.0);

    normalize_zoom(next.max(MIN_ZOOM))
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

impl Default for Camera {
    fn default() -> Self {
        IDENTITY
    }
}

pub const IDENTITY: Camera = Camera {
    x: 0.0,
    y: 0.0,
    scale: 1.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl WorldBounds {
    pub fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(self) -> f64 {
        self.max_y - self.min_y
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub fn world_to_screen(camera: Camera, wx: f64, wy: f64) -> Point {
    Point {
        x: wx * camera.scale + camera.x,
        y: wy * camera.scale + camera.y,
    }
}

pub fn screen_to_world(camera: Camera, sx: f64, sy: f64) -> Point {
    Point {
        x: (sx - camera.x) / camera.scale,
        y: (sy - camera.y) / camera.scale,
    }
}

pub fn zoom_at(camera: Camera, sx: f64, sy: f64, factor: f64) -> Camera {
    zoom_to(camera, sx, sy, camera.scale * factor)
}

pub fn zoom_to(camera: Camera, sx: f64, sy: f64, next_scale: f64) -> Camera {
    let scale = clamp(next_scale, MIN_ZOOM, MAX_ZOOM);
    let ratio = scale / camera.scale;
    Camera {
        scale,
        x: sx - (sx - camera.x) * ratio,
        y: sy - (sy - camera.y) * ratio,
    }
}

pub fn pan_by(camera: Camera, dx_screen: f64, dy_screen: f64) -> Camera {
    Camera {
        x: camera.x + dx_screen,
        y: camera.y + dy_screen,
        scale: camera.scale,
    }
}

pub fn fit_bounds(bounds: WorldBounds, width: f64, height: f64, padding: f64) -> Camera {
    let world_w = (bounds.max_x - bounds.min_x).max(1.0);
    let world_h = (bounds.max_y - bounds.min_y).max(1.0);
    let scale = clamp(
        ((width - padding * 2.0) / world_w).min((height - padding * 2.0) / world_h),
        MIN_ZOOM,
        MAX_ZOOM,
    );
    let center_x = (bounds.min_x + bounds.max_x) / 2.0;
    let center_y = (bounds.min_y + bounds.max_y) / 2.0;
    Camera {
        scale,
        x: width / 2.0 - center_x * scale,
        y: height / 2.0 - center_y * scale,
    }
}

pub fn visible_world_rect(camera: Camera, width: f64, height: f64) -> WorldBounds {
    let top_left = screen_to_world(camera, 0.0, 0.0);
    let bottom_right = screen_to_world(camera, width, height);
    WorldBounds {
        min_x: top_left.x,
        min_y: top_left.y,
        max_x: bottom_right.x,
        max_y: bottom_right.y,
    }
}
