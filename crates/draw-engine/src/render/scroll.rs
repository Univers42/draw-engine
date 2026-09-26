//! Deciding how much of a frame has to be drawn again.
//!
//! The painter keeps the static part of the scene — background, grid, elements, frame
//! names — in an offscreen layer, and draws only the selection chrome and the laser on
//! top of it each frame. That makes the interesting question "what changed since the last
//! frame", and there are three useful answers:
//!
//! - **nothing**, so the layer can be blitted as it is;
//! - **only where the camera is looking**, so the layer can be blitted *shifted* and only
//!   the strip that has come into view redrawn;
//! - **something else**, so it has to be drawn again.
//!
//! The middle one is the point. Panning a board of screen-sized shapes was the one thing
//! measured slower than Excalidraw, and it was slower because every frame re-stroked
//! every visible element — three quarters of that cost being pattern fills, which are
//! some eighty double-stroked lines apiece. A 20px pan exposes about a twentieth of the
//! viewport, so drawing only that strip is the difference between redrawing a hundred
//! screen-sized shapes and redrawing the two or three that have just appeared.
//!
//! Lossless, which is why it is this rather than a per-element bitmap cache: a scroll by
//! a whole number of device pixels is an exact copy, so a panned frame is identical to a
//! freshly drawn one. Rasterising elements into bitmaps and scaling them — Excalidraw's
//! approach — resamples, and at a zoom the bitmap was not rendered for it shows.
//!
//! This module decides; `wasm/paint.rs` carries it out. The decision is here because it
//! is arithmetic, and because every host that mounts this engine wants the same answer.

use crate::camera::{Camera, WorldBounds};
use crate::scene::geometry::Rect;

/// Everything about a frame that, if it changes, invalidates the whole layer.
///
/// The camera's *position* is deliberately not in here — that is what the scroll path
/// exists to handle. Its scale is, because a different zoom is different geometry on
/// screen and nothing about the old layer can be reused.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerKey {
    pub scale: f64,
    pub dpr: f64,
    pub width: f64,
    pub height: f64,
    /// A digest of what is drawn into the layer: which elements, where, and in what
    /// state. Whatever produces it must change when the picture does.
    pub content: u64,
    /// Theme and grid, which are painted into the layer but are not elements.
    pub chrome: u64,
}

/// What the painter should do with the layer it already has.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerPlan {
    /// Draw the whole static scene again.
    Redraw,
    /// Blit it unchanged.
    Reuse,
    /// Blit it offset by `(dx, dy)` device pixels, then draw `exposed` again.
    Scroll {
        dx: f64,
        dy: f64,
        /// The strips that have come into view, in device pixels. One or two of them —
        /// a diagonal pan exposes an L.
        exposed: Vec<Rect>,
    },
}

/// How close to a whole device pixel a shift has to be to count as one.
///
/// A scroll is only lossless if it is an exact pixel copy. A fractional offset resamples
/// the whole layer, which both blurs it and compounds — twenty frames of quarter-pixel
/// drift and the board is visibly soft. Anything that is not whole is redrawn instead.
const PIXEL_EPSILON: f64 = 1e-6;

fn is_whole(value: f64) -> bool {
    (value - value.round()).abs() < PIXEL_EPSILON
}

/// What to do, given the layer's state and the frame that is wanted now.
pub fn plan_layer(
    previous: Option<(LayerKey, Camera)>,
    next: LayerKey,
    camera: Camera,
) -> LayerPlan {
    let Some((old_key, old_camera)) = previous else {
        return LayerPlan::Redraw;
    };
    if old_key != next {
        return LayerPlan::Redraw;
    }

    // Screen pixels first, then device pixels: the camera is in CSS pixels and the layer
    // is in device pixels, and it is the device-pixel shift that has to be whole.
    let dx = (camera.x - old_camera.x) * next.dpr;
    let dy = (camera.y - old_camera.y) * next.dpr;
    if !is_whole(dx) || !is_whole(dy) {
        return LayerPlan::Redraw;
    }
    let (dx, dy) = (dx.round(), dy.round());
    if dx == 0.0 && dy == 0.0 {
        return LayerPlan::Reuse;
    }

    let width = next.width * next.dpr;
    let height = next.height * next.dpr;
    // A jump of a whole viewport leaves nothing worth keeping, and the blit would cost
    // more than the redraw it saves.
    if dx.abs() >= width || dy.abs() >= height {
        return LayerPlan::Redraw;
    }

    LayerPlan::Scroll {
        dx,
        dy,
        exposed: exposed_rects(dx, dy, width, height),
    }
}

