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
        reach_x.abs().max(min_size)
    } else {
        element.width.abs()
    };
    let mut height = if controls_y {
        reach_y.abs().max(min_size)
    } else {
        element.height.abs()
    };
    if let Some(aspect) = aspect.filter(|value| value.is_finite() && *value > 0.0) {
        if !controls_y || (controls_x && width / aspect >= height) {
            height = (width / aspect).max(min_size);
        } else {
            width = (height * aspect).max(min_size);
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
