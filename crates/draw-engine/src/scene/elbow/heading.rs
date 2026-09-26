//! Which way a route leaves or enters a point: Excalidraw's `heading.ts@1118751f`.

use super::outline::{bounds_center, cross, rotate, scale_from, Bounds, Outline, Pt, Target};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    Up,
    Right,
    Down,
    Left,
}

impl Heading {
    pub fn is_horizontal(self) -> bool {
        matches!(self, Heading::Left | Heading::Right)
    }

    /// `flipHeading`.
    pub fn flip(self) -> Heading {
        match self {
            Heading::Up => Heading::Down,
            Heading::Right => Heading::Left,
            Heading::Down => Heading::Up,
            Heading::Left => Heading::Right,
        }
    }

    /// Right and down: the headings that grow a coordinate.
    pub fn is_positive(self) -> bool {
        matches!(self, Heading::Right | Heading::Down)
    }
}

/// `vectorToHeading`: a diagonal goes left or up, and a zero vector left.
pub fn vector_to_heading([x, y]: Pt) -> Heading {
    let (abs_x, abs_y) = (x.abs(), y.abs());
    if x > abs_y {
        Heading::Right
    } else if x <= -abs_y {
        Heading::Left
    } else if y > abs_x {
        Heading::Down
    } else {
        Heading::Up
    }
}

/// `headingForPoint(p, o)`: the heading of `p` seen from `o`.
pub fn heading_for_point(p: Pt, o: Pt) -> Heading {
    vector_to_heading([p[0] - o[0], p[1] - o[1]])
}

/// `headingForPointIsHorizontal`.
pub fn heading_for_point_is_horizontal(p: Pt, o: Pt) -> bool {
    heading_for_point(p, o).is_horizontal()
}

/// `triangleIncludesPoint`: true on the edges too, whatever its comment says.
fn triangle_includes([a, b, c]: [Pt; 3], p: Pt) -> bool {
    let sign = |p1: Pt, p2: Pt, p3: Pt| {
        (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])
    };
    let (d1, d2, d3) = (sign(p, a, b), sign(p, b, c), sign(p, c, a));
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

/// `headingForPointFromElement`: which of four cones around `aabb`'s middle `p` is in.
pub fn heading_for_point_from_element(t: &Target, aabb: Bounds, p: Pt) -> Heading {
    const SEARCH_CONE_MULTIPLIER: f64 = 2.0;
    if t.outline == Outline::Diamond {
        return heading_for_point_from_diamond(t, aabb, p);
    }
    let mid = bounds_center(aabb);
    let top_left = scale_from([aabb[0], aabb[1]], mid, SEARCH_CONE_MULTIPLIER);
    let top_right = scale_from([aabb[2], aabb[1]], mid, SEARCH_CONE_MULTIPLIER);
    let bottom_left = scale_from([aabb[0], aabb[3]], mid, SEARCH_CONE_MULTIPLIER);
    let bottom_right = scale_from([aabb[2], aabb[3]], mid, SEARCH_CONE_MULTIPLIER);
    if triangle_includes([top_left, top_right, mid], p) {
        Heading::Up
    } else if triangle_includes([top_right, bottom_right, mid], p) {
        Heading::Right
    } else if triangle_includes([bottom_right, bottom_left, mid], p) {
        Heading::Down
    } else {
        Heading::Left
    }
}

/// `headingForPointFromDiamondElement`: the corners first, then the sides — a side hands
/// its heading to the corner along the diamond's long axis.
fn heading_for_point_from_diamond(t: &Target, aabb: Bounds, point: Pt) -> Heading {
    const SHRINK: f64 = 0.95;
    let mid = bounds_center(aabb);
    let vertex = |x: f64, y: f64| {
        let r = rotate([x, y], mid, t.angle);
        [
            mid[0] + (r[0] - mid[0]) * SHRINK,
            mid[1] + (r[1] - mid[1]) * SHRINK,
        ]
    };
    let top = vertex(t.x + t.width / 2.0, t.y);
    let right = vertex(t.x + t.width, t.y + t.height / 2.0);
    let bottom = vertex(t.x + t.width / 2.0, t.y + t.height);
    let left = vertex(t.x, t.y + t.height / 2.0);
    let from = |p: Pt, o: Pt| [p[0] - o[0], p[1] - o[1]];
    let corner = |v: Pt, next: Pt, previous: Pt| {
        cross(from(point, v), from(v, next)) <= 0.0
            && cross(from(point, v), from(v, previous)) > 0.0
    };
    if corner(top, right, left) {
        return heading_for_point(top, mid);
    }
    if corner(right, bottom, top) {
        return heading_for_point(right, mid);
    }
    if corner(bottom, left, right) {
        return heading_for_point(bottom, mid);
    }
    if corner(left, top, bottom) {
        return heading_for_point(left, mid);
    }
    let side = |a: Pt, b: Pt| {
        cross(from(point, mid), from(a, mid)) <= 0.0 && cross(from(point, mid), from(b, mid)) > 0.0
    };
    let wide = t.width > t.height;
    let p = if side(top, right) {
        if wide {
            top
        } else {
            right
        }
    } else if side(right, bottom) {
        if wide {
            bottom
        } else {
            right
        }
    } else if side(bottom, left) {
        if wide {
            bottom
        } else {
            left
        }
    } else if wide {
        top
    } else {
        left
    };
    heading_for_point(p, mid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_heads_where_the_oracle_says() {
        assert_eq!(vector_to_heading([1.0, 0.0]), Heading::Right);
        // A diagonal is not "more" right than down: it goes left or up.
        assert_eq!(vector_to_heading([1.0, 1.0]), Heading::Up);
        assert_eq!(vector_to_heading([-1.0, 1.0]), Heading::Left);
        assert_eq!(vector_to_heading([0.0, 0.0]), Heading::Left);
        assert_eq!(vector_to_heading([0.0, 2.0]), Heading::Down);
    }
}
