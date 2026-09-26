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

/// The scroll distance, in pixels, worth one whole notch of zoom.
///
/// A Chrome mouse notch, which is the one device everything else is calibrated against.
/// `host/wheel.ts` converts lines and pages to pixels before any of this, so a Firefox
/// notch (3 lines) arrives here as 120 and a Windows mouse sends 120 directly.
const PIXELS_PER_NOTCH: f64 = 100.0;

/// The most zoom one event may spend, however far it claims to have scrolled.
///
/// This and `PIXELS_PER_NOTCH` used to be the same constant (`ZOOM_STEP * 100` = 10),
/// which quietly meant "any event of 10px or more is a whole notch". That is correct for
/// a plain mouse, which reports a notch as one event of 100 — and catastrophic for a
/// high-resolution or smooth-scroll wheel, which reports the same physical notch as a
/// dozen-odd events of 8 to 20. Each one became a full notch, so one notch of scroll was
/// fourteen notches of zoom: 1.1^14 = 3.8x per flick, at every zoom level.
///
/// They are separate now because they are answers to different questions. How much zoom
/// a delta is worth is a matter of *distance*, so it divides by `PIXELS_PER_NOTCH` and a
/// device that sends ten small events reaches the same place as one that sends a big one.
/// The clamp is a matter of *safety* — a trackpad fling coalesces into whatever delta it
/// likes and must not throw the board off screen in a single tick — so it bounds the
/// notches, not the delta that defines one.
const MAX_NOTCHES_PER_EVENT: f64 = 1.0;

/// `Math.sign`, which is 0 at 0 — unlike `f64::signum`, which is 1.0.
///
/// Not a nicety: the notch count below is multiplied by the sign, so borrowing `signum`'s
/// answer at zero would spend a whole notch of zoom on a purely horizontal wheel — a tilt
/// wheel, or a sideways two-finger scroll with ctrl still held from the last pinch.
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
/// Two deliberate divergences from Excalidraw's `App.wheel.ts::zoomBy`, both pinned in
/// `tests/ci_zoom_wheel.rs`: the step is geometric rather than linear, and the bound is on
/// the notches an event may spend rather than on the delta that defines a notch.
/// Positive `delta_y` zooms out, matching the event.
///
/// Pure, and takes the scale rather than reading the camera, because wheel events arrive
/// faster than frames: each one has to be applied to the scale the one before it produced
/// or a burst of ticks all start from the same scale and collapse into a single step.
pub fn wheel_zoom_scale(scale: f64, delta_y: f64) -> f64 {
    // How much of a notch this event is worth, signed. Proportional to the distance
    // scrolled, so ten events of 10px are worth exactly what one event of 100px is —
    // the device's reporting granularity must not change how far the zoom travels.
    // Negative `delta_y` zooms in, hence the flip.
    //
    // The sign comes from `js_sign` rather than `f64::signum`, which answers 1.0 at zero
    // where `Math.sign` answers 0 — a purely horizontal wheel would otherwise be worth a
    // whole notch of zoom while you are plainly scrolling sideways.
    let notches = -js_sign(delta_y) * (delta_y.abs() / PIXELS_PER_NOTCH).min(MAX_NOTCHES_PER_EVENT);

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

/// Screen space, in CSS pixels, kept clear on each side of the canvas — Excalidraw's
/// `Offsets`: what its own UI covers there, and a margin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Offsets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// The camera that centres `bounds` in the room `offsets` leave on a `width` x `height`
/// canvas, at the zoom that fits it there but never past 100%: zoomed out to hold what is
/// too big, and in, up to 100%, on what is small. Excalidraw's `zoomToFitBounds` with
/// `fit: "scale-down"`, then `centerScrollOn` (`packages/excalidraw/viewport.ts@1118751f:
/// 262-368`, `:397-424`), in its own order of operations; its `scrollX` is `x / scale` here.
pub fn scale_down_fit(bounds: WorldBounds, width: f64, height: f64, offsets: Offsets) -> Camera {
    let room_w = width - offsets.left - offsets.right;
    let room_h = height - offsets.top - offsets.bottom;
    let zoom = normalize_zoom(
        (room_w / (bounds.max_x - bounds.min_x))
            .min(room_h / (bounds.max_y - bounds.min_y))
            .min(1.0),
    );
    let center_x = (bounds.min_x + bounds.max_x) / 2.0;
    let center_y = (bounds.min_y + bounds.max_y) / 2.0;
    let scroll_x = (width - offsets.right) / 2.0 / zoom - center_x + offsets.left / 2.0 / zoom;
    let scroll_y = (height - offsets.bottom) / 2.0 / zoom - center_y + offsets.top / 2.0 / zoom;
    Camera {
        x: scroll_x * zoom,
        y: scroll_y * zoom,
        scale: zoom,
    }
}

/// Where a camera move is `factor` of the way (already eased) from `from` to `to`.
/// Excalidraw's `interpolateViewport` (`components/App.viewport.ts@1118751f:262-301`): the
/// zoom moves geometrically, and the pan by the share of the zoom change made so far, so
/// every point on the board travels a straight line on screen; a pure pan moves by
/// `factor`. Lands exactly on `to` at 1.
pub fn interpolate_camera(from: Camera, to: Camera, factor: f64) -> Camera {
    if factor >= 1.0 {
        return to;
    }
    let scale = from.scale * (to.scale / from.scale).powf(factor);
    let m = if to.scale == from.scale {
        factor
    } else {
        (scale - from.scale) / (to.scale - from.scale)
    };
    Camera {
        x: (1.0 - m) * from.x + m * to.x,
        y: (1.0 - m) * from.y + m * to.y,
        scale,
    }
}
