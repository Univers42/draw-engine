use crate::camera::{Point, WorldBounds};
use crate::scene::geometry::element_bounds;
use crate::scene::{DrawElement, DrawElementType};

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
            // Half the handle's *diagonal*, not half its side. The drawn handle is a
            // square and the reach is radial, so half the side would inscribe a circle
            // inside the painted box and leave its four corners — the part of a corner
            // handle a person actually aims at — outside the target. A grab three pixels
            // inside the box used to fall through to the element and move the shape
            // instead of resizing it.
            //
            // This is Excalidraw's reach, derived from their geometry: their corner grab
            // box is the handle's own 8px square, so it reaches 5.66px diagonally from
            // its centre (`transformHandles.ts:138-200`).
            hit: (handle_px * std::f64::consts::SQRT_2 / 2.0) / scale,
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

/// The element's world-space centre and unrotated half-extents.
///
/// `element.x + element.width / 2.0` only gives the centre for a box element, whose
/// `x`/`width` describe a directed interval — for a line or arrow `x` is the position of
/// its **first point**, not a box corner (`scene::geometry::is_point_based`), so the same
/// arithmetic centres on whatever the first point happens to be rather than on the
/// drawing. `element_bounds` already carries the fix (it is where the marquee, the
/// eraser sweep and `rotation_center` get theirs); this is the one place selection reads
/// it from, so the frame, the handles and the hit test cannot drift apart on it again.
fn world_box(element: &DrawElement) -> (f64, f64, f64, f64) {
    let b = element_bounds(element);
    (
        (b.min_x + b.max_x) / 2.0,
        (b.min_y + b.max_y) / 2.0,
        (b.max_x - b.min_x) / 2.0,
        (b.max_y - b.min_y) / 2.0,
    )
}

