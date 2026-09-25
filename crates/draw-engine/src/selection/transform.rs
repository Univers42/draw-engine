use crate::camera::Point;
use crate::scene::element::DrawElement;
use crate::selection::handles::{handle_local_point, rotate_point, HandleKind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn opposite(handle: HandleKind) -> Option<HandleKind> {
    Some(match handle {
        HandleKind::Nw => HandleKind::Se,
        HandleKind::N => HandleKind::S,
        HandleKind::Ne => HandleKind::Sw,
        HandleKind::E => HandleKind::W,
        HandleKind::Se => HandleKind::Nw,
        HandleKind::S => HandleKind::N,
        HandleKind::Sw => HandleKind::Ne,
        HandleKind::W => HandleKind::E,
        HandleKind::Rotate => return None,
    })
}

pub fn resize_element(
    element: &DrawElement,
    handle: HandleKind,
    wx: f64,
    wy: f64,
    min_size: f64,
    aspect: Option<f64>,
) -> Geometry {
    resize_element_within(element, handle, wx, wy, (min_size, min_size), aspect)
}

/// [`resize_element`] with a minimum per axis, `(width, height)`: the smallest a shape
/// holding a label may become (`resizeSingleElement`, `resizeElements.ts@1118751f:778-803`).
/// A minimum bounds the size, not the side the pointer is on, so the shape still turns
/// through its anchor.
pub fn resize_element_within(
    element: &DrawElement,
    handle: HandleKind,
    wx: f64,
    wy: f64,
    min_size: (f64, f64),
    aspect: Option<f64>,
) -> Geometry {
    let (min_width, min_height) = min_size;
    if handle == HandleKind::Rotate {
        return Geometry {
            x: element.x,
            y: element.y,
            width: element.width,
            height: element.height,
        };
    }
    let angle = element.angle;
    // The centre of the box as it is *seen*. `x + width / 2` is that centre whatever the
    // sign of the extent, but the half-extents must be positive: a mirrored element is
    // the same size as an unmirrored one, and its north-west handle is still visually
    // north-west.
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let hw = element.width.abs() / 2.0;
    let hh = element.height.abs() / 2.0;
    let Some(opp) = opposite(handle) else {
        return Geometry {
            x: element.x,
            y: element.y,
            width: element.width,
            height: element.height,
        };
    };
    let anchor_local = handle_local_point(opp, hw, hh);
    let aw = rotate_point(anchor_local.x, anchor_local.y, angle);
    let anchor_world = Point {
        x: cx + aw.x,
        y: cy + aw.y,
    };
    let rel = rotate_point(wx - anchor_world.x, wy - anchor_world.y, -angle);
    let controls_x = handle.has_ew();
    let controls_y = handle.has_ns();

    // Which way the box lies from its anchor: the anchor is the corner that stays put, so
    // the element extends away from it.
    let sign_x = if anchor_local.x <= 0.0 { 1.0 } else { -1.0 };
    let sign_y = if anchor_local.y <= 0.0 { 1.0 } else { -1.0 };

    // How far the pointer is from the anchor along that direction. **Signed**: negative
    // means the pointer has crossed the anchor and the element should turn through and
    // come out mirrored on the other side.
    //
    // This used to be `rel.x.abs()`, which threw the side away and then re-derived it
    // from `sign_x` — so dragging a handle past the opposite corner shrank the element to
    // the minimum and grew it again on the side it started from. It bounced off the
    // anchor instead of passing through it.
    let reach_x = sign_x * rel.x;
    let reach_y = sign_y * rel.y;

    let mut width = if controls_x {
        reach_x.abs().max(min_width)
    } else {
        element.width.abs()
    };
    let mut height = if controls_y {
        reach_y.abs().max(min_height)
    } else {
        element.height.abs()
    };
    if let Some(aspect) = aspect.filter(|value| value.is_finite() && *value > 0.0) {
        if !controls_y || (controls_x && width / aspect >= height) {
            height = (width / aspect).max(min_height);
        } else {
            width = (height * aspect).max(min_width);
        }
    }

    // Has this drag turned the element through its own anchor?
    let flipped_x = controls_x && reach_x < 0.0;
    let flipped_y = controls_y && reach_y < 0.0;
    let dir_x = sign_x * if flipped_x { -1.0 } else { 1.0 };
    let dir_y = sign_y * if flipped_y { -1.0 } else { 1.0 };

    let off_local_x = if controls_x { dir_x * width / 2.0 } else { 0.0 };
    let off_local_y = if controls_y {
        dir_y * height / 2.0
    } else {
        0.0
    };
    let off = rotate_point(off_local_x, off_local_y, angle);
    let ncx = anchor_world.x + off.x;
    let ncy = anchor_world.y + off.y;

    // Mirror state accumulates: an already-mirrored element turned through again comes
    // back the right way round. The sign of the extent carries it, and the anchor corner
    // moves to the far edge, which is the convention `normalize_rect` implements and the
    // painter reads as a negative scale.
    let (sx0, sy0) = crate::scene::geometry::mirror_signs(element);
    let mirror_x = sx0 * if flipped_x { -1.0 } else { 1.0 };
    let mirror_y = sy0 * if flipped_y { -1.0 } else { 1.0 };

    let (x, out_width) = if mirror_x < 0.0 {
        (ncx + width / 2.0, -width)
    } else {
        (ncx - width / 2.0, width)
    };
    let (y, out_height) = if mirror_y < 0.0 {
        (ncy + height / 2.0, -height)
    } else {
        (ncy - height / 2.0, height)
    };

    Geometry {
        x,
        y,
        width: out_width,
        height: out_height,
    }
}

/// Excalidraw's `MIN_FONT_SIZE` (`packages/common/src/constants.ts@1118751f:219`): a resize
/// that would take a text below it changes nothing.
pub const MIN_FONT_SIZE: f64 = 1.0;

/// The width and height a drag of `handle` to `(wx, wy)` asks of a box that was `origin`,
/// turned by `angle`, when the drag began: `getNextSingleWidthAndHeightFromPointer`
/// (`resizeElements.ts@1118751f:988-1080`) for a box with positive extents. Signed: a
/// pointer past the far side asks for a negative size. An axis the handle does not move
/// keeps its size. With `keep_aspect` (Shift) a side handle scales the other axis with it
/// and a corner takes the larger of its two scales.
pub fn next_box_size(
    origin: &Geometry,
    angle: f64,
    handle: HandleKind,
    wx: f64,
    wy: f64,
    keep_aspect: bool,
) -> (f64, f64) {
    let (cx, cy) = (
        origin.x + origin.width / 2.0,
        origin.y + origin.height / 2.0,
    );
    let turned = rotate_point(wx - cx, wy - cy, -angle);
    let (px, py) = (cx + turned.x, cy + turned.y);
    let (x2, y2) = (origin.x + origin.width, origin.y + origin.height);
    let mut width = match handle {
        HandleKind::E | HandleKind::Ne | HandleKind::Se => px - origin.x,
        HandleKind::W | HandleKind::Nw | HandleKind::Sw => x2 - px,
        _ => origin.width,
    };
    let mut height = match handle {
        HandleKind::S | HandleKind::Se | HandleKind::Sw => py - origin.y,
        HandleKind::N | HandleKind::Ne | HandleKind::Nw => y2 - py,
        _ => origin.height,
    };
    if keep_aspect && origin.width != 0.0 && origin.height != 0.0 {
        let width_ratio = width.abs() / origin.width;
        let height_ratio = height.abs() / origin.height;
        if handle.has_ew() && handle.has_ns() {
            let ratio = width_ratio.max(height_ratio);
            width = origin.width * ratio * sign(width);
            height = origin.height * ratio * sign(height);
        } else {
            height *= width_ratio;
            width *= height_ratio;
        }
    }
    (width, height)
}

/// `Math.sign`, which is 0 at 0 where `f64::signum` is 1.
fn sign(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value.signum()
    }
}

