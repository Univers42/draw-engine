use crate::camera::Point;
use crate::scene::DrawElement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKind {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Rotate,
}

pub const RESIZE_HANDLES: [HandleKind; 8] = [
    HandleKind::Nw,
    HandleKind::N,
    HandleKind::Ne,
    HandleKind::E,
    HandleKind::Se,
    HandleKind::S,
    HandleKind::Sw,
    HandleKind::W,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandlePoint {
    pub kind: HandleKind,
    pub x: f64,
    pub y: f64,
}

pub fn rotate_point(px: f64, py: f64, angle: f64) -> Point {
    let c = angle.cos();
    let s = angle.sin();
    Point {
        x: px * c - py * s,
        y: px * s + py * c,
    }
}

pub fn handle_local_point(kind: HandleKind, half_w: f64, half_h: f64) -> Point {
    let x = if kind.has('w') {
        -half_w
    } else if kind.has('e') {
        half_w
    } else {
        0.0
    };
    let y = if kind.has('n') {
        -half_h
    } else if kind.has('s') {
        half_h
    } else {
        0.0
    };
    Point { x, y }
}

impl HandleKind {
    fn has(self, ch: char) -> bool {
        matches!(
            (self, ch),
            (Self::Nw | Self::W | Self::Sw, 'w')
                | (Self::Ne | Self::E | Self::Se, 'e')
                | (Self::Nw | Self::N | Self::Ne, 'n')
                | (Self::Sw | Self::S | Self::Se, 's')
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nw => "nw",
            Self::N => "n",
            Self::Ne => "ne",
            Self::E => "e",
            Self::Se => "se",
            Self::S => "s",
            Self::Sw => "sw",
            Self::W => "w",
            Self::Rotate => "rotate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "nw" => Some(Self::Nw),
            "n" => Some(Self::N),
            "ne" => Some(Self::Ne),
            "e" => Some(Self::E),
            "se" => Some(Self::Se),
            "s" => Some(Self::S),
            "sw" => Some(Self::Sw),
            "w" => Some(Self::W),
            "rotate" => Some(Self::Rotate),
            _ => None,
        }
    }
}

pub fn selection_corners(element: &DrawElement) -> [Point; 4] {
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let hw = element.width / 2.0;
    let hh = element.height / 2.0;
    [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]].map(|[lx, ly]| {
        let rotated = rotate_point(lx, ly, element.angle);
        Point {
            x: cx + rotated.x,
            y: cy + rotated.y,
        }
    })
}

pub fn selection_handle_points(element: &DrawElement, rotate_gap: f64) -> Vec<HandlePoint> {
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let hw = element.width / 2.0;
    let hh = element.height / 2.0;
    let mut points: Vec<HandlePoint> = RESIZE_HANDLES
        .iter()
        .map(|&kind| {
            let local = handle_local_point(kind, hw, hh);
            let rotated = rotate_point(local.x, local.y, element.angle);
            HandlePoint {
                kind,
                x: cx + rotated.x,
                y: cy + rotated.y,
            }
        })
        .collect();
    let rotate = rotate_point(0.0, -hh - rotate_gap, element.angle);
    points.push(HandlePoint {
        kind: HandleKind::Rotate,
        x: cx + rotate.x,
        y: cy + rotate.y,
    });
    points
}

pub fn hit_handle(points: &[HandlePoint], wx: f64, wy: f64, tolerance: f64) -> Option<HandleKind> {
    points.iter().find_map(|point| {
        if (wx - point.x).abs() <= tolerance && (wy - point.y).abs() <= tolerance {
            Some(point.kind)
        } else {
            None
        }
    })
}
