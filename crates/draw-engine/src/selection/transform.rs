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
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let hw = element.width / 2.0;
    let hh = element.height / 2.0;
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
    let mut width = if controls_x {
        rel.x.abs().max(min_size)
    } else {
        element.width
    };
    let mut height = if controls_y {
        rel.y.abs().max(min_size)
    } else {
        element.height
    };
    if let Some(aspect) = aspect.filter(|value| value.is_finite() && *value > 0.0) {
        if !controls_y || (controls_x && width / aspect >= height) {
            height = (width / aspect).max(min_size);
        } else {
            width = (height * aspect).max(min_size);
        }
    }
    let sign_x = if anchor_local.x <= 0.0 { 1.0 } else { -1.0 };
    let sign_y = if anchor_local.y <= 0.0 { 1.0 } else { -1.0 };
    let off_local_x = if controls_x {
        (sign_x * width) / 2.0
    } else {
        0.0
    };
    let off_local_y = if controls_y {
        (sign_y * height) / 2.0
    } else {
        0.0
    };
    let off = rotate_point(off_local_x, off_local_y, angle);
    let ncx = anchor_world.x + off.x;
    let ncy = anchor_world.y + off.y;
    Geometry {
        x: ncx - width / 2.0,
        y: ncy - height / 2.0,
        width,
        height,
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

pub fn rotate_element(element: &DrawElement, wx: f64, wy: f64) -> f64 {
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    (wy - cy).atan2(wx - cx) + std::f64::consts::PI / 2.0
}