/// Where a box that was `prev` (its top-left, width and height), turned by `angle`, goes
/// when it becomes `width × height` by `handle`: the corner or side opposite the handle
/// stays put — `getResizedOrigin` with the anchor `getResizeAnchor` gives it
/// (`resizeElements.ts@1118751f:580-727`), neither from the centre nor keeping the aspect,
/// which is how a text is resized (`:338-348`, `:386-396`).
pub fn resized_origin(
    prev: &Geometry,
    width: f64,
    height: f64,
    angle: f64,
    handle: HandleKind,
) -> Point {
    let (sin, cos) = angle.sin_cos();
    let (dw, dh) = ((prev.width - width) / 2.0, (prev.height - height) / 2.0);
    let (x, y) = (prev.x, prev.y);
    match handle {
        // top-left
        HandleKind::E | HandleKind::Se | HandleKind::S => Point {
            x: x + dw - dw * cos + dh * sin,
            y: y + dh - dw * sin - dh * cos,
        },
        // bottom-right
        HandleKind::N | HandleKind::Nw | HandleKind::W => Point {
            x: x + dw * (cos + 1.0) - dh * sin,
            y: y + dh * (cos + 1.0) + dw * sin,
        },
        // bottom-left
        HandleKind::Ne => Point {
            x: x + dw * (1.0 - cos) - dh * sin,
            y: y + dh * (cos + 1.0) - dw * sin,
        },
        // top-right
        HandleKind::Sw => Point {
            x: x + dw * (cos + 1.0) + dh * sin,
            y: y + dh + dw * sin - dh * cos,
        },
        HandleKind::Rotate => Point { x, y },
    }
}

impl HandleKind {
    fn has_ew(self) -> bool {
        matches!(
            self,
            Self::E | Self::W | Self::Ne | Self::Nw | Self::Se | Self::Sw
        )
    }

    fn has_ns(self) -> bool {
        matches!(
            self,
            Self::N | Self::S | Self::Ne | Self::Nw | Self::Se | Self::Sw
        )
    }
}

/// The angle that points the element's top at the pointer.
///
/// Measured from the same pivot the painter turns about, which for a line or arrow is the
/// middle of its points rather than `x + width / 2` — that expression sits outside a
/// leftward arrow entirely, so turning one used to track a pointer relative to a centre
/// beside the arrow instead of inside it.
pub fn rotate_element(element: &DrawElement, wx: f64, wy: f64) -> f64 {
    let c = crate::scene::geometry::rotation_center(element);
    (wy - c.y).atan2(wx - c.x) + std::f64::consts::PI / 2.0
}