/// The strips that come into view when the layer is shifted by `(dx, dy)`.
///
/// Shifting the old picture right by `dx` leaves a `dx`-wide band uncovered down the left
/// edge; shifting it down by `dy` leaves a `dy`-tall band across the top. A diagonal pan
/// exposes both, and they are returned whole rather than as an exact L — the small
/// overlap in the corner is cheaper to draw twice than to describe.
pub fn exposed_rects(dx: f64, dy: f64, width: f64, height: f64) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(2);
    if dx > 0.0 {
        rects.push(Rect {
            x: 0.0,
            y: 0.0,
            width: dx,
            height,
        });
    } else if dx < 0.0 {
        rects.push(Rect {
            x: width + dx,
            y: 0.0,
            width: -dx,
            height,
        });
    }
    if dy > 0.0 {
        rects.push(Rect {
            x: 0.0,
            y: 0.0,
            width,
            height: dy,
        });
    } else if dy < 0.0 {
        rects.push(Rect {
            x: 0.0,
            y: height + dy,
            width,
            height: -dy,
        });
    }
    rects
}

/// Whether a box on screen touches any of the strips being redrawn.
///
/// The saving comes from here as much as from the clip: a clipped draw still builds every
/// path before the rasteriser throws it away, and building a hachure fill's paths is most
/// of what a pattern-filled shape costs.
pub fn intersects_any(bounds: Rect, rects: &[Rect]) -> bool {
    rects.iter().any(|r| {
        bounds.x < r.x + r.width
            && bounds.x + bounds.width > r.x
            && bounds.y < r.y + r.height
            && bounds.y + bounds.height > r.y
    })
}

/// Whether redrawing only the exposed strips is cheaper than redrawing the frame.
///
/// It usually is, and on a board of small shapes it is dramatically so. But a strip runs
/// the full width or height of the viewport, so on a board of *large overlapping* shapes
/// it crosses nearly all of them — and then the scroll path pays for the blit, builds
/// almost every path anyway, and comes out behind a plain redraw. Measured on 300
/// screen-sized shapes: 1592ms scrolling against 1006ms redrawing.
///
/// So the choice is made on what the strip actually contains rather than assumed. Half is
/// the threshold because the blit and the clip are not free: below it the saving is real,
/// above it the work is being done twice.
pub fn scroll_is_worth_it(intersecting: usize, visible: usize) -> bool {
    visible > 0 && intersecting * 2 <= visible
}

/// Whether anything is drawn on top of the static layer this frame.
///
/// Selection chrome, what other people hold, a marquee, a lasso, snap guides, a binding
/// halo, the laser. When
/// none of them is there, a frame whose static layer is unchanged has nothing new to show
/// at all — the canvas already holds exactly the right pixels — and the whole frame can be
/// skipped, blit included.
///
/// Worth the trouble because those frames are common and not free. A pan keeps asking for
/// frames for 140ms after the last event so that motion settles smoothly, and on a board
/// of screen-sized shapes each of those was costing a full-canvas copy: 243 of them in a
/// measured pan, against 120 that actually had something to draw.
///
/// `outlined` counts what is outlined: the selection, and each peer's hold.
pub fn overlay_is_empty(
    outlined: usize,
    has_marquee: bool,
    lasso_points: usize,
    laser_strokes: usize,
    snap_guides: usize,
    has_binding_highlight: bool,
    linear_handles: usize,
) -> bool {
    outlined == 0
        && !has_marquee
        && lasso_points == 0
        && laser_strokes == 0
        && snap_guides == 0
        && !has_binding_highlight
        && linear_handles == 0
}

/// How to show a cached layer under a camera it was not drawn for: scale it by `scale`
/// and move it by `(dx, dy)` device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionBlit {
    pub scale: f64,
    pub dx: f64,
    pub dy: f64,
}

impl MotionBlit {
    /// The parts of a `width` × `height` device canvas the moved picture does not cover,
    /// in whole device pixels, rounded outward so they overlap the picture's edge.
    ///
    /// They are painted for real, over the moved picture. Left bare, a pan showed a strip
    /// of empty paper along the edge the board was coming in from until the camera
    /// stopped — at the edge someone is looking at, because it is where they are going.
    pub fn exposed(&self, width: f64, height: f64) -> Vec<Rect> {
        let left = self.dx;
        let right = self.dx + width * self.scale;
        let top = self.dy;
        let bottom = self.dy + height * self.scale;
        let mut rects = Vec::with_capacity(4);
        let mut strip = |x0: f64, y0: f64, x1: f64, y1: f64| {
            let (x0, y0) = (x0.floor().max(0.0), y0.floor().max(0.0));
            let (x1, y1) = (x1.ceil().min(width), y1.ceil().min(height));
            if x1 > x0 && y1 > y0 {
                rects.push(Rect {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                });
            }
        };
        if left > 0.0 {
            strip(0.0, 0.0, left, height);
        }
        if right < width {
            strip(right, 0.0, width, height);
        }
        if top > 0.0 {
            strip(0.0, 0.0, width, top);
        }
        if bottom < height {
            strip(0.0, bottom, width, height);
        }
        rects
    }
}

