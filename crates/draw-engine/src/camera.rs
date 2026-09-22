use serde::{Deserialize, Serialize};

use crate::math::clamp;

pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 30.0;

/// One zoom step, as a *proportion* of the current scale: a notch is 10% more picture.
///
/// The proportion is the point. Excalidraw's `ZOOM_STEP` is a flat tenth of the *scale*,
/// which is a different thing to look at depending on where you already are — at 10% it
/// doubles the picture, at 1000% it is a hundredth of it. Their `log10` amplification
/// papers over the top half of that range and leaves the bottom half alone, so zooming
/// out stepped in leaps: 10% to 20% to 30% is 2.0x then 1.5x, in the range you are in
/// precisely when you are looking for something.
///
/// Applied geometrically here instead, which is the same thing `zoom_in`/`zoom_out`
/// already do for the toolbar buttons (x1.2), so the wheel is no longer the one control
/// that steps by a different rule than the rest of the app.
pub const ZOOM_STEP: f64 = 0.1;

/// The wheel delta worth one whole notch, and the ceiling on what one event may spend.
///
/// The single most important number here. A wheel delta is not a quantity the browser
/// agrees on with anyone: Firefox reports 3 (lines) for the notch Chrome reports as 100
/// (pixels), a Windows mouse sends 120, and a trackpad fling coalesces into whatever it
/// likes. Clamping means the *worst* an event can do is bounded no matter which of those
/// it is, and that bound is what makes zooming feel like a movement rather than a jump.
const MAX_WHEEL_DELTA: f64 = ZOOM_STEP * 100.0;

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
/// Excalidraw's clamp (`App.wheel.ts::zoomBy`) applied to a geometric step rather than a
/// linear one — a deliberate divergence from the oracle, pinned in `tests/ci_zoom_wheel.rs`.
/// Positive `delta_y` zooms out, matching the event.
///
/// Pure, and takes the scale rather than reading the camera, because wheel events arrive
/// faster than frames: each one has to be applied to the scale the one before it produced
/// or a burst of ticks all start from the same scale and collapse into a single step.
pub fn wheel_zoom_scale(scale: f64, delta_y: f64) -> f64 {
    // How much of a notch this event is worth, signed: exactly one past the clamp, and
    // proportionally less below it, so a trackpad's stream of small deltas stays smooth
    // instead of moving in notches. Negative `delta_y` zooms in, hence the flip.
    //
    // The sign comes from `js_sign` rather than `f64::signum`, which answers 1.0 at zero
    // where `Math.sign` answers 0 — a purely horizontal wheel would otherwise be worth a
    // whole notch of zoom while you are plainly scrolling sideways.
    let notches = -js_sign(delta_y) * (delta_y.abs().min(MAX_WHEEL_DELTA) / MAX_WHEEL_DELTA);

    // Geometric, so the step is the same share of the picture at every zoom, and the
    // step out is the exact inverse of the step in — one notch back undoes one notch.
    // No amplification term: a proportional step needs no correction for where it is,
    // which is the whole reason `log10(max(1, zoom))` existed.
    normalize_zoom(scale * (1.0 + ZOOM_STEP).powf(notches))
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