/// The frame corners: the element's box grown by `pad`, then rotated about its centre.
pub fn selection_corners_padded(element: &DrawElement, pad: f64) -> [Point; 4] {
    let (cx, cy, hw, hh) = world_box(element);
    let hw = hw + pad;
    let hh = hh + pad;
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
///
/// A text shows its corners and its rotation handle only, as the oracle shows every
/// element on a desktop (`DEFAULT_OMIT_SIDES`, `transformHandles.ts@1118751f:57-62`,
/// `:112-131`): its sides are taken on the frame line instead ([`side_at`]). Other kinds
/// keep their side handles above `min_side` — a divergence from the desktop oracle, whose
/// sides are all taken by the line (`docs/reference/resize.md` › Text and labels).
pub fn selection_handles(element: &DrawElement, layout: HandleLayout) -> Vec<HandlePoint> {
    let (cx, cy, hw, hh) = world_box(element);
    let sides = element.kind != DrawElementType::Text;
    let wide = sides && hw * 2.0 > layout.min_side;
    let tall = sides && hh * 2.0 > layout.min_side;
    let hw = hw + layout.handle_offset;
    let hh = hh + layout.handle_offset;

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

/// The oracle's `SIDE_RESIZING_THRESHOLD` (`packages/common/src/constants.ts@1118751f:276`),
/// in screen pixels: how far out a text's frame line is drawn, and how near it a press has
/// to be to take that side.
pub const SIDE_RESIZING_PX: f64 = 4.0;

/// The point on the element's own outline that `kind` moves, in world space: a corner, or
/// the middle of a side — where `getResizeOffsetXY` measures a grab from
/// (`resizeElements.ts@1118751f:497-554`). `None` for the rotation handle, which moves no
/// edge.
pub fn handle_edge_point(element: &DrawElement, kind: HandleKind) -> Option<Point> {
    if kind == HandleKind::Rotate {
        return None;
    }
    let (cx, cy, hw, hh) = world_box(element);
    let local = handle_local_point(kind, hw, hh);
    let turned = rotate_point(local.x, local.y, element.angle);
    Some(Point {
        x: cx + turned.x,
        y: cy + turned.y,
    })
}

/// Which side of `element` a press at `(wx, wy)` takes by its frame line, for a kind whose
/// sides have no handle: `resizeTest` (`resizeTest.ts@1118751f:96-121`). The line runs
/// `reach` outside the box, turned with it, and a press nearer to it than `reach` takes
/// that side — tested north, east, south, west, as the oracle walks them.
pub fn side_at(element: &DrawElement, wx: f64, wy: f64, reach: f64) -> Option<HandleKind> {
    let [nw, ne, se, sw] = selection_corners_padded(element, reach);
    [
        (HandleKind::N, nw, ne),
        (HandleKind::E, ne, se),
        (HandleKind::S, se, sw),
        (HandleKind::W, sw, nw),
    ]
    .into_iter()
    .find(|&(_, a, b)| {
        let distance = crate::scene::geometry::distance_to_segment(wx, wy, a.x, a.y, b.x, b.y);
        distance == 0.0 || distance < reach
    })
    .map(|(kind, _, _)| kind)
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

/// The point on a multi-selection's own frame `b` that `kind` sits at, unpadded: a
/// corner, or the middle of a side. The group's analogue of [`handle_edge_point`] — the
/// frame is never turned, so there is no rotation to apply.
pub fn group_handle_point(kind: HandleKind, b: WorldBounds) -> Point {
    let mid_x = (b.min_x + b.max_x) / 2.0;
    let mid_y = (b.min_y + b.max_y) / 2.0;
    match kind {
        HandleKind::Nw => Point {
            x: b.min_x,
            y: b.min_y,
        },
        HandleKind::Ne => Point {
            x: b.max_x,
            y: b.min_y,
        },
        HandleKind::Se => Point {
            x: b.max_x,
            y: b.max_y,
        },
        HandleKind::Sw => Point {
            x: b.min_x,
            y: b.max_y,
        },
        HandleKind::N => Point {
            x: mid_x,
            y: b.min_y,
        },
        HandleKind::S => Point {
            x: mid_x,
            y: b.max_y,
        },
        HandleKind::E => Point {
            x: b.max_x,
            y: mid_y,
        },
        HandleKind::W => Point {
            x: b.min_x,
            y: mid_y,
        },
        HandleKind::Rotate => Point {
            x: mid_x,
            y: b.min_y,
        },
    }
}

/// Every handle a multi-selection's frame offers, in world space — corners and rotation
/// always, the cardinal sides only when the frame is big enough along that axis not to
/// crowd its corners: Excalidraw's `minimumSizeForEightHandles`
/// (`transformHandles.ts@1118751f:218-244`), read off `layout.min_side` as a single
/// element's own handles already are ([`selection_handles`]).
///
/// The oracle omits every side on the desktop, single element or group, and takes them on
/// the frame line instead (`DEFAULT_OMIT_SIDES`, `resizeTest.ts@1118751f:96-121`). This
/// engine already diverges for a single shape, which keeps its drawn side handles
/// (`docs/reference/resize.md` › "the sides of anything but a text"); a group's follow the
/// same, already-pinned convention rather than adding a second way of taking a side.
pub fn group_selection_handles(b: WorldBounds, layout: HandleLayout) -> Vec<HandlePoint> {
    let wide = (b.max_x - b.min_x) > layout.min_side;
    let tall = (b.max_y - b.min_y) > layout.min_side;
    let padded = WorldBounds {
        min_x: b.min_x - layout.handle_offset,
        min_y: b.min_y - layout.handle_offset,
        max_x: b.max_x + layout.handle_offset,
        max_y: b.max_y + layout.handle_offset,
    };
    let mut kinds = vec![
        HandleKind::Nw,
        HandleKind::Ne,
        HandleKind::Se,
        HandleKind::Sw,
    ];
    if wide {
        kinds.push(HandleKind::N);
        kinds.push(HandleKind::S);
    }
    if tall {
        kinds.push(HandleKind::E);
        kinds.push(HandleKind::W);
    }
    let mut points: Vec<HandlePoint> = kinds
        .into_iter()
        .map(|kind| {
            let p = group_handle_point(kind, padded);
            HandlePoint {
                kind,
                x: p.x,
                y: p.y,
            }
        })
        .collect();
    points.push(HandlePoint {
        kind: HandleKind::Rotate,
        x: (padded.min_x + padded.max_x) / 2.0,
        y: padded.min_y - layout.rotate_gap,
    });
    points
}
