use crate::scene::geometry::{normalize_rect, Rect};

/// The box a drag from `(sx, sy)` to `(cx, cy)` grows into.
///
/// `square` is Shift and `from_center` is Alt
/// (`shouldMaintainAspectRatio` / `shouldResizeFromCenter`,
/// `packages/common/src/keys.ts@1118751f:145-149`). The oracle applies the aspect lock
/// first and the centre second (`dragNewElement`, `dragElements.ts@1118751f:325-350`
/// before `:368-376`), and the order matters: squaring a centred reach would give a box
/// twice the side.
///
/// Square is the larger of the two reaches on both axes, which is Excalidraw's rule
/// (`getPerfectElementSize`, `sizeHelpers.ts@1118751f:181-183`: `height = absWidth *
/// sign(height)` — the width decides, the height follows, and the sign only says which way
/// round the shape points).
///
/// `from_center` follows `dragElements.ts@1118751f:371-376`:
///
/// ```text
/// if (shouldResizeFromCenter) {
///   width += width;
///   height += height;
///   newX = originX - width / 2;
///   newY = originY - height / 2;
/// }
/// ```
///
/// The press is the box's middle and each reach is doubled, so a drag reaching `w` by `h`
/// from `(sx, sy)` gives a box of `2w` by `2h` centred on it. The oracle's `width` and
/// `height` arrive as `distance()` — `Math.abs` (`App.tsx@1118751f:13417-13418`) — and the
/// centred corner reads only those, never which side of the origin the pointer went, so
/// neither the drag's direction nor its crossing back over the press can reach the result.
pub fn rect_from_drag(sx: f64, sy: f64, cx: f64, cy: f64, square: bool, from_center: bool) -> Rect {
    let mut dx = cx - sx;
    let mut dy = cy - sy;
    if square {
        let side = dx.abs().max(dy.abs());
        dx = if dx < 0.0 { -1.0 } else { 1.0 } * side;
        dy = if dy < 0.0 { -1.0 } else { 1.0 } * side;
    }
    match from_center {
        false => normalize_rect(sx, sy, dx, dy),
        // The press is the middle, so each axis reaches half a box to either side of it.
        true => Rect {
            x: sx - dx.abs(),
            y: sy - dy.abs(),
            width: 2.0 * dx.abs(),
            height: 2.0 * dy.abs(),
        },
    }
}

pub fn is_degenerate_rect(rect: Rect, min: f64) -> bool {
    rect.width < min && rect.height < min
}
