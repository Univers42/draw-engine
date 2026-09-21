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

/// Where the selection frame and its handles sit relative to the element.
///
/// Every field is in **world units**, so the caller converts from screen pixels once and
/// the geometry stays stable across zoom levels.
///
/// The numbers come from Excalidraw's `getTransformHandlesFromCoords`, and the property
/// that matters is that **the handles never overlap the element**. Their frame sits
/// `dashedLineMargin` (4px) outside the outline; each handle is an 8px box whose inner
/// edge is on that frame, so it occupies the ring from 4px to 12px out and leaves a clear
/// 4px gap between the element's own border and anything that resizes it.
///
/// Getting that gap wrong is what made a shape feel unmovable: with the handle centred
/// 8px out but reaching 10px, it covered the element's own edge, and grabbing the border
/// to drag a shape resized it instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandleLayout {
    /// How far outside the element's outline the selection frame is drawn.
    pub frame_pad: f64,
    /// How far outside the outline the handle *centres* sit.
    pub handle_offset: f64,
    /// Radial reach of one handle. Kept at or below `handle_offset - frame_pad` so a
    /// handle can never reach back inside the element.
    pub hit: f64,
    /// Distance from the top of the frame up to the rotation handle.
    pub rotate_gap: f64,
    /// Below this edge length the cardinal handles are dropped.
    ///
    /// Excalidraw's `minimumSizeForEightHandles`: on a short edge the side handle sits
    /// close enough to both corners that it cannot be aimed at deliberately, so it only
    /// steals drags that were meant for a corner. Pass `0.0` to keep all eight.
    pub min_side: f64,
}

impl HandleLayout {
    /// Excalidraw's `dashedLineMargin`: the gap between the outline and the frame.
    pub const FRAME_MARGIN_PX: f64 = 4.0;

    /// The layout used for hit testing and painting alike.
    ///
    /// `scale` is the camera scale, so a handle stays the same size on screen however far
    /// the board is zoomed.
    pub fn screen(handle_px: f64, rotate_gap_px: f64, scale: f64) -> Self {
        let margin = Self::FRAME_MARGIN_PX;
        Self {
            frame_pad: margin / scale,
            // Inner edge of the handle box on the frame, so its centre is half a box
            // further out.
            handle_offset: (margin + handle_px / 2.0) / scale,
            hit: (handle_px / 2.0) / scale,
            rotate_gap: rotate_gap_px / scale,
            min_side: 5.0 * handle_px / scale,
        }
    }

    /// A layout with everything collapsed onto the element's own outline.
    ///
    /// The pre-padding geometry, kept for the callers that ask "where is the corner of
    /// this element" rather than "where do I grab it".
    pub fn bare(rotate_gap: f64) -> Self {
        Self {
            frame_pad: 0.0,
            handle_offset: 0.0,
            hit: 0.0,
            rotate_gap,
            min_side: 0.0,
        }
    }
}

/// The frame corners: the element's box grown by `pad`, then rotated about its centre.
pub fn selection_corners_padded(element: &DrawElement, pad: f64) -> [Point; 4] {
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    let hw = element.width.abs() / 2.0 + pad;
    let hh = element.height.abs() / 2.0 + pad;
    [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]].map(|[lx, ly]| {
        let rotated = rotate_point(lx, ly, element.angle);
        Point {
            x: cx + rotated.x,
            y: cy + rotated.y,
        }
    })
}

pub fn selection_corners(element: &DrawElement) -> [Point; 4] {
    selection_corners_padded(element, 0.0)
}

/// Every handle a pointer may grab, in world space.
///
/// **This is the one list.** The painter and the hit test both call it, because when they
/// disagree the user is left aiming at handles that are not drawn: the cardinal handles
/// used to be hit-testable but never painted, so grabbing the middle of an edge to move a
/// shape silently resized it instead.
pub fn selection_handles(element: &DrawElement, layout: HandleLayout) -> Vec<HandlePoint> {
    let cx = element.x + element.width / 2.0;
    let cy = element.y + element.height / 2.0;
    // `abs` because a shape dragged out leftwards or upwards has negative extent, and a
    // handle on the "west" side must stay on the west side regardless.
    let hw = element.width.abs() / 2.0 + layout.handle_offset;
    let hh = element.height.abs() / 2.0 + layout.handle_offset;
    let wide = element.width.abs() > layout.min_side;
    let tall = element.height.abs() > layout.min_side;

    let mut points: Vec<HandlePoint> = RESIZE_HANDLES
        .iter()
        .copied()
        .filter(|kind| match kind {
            HandleKind::N | HandleKind::S => wide,
            HandleKind::E | HandleKind::W => tall,
            _ => true,
        })
        .map(|kind| {
            let local = handle_local_point(kind, hw, hh);
            let rotated = rotate_point(local.x, local.y, element.angle);
            HandlePoint {
                kind,
                x: cx + rotated.x,
                y: cy + rotated.y,
            }
        })
        .collect();
    let rotate = rotate_point(0.0, -hh - layout.rotate_gap, element.angle);
    points.push(HandlePoint {
        kind: HandleKind::Rotate,
        x: cx + rotate.x,
        y: cy + rotate.y,
    });
    points
}

pub fn selection_handle_points(element: &DrawElement, rotate_gap: f64) -> Vec<HandlePoint> {
    selection_handles(element, HandleLayout::bare(rotate_gap))
}

/// Which handle, if any, is within `tolerance` of the pointer.
///
/// The reach is **radial**. It used to be a box aligned to the world axes, which is only
/// correct for an unrotated element: turn a shape 45 degrees and the hit boxes stayed
/// square to the screen while the painted handles rotated with the shape, so a corner
/// could be grabbed from 1.41x the nominal distance in one direction and not at all from
/// another.
pub fn hit_handle(points: &[HandlePoint], wx: f64, wy: f64, tolerance: f64) -> Option<HandleKind> {
    let limit = tolerance * tolerance;
    let mut best: Option<(f64, HandleKind)> = None;
    for point in points {
        let dx = wx - point.x;
        let dy = wy - point.y;
        let d2 = dx * dx + dy * dy;
        if d2 <= limit && best.is_none_or(|(b, _)| d2 < b) {
            best = Some((d2, point.kind));
        }
    }
    best.map(|(_, kind)| kind)
}