/// How much a reused picture may leave uncovered before it is repainted instead.
const MOTION_MIN_COVERAGE: f64 = 0.85;
/// How far the zoom may drift from the picture's before it is repainted instead: the
/// picture is resampled, which a quarter either way hides while it is moving.
const MOTION_MAX_DRIFT: f64 = 1.25;

/// Whether a frame **in motion** can reuse a layer drawn for another camera — moved, or
/// scaled about the new camera — and how.
///
/// A pan or a zoom used to repaint every visible element on every frame, which at 15% on
/// a board of 2,000 strokes was the whole board, fourteen milliseconds a frame and more
/// with every stroke added. While the camera is moving the eye cannot tell a moved or
/// slightly scaled picture from a repainted one, so the picture is reused — with the
/// edges it no longer covers painted for real, see [`MotionBlit::exposed`] — until those
/// edges would be more than a sixth of the screen or it has been scaled by more than a
/// quarter; then one real repaint, and reuse again from that. The frame after motion stops is
/// always drawn from scratch, so nothing approximate is ever left on screen.
///
/// `None` means repaint.
pub fn plan_motion(
    painted: Camera,
    now: Camera,
    dpr: f64,
    device_width: f64,
    device_height: f64,
) -> Option<MotionBlit> {
    if painted.scale <= 0.0 || device_width <= 0.0 || device_height <= 0.0 {
        return None;
    }
    let scale = now.scale / painted.scale;
    if !(1.0 / MOTION_MAX_DRIFT..=MOTION_MAX_DRIFT).contains(&scale) {
        return None;
    }
    // A screen point s under the old camera shows world point (s - c_old) / s_old, which
    // the new camera puts at (s - c_old) * scale + c_new.
    let dx = (now.x - scale * painted.x) * dpr;
    let dy = (now.y - scale * painted.y) * dpr;
    let covered_w = (dx + device_width * scale).min(device_width) - dx.max(0.0);
    let covered_h = (dy + device_height * scale).min(device_height) - dy.max(0.0);
    if covered_w <= 0.0 || covered_h <= 0.0 {
        return None;
    }
    let coverage = (covered_w * covered_h) / (device_width * device_height);
    (coverage >= MOTION_MIN_COVERAGE).then_some(MotionBlit { scale, dx, dy })
}

/// How much further out than the camera a picture repainted while zooming out is drawn —
/// under [`MOTION_MAX_DRIFT`], so [`plan_motion`] takes it at once.
const MOTION_AHEAD: f64 = 1.2;

/// While zooming out, the camera to repaint the layer for — `None` means the camera the
/// frame is at.
///
/// A picture drawn for the camera it is at covers the screen exactly, so a zoom out
/// leaves its edges bare at the next step and [`plan_motion`] repaints it a few steps
/// later. On a board of 2,000 shapes that was a full repaint in one frame of every
/// sixteen, each one longer than a frame on a slow machine (`e2e/cameraBudget.spec.ts`).
/// So it is drawn further out, about the point the zoom holds still: shown enlarged at
/// first, then as drawn, then covering less and less until the next repaint — about
/// three times as much zoom out between repaints. The frame after motion stops is drawn
/// for its own camera, as every frame at rest is.
pub fn motion_repaint_camera(
    painted: Camera,
    now: Camera,
    width: f64,
    height: f64,
) -> Option<Camera> {
    const ZOOMING_OUT: f64 = 0.99;
    let ratio = now.scale / painted.scale;
    if ratio.is_nan() || ratio >= ZOOMING_OUT {
        return None;
    }
    // The screen point both cameras show the same world point at: the zoom's centre.
    let still = |painted: f64, now: f64, size: f64| {
        ((now - ratio * painted) / (1.0 - ratio)).clamp(0.0, size)
    };
    let (fx, fy) = (
        still(painted.x, now.x, width),
        still(painted.y, now.y, height),
    );
    let k = 1.0 / MOTION_AHEAD;
    Some(Camera {
        x: fx - (fx - now.x) * k,
        y: fy - (fy - now.y) * k,
        scale: now.scale * k,
    })
}

/// What a frame in motion takes from the scene: `visible` and as much around it as a
/// picture [`motion_repaint_camera`] draws can show, since that picture is painted from
/// the same frame.
pub fn motion_cull(visible: WorldBounds) -> WorldBounds {
    let margin_x = (visible.max_x - visible.min_x) * (MOTION_AHEAD - 1.0);
    let margin_y = (visible.max_y - visible.min_y) * (MOTION_AHEAD - 1.0);
    WorldBounds {
        min_x: visible.min_x - margin_x,
        min_y: visible.min_y - margin_y,
        max_x: visible.max_x + margin_x,
        max_y: visible.max_y + margin_y,
    }
}
